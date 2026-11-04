//! 有容量和 TTL 上限的匿名公开页 HTML 缓存。
//!
//! 不依赖框架的磁盘 ISR：生产非 root 容器无须写 /app，写操作能真正失效内存。
//! 透传响应流，在完整成功读完后保存；取消、超大响应和跨失效世代的响应不会回填。
//! 写入后在阻塞线程池按已启用的算法高档压缩一次（`Effort::Cached`），命中时按
//! `Accept-Encoding` 直接返回预压缩变体，不再每次现压。
#![cfg(feature = "server")]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, BodyDataStream};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::Response;
use bytes::Bytes;
use futures::StreamExt;
use sha2::{Digest, Sha256};

use crate::compression::{self, CompressionAlgorithms, Effort, Encoding};

const MAX_PAGE_BYTES: usize = 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SsrGeneration(pub u64);

#[derive(Clone)]
struct CachedPage {
    body: Bytes,
    /// 写入后补齐的预压缩变体；补齐前为空，命中交给外层压缩层现压。
    encoded: Arc<[(Encoding, Bytes)]>,
    headers: HeaderMap,
    created: Instant,
    etag: HeaderValue,
    generation: u64,
}

struct HtmlCache {
    pages: moka::sync::Cache<String, CachedPage>,
    generation: AtomicU64,
    gate: Mutex<()>,
    enabled: bool,
    /// 当前缓存命名空间（release×flavor，plan §6.7）。空串 = 尚未初始化。
    namespace: Mutex<String>,
}

