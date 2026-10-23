//! Pagination, tabs and navigable tag chips.
use super::controls::{ButtonSize, IconButton};
use super::icons::{Icon, IconName};
use leptos::ev::MouseEvent;
use leptos::prelude::*;

/// 前台列表分页：上一页 / 下一页描边按钮与居中的「当前 / 总」页码，首尾页不渲染越界按钮。
///
/// Props：
/// - `current_page`：当前页码（从 1 开始）
/// - `total`：数据总条数
/// - `per_page`：每页条数，用于计算总页数
/// - `prev_route`：上一页的目标地址（站内路径，如 `Route::HomePage { page: 1 }.to_string()`；
///   是普通 `<a href>`，由路由协调器做客户端导航）
/// - `next_route`：下一页的目标地址
///
/// `current_page` / `total` / `prev_route` / `next_route` 既可传值也可传信号或闭包。
#[component]
pub fn Pagination(
    #[prop(into)] current_page: Signal<i32>,
    #[prop(into)] total: Signal<i64>,
    per_page: i32,
    #[prop(into)] prev_route: Signal<String>,
    #[prop(into)] next_route: Signal<String>,
) -> impl IntoView {
    let total_pages =
        Memo::new(move |_| ((total.get() + per_page as i64 - 1) / per_page as i64).max(1) as i32);
    let has_prev = move || current_page.get() > 1;
    let has_next = move || current_page.get() < total_pages.get();

    view! {
        <nav class="frontend-pagination" aria-label="分页导航">
            {move || {
                has_prev()
                    .then(|| {
                        view! {
                            <a class="btn btn-sm btn-outline" href=prev_route.get()>
                                <Icon name=IconName::ChevronLeft size=15/>
                                "上一页"
                            </a>
                        }
                    })
            }}
            <span
                class="col-start-2 row-start-1 text-xs text-fg-3 whitespace-nowrap tabular-nums"
                aria-label=move || format!("第 {} 页，共 {} 页", current_page.get(), total_pages.get())
                aria-current="page"
            >
                {move || format!("{} / {}", current_page.get(), total_pages.get())}
            </span>
            {move || {
                has_next()
                    .then(|| {
                        view! {
                            <a
                                class="btn btn-sm btn-outline col-start-3 row-start-1 justify-self-end"
                                href=next_route.get()
                            >
                                "下一页"
                                <Icon name=IconName::ChevronRight size=15/>
                            </a>
                        }
                    })
            }}
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
    #[prop(into)] unit: String,
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

/// 面包屑中的一级。最后一级是当前页（不可点击，标 `aria-current`）；中间级 `to` 为
/// `None` 时是不可点击的分组名（如后台的「内容」）。
///
/// `to` 是站内路径（`<a href>`），原先泛型的 `Route` 由调用方 `.to_string()` 转成地址；
/// [`Crumb::link`] 直接接受任何 `ToString`（含 `Route`）。
#[derive(Clone, Debug, PartialEq)]
pub struct Crumb {
    pub label: String,
    pub to: Option<String>,
}

impl Crumb {
    pub fn link(label: impl Into<String>, to: impl ToString) -> Self {
        Self {
            label: label.into(),
            to: Some(to.to_string()),
        }
    }

    pub fn current(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            to: None,
        }
    }

    /// 不可点击的中间级（分组名）。
    pub fn text(label: impl Into<String>) -> Self {
        Self::current(label)
    }
}

/// 面包屑：`关于 / 组件图鉴 / 复选框`，最后一级为当前页并在过长时截断。
///
/// `items` 既可传 `Vec<Crumb>`，也可传信号或闭包。
#[component]
pub fn Breadcrumb(#[prop(into)] items: Signal<Vec<Crumb>>) -> impl IntoView {
    view! {
        <nav class="breadcrumb" aria-label="面包屑">
            {move || {
                let items = items.get();
                let last = items.len().saturating_sub(1);
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, crumb)| {
                        let separator = (i > 0).then(|| view! { <span aria-hidden="true">"/"</span> });
                        let node = match crumb.to {
                            Some(to) if i < last => {
                                view! { <a href=to>{crumb.label}</a> }.into_any()
                            }
                            None if i < last => view! { <span>{crumb.label}</span> }.into_any(),
                            _ => {
                                view! { <span aria-current="page">{crumb.label}</span> }.into_any()
                            }
                        };
                        view! {
                            {separator}
                            {node}
                        }
                    })
                    .collect_view()
            }}
        </nav>
    }
}

