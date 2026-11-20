//! 有容量和 TTL 上限的匿名公开页 HTML 缓存。
//!
//! 不依赖框架的磁盘 ISR：生产非 root 容器无须写 /app，写操作能真正失效内存。
//! 透传响应流，在完整成功读完后保存；取消、超大响应和跨失效世代的响应不会回填。
//! 写入后在阻塞线程池按已启用的算法高档压缩一次（`Effort::Cached`），命中时按
//! `Accept-Encoding` 直接返回预压缩变体，不再每次现压。
#![cfg(feature = "server")]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, BodyDataStream};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::Response;
use bytes::Bytes;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::sync::watch;

use crate::compression::{self, CompressionAlgorithms, Effort, Encoding};

/// 缓存总量。单页上限（`SsrCacheSettings::max_page_bytes`，后台「页面缓存」可调，默认 2 MiB）
/// 不超过它，否则一页就会挤掉整个缓存。
const MAX_CACHE_BYTES: u64 = 32 * 1024 * 1024;
const _: () = assert!(crate::models::settings::MAX_SSR_CACHE_MAX_PAGE_BYTES <= MAX_CACHE_BYTES);

pub(crate) const HOME_PATH: &str = "/";
/// 同路由的首个客户端可能很慢。其他客户端最多等这么久，之后各自完成渲染。
const FILL_WAIT: Duration = Duration::from_secs(1);

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
    home_generation: AtomicU64,
    gate: Mutex<HashMap<String, Arc<FillSignal>>>,
    enabled: bool,
    /// 单页上限：渲染结果更大的页面不缓存，每次请求完整 SSR 再现压。
    max_page_bytes: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FillOutcome {
    /// 已回填、被失效或被取消。等待者重新检查缓存，再决定是否渲染。
    Retry,
    /// 响应不能缓存。已等待的请求各自渲染，避免逐个排队。
    Bypass,
}

type FillSignal = watch::Sender<Option<FillOutcome>>;

enum CacheAccess<'a> {
    Hit(CachedPage),
    Fill(Fill<'a>),
    Wait(watch::Receiver<Option<FillOutcome>>),
}

struct Fill<'a> {
    cache: &'a HtmlCache,
    path: String,
    generation: u64,
    signal: Arc<FillSignal>,
    outcome: FillOutcome,
}

impl Fill<'_> {
    fn bypass(mut self) {
        self.outcome = FillOutcome::Bypass;
    }
}

impl Drop for Fill<'_> {
    fn drop(&mut self) {
        let mut fills = self.cache.gate.lock().unwrap_or_else(|e| e.into_inner());
        // 失效可能已唤醒旧等待者并允许同路径建立新回填。
        if fills
            .get(&self.path)
            .is_some_and(|active| Arc::ptr_eq(active, &self.signal))
        {
            fills.remove(&self.path);
            self.signal.send_replace(Some(self.outcome));
        }
    }
}

