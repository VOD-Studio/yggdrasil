//! 请求上下文访问：server fn 与服务端渲染期间读取当前请求、写入响应。
//!
//! `leptos_axum` 在处理 server fn 与渲染页面时，把 [`http::request::Parts`] 和
//! [`leptos_axum::ResponseOptions`] 放进响应式上下文。本模块封装对它们的读取，
//! 业务代码不直接依赖 `leptos_axum`。无请求上下文（后台任务、MCP 工具、单元测试）
//! 时各函数返回 `None` 或空操作。

#[cfg(all(feature = "server", test))]
use std::future::Future;

/// 当前请求的 [`http::request::Parts`]（含请求头与扩展）。
#[cfg(feature = "server")]
pub fn request_parts() -> Option<http::request::Parts> {
    leptos::prelude::use_context::<http::request::Parts>()
}

/// 当前请求的请求头副本。
#[cfg(feature = "server")]
pub fn request_headers() -> Option<http::HeaderMap> {
    request_parts().map(|parts| parts.headers)
}

/// 读取中间件放进请求扩展的值（例如 `ShareContext`）。
#[cfg(feature = "server")]
pub fn request_extension<T: Clone + Send + Sync + 'static>() -> Option<T> {
    request_parts().and_then(|parts| parts.extensions.get::<T>().cloned())
}

/// 追加一个响应头（例如登录时的 `Set-Cookie`）；无响应上下文时忽略。
#[cfg(feature = "server")]
pub fn append_response_header(name: http::HeaderName, value: http::HeaderValue) {
    if let Some(options) = leptos::prelude::use_context::<leptos_axum::ResponseOptions>() {
        options.append_header(name, value);
    }
}

/// 设置响应状态码。服务端写入 `ResponseOptions`，浏览器端空操作。
///
/// 只在首个响应块发出前有效：leptos 取到首块后才写入状态与响应头。`SsrMode::Async` 等全部数据后
/// 才出首块；流式路由里，首个未就绪的 Suspense 之前同步渲染的组件仍可改状态。
pub fn set_status(code: u16) {
    #[cfg(feature = "server")]
    if let Some(options) = leptos::prelude::use_context::<leptos_axum::ResponseOptions>() {
        if let Ok(status) = http::StatusCode::from_u16(code) {
            options.set_status(status);
        }
    }
    #[cfg(not(feature = "server"))]
    let _ = code;
}

/// 测试辅助：在带有请求上下文的作用域内执行异步代码，等价于 server fn 被调用时的环境。
#[cfg(all(feature = "server", test))]
pub async fn scope_with_request<F: Future>(parts: http::request::Parts, fut: F) -> F::Output {
    use leptos::prelude::{provide_context, Owner};

    let owner = Owner::new();
    owner
        .with(|| {
            provide_context(parts);
            provide_context(leptos_axum::ResponseOptions::default());
            leptos::reactive::computed::ScopedFuture::new(fut)
        })
        .await
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    fn parts() -> http::request::Parts {
        let mut request = http::Request::builder()
            .header("cookie", "session=abc")
            .body(())
            .unwrap();
        request.extensions_mut().insert(7_u32);
        request.into_parts().0
    }

    #[tokio::test]
    async fn scope_exposes_request_headers_and_extensions() {
        let (cookie, extension, status_ok) = scope_with_request(parts(), async {
            set_status(404);
            let cookie = request_headers()
                .and_then(|headers| headers.get("cookie").cloned())
                .map(|value| value.to_str().unwrap().to_string());
            let status = leptos::prelude::use_context::<leptos_axum::ResponseOptions>()
                .map(|options| options.0.read().unwrap().status);
            (
                cookie,
                request_extension::<u32>(),
                status == Some(Some(http::StatusCode::NOT_FOUND)),
            )
        })
        .await;
        assert_eq!(cookie.as_deref(), Some("session=abc"));
        assert_eq!(extension, Some(7));
        assert!(status_ok);
    }

    #[tokio::test]
    async fn outside_a_request_everything_is_empty() {
        assert!(request_parts().is_none());
        assert!(request_headers().is_none());
        assert_eq!(request_extension::<u32>(), None);
        set_status(500);
    }
}