/// 标签芯片组件：统一软底标签、卡片胶囊与文字链接的展示。
///
/// 各处跳转到 `Route::TagDetail` 的可点击标签共用此组件，仅以 `variant` 区分视觉。
///
/// Props：
/// - `label`：标签名
/// - `to`：跳转目标地址（站内路径，调用方传 `Route::TagDetail { .. }.to_string()`，原子层不绑定 app 路由类型）
/// - `variant`：`"solid"`（软底）/ `"outline"`（卡片胶囊，默认）/ `"text"`（紧凑文章流）
/// - `count`：可选的文章计数
/// - `stop_propagation`：是否阻止点击冒泡（卡片内覆盖层链接场景需要）。注意：冒泡被拦截后，
///   文档级的路由协调器收不到这次点击，链接会退化为整页跳转。
#[component]
pub fn TagChip(
    #[prop(into)] label: String,
    #[prop(into)] to: String,
    #[prop(default = "outline".to_string(), into)] variant: String,
    #[prop(optional)] count: Option<i64>,
    #[prop(optional)] stop_propagation: bool,
) -> impl IntoView {
    let class = match variant.as_str() {
        "solid" => "chip chip-clay",
        "text" => "inline-flex items-center py-1 text-fg-2 hover:text-fg underline decoration-hairline underline-offset-4 hover:decoration-clay transition-colors focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-clay",
        _ => "chip",
    };
    view! {
        <a
            class=class
            href=to
            on:click=move |evt: MouseEvent| {
                if stop_propagation {
                    evt.stop_propagation();
                }
            }
        >
            {label}
            {count.map(|c| view! { <span class="chip-count">{c}</span> })}
        </a>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render<V: IntoView>(build: impl FnOnce() -> V) -> String {
        Owner::new().with(|| build().to_html())
    }

    #[test]
    fn pagination_hides_out_of_range_buttons() {
        let first = render(|| {
            view! {
                <Pagination current_page=1 total=45i64 per_page=20 prev_route="/page/0" next_route="/page/2"/>
            }
        });
        assert!(first.contains("href=\"/page/2\""), "{first}");
        assert!(!first.contains("上一页"), "{first}");
        assert!(first.contains("1 / 3"), "{first}");
        let last = render(|| {
            view! {
                <Pagination current_page=3 total=45i64 per_page=20 prev_route="/page/2" next_route="/page/4"/>
            }
        });
        assert!(last.contains("href=\"/page/2\""), "{last}");
        assert!(!last.contains("下一页"), "{last}");
    }

    #[test]
    fn table_pager_shows_range_and_controls_only_for_many_pages() {
        let many = render(|| {
            view! {
                <TablePager page=2 total=57i64 per_page=20 unit="篇" on_change=|_| {}/>
            }
        });
        assert!(many.contains("第 21–40 篇，共 57 篇"), "{many}");
        assert!(many.contains("table-pager-controls"), "{many}");
        let one = render(|| {
            view! {
                <TablePager page=1 total=3i64 per_page=20 unit="篇" on_change=|_| {}/>
            }
        });
        assert!(one.contains("共 3 篇"), "{one}");
        assert!(!one.contains("table-pager-controls"), "{one}");
    }

    #[test]
    fn breadcrumb_marks_last_crumb_as_current_page() {
        let html = render(|| {
            view! {
                <Breadcrumb items=vec![
                    Crumb::link("关于", "/about"),
                    Crumb::text("分组"),
                    Crumb::current("复选框"),
                ]/>
            }
        });
        assert!(html.contains("href=\"/about\""), "{html}");
        assert!(html.contains("aria-current=\"page\""), "{html}");
        assert_eq!(html.matches("aria-hidden=\"true\"").count(), 2, "{html}");
    }

    #[test]
    fn tag_chip_renders_variant_class_and_count() {
        let html = render(
            || view! { <TagChip label="设计" to="/tags/设计" variant="solid" count=12i64/> },
        );
        assert!(html.contains("chip chip-clay"), "{html}");
        assert!(html.contains("chip-count"), "{html}");
    }
}