impl HtmlCache {
    fn new(ttl: Duration, capacity: u64, max_page_bytes: usize) -> Self {
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
            home_generation: AtomicU64::new(0),
            gate: Mutex::new(HashMap::new()),
            enabled: !ttl.is_zero(),
            max_page_bytes,
        }
    }

    fn generation(&self, path: &str) -> u64 {
        let global = self.generation.load(Ordering::Acquire);
        if path == HOME_PATH {
            global.wrapping_add(self.home_generation.load(Ordering::Acquire))
        } else {
            global
        }
    }

    fn invalidate_all(&self) {
        let mut fills = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.pages.invalidate_all();
        for (_, signal) in fills.drain() {
            signal.send_replace(Some(FillOutcome::Retry));
        }
    }

    fn invalidate_home(&self) {
        let mut fills = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        self.home_generation.fetch_add(1, Ordering::AcqRel);
        self.pages.invalidate(HOME_PATH);
        if let Some(signal) = fills.remove(HOME_PATH) {
            signal.send_replace(Some(FillOutcome::Retry));
        }
    }

    fn get(&self, path: &str) -> Option<CachedPage> {
        self.pages
            .get(path)
            .filter(|page| page.generation == self.generation(path))
    }

    fn access(&self, path: &str) -> CacheAccess<'_> {
        if let Some(page) = self.get(path) {
            return CacheAccess::Hit(page);
        }
        let mut fills = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(page) = self.get(path) {
            return CacheAccess::Hit(page);
        }
        if let Some(signal) = fills.get(path) {
            return CacheAccess::Wait(signal.subscribe());
        }
        let signal = Arc::new(watch::Sender::new(None));
        let generation = self.generation(path);
        fills.insert(path.to_owned(), signal.clone());
        CacheAccess::Fill(Fill {
            cache: self,
            path: path.to_owned(),
            generation,
            signal,
            outcome: FillOutcome::Retry,
        })
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
        if !self.enabled || body.len() > self.max_page_bytes {
            return None;
        }
        let etag = HeaderValue::from_str(&format!("\"{}\"", hex::encode(Sha256::digest(&body))))
            .expect("SHA256 is an ASCII entity tag");
        let _guard = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        if generation != self.generation(&path)
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
    headers.insert(header::CACHE_CONTROL, public_page_cache_control());
    response
}

