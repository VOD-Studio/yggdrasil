//! 主 crate `src/ssr_cache.rs` 的原样移植：白名单 + TTL + 世代号的匿名 HTML 缓存。
//!
//! 验证点：Leptos 的流式 SSR 响应能否被完整捕获、回放后仍能正确水合。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, BodyDataStream};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::Response;
use bytes::Bytes;
use futures::StreamExt;
use sha2::{Digest, Sha256};

const MAX_PAGE_BYTES: usize = 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone)]
struct CachedPage {
    body: Bytes,
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
}

impl HtmlCache {
    fn new(ttl: Duration, capacity: u64) -> Self {
        Self {
            pages: moka::sync::Cache::builder()
                .max_capacity(capacity)
                .weigher(|key: &String, page: &CachedPage| {
                    (key.len() + page.body.len() + 1024) as u32
                })
                .time_to_live(ttl)
                .build(),
            generation: AtomicU64::new(0),
            gate: Mutex::new(()),
            enabled: !ttl.is_zero(),
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

    fn get(&self, path: &str) -> Option<CachedPage> {
        self.pages
            .get(path)
            .filter(|page| page.generation == self.generation())
    }

    fn insert(&self, path: String, headers: HeaderMap, body: Vec<u8>, generation: u64) {
        if !self.enabled || body.len() > MAX_PAGE_BYTES {
            return;
        }
        let etag = HeaderValue::from_str(&format!("\"{}\"", hex::encode(Sha256::digest(&body))))
            .expect("SHA256 is an ASCII entity tag");
        let _guard = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        if generation != self.generation() {
            return;
        }
        self.pages.insert(
            path,
            CachedPage {
                body: body.into(),
                headers,
                etag,
                generation,
                created: Instant::now(),
            },
        );
    }
}

static HTML_CACHE: LazyLock<HtmlCache> = LazyLock::new(|| {
    let secs = std::env::var("SSR_CACHE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    HtmlCache::new(Duration::from_secs(secs), MAX_CACHE_BYTES)
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
    path == "/"
        || ["/post/", "/page/"].iter().any(|prefix| {
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
        let unchanged = req
            .headers()
            .get(header::IF_NONE_MATCH)
            .and_then(|h| h.to_str().ok())
            .is_some_and(|value| {
                value.split(',').any(|tag| {
                    let tag = tag.trim().strip_prefix("W/").unwrap_or(tag.trim());
                    tag == "*" || tag == page.etag.to_str().unwrap_or("")
                })
            });
        let mut response = Response::new(if unchanged {
            Body::empty()
        } else {
            Body::from(page.body)
        });
        *response.headers_mut() = page.headers;
        if unchanged {
            *response.status_mut() = StatusCode::NOT_MODIFIED;
        }
        let headers = response.headers_mut();
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
        return response;
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
                    HTML_CACHE.insert(state.path, state.headers, buffer, state.generation);
                }
                None
            }
        }
    })
    .fuse();
    Response::from_parts(parts, Body::from_stream(stream))
}

/// 评论审核等写操作后调用，与主 crate `invalidate_ssr_all_public` 等价。
pub fn invalidate_all() {
    HTML_CACHE.invalidate();
}
