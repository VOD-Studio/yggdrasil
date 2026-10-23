//! 通用客户端数据加载 hook。
//!
//! 封装 `Effect + spawn_local + 多个 RwSignal` 的三件套，统一 loading / data / error
//! 三态与请求世代号竞态保护。后台 CSR 页面用这套；公共 SSR 页面仍用路由
//! `LazyRoute::data()` 里创建的 `Resource`（带 Suspense 的真 SSR 预取），二者职责区分明确。
//!
//! 当前只提供 `use_paginated`（分页列表）。system tab 的"load_once + 定时轮询"形态、
//! dashboard 的多源聚合形态保持手写——强行套 hook 反而更复杂。

use leptos::prelude::*;

/// 分页列表加载状态。
///
/// 各字段是独立 `RwSignal`（`Copy`），调用方可按名读取、传给子组件，
/// 或对 `items` 做 `update` 乐观更新。
pub struct PaginatedState<T: Send + Sync + 'static> {
    /// 是否正在加载（初始为 `true`；服务端不发请求，保持 `true`，与水合首帧一致）。
    pub loading: RwSignal<bool>,
    /// 当前页数据。
    pub items: RwSignal<Vec<T>>,
    /// 总条数（用于分页器）。
    pub total: RwSignal<i64>,
    /// 最近一次加载错误（成功时为 None）。
    pub error: RwSignal<Option<String>>,
}

// 手写而非 derive：derive 会给 `T` 加上多余的 `Clone`/`Copy` 约束。
impl<T: Send + Sync + 'static> Clone for PaginatedState<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for PaginatedState<T> {}

/// 分页列表加载 hook。
///
/// - `page`：返回当前页码的闭包，在 Effect 内调用，其读取的信号被追踪
///   （变化时自动重新加载）。
/// - `per_page`：每页条数。
/// - `fetch`：接收 `(page, per_page)`，返回 `Future<Output = Result<(Vec<T>, i64), E>>`。
///   `E: Display` 用于把错误写入 `error` 信号；`ServerFnError` 请先经
///   `crate::utils::error::failure_message` 映射成可读文案（UI 不得直接展示
///   `err.to_string()`）。`fetch` 在 Effect 内**同步**调用（返回 Future 之前的部分），
///   因此构造请求参数时读取的信号（筛选条件等）同样被追踪；`async` 块内的读取不追踪。
///   `fetch` 与 `page` 都要求 `Send + Sync + 'static`，捕获的信号都是 `Copy` 句柄，
///   满足该要求；Future 本身在浏览器本地执行，无需 `Send`。
///
/// 仅在浏览器（Effect 运行时）发起请求，服务端不请求；快速翻页/切筛选时，
/// 只有最后一次请求的响应能落地。
///
/// # 例
///
/// ```ignore
/// let state = use_paginated(
///     move || current_page.get(),
///     POSTS_PER_PAGE,
///     move |p, pp| {
///         let query = build_query(); // 同步读信号：被追踪
///         async move {
///             list_posts(p, pp, query).await
///                 .map(|r| (r.posts, r.total))
///                 .map_err(|e| failure_message("加载文章", &e))
///         }
///     },
/// );
/// let (posts, loading) = (state.items, state.loading);
/// ```
pub fn use_paginated<T, P, F, Fut, E>(page: P, per_page: i32, fetch: F) -> PaginatedState<T>
where
    T: Clone + Send + Sync + 'static,
    P: Fn() -> i32 + Send + Sync + 'static,
    F: Fn(i32, i32) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<(Vec<T>, i64), E>> + 'static,
    E: std::fmt::Display + 'static,
{
    let loading = RwSignal::new(true);
    let items = RwSignal::new(Vec::new());
    let total = RwSignal::new(0_i64);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    // 请求世代号：快速翻页/切筛选时，只有最后一次请求的响应能落地。
    let generation = StoredValue::new(0_u64);

    Effect::new(move |_| {
        let p = page();
        let future = fetch(p, per_page);
        let Some(id) = generation.try_update_value(|g| {
            *g = g.wrapping_add(1);
            *g
        }) else {
            return;
        };
        loading.set(true);
        error.set(None);
        leptos::task::spawn_local(async move {
            let result = future.await;
            // 过期响应（或组件已卸载）直接丢弃，不写任何信号（含 loading）。
            if generation.try_get_value() != Some(id) {
                return;
            }
            match result {
                Ok((list, t)) => {
                    items.set(list);
                    total.set(t);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    });

    PaginatedState {
        loading,
        items,
        total,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use leptos::reactive::owner::Owner;

    fn assert_copy<S: Copy>() {}

    #[test]
    fn state_is_copy_even_for_non_copy_items() {
        assert_copy::<PaginatedState<String>>();
    }

    #[test]
    fn server_side_keeps_initial_loading_state_without_fetching() {
        Owner::new().with(|| {
            let state = use_paginated(
                || 1,
                20,
                |_, _| async {
                    panic!("服务端不应发起请求") as Result<(Vec<String>, i64), String>
                },
            );
            assert!(state.loading.get_untracked());
            assert!(state.items.get_untracked().is_empty());
            assert_eq!(state.total.get_untracked(), 0);
            assert!(state.error.get_untracked().is_none());
        });
    }
}
