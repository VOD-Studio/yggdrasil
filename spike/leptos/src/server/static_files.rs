//! 静态资源链：当前发布 site-root → 上一发布 site-root（仅 /pkg/）→ 主仓库 public/。
//!
//! 对应主 crate 的 `YGG_PREVIOUS_PUBLIC_DIR`：滚动发布后，旧标签页按需拉取旧
//! 发布的分包 WASM 仍要命中。这是代码分割引入的新约束：分包是运行期懒加载的，
//! 旧主包只认识自己那一版的分包。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Method, Request, Response, StatusCode, Uri};
use tower::util::ServiceExt;
use tower_http::services::ServeDir;

fn env_dir(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// cargo-leptos `hash-files` 产物形如 `name.<22 位 base64url 哈希>.ext`，
/// 只有带哈希的文件允许 immutable，未带哈希的入口只能 no-cache。
fn cache_control(path: &str) -> &'static str {
    let file = path.rsplit('/').next().unwrap_or(path);
    let hashed = file.split('.').any(|part| {
        part.len() == 22
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    });
    if path.starts_with("/pkg/") && hashed {
        "public, max-age=31536000, immutable"
    } else {
        "public, no-cache"
    }
}

async fn try_dir(
    dir: &Path,
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
) -> Option<Response<Body>> {
    let mut probe = Request::builder()
        .method(method.clone())
        .uri(uri.clone())
        .body(Body::empty())
        .ok()?;
    *probe.headers_mut() = headers.clone();
    let resp = ServeDir::new(dir)
        .precompressed_br()
        .oneshot(probe)
        .await
        .ok()?;
    if resp.status() == StatusCode::NOT_FOUND {
        return None;
    }
    Some(resp.map(Body::new))
}

/// 依次尝试三个目录；全部未命中返回 `None`，由调用方交给 Leptos 渲染 404。
///
/// 只借用请求头部（`Body` 非 `Sync`，借用整个请求跨 await 会让 handler 失去 `Send`）。
pub async fn serve(
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    site_root: &Path,
) -> Option<Response<Body>> {
    let path = uri.path();
    let mut dirs = vec![site_root.to_path_buf()];
    if path.starts_with("/pkg/") {
        if let Some(prev) = env_dir("SPIKE_PREVIOUS_SITE_ROOT") {
            dirs.push(prev);
        }
    } else if path.starts_with("/uploads/") {
        // 文章封面与正文图片：与主应用共用 uploads 目录（ServeDir 以其父目录为根）。
        let uploads =
            env_dir("SPIKE_UPLOADS_DIR").unwrap_or_else(|| PathBuf::from("../../uploads"));
        if let Some(parent) = uploads.parent() {
            dirs.push(parent.to_path_buf());
        }
    } else {
        dirs.push(env_dir("SPIKE_PUBLIC_DIR").unwrap_or_else(|| PathBuf::from("../../public")));
    }
    for (index, dir) in dirs.iter().enumerate() {
        if let Some(mut resp) = try_dir(dir, method, uri, headers).await {
            let out = resp.headers_mut();
            out.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static(cache_control(path)),
            );
            if index > 0 && path.starts_with("/pkg/") {
                out.insert("x-spike-asset-source", HeaderValue::from_static("previous"));
            }
            return Some(resp);
        }
    }
    None
}