impl HtmlCache {
    fn new(ttl: Duration, capacity: u64) -> Self {
        Self {
            pages: moka::sync::Cache::builder()
                .max_capacity(capacity)
                .weigher(|key: &String, page: &CachedPage| {
                    let encoded: usize = page.encoded.iter().map(|(_, bytes)| bytes.len()).sum();
                    (key.len() + page.body.len() + encoded + 1024) as u32
                })
                .time_to_live(ttl)
                .build(),
            generation: AtomicU64::new(0),
            gate: Mutex::new(()),
            enabled: !ttl.is_zero(),
            namespace: Mutex::new(String::new()),
        }
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn invalidate(&self) -> u64 {
        let _guard = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        let generation = self
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        self.pages.invalidate_all();
        generation
    }

    /// 切换命名空间：内容不同则整表失效并推进世代，保证旧命名空间下的已缓存
    /// 页面与在途回填都绝不返回给新命名空间（plan §6.7「release/flavor 纳入隔离」）。
    /// 同一进程内一个缓存实例只服务一个 release×flavor 组合，切换只发生在
    /// 启动/开发热重载重新组装路由时。返回是否真的发生了切换。
    fn swap_namespace(&self, namespace: &str) -> bool {
        let _guard = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        let mut current = self.namespace.lock().unwrap_or_else(|e| e.into_inner());
        if *current == namespace {
            return false;
        }
        *current = namespace.to_owned();
        // 世代号与 invalidate() 同步推进；新值无需使用。
        let _ = self
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        self.pages.invalidate_all();
        true
    }

    fn get(&self, path: &str) -> Option<CachedPage> {
        self.pages
            .get(path)
            .filter(|page| page.generation == self.generation())
    }

    /// 新写入时返回 ETag 与正文，供调用方预压缩；同世代已有相同内容（并发未命中的
    /// 先到者已写入）时不覆盖，返回 None，避免重复预压缩并保留已补齐的变体。
    fn insert(
        &self,
        path: String,
        headers: HeaderMap,
        body: Vec<u8>,
        generation: u64,
    ) -> Option<(HeaderValue, Bytes)> {
        if !self.enabled || body.len() > MAX_PAGE_BYTES {
            return None;
        }
        let etag = HeaderValue::from_str(&format!("\"{}\"", hex::encode(Sha256::digest(&body))))
            .expect("SHA256 is an ASCII entity tag");
        let _guard = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        if generation != self.generation()
            || self
                .pages
                .get(&path)
                .is_some_and(|page| page.generation == generation && page.etag == etag)
        {
            return None;
        }
        let body = Bytes::from(body);
        self.pages.insert(
            path,
            CachedPage {
                body: body.clone(),
                encoded: Arc::new([]),
                headers,
                etag: etag.clone(),
                generation,
                created: Instant::now(),
            },
        );
        Some((etag, body))
    }

    /// 补上预压缩变体；页面已失效、过期或被不同内容替换时丢弃。
    fn attach_encoded(
        &self,
        path: &str,
        etag: &HeaderValue,
        generation: u64,
        encoded: Vec<(Encoding, Bytes)>,
    ) {
        let _guard = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        let Some(page) = self
            .get(path)
            .filter(|page| page.generation == generation && page.etag == *etag)
        else {
            return;
        };
        self.pages.insert(
            path.to_owned(),
            CachedPage {
                encoded: encoded.into(),
                ..page
            },
        );
    }
}

/// 在阻塞线程池按已启用的全部算法压缩一次再写回缓存。brotli 11 每 100 KB 约 0.1 s，
/// 不占请求路径；补齐前的命中照旧由外层压缩层现压。
fn precompress(path: String, etag: HeaderValue, generation: u64, body: Bytes) {
    let Some(algorithms) = compression::algorithms() else {
        return;
    };
    tokio::task::spawn_blocking(move || {
        let mut encoded = Vec::new();
        for encoding in algorithms.encodings() {
            match compression::encode(encoding, Effort::Cached, &body) {
                Some(Ok(bytes)) => encoded.push((encoding, Bytes::from(bytes))),
                Some(Err(error)) => {
                    tracing::warn!(%error, %path, "SSR 缓存预压缩失败，命中继续现压");
                    return;
                }
                None => {}
            }
        }
        HTML_CACHE.attach_encoded(&path, &etag, generation, encoded);
    });
}

/// 命中响应。协商出的编码有预压缩变体时直接返回；否则给出原始 HTML，由外层压缩层
/// 现压或回 406。启用压缩时一律带 `Vary: Accept-Encoding`（含 304）。
fn hit_response(
    page: CachedPage,
    request: &HeaderMap,
    algorithms: Option<CompressionAlgorithms>,
) -> Response {
    let unchanged = request
        .get(header::IF_NONE_MATCH)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|value| {
            value.split(',').any(|tag| {
                let tag = tag.trim().strip_prefix("W/").unwrap_or(tag.trim());
                tag == "*" || tag == page.etag.to_str().unwrap_or("")
            })
        });
    let variant = algorithms
        .and_then(|algorithms| compression::negotiate(request, algorithms))
        .and_then(|encoding| page.encoded.iter().find(|(e, _)| *e == encoding).cloned())
        .filter(|_| !unchanged);
    let mut response = Response::new(match (&variant, unchanged) {
        (_, true) => Body::empty(),
        (Some((_, bytes)), false) => Body::from(bytes.clone()),
        (None, false) => Body::from(page.body),
    });
    *response.headers_mut() = page.headers;
    if unchanged {
        *response.status_mut() = StatusCode::NOT_MODIFIED;
    }
    let headers = response.headers_mut();
    if let Some((encoding, _)) = variant {
        headers.insert(header::CONTENT_ENCODING, encoding.header_value());
    }
    if algorithms.is_some() {
        headers.insert(header::VARY, HeaderValue::from_static("accept-encoding"));
    }
    headers.insert(header::ETAG, page.etag);
    headers.insert(
        header::AGE,
        HeaderValue::from(page.created.elapsed().as_secs()),
    );
    headers.insert("x-ssr-cache", HeaderValue::from_static("HIT"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, no-cache"),
    );
    response
}

static HTML_CACHE: LazyLock<HtmlCache> = LazyLock::new(|| {
    HtmlCache::new(
        Duration::from_secs(crate::utils::server::parse_ssr_cache_secs()),
        MAX_CACHE_BYTES,
    )
});

/// 采用白名单，查询参数、登录 Cookie、认证头和私有路由永远不进入共享 HTML 缓存。
fn cacheable_request(method: &Method, uri: &Uri, headers: &HeaderMap) -> bool {
    if method != Method::GET
        || uri.query().is_some()
        || headers.contains_key(header::COOKIE)
        || headers.contains_key(header::AUTHORIZATION)
        || uri.path().len() > 512
    {
        return false;
    }
    let path = uri.path();
    matches!(
        path,
        "/" | "/archives" | "/friends" | "/about" | "/changelog"
    ) || ["/post/", "/page/", "/tags/"].iter().any(|prefix| {
        path.strip_prefix(prefix)
            .is_some_and(|tail| !tail.is_empty() && !tail.contains('/'))
    })
}

fn cacheable_response(response: &Response) -> bool {
    let headers = response.headers();
    response.status() == StatusCode::OK
        && headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/html"))
        && !headers.contains_key(header::SET_COOKIE)
        && !headers.contains_key(header::CONTENT_ENCODING)
        && !headers.contains_key(header::VARY)
        && !headers
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.split(',')
                    .any(|p| matches!(p.trim(), "private" | "no-store"))
            })
}

