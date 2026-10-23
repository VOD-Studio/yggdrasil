//! Pagination.
use super::controls::{ButtonSize, IconButton};
use super::icons::{Icon, IconName};
use leptos::prelude::*;

/// 前台列表分页：上一页 / 下一页描边按钮与居中的「当前 / 总」页码，首尾页不渲染越界按钮。
///
/// Props：
/// - `current_page`：当前页码（从 1 开始）
/// - `total`：数据总条数
/// - `per_page`：每页条数，用于计算总页数
/// - `prev_route`：上一页的目标地址（站内路径，由路由拦截做客户端导航）
/// - `next_route`：下一页的目标地址
#[component]
pub fn Pagination(
    current_page: i32,
    total: i64,
    per_page: i32,
    #[prop(into)] prev_route: String,
    #[prop(into)] next_route: String,
) -> impl IntoView {
    let has_prev = current_page > 1;
    let total_pages = ((total + per_page as i64 - 1) / per_page as i64).max(1) as i32;
    let has_next = current_page < total_pages;

    view! {
        <nav class="frontend-pagination" aria-label="分页导航">
            {has_prev
                .then(|| {
                    view! {
                        <a class="btn btn-sm btn-outline" href=prev_route>
                            <Icon name=IconName::ChevronLeft size=15/>
                            "上一页"
                        </a>
                    }
                })}
            <span
                class="col-start-2 row-start-1 text-xs text-fg-3 whitespace-nowrap tabular-nums"
                aria-label=format!("第 {current_page} 页，共 {total_pages} 页")
                aria-current="page"
            >
                {format!("{current_page} / {total_pages}")}
            </span>
            {has_next
                .then(|| {
                    view! {
                        <a
                            class="btn btn-sm btn-outline col-start-3 row-start-1 justify-self-end"
                            href=next_route
                        >
                            "下一页"
                            <Icon name=IconName::ChevronRight size=15/>
                        </a>
                    }
                })}
        </nav>
    }
}

/// 表格底栏分页：左侧「第 21–40 篇，共 57 篇」，右侧上一页、页码与下一页图标按钮。
/// 只有一页时只显示总数。
///
/// Props：
/// - `page`：当前页（从 1 开始）；`total`：总条数；`per_page`：每页条数
/// - `unit`：计数单位，如「篇」「条」
/// - `on_change`：翻到的页码
#[component]
pub fn TablePager(
    #[prop(into)] page: Signal<i32>,
    #[prop(into)] total: Signal<i64>,
    per_page: i32,
    unit: &'static str,
    #[prop(into)] on_change: Callback<i32>,
) -> impl IntoView {
    let pages =
        Memo::new(move |_| ((total.get() + per_page as i64 - 1) / per_page as i64).max(1) as i32);
    let page = Memo::new(move |_| page.get().clamp(1, pages.get()));
    let range = move || {
        let total = total.get();
        let pages = pages.get();
        let page = page.get();
        let first = (page as i64 - 1) * per_page as i64 + 1;
        let last = (page as i64 * per_page as i64).min(total);
        if total == 0 {
            format!("共 0 {unit}")
        } else if pages == 1 {
            format!("共 {total} {unit}")
        } else {
            format!("第 {first}–{last} {unit}，共 {total} {unit}")
        }
    };
    view! {
        <nav class="table-pager" aria-label="分页导航">
            <span class="table-pager-range">{range}</span>
            <Show when=move || { pages.get() > 1 }>
                <div class="table-pager-controls">
                    <IconButton
                        icon=IconName::ChevronLeft
                        label="上一页"
                        size=ButtonSize::Sm
                        disabled=Signal::derive(move || page.get() <= 1)
                        onclick=Callback::new(move |_| on_change.run(page.get_untracked() - 1))
                    />
                    <span class="table-pager-page" aria-current="page">
                        {move || format!("{} / {}", page.get(), pages.get())}
                    </span>
                    <IconButton
                        icon=IconName::ChevronRight
                        label="下一页"
                        size=ButtonSize::Sm
                        tooltip_align="end"
                        disabled=Signal::derive(move || page.get() >= pages.get())
                        onclick=Callback::new(move |_| on_change.run(page.get_untracked() + 1))
                    />
                </div>
            </Show>
        </nav>
    }
}