static HTML_CACHE: LazyLock<HtmlCache> = LazyLock::new(|| {
    // 启动设置在路由装配前已写入 config；值随进程固定，改面板后重启生效。
    HtmlCache::new(
        Duration::from_secs(crate::utils::server::parse_ssr_cache_secs()),
        MAX_CACHE_BYTES,
        crate::config::ssr_cache().max_page_bytes as usize,
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
    CACHEABLE_PAGES.contains(&path)
        || CACHEABLE_PREFIXES.iter().any(|prefix| {
            path.strip_prefix(prefix)
                .is_some_and(|tail| !tail.is_empty() && !tail.contains('/'))
        })
}

/// 白名单内的固定页面。源站缓存与边缘缓存共用这份名单：`tasks::edge_purge` 按它清 EdgeOne，
/// 新增公开路由要同时出现在两处，否则边缘缓存清不掉。
pub(crate) const CACHEABLE_PAGES: [&str; 5] =
    [HOME_PATH, "/archives", "/friends", "/about", "/changelog"];
/// 白名单内的前缀：只匹配其下一级路径（`/post/<slug>`）。
pub(crate) const CACHEABLE_PREFIXES: [&str; 3] = ["/post/", "/page/", "/tags/"];

/// 白名单匿名页的 `Cache-Control`。浏览器缓存清不掉，所以浏览器一律每次校验（`max-age=0`）；
/// 边缘缓存时长（`SsrCacheSettings::edge_ttl_secs`）大于 0 时用 `s-maxage` 让 CDN 缓存，
/// 内容变更后由 `tasks::edge_purge` 清除。为 0 时 `no-cache`，EdgeOne 不缓存、每次回源。
fn public_page_cache_control() -> HeaderValue {
    static VALUE: LazyLock<HeaderValue> =
        LazyLock::new(|| edge_cache_control(crate::config::ssr_cache().edge_ttl_secs));
    VALUE.clone()
}

fn edge_cache_control(edge_ttl_secs: u64) -> HeaderValue {
    if edge_ttl_secs == 0 {
        return HeaderValue::from_static("public, no-cache");
    }
    HeaderValue::from_str(&format!("public, max-age=0, s-maxage={edge_ttl_secs}"))
        .expect("数字组成的 Cache-Control 是合法头部值")
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
    headers: HeaderMap,
    fill: Option<Fill<'static>>,
}

/// 在压缩层内侧缓存原始 HTML，保持 SSR 流式响应和现有静态资源预压缩行为。
pub async fn cache_public_html(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let authenticated = req.headers().contains_key(header::COOKIE)
        || req.headers().contains_key(header::AUTHORIZATION);
    let cacheable = cacheable_request(req.method(), req.uri(), req.headers());
    if !HTML_CACHE.enabled || !cacheable {
        let mut response = next.run(req).await;
        response
            .headers_mut()
            .insert("x-ssr-cache", HeaderValue::from_static("BYPASS"));
        // 源站缓存关闭（`SSR_CACHE_SECS=0`）时白名单页照样按边缘策略发头：边缘失效不依赖源站缓存。
        if cacheable && cacheable_response(&response) {
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, public_page_cache_control());
        }
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
    let mut deadline = None;
    let fill = loop {
        match HTML_CACHE.access(req.uri().path()) {
            CacheAccess::Hit(page) => {
                return hit_response(page, req.headers(), compression::algorithms());
            }
            CacheAccess::Fill(fill) => break Some(fill),
            CacheAccess::Wait(mut signal) => {
                let deadline =
                    *deadline.get_or_insert_with(|| tokio::time::Instant::now() + FILL_WAIT);
                let outcome =
                    tokio::time::timeout_at(deadline, signal.wait_for(Option::is_some)).await;
                if !matches!(outcome, Ok(Ok(ref value)) if **value == Some(FillOutcome::Retry)) {
                    break None;
                }
            }
        }
    };
    let mut response = next.run(req).await;
    if !cacheable_response(&response) {
        if let Some(fill) = fill {
            fill.bypass();
        }
        return response;
    }
    response
        .headers_mut()
        .insert("x-ssr-cache", HeaderValue::from_static("MISS"));
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, public_page_cache_control());
    let Some(fill) = fill else {
        return response;
    };
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
        headers,
        fill: Some(fill),
    };
    let stream = futures::stream::unfold(state, |mut state| async move {
        match state.input.next().await {
            Some(Ok(chunk)) => {
                if let Some(buffer) = state.buffer.as_mut() {
                    if buffer.len() + chunk.len() <= HTML_CACHE.max_page_bytes {
                        buffer.extend_from_slice(&chunk);
                    } else {
                        state.buffer = None;
                        if let Some(fill) = state.fill.take() {
                            fill.bypass();
                        }
                    }
                }
                Some((Ok::<_, axum::Error>(chunk), state))
            }
            Some(Err(error)) => {
                state.buffer = None;
                state.fill.take();
                Some((Err(error), state))
            }
            None => {
                if let (Some(buffer), Some(fill)) = (state.buffer.take(), state.fill.take()) {
                    if let Some((etag, body)) =
                        HTML_CACHE.insert(fill.path.clone(), state.headers, buffer, fill.generation)
                    {
                        precompress(fill.path.clone(), etag, fill.generation, body);
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
// 每次源站失效都同步请求清边缘缓存：`tasks::edge_purge` 防抖合并，未配置凭据时只是空信号。
pub fn invalidate_ssr_all_public() {
    HTML_CACHE.invalidate_all();
    crate::tasks::edge_purge::request_purge();
}

pub(crate) fn invalidate_home() {
    HTML_CACHE.invalidate_home();
    crate::tasks::edge_purge::request_home_purge();
}

pub(crate) fn current_generation(path: &str) -> u64 {
    HTML_CACHE.generation(path)
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
    fn edge_ttl_controls_shared_cache_only() {
        assert_eq!(edge_cache_control(0), "public, no-cache");
        // 浏览器每次校验（缓存清不掉），只有 CDN 按 s-maxage 缓存；不能带 no-cache，
        // 否则共享缓存也不存。
        assert_eq!(edge_cache_control(3600), "public, max-age=0, s-maxage=3600");
        // 配置未加载（测试与 dev 默认）时不让 CDN 缓存。
        assert_eq!(public_page_cache_control(), "public, no-cache");
    }

    #[test]
    fn invalidation_rejects_inflight_old_responses() {
        let cache = HtmlCache::new(Duration::from_secs(60), MAX_CACHE_BYTES, 1024);
        let generation = cache.generation("/");
        cache.insert("/".into(), HeaderMap::new(), b"old".to_vec(), generation);
        assert!(cache.get("/").is_some());
        cache.invalidate_all();
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
            cache.generation("/"),
        );
        assert_eq!(cache.get("/").unwrap().body.as_ref(), b"new");
    }

    #[test]
    fn home_invalidation_preserves_other_pages_and_discards_its_old_fill() {
        let cache = HtmlCache::new(Duration::from_secs(60), MAX_CACHE_BYTES, 1024);
        cache.insert("/post/a".into(), HeaderMap::new(), b"post".to_vec(), 0);
        let CacheAccess::Fill(old_fill) = cache.access("/") else {
            panic!("首页尚未回填");
        };
        let CacheAccess::Wait(waiting) = cache.access("/") else {
            panic!("同路径已有回填");
        };

        cache.invalidate_home();
        assert!(*waiting.borrow() == Some(FillOutcome::Retry));
        assert_eq!(cache.get("/post/a").unwrap().body.as_ref(), b"post");
        assert!(cache
            .insert(
                "/".into(),
                HeaderMap::new(),
                b"old".to_vec(),
                old_fill.generation,
            )
            .is_none());
        let CacheAccess::Fill(new_fill) = cache.access("/") else {
            panic!("首页可立即按新世代回填");
        };
        drop(old_fill);
        assert!(matches!(cache.access("/"), CacheAccess::Wait(_)));
        cache.insert(
            "/".into(),
            HeaderMap::new(),
            b"home".to_vec(),
            new_fill.generation,
        );
        drop(new_fill);
        assert!(matches!(cache.access("/"), CacheAccess::Hit(_)));
    }

    #[test]
    fn cancelled_and_uncacheable_fills_release_waiters() {
        let cache = HtmlCache::new(Duration::from_secs(60), MAX_CACHE_BYTES, 1024);
        for outcome in [FillOutcome::Retry, FillOutcome::Bypass] {
            let CacheAccess::Fill(fill) = cache.access("/post/a") else {
                panic!("取消或绕过后应允许新请求回填");
            };
            let CacheAccess::Wait(waiting) = cache.access("/post/a") else {
                panic!("同路径已有回填");
            };
            match outcome {
                FillOutcome::Retry => drop(fill),
                FillOutcome::Bypass => fill.bypass(),
            }
            assert!(*waiting.borrow() == Some(outcome));
        }
    }

    #[test]
    fn cached_pages_expire_and_oversize_pages_are_not_stored() {
        let cache = HtmlCache::new(Duration::from_millis(20), MAX_CACHE_BYTES, 1024);
        cache.insert("/".into(), HeaderMap::new(), b"html".to_vec(), 0);
        assert!(cache.get("/").is_some());
        std::thread::sleep(Duration::from_millis(40));
        assert!(cache.get("/").is_none());
        cache.insert("/limit".into(), HeaderMap::new(), vec![0; 1024], 0);
        assert!(cache.get("/limit").is_some());
        cache.insert("/large".into(), HeaderMap::new(), vec![0; 1025], 0);
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
        let cache = HtmlCache::new(Duration::from_secs(60), MAX_CACHE_BYTES, 1 << 20);
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
        let cache = HtmlCache::new(Duration::from_secs(60), MAX_CACHE_BYTES, 1024);
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
        cache.invalidate_all();
        let generation = cache.generation("/");
        let (etag, _) = cache
            .insert("/".into(), HeaderMap::new(), b"new".to_vec(), generation)
            .unwrap();
        cache.invalidate_all();
        cache.attach_encoded("/", &etag, generation, variant());
        assert!(cache.get("/").is_none());
    }
}