struct CaptureBody {
    input: BodyDataStream,
    buffer: Option<Vec<u8>>,
    path: String,
    headers: HeaderMap,
    generation: u64,
}

/// 在压缩层内侧缓存原始 HTML，保持 SSR 流式响应和现有静态资源预压缩行为。
pub async fn cache_public_html(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let authenticated = req.headers().contains_key(header::COOKIE)
        || req.headers().contains_key(header::AUTHORIZATION);
    if !HTML_CACHE.enabled || !cacheable_request(req.method(), req.uri(), req.headers()) {
        let mut response = next.run(req).await;
        response
            .headers_mut()
            .insert("x-ssr-cache", HeaderValue::from_static("BYPASS"));
        if authenticated
            && response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.starts_with("text/html"))
        {
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            );
        }
        return response;
    }
    let path = req.uri().path().to_owned();
    if let Some(page) = HTML_CACHE.get(&path) {
        return hit_response(page, req.headers(), compression::algorithms());
    }
    let generation = HTML_CACHE.generation();
    let mut response = next.run(req).await;
    if !cacheable_response(&response) {
        return response;
    }
    response
        .headers_mut()
        .insert("x-ssr-cache", HeaderValue::from_static("MISS"));
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, no-cache"),
    );
    let (parts, body) = response.into_parts();
    let mut headers = parts.headers.clone();
    for name in [
        header::CONTENT_LENGTH,
        header::TRANSFER_ENCODING,
        header::AGE,
    ] {
        headers.remove(name);
    }
    let state = CaptureBody {
        input: body.into_data_stream(),
        buffer: Some(Vec::new()),
        path,
        headers,
        generation,
    };
    let stream = futures::stream::unfold(state, |mut state| async move {
        match state.input.next().await {
            Some(Ok(chunk)) => {
                if let Some(buffer) = state.buffer.as_mut() {
                    if buffer.len() + chunk.len() <= MAX_PAGE_BYTES {
                        buffer.extend_from_slice(&chunk);
                    } else {
                        state.buffer = None;
                    }
                }
                Some((Ok::<_, axum::Error>(chunk), state))
            }
            Some(Err(error)) => {
                state.buffer = None;
                Some((Err(error), state))
            }
            None => {
                if let Some(buffer) = state.buffer.take() {
                    if let Some((etag, body)) = HTML_CACHE.insert(
                        state.path.clone(),
                        state.headers,
                        buffer,
                        state.generation,
                    ) {
                        precompress(state.path, etag, state.generation, body);
                    }
                }
                None
            }
        }
    })
    // 压缩层等下游在流结束后可能再次 poll；unfold 对此会 panic，中断该响应。
    .fuse();
    Response::from_parts(parts, Body::from_stream(stream))
}

