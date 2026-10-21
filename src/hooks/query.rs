//! 通用客户端数据加载 hook。
//!
//! 封装 `use_effect + spawn + 多个 signal` 的三件套，统一 loading / data / error
//! 三态与非 wasm cfg 处理。后台 CSR 页面用这套；公共 SSR 页面仍用
//! `use_server_future`（带 suspend 的真 SSR 预取），二者职责区分明确。
//!
//! 当前只提供 `use_paginated`（分页列表）。system tab 的"load_once + use_future
//! 轮询"形态、dashboard 的多源聚合形态保持手写——强行套 hook 反而更复杂。

use dioxus::prelude::*;

/// 分页列表加载状态。
///
/// 各字段是独立 `Signal`，调用方可按名读取、传给子组件，或对 `items` 做
/// `with_mut` 乐观更新。
pub struct PaginatedState<T> {
    /// 是否正在加载。
    pub loading: Signal<bool>,
    /// 当前页数据。
    pub items: Signal<Vec<T>>,
    /// 总条数（用于分页器）。
    pub total: Signal<i64>,
    /// 最近一次加载错误（成功时为 None）。
    pub error: Signal<Option<String>>,
}

/// 分页列表加载 hook。
///
/// - `page`：返回当前页码的信号读取闭包（返回值变化时自动重新加载）。
/// - `per_page`：每页条数。
/// - `fetch`：接收 `(page, per_page)`，返回 `Future<Output = Result<(Vec<T>, i64), E>>`。
///   `E: Display` 用于把错误写入 `error` signal。
///
/// 非 wasm32 下 `loading` 立即置 false（SSR 不发起请求）。
///
/// # 例
///
/// ```ignore
/// let state = use_paginated(
///     || current_page,
///     POSTS_PER_PAGE,
///     |p, pp| async move {
///         list_posts(p, pp, AdminPostQuery::default()).await
///             .map(|r| (r.posts, r.total))
///             .map_err(|e| e.to_string())
///     },
/// );
/// ```
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
#[allow(unused_variables)]
pub fn use_paginated<T, P, F, Fut, E>(page: P, per_page: i32, fetch: F) -> PaginatedState<T>
where
    T: PartialEq + Clone + 'static,
    P: Fn() -> i32 + Copy + 'static,
    F: Fn(i32, i32) -> Fut + Copy + 'static,
    Fut: std::future::Future<Output = Result<(Vec<T>, i64), E>> + 'static,
    E: std::fmt::Display + 'static,
{
    let mut loading = use_signal(|| true);
    let mut items = use_signal(Vec::new);
    let mut total = use_signal(|| 0_i64);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    use_effect(move || {
        let p = page();
        loading.set(true);
        error.set(None);
        #[cfg(target_arch = "wasm32")]
        {
            let f = fetch;
            spawn(async move {
                match f(p, per_page).await {
                    Ok((list, t)) => {
                        items.set(list);
                        total.set(t);
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
                loading.set(false);
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            loading.set(false);
            // 抚平非 wasm 下未使用的变量（p / per_page / fetch）。
            let _ = (p, per_page, fetch);
        }
    });

    PaginatedState {
        loading,
        items,
        total,
        error,
    }
}