// 内容写入通常同时影响详情、列表、归档与相邻导航；小型缓存统一失效避免遗漏。
pub fn invalidate_ssr_route(_route: &str) {
    HTML_CACHE.invalidate();
}
pub fn invalidate_post_preview(_slug: &str) { /* 管理预览从不缓存。 */
}
pub fn invalidate_ssr_all_public() {
    HTML_CACHE.invalidate();
}
/// 设置发布×flavor 命名空间（plan §6.7），变化即整表失效。启动期与开发热重载
/// 重组路由时调用；同一进程内 release/flavor 不变时是零成本空操作。
pub fn set_release_namespace(namespace: &str) {
    if HTML_CACHE.swap_namespace(namespace) {
        tracing::info!(%namespace, "SSR 缓存命名空间切换，旧 release/flavor 页面已失效");
    }
}
pub fn bump_global_generation() -> u64 {
    HTML_CACHE.invalidate()
}
pub fn current_global_generation() -> u64 {
    HTML_CACHE.generation()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_excludes_auth_private_routes_and_query_strings() {
        let mut headers = HeaderMap::new();
        for path in [
            "/admin",
            "/login",
            "/notes",
            "/notes/example",
            "/note-media/1",
            "/api/test",
            "/?page=1",
            "/post/a?preview=1",
        ] {
            assert!(
                !cacheable_request(&Method::GET, &path.parse().unwrap(), &headers),
                "{path}"
            );
        }
        assert!(cacheable_request(
            &Method::GET,
            &"/post/a".parse().unwrap(),
            &headers
        ));
        headers.insert(header::COOKIE, HeaderValue::from_static("session=private"));
        assert!(!cacheable_request(
            &Method::GET,
            &"/".parse().unwrap(),
            &headers
        ));
        headers.clear();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer private"),
        );
        assert!(!cacheable_request(
            &Method::GET,
            &"/".parse().unwrap(),
            &headers
        ));
    }

    #[test]
    fn invalidation_rejects_inflight_old_responses() {
        let cache = HtmlCache::new(Duration::from_secs(60), MAX_CACHE_BYTES);
        let generation = cache.generation();
        cache.insert("/".into(), HeaderMap::new(), b"old".to_vec(), generation);
        assert!(cache.get("/").is_some());
        cache.invalidate();
        cache.insert(
            "/".into(),
            HeaderMap::new(),
            b"stale inflight".to_vec(),
            generation,
        );
        assert!(cache.get("/").is_none());
        cache.insert(
            "/".into(),
            HeaderMap::new(),
            b"new".to_vec(),
            cache.generation(),
        );
        assert_eq!(cache.get("/").unwrap().body.as_ref(), b"new");
    }

    #[test]
    fn namespace_swap_isolates_release_and_flavor_variants() {
        let cache = HtmlCache::new(Duration::from_secs(60), MAX_CACHE_BYTES);
        let generation = cache.generation();
        cache.insert(
            "/post/a".into(),
            HeaderMap::new(),
            b"dual page".to_vec(),
            generation,
        );
        assert!(cache.get("/post/a").is_some());

        // 切换命名空间（dual → 完整应用回退）：旧页面不可见。
        assert!(cache.swap_namespace("full/rel-2"));
        assert!(cache.get("/post/a").is_none());
        // 旧命名空间的在途回填同样被世代号拒绝。
        cache.insert(
            "/post/a".into(),
            HeaderMap::new(),
            b"stale dual page".to_vec(),
            generation,
        );
        assert!(cache.get("/post/a").is_none());

        // 新命名空间正常写入读取。
        cache.insert(
            "/post/a".into(),
            HeaderMap::new(),
            b"full page".to_vec(),
            cache.generation(),
        );
        assert_eq!(cache.get("/post/a").unwrap().body.as_ref(), b"full page");

        // 相同命名空间重复设置不失效（开发热重载零成本路径）。
        assert!(!cache.swap_namespace("full/rel-2"));
        assert_eq!(cache.get("/post/a").unwrap().body.as_ref(), b"full page");
    }

    #[test]
    fn cached_pages_expire_and_oversize_pages_are_not_stored() {
        let cache = HtmlCache::new(Duration::from_millis(20), MAX_CACHE_BYTES);
        cache.insert("/".into(), HeaderMap::new(), b"html".to_vec(), 0);
        assert!(cache.get("/").is_some());
        std::thread::sleep(Duration::from_millis(40));
        assert!(cache.get("/").is_none());
        cache.insert(
            "/large".into(),
            HeaderMap::new(),
            vec![0; MAX_PAGE_BYTES + 1],
            0,
        );
        assert!(cache.get("/large").is_none());
    }

    #[test]
    fn sensitive_and_failed_responses_are_not_cached() {
        let mut response = Response::builder()
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::empty())
            .unwrap();
        assert!(cacheable_response(&response));
        response.headers_mut().insert(
            header::SET_COOKIE,
            HeaderValue::from_static("session=private"),
        );
        assert!(!cacheable_response(&response));
        response.headers_mut().remove(header::SET_COOKIE);
        *response.status_mut() = StatusCode::NOT_FOUND;
        assert!(!cacheable_response(&response));
    }

    fn request(accept_encoding: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_str(accept_encoding).unwrap(),
        );
        headers
    }

    async fn served(response: Response) -> (Option<Encoding>, Vec<u8>) {
        let encoding = response
            .headers()
            .get(header::CONTENT_ENCODING)
            .map(|value| match value.to_str().unwrap() {
                "br" => Encoding::Brotli,
                "gzip" => Encoding::Gzip,
                other => panic!("unexpected encoding {other}"),
            });
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = compression::decode(encoding.unwrap_or(Encoding::Identity), &body);
        (encoding, html)
    }

    #[tokio::test]
    async fn hits_serve_negotiated_precompressed_variants() {
        let algorithms = compression::parse_compression_algorithms("br, gzip");
        let cache = HtmlCache::new(Duration::from_secs(60), MAX_CACHE_BYTES);
        let html = "<article><p>预压缩命中 precompressed hit.</p></article>".repeat(500);
        let (etag, body) = cache
            .insert("/".into(), HeaderMap::new(), html.clone().into_bytes(), 0)
            .unwrap();

        // 变体补齐前：交给外层压缩层现压。
        let response = hit_response(cache.get("/").unwrap(), &request("br"), algorithms);
        assert_eq!(response.headers()[header::VARY], "accept-encoding");
        assert_eq!(served(response).await, (None, html.clone().into_bytes()));

        let encoded = algorithms
            .unwrap()
            .encodings()
            .map(|encoding| {
                let bytes = compression::encode(encoding, Effort::Cached, &body);
                (encoding, Bytes::from(bytes.unwrap().unwrap()))
            })
            .collect();
        cache.attach_encoded("/", &etag, 0, encoded);
        for (accept, expected) in [
            ("gzip, deflate, br, zstd", Some(Encoding::Brotli)),
            ("gzip", Some(Encoding::Gzip)),
            ("zstd", None),
        ] {
            let response = hit_response(cache.get("/").unwrap(), &request(accept), algorithms);
            assert_eq!(response.headers()[header::ETAG], etag, "{accept}");
            assert_eq!(
                served(response).await,
                (expected, html.clone().into_bytes()),
                "{accept}"
            );
        }

        let mut revalidate = request("br");
        revalidate.insert(header::IF_NONE_MATCH, etag);
        let response = hit_response(cache.get("/").unwrap(), &revalidate, algorithms);
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert!(!response.headers().contains_key(header::CONTENT_ENCODING));
        assert_eq!(served(response).await, (None, Vec::new()));

        // 关闭压缩时只回原文，也不声明 Vary。
        let response = hit_response(cache.get("/").unwrap(), &request("br"), None);
        assert!(!response.headers().contains_key(header::VARY));
        assert_eq!(served(response).await, (None, html.into_bytes()));
    }

    #[test]
    fn variants_only_attach_to_the_page_they_were_encoded_from() {
        let cache = HtmlCache::new(Duration::from_secs(60), MAX_CACHE_BYTES);
        let variant = || vec![(Encoding::Brotli, Bytes::from_static(b"br"))];
        let (old_etag, _) = cache
            .insert("/".into(), HeaderMap::new(), b"old".to_vec(), 0)
            .unwrap();
        // 并发未命中的后到者内容相同：不覆盖、不重复预压缩。
        cache.attach_encoded("/", &old_etag, 0, variant());
        assert!(cache
            .insert("/".into(), HeaderMap::new(), b"old".to_vec(), 0)
            .is_none());
        assert_eq!(cache.get("/").unwrap().encoded.len(), 1);

        // 内容变了：新页面替换旧页面，旧页面的变体不能挂到新正文上。
        assert!(cache
            .insert("/".into(), HeaderMap::new(), b"new".to_vec(), 0)
            .is_some());
        cache.attach_encoded("/", &old_etag, 0, variant());
        assert!(cache.get("/").unwrap().encoded.is_empty());

        // 失效后在途的预压缩结果同样丢弃。
        let generation = cache.invalidate();
        let (etag, _) = cache
            .insert("/".into(), HeaderMap::new(), b"new".to_vec(), generation)
            .unwrap();
        cache.invalidate();
        cache.attach_encoded("/", &etag, generation, variant());
        assert!(cache.get("/").is_none());
    }
}
