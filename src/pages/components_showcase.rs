//! 项目自写组件的公开图鉴与独立用法页。
//!
//! 目录、文案和属性说明来自本模块的静态数据；预览直接渲染项目里的组件。
#[cfg(feature = "server")]
use std::sync::LazyLock;
#[cfg(not(feature = "server"))]
use std::sync::OnceLock;

use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;
use leptos_router::{lazy_route, LazyRoute};
use serde::{Deserialize, Serialize};

use crate::api::error::ApiError;
use crate::components::forms::FormInput;
use crate::components::ui::{
    button_class, Button, ButtonSize, ButtonVariant, Icon, IconName, SegmentedControl,
    SegmentedItem,
};
use crate::context::ShowcaseFilters;
use crate::hooks::ssr_resource::{route_data, PageJson, RouteData};
use crate::pages::not_found::PageLoadFailed;
use crate::routes::Route;

mod browser_previews;
mod business_previews;
mod comment_previews;
mod layout_article_previews;
mod live_previews;

// 供各预览子模块通过 `super::sample_post` 共享示例文章，主文件自身不使用。
#[allow(unused_imports)]
use live_previews::sample_post;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ShowcaseData {
    groups: Vec<[String; 2]>,
    components: Vec<ComponentSpec>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ComponentSpec {
    name: String,
    slug: String,
    label: String,
    group: String,
    summary: String,
    states: String,
    description: String,
    props: Vec<[String; 4]>,
    keys: Vec<[String; 2]>,
    source: String,
    code: String,
    preview_mode: PreviewMode,
    usage_kind: UsageKind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PreviewMode {
    Interactive,
    Static,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum UsageKind {
    Rust,
    Api,
    Router,
    Browser,
}

// 图鉴数据只嵌入服务端二进制；浏览器从水合数据或 server function 取得，不随 WASM 下发。
#[cfg(feature = "server")]
static SHOWCASE: LazyLock<ShowcaseData> = LazyLock::new(|| {
    serde_json::from_str(include_str!("components_showcase_data.json"))
        .expect("组件图鉴数据必须是有效 JSON")
});

#[cfg(not(feature = "server"))]
static CLIENT_SHOWCASE: OnceLock<ShowcaseData> = OnceLock::new();

/// 当前图鉴数据。
#[cfg(feature = "server")]
fn showcase() -> &'static ShowcaseData {
    &SHOWCASE
}

/// 当前图鉴数据。客户端须先经 `ShowcaseDataGate` 加载。
#[cfg(not(feature = "server"))]
fn showcase() -> &'static ShowcaseData {
    CLIENT_SHOWCASE
        .get()
        .expect("组件图鉴数据须先由 ShowcaseDataGate 加载")
}

#[server(endpoint = "get_showcase_data", input = leptos::server_fn::codec::Json)]
pub(crate) async fn get_showcase_data() -> Result<ShowcaseData, ApiError> {
    Ok(SHOWCASE.clone())
}

/// 客户端会话内只请求一次；首屏直接复用 SSR 水合数据。取数函数随分包加载，
/// 见 `hooks::ssr_resource`。
#[leptos::lazy]
async fn fetch_showcase() -> PageJson<ShowcaseData> {
    #[cfg(not(feature = "server"))]
    if let Some(data) = CLIENT_SHOWCASE.get() {
        return PageJson::new(Ok(data.clone()));
    }
    PageJson::new(get_showcase_data().await)
}

/// 加载图鉴数据后再渲染子页面，使目录、详情与预览可同步读取 `showcase()`。
#[component]
fn ShowcaseDataGate(data: RouteData<ShowcaseData>, children: ChildrenFn) -> impl IntoView {
    view! {
        <Suspense fallback=|| ()>
            {move || {
                let children = children.clone();
                Suspend::new(async move {
                    match data.await {
                        Ok(_data) => {
                            #[cfg(not(feature = "server"))]
                            if CLIENT_SHOWCASE.get().is_none() {
                                let _ = CLIENT_SHOWCASE.set(_data);
                            }
                            children().into_any()
                        }
                        Err(_) => view! { <PageLoadFailed/> }.into_any(),
                    }
                })
            }}
        </Suspense>
    }
}

/// 组件图鉴目录路由，对应 `/about/components`。
pub struct ShowcaseRoute {
    data: RouteData<ShowcaseData>,
}

#[lazy_route]
impl LazyRoute for ShowcaseRoute {
    fn data() -> Self {
        crate::static_assets::ensure_admin_css();
        Self {
            data: route_data(|| (), |_| fetch_showcase()),
        }
    }

    fn view(this: Self) -> AnyView {
        view! {
            <ShowcaseDataGate data=this.data>
                <ShowcaseGallery/>
            </ShowcaseDataGate>
        }
        .into_any()
    }
}

/// 单枚组件详情路由，对应 `/about/components/:component`。
pub struct ShowcaseDetailRoute {
    data: RouteData<ShowcaseData>,
}

#[lazy_route]
impl LazyRoute for ShowcaseDetailRoute {
    fn data() -> Self {
        crate::static_assets::ensure_admin_css();
        Self {
            data: route_data(|| (), |_| fetch_showcase()),
        }
    }

    fn view(this: Self) -> AnyView {
        let params = use_params_map();
        view! {
            <ShowcaseDataGate data=this.data>
                // 按组件重建详情，切换相邻组件时预览状态、复制提示与目录高亮都回到初始值。
                {move || {
                    let component = params.read().get("component").unwrap_or_default();
                    view! { <ShowcaseDetail component=component/> }
                }}
            </ShowcaseDataGate>
        }
        .into_any()
    }
}

fn group_label(group: &str) -> &str {
    showcase()
        .groups
        .iter()
        .find(|item| item[0] == group)
        .map(|item| item[1].as_str())
        .unwrap_or("全部组件")
}

struct CodeToken<'a> {
    class: Option<&'static str>,
    text: &'a str,
}

fn highlight_snippet(code: &str) -> Vec<CodeToken<'_>> {
    const KEYWORDS: &str = concat!(
        "as async await const crate else enum export fn for from if impl import in ",
        "interface let match mod move mut new pub return self static struct super trait ",
        "type use where while",
    );
    let bytes = code.as_bytes();
    let mut tokens = Vec::new();
    let mut position = 0;
    let mut plain_start = 0;

    while position < bytes.len() {
        let start = position;
        let class = if bytes[position..].starts_with(b"//") {
            position = bytes[position..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |offset| position + offset);
            Some("comment")
        } else if bytes[position..].starts_with(b"/*") {
            position = bytes[position + 2..]
                .windows(2)
                .position(|pair| pair == b"*/")
                .map_or(bytes.len(), |offset| position + offset + 4);
            Some("comment")
        } else if matches!(bytes[position], b'"' | b'`') {
            let quote = bytes[position];
            position += 1;
            while position < bytes.len() {
                if bytes[position] == b'\\' {
                    position = (position + 2).min(bytes.len());
                } else if bytes[position] == quote {
                    position += 1;
                    break;
                } else {
                    position += 1;
                }
            }
            Some("string")
        } else if bytes[position].is_ascii_digit() {
            position += 1;
            while position < bytes.len()
                && (bytes[position].is_ascii_alphanumeric() || bytes[position] == b'_')
            {
                position += 1;
            }
            Some("constant numeric")
        } else if bytes[position].is_ascii_alphabetic() || bytes[position] == b'_' {
            let raw_identifier = bytes[position..].starts_with(b"r#");
            position += if raw_identifier { 2 } else { 1 };
            while position < bytes.len()
                && (bytes[position].is_ascii_alphanumeric() || bytes[position] == b'_')
            {
                position += 1;
            }
            let word = &code[start..position];
            let next = bytes.get(position).copied();
            if matches!(word, "true" | "false") {
                Some("constant language boolean")
            } else if !raw_identifier && KEYWORDS.split_ascii_whitespace().any(|item| item == word)
            {
                Some("keyword")
            } else if next == Some(b':') && bytes.get(position + 1) != Some(&b':') {
                Some("meta property object")
            } else if word.starts_with(|character: char| character.is_ascii_uppercase()) {
                Some("entity name class")
            } else if matches!(next, Some(b'(' | b'!')) {
                Some("entity name function")
            } else if next == Some(b'{') {
                Some("keyword")
            } else {
                None
            }
        } else {
            position += code[position..].chars().next().unwrap().len_utf8();
            None
        };

        if let Some(class) = class {
            if plain_start < start {
                tokens.push(CodeToken {
                    class: None,
                    text: &code[plain_start..start],
                });
            }
            tokens.push(CodeToken {
                class: Some(class),
                text: &code[start..position],
            });
            plain_start = position;
        }
    }

    if plain_start < code.len() {
        tokens.push(CodeToken {
            class: None,
            text: &code[plain_start..],
        });
    }
    tokens
}

fn checkbox_snippet(checked: bool, danger: bool) -> String {
    format!(
        "let checked = RwSignal::new({checked});\n\nview! {{\n    <label>\n        <Checkbox\n            checked=checked\n            onchange=move |value: bool| checked.set(value){}\n        />\n        \"记住这个小小的选择\"\n    </label>\n}}",
        if danger { "\n            danger=true" } else { "" },
    )
}

fn alert_box_preset_message(variant: &str) -> &'static str {
    match variant {
        "error" => "保存失败，网络连接似乎中断了。",
        "default" => "草稿已自动保存至本地缓存。",
        _ => "已保存，新的想法正在生长。",
    }
}

fn alert_box_next_variant(variant: &str) -> &'static str {
    match variant {
        "success" => "error",
        "error" => "default",
        _ => "success",
    }
}

fn alert_box_snippet(variant: &str, message: &str) -> String {
    let escaped_message = message.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "view! {{\n    <AlertBox\n        message=\"{escaped_message}\"\n        variant=\"{variant}\"\n    />\n}}"
    )
}

/// 高亮后的用法片段。
fn highlighted_view(code: &str) -> impl IntoView {
    highlight_snippet(code)
        .into_iter()
        .map(
            |token| view! { <span class=token.class.unwrap_or("")>{token.text.to_string()}</span> },
        )
        .collect_view()
}

#[component]
fn AlertBoxDemo(
    detail: bool,
    alert_variant: RwSignal<String>,
    alert_message: RwSignal<String>,
) -> impl IntoView {
    let variant_str = Signal::derive(move || match alert_variant.read().as_str() {
        "error" => "error",
        "default" => "default",
        _ => "success",
    });
    let cycle = move || {
        let next = alert_box_next_variant(&alert_variant.read_untracked());
        alert_variant.set(next.to_string());
        alert_message.set(alert_box_preset_message(next).to_string());
    };

    view! {
        <div class="showcase-alert-demo">
            // 详情页由预览工具栏切换状态，这里只在目录卡片里给出切换控件。
            {(!detail)
                .then(|| {
                    view! {
                        <SegmentedControl
                            items=vec![
                                SegmentedItem::new("success".to_string(), "成功"),
                                SegmentedItem::new("error".to_string(), "错误"),
                                SegmentedItem::new("default".to_string(), "普通"),
                            ]
                            value=Signal::derive(move || variant_str.get().to_string())
                            aria_label="切换提示状态"
                            on_change=move |v: String| {
                                alert_message.set(alert_box_preset_message(&v).to_string());
                                alert_variant.set(v);
                            }
                        />
                    }
                })}
            <div
                class="showcase-alert-box-target"
                role="button"
                tabindex="0"
                title="点击循环切换状态"
                aria-label="点击循环切换状态"
                on:click=move |_| cycle()
                on:keydown=move |event: KeyboardEvent| {
                    if event.key() == "Enter" {
                        cycle();
                    }
                }
            >
                <crate::components::forms::AlertBox message=alert_message variant=variant_str/>
            </div>
        </div>
    }
}

/// 宽屏时目录右侧有详情栏，点卡片只切换选中；窄屏没有详情栏，点卡片直接进入详情页。
fn inspector_visible() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|window| window.match_media("(min-width: 1101px)").ok().flatten())
            .is_some_and(|query| query.matches())
    }
    #[cfg(not(target_arch = "wasm32"))]
    false
}

/// 点击是否落在卡片内的交互控件上（链接、按钮、编辑器等），这类点击不触发卡片选中。
fn click_hits_interactive(event: &MouseEvent) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
            .and_then(|element| {
                element
                    .closest(
                        "a, button, input, select, textarea, label, summary, [contenteditable], [role=button], [role=tab], .cm-editor, .xterm",
                    )
                    .ok()
                    .flatten()
            })
            .is_some()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = event;
        false
    }
}

#[component]
fn ShowcaseGallery() -> impl IntoView {
    let filters = expect_context::<RwSignal<ShowcaseFilters>>();

    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = web_sys::window() {
            crate::utils::js::invoke_optional_global(&window, "__initShowcaseMasonry", &[]);
        }
    });
    on_cleanup(|| {
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = web_sys::window() {
            crate::utils::js::invoke_optional_global(&window, "__disposeShowcaseMasonry", &[]);
        }
    });

    let active_group = Memo::new(move |_| {
        let category = filters.read().category.clone();
        if category.is_empty() {
            "all".to_string()
        } else {
            category
        }
    });
    let shown = Memo::new(move |_| {
        let group = active_group.get();
        let query = filters.read().search.trim().to_lowercase();
        showcase()
            .components
            .iter()
            .filter(|spec| {
                (group == "all" || spec.group == group)
                    && (query.is_empty()
                        || spec.name.to_lowercase().contains(&query)
                        || spec.label.to_lowercase().contains(&query)
                        || spec.summary.to_lowercase().contains(&query))
            })
            .map(|spec| spec.slug.clone())
            .collect::<Vec<_>>()
    });
    let total = showcase().components.len();
    let interactive_count = showcase()
        .components
        .iter()
        .filter(|spec| spec.preview_mode == PreviewMode::Interactive)
        .count();
    // 选中项被筛掉时，详情栏落到当前结果的第一项。
    let selected = Memo::new(move |_| {
        let shown = shown.get();
        let wanted = filters.read().selected.clone();
        wanted
            .filter(|slug| shown.contains(slug))
            .or_else(|| shown.first().cloned())
    });

    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let return_to = filters.read().return_to.clone();
            if let Some(slug) = return_to {
                if let Some(target) = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.get_element_by_id(&format!("showcase-{slug}")))
                {
                    target.scroll_into_view();
                    filters.update(|state| state.return_to = None);
                }
            }
        }
    });

    let categories = showcase()
        .groups
        .iter()
        .map(|group| {
            let id = group[0].clone();
            let amount = if id == "all" {
                total
            } else {
                showcase()
                    .components
                    .iter()
                    .filter(|spec| spec.group == id)
                    .count()
            };
            let label = if id == "all" {
                "全部".to_string()
            } else {
                group[1].clone()
            };
            let pressed = {
                let id = id.clone();
                Memo::new(move |_| active_group.get() == id)
            };
            view! {
                <button
                    type="button"
                    class=move || if pressed.get() { "chip is-active" } else { "chip" }
                    aria-pressed=move || pressed.get().to_string()
                    on:click=move |_| filters.update(|state| state.category = id.clone())
                >
                    {label}
                    <span class="chip-count">{amount}</span>
                </button>
            }
        })
        .collect_view();

    view! {
        <Title text="组件图鉴 · Yggdrasil"/>
        // P5 §8.2：图鉴 masonry 增强与灯箱预览按需注入（SSR 直达带 defer，先于
        // hydration；SPA 首跳由增强调用队列兜底）。阅读器客户端不含图鉴路由。
        <crate::components::enhance_assets::EnhanceScript/>
        <crate::components::enhance_assets::LightboxAssets/>
        <article class="showcase showcase-gallery">
            <a class="showcase-up" href=Route::About.to_string()>
                <Icon name=IconName::ArrowLeft size=14/>
                "关于"
            </a>
            <header class="showcase-header page-enter">
                <div>
                    <h1 class="showcase-title">"组件图鉴"</h1>
                    <p class="showcase-intro tabular-nums">
                        {format!("{total} 个组件，都来自正式页面，其中 {interactive_count} 个可以直接操作。")}
                    </p>
                </div>
                <label class="showcase-search">
                    <Icon name=IconName::Search size=15/>
                    <FormInput
                        class="showcase-search-input"
                        r#type="search"
                        aria_label="搜索组件"
                        placeholder="组件名或用途"
                        value=Signal::derive(move || filters.read().search.clone())
                        oninput=move |value: String| filters.update(|state| state.search = value)
                    />
                </label>
            </header>
            <nav class="showcase-categories page-enter" style="--enter-index: 1" aria-label="组件分类">
                {categories}
            </nav>
            <div class="showcase-catalog page-enter" style="--enter-index: 2">
                <div class="showcase-grid">
                    <For
                        each=move || shown.get()
                        key=|slug| slug.clone()
                        children=move |slug| {
                            let spec = showcase()
                                .components
                                .iter()
                                .find(|item| item.slug == slug)
                                .expect("目录卡片必须对应图鉴条目");
                            let card_slug = slug.clone();
                            let is_selected = {
                                let slug = slug.clone();
                                move || selected.read().as_deref() == Some(slug.as_str())
                            };
                            view! {
                                <article
                                    class=move || {
                                        if is_selected() {
                                            "showcase-card is-selected"
                                        } else {
                                            "showcase-card"
                                        }
                                    }
                                    id=format!("showcase-{slug}")
                                    on:click=move |event: MouseEvent| {
                                        if click_hits_interactive(&event) {
                                            return;
                                        }
                                        if inspector_visible() {
                                            filters
                                                .update(|state| state.selected = Some(card_slug.clone()));
                                        } else {
                                            crate::navigation::push(
                                                &Route::ComponentDetail {
                                                        component: card_slug.clone(),
                                                    }
                                                    .to_string(),
                                            );
                                        }
                                    }
                                >
                                    <div class=format!(
                                        "showcase-card-preview showcase-card-preview--{slug}",
                                    )>
                                        <ComponentPreview slug=slug.clone() detail=false/>
                                    </div>
                                    <div class="showcase-card-caption">
                                        <div class="showcase-card-title">
                                            <h3>
                                                <a href=Route::ComponentDetail {
                                                    component: slug.clone(),
                                                }
                                                    .to_string()>{spec.label.clone()}</a>
                                            </h3>
                                            <code>{spec.name.clone()}</code>
                                        </div>
                                        <p>{spec.summary.clone()}</p>
                                    </div>
                                </article>
                            }
                        }
                    />
                    {move || {
                        shown
                            .read()
                            .is_empty()
                            .then(|| {
                                view! {
                                    <p class="showcase-no-results">
                                        "没有找到这个组件，换个名字或用途试试。"
                                    </p>
                                }
                            })
                    }}
                </div>
                {move || selected.get().map(|slug| view! { <ShowcaseInspector slug=slug/> })}
            </div>
        </article>
    }
}

/// 目录右侧的详情栏：说明、状态、属性与用法，并给出详情页入口。
#[component]
fn ShowcaseInspector(slug: String) -> impl IntoView {
    let spec = showcase()
        .components
        .iter()
        .find(|item| item.slug == slug)
        .expect("详情栏必须对应图鉴条目");

    let props = (!spec.props.is_empty()).then(|| {
        let rows = spec
            .props
            .iter()
            .map(|prop| {
                view! {
                    <div>
                        <dt>
                            <code>{prop[0].clone()}</code>
                            <span>{prop[1].clone()}</span>
                        </dt>
                        <dd>{prop[3].clone()}</dd>
                    </div>
                }
            })
            .collect_view();
        view! {
            <section class="showcase-inspector-section">
                <h3>"属性"</h3>
                <dl class="showcase-inspector-props">{rows}</dl>
            </section>
        }
    });

    view! {
        <aside class="showcase-inspector" aria-label="组件详情">
            <div class="showcase-inspector-head">
                <div class="showcase-inspector-title">
                    <h2>{spec.label.clone()}</h2>
                    <code>{spec.name.clone()}</code>
                </div>
                <p>{spec.description.clone()}</p>
                <p class="showcase-inspector-states">{format!("状态：{}", spec.states)}</p>
            </div>
            {props}
            <section class="showcase-inspector-section">
                <h3>"用法"</h3>
                <div class="showcase-code-panel">
                    <pre>
                        <code>{highlighted_view(&spec.code)}</code>
                    </pre>
                </div>
                <p class="showcase-inspector-source">{spec.source.clone()}</p>
            </section>
            <div class="showcase-inspector-foot">
                <a
                    class=button_class(ButtonVariant::Secondary, ButtonSize::Sm)
                    href=Route::ComponentDetail {
                        component: spec.slug.clone(),
                    }
                        .to_string()
                >
                    "打开详情页"
                    <Icon name=IconName::ArrowRight size=14/>
                </a>
            </div>
        </aside>
    }
}

/// 详情页的章节，与右侧目录一一对应。
const DETAIL_SECTIONS: [(&str, &str); 4] = [
    ("preview", "预览"),
    ("props", "属性"),
    ("keys", "键盘"),
    ("usage", "用法"),
];

fn checkbox_state(checked: bool, danger: bool) -> &'static str {
    if danger {
        "danger"
    } else if checked {
        "checked"
    } else {
        "default"
    }
}

/// 目录高亮最后一个越过页头的章节；滚到底时高亮最后一节，短章节也能被选中。
#[cfg(target_arch = "wasm32")]
fn update_active_section(active_section: RwSignal<&'static str>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let present: Vec<(&'static str, f64)> = DETAIL_SECTIONS
        .iter()
        .filter_map(|(id, _)| {
            document
                .get_element_by_id(id)
                .map(|element| (*id, element.get_bounding_client_rect().top()))
        })
        .collect();
    let viewport = window
        .inner_height()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    let scrolled = window.scroll_y().unwrap_or(0.0);
    let height = document
        .document_element()
        .map(|root| f64::from(root.scroll_height()))
        .unwrap_or(0.0);
    let current = if scrolled > 0.0 && viewport + scrolled >= height - 2.0 {
        present.last().map(|(id, _)| *id)
    } else {
        present
            .iter()
            .rev()
            .find(|(_, top)| *top <= 120.0)
            .map(|(id, _)| *id)
    }
    .unwrap_or("preview");
    if active_section.try_get_untracked() != Some(current) {
        active_section.try_set(current);
    }
}

#[component]
fn ShowcaseDetail(component: String) -> impl IntoView {
    let filters = expect_context::<RwSignal<ShowcaseFilters>>();
    let checked = RwSignal::new(false);
    let danger = RwSignal::new(false);
    let alert_variant = RwSignal::new("success".to_string());
    let alert_message = RwSignal::new(alert_box_preset_message("success").to_string());
    let reset_generation = RwSignal::new(0_u32);
    let copied = RwSignal::new(false);
    let active_section = RwSignal::new("preview");

    #[cfg(target_arch = "wasm32")]
    crate::hooks::event_listener::use_event_listener(web_sys::window, "scroll", move || {
        update_active_section(active_section)
    });

    let Some((index, spec)) = showcase()
        .components
        .iter()
        .enumerate()
        .find(|(_, spec)| spec.slug == component)
    else {
        crate::request_ctx::set_status(404);
        return view! {
            <Title text="组件未找到 · Yggdrasil"/>
            <div class="showcase showcase-missing">
                <h1>"图鉴里没有这个组件"</h1>
                <p>"地址可能写错了，或者组件已经改名。"</p>
                <a
                    class=button_class(ButtonVariant::Secondary, ButtonSize::Md)
                    href=Route::ComponentShowcase.to_string()
                >
                    "返回组件图鉴"
                </a>
            </div>
        }
        .into_any();
    };
    let is_checkbox = spec.name == "Checkbox";
    let is_alert = spec.name == "AlertBox";
    let snippet = {
        let code = spec.code.clone();
        move || {
            if is_checkbox {
                checkbox_snippet(checked.get(), danger.get())
            } else if is_alert {
                alert_box_snippet(&alert_variant.read(), &alert_message.read())
            } else {
                code.clone()
            }
        }
    };
    let interactive = spec.preview_mode == PreviewMode::Interactive;
    let stage_class = match (interactive, is_wide_preview(&spec.slug)) {
        (false, true) => "showcase-live-preview is-static is-wide",
        (false, false) => "showcase-live-preview is-static",
        (true, true) => "showcase-live-preview is-wide",
        (true, false) => "showcase-live-preview",
    };
    let status = {
        let local_demo = is_local_demo(&spec.slug);
        move || {
            if is_checkbox {
                Some(format!(
                    "checked: {} · danger: {}",
                    checked.get(),
                    danger.get()
                ))
            } else if is_alert {
                Some(format!(
                    "variant: \"{}\" · message: \"{}\"",
                    alert_variant.get(),
                    alert_message.get()
                ))
            } else if local_demo {
                Some("本地演示，内容不会保存".to_string())
            } else {
                None
            }
        }
    };
    let sections: Vec<(&str, &str)> = DETAIL_SECTIONS
        .into_iter()
        .filter(|(id, _)| *id != "keys" || !spec.keys.is_empty())
        .collect();
    let previous = index.checked_sub(1).map(|i| &showcase().components[i]);
    let next = showcase().components.get(index + 1);
    let gallery_slug = spec.slug.clone();
    let slug = spec.slug.clone();

    let state_control = if is_checkbox {
        Some(
            view! {
                <SegmentedControl
                    items=vec![
                        SegmentedItem::new("default".to_string(), "默认"),
                        SegmentedItem::new("checked".to_string(), "已选"),
                        SegmentedItem::new("danger".to_string(), "危险态"),
                    ]
                    value=Signal::derive(move || {
                        checkbox_state(checked.get(), danger.get()).to_string()
                    })
                    aria_label="预览状态"
                    on_change=move |state: String| {
                        checked.set(state != "default");
                        danger.set(state == "danger");
                    }
                />
            }
            .into_any(),
        )
    } else if is_alert {
        Some(
            view! {
                <SegmentedControl
                    items=vec![
                        SegmentedItem::new("success".to_string(), "成功"),
                        SegmentedItem::new("error".to_string(), "错误"),
                        SegmentedItem::new("default".to_string(), "普通"),
                    ]
                    value=alert_variant
                    aria_label="预览状态"
                    on_change=move |variant: String| {
                        alert_message.set(alert_box_preset_message(&variant).to_string());
                        alert_variant.set(variant);
                    }
                />
            }
            .into_any(),
        )
    } else {
        None
    };
    let reset_button = interactive.then(|| {
        view! {
            <Button
                variant=ButtonVariant::Ghost
                size=ButtonSize::Sm
                icon=IconName::RotateCcw
                class="showcase-reset"
                onclick=move |_: MouseEvent| {
                    checked.set(false);
                    danger.set(false);
                    alert_variant.set("success".to_string());
                    alert_message.set(alert_box_preset_message("success").to_string());
                    reset_generation.update(|generation| *generation = generation.wrapping_add(1));
                }
            >
                "恢复默认"
            </Button>
        }
    });
    let alert_field = is_alert.then(|| {
        view! {
            <label class="showcase-live-field">
                <span>"message"</span>
                <FormInput
                    r#type="text"
                    placeholder="输入自定义提示文案…"
                    value=alert_message
                    oninput=move |value: String| alert_message.set(value)
                />
            </label>
        }
    });

    let props_section = if spec.props.is_empty() {
        view! { <p class="showcase-detail-note">"这个组件不接收属性。"</p> }.into_any()
    } else {
        let rows = spec
            .props
            .iter()
            .map(|prop| {
                let default = if prop[2] == "必填" {
                    view! { <span class="showcase-prop-required">"必填"</span> }.into_any()
                } else {
                    view! { <code class="showcase-prop-default">{prop[2].clone()}</code> }
                        .into_any()
                };
                view! {
                    <tr>
                        <td>
                            <code>{prop[0].clone()}</code>
                        </td>
                        <td>
                            <code class="showcase-prop-type">{prop[1].clone()}</code>
                        </td>
                        <td>{default}</td>
                        <td>{prop[3].clone()}</td>
                    </tr>
                }
            })
            .collect_view();
        view! {
            <div class="showcase-props-wrap">
                <table>
                    <thead>
                        <tr>
                            <th>"名称"</th>
                            <th>"类型"</th>
                            <th>"默认值"</th>
                            <th>"说明"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        }
        .into_any()
    };

    let keys_section = (!spec.keys.is_empty()).then(|| {
        let items = spec
            .keys
            .iter()
            .map(|item| {
                view! {
                    <div>
                        <dt>
                            <kbd>{item[0].clone()}</kbd>
                        </dt>
                        <dd>{item[1].clone()}</dd>
                    </div>
                }
            })
            .collect_view();
        view! {
            <section id="keys" class="showcase-detail-section" aria-labelledby="showcase-keys-title">
                <h2 id="showcase-keys-title">"键盘"</h2>
                <dl class="showcase-key-list">{items}</dl>
            </section>
        }
    });

    let usage_label = match spec.usage_kind {
        UsageKind::Rust => "Leptos view!",
        UsageKind::Api => "Rust 接口",
        UsageKind::Router => "路由配置",
        UsageKind::Browser => "TypeScript",
    };
    let api_note = (spec.usage_kind == UsageKind::Api).then(|| {
        view! {
            <p class="showcase-detail-note">
                "下方列出组件接口；正式调用由所在页面提供数据与回调。"
            </p>
        }
    });
    let copy_snippet = snippet.clone();
    let on_copy = move |_: MouseEvent| {
        #[cfg(target_arch = "wasm32")]
        {
            let text = copy_snippet();
            leptos::task::spawn_local(async move {
                if crate::utils::js::copy_text(&text).await {
                    copied.try_set(true);
                    crate::utils::time::sleep_ms(1600).await;
                    copied.try_set(false);
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = &copy_snippet;
    };

    let neighbor = |item: &ComponentSpec, class: &'static str, hint: &'static str| {
        view! {
            <a
                class=class
                href=Route::ComponentDetail {
                    component: item.slug.clone(),
                }
                    .to_string()
            >
                <span class="showcase-neighbor-hint">{hint}</span>
                <span class="showcase-neighbor-name">
                    {format!("{} ", item.label)} <code>{item.name.clone()}</code>
                </span>
            </a>
        }
    };
    let previous_link = previous.map(|item| neighbor(item, "", "← 上一个"));
    let next_link = next.map(|item| neighbor(item, "is-next", "下一个 →"));

    let toc = sections
        .into_iter()
        .map(|(id, label)| {
            view! {
                <li>
                    <a
                        href=format!("#{id}")
                        class=move || if active_section.get() == id { "is-active" } else { "" }
                        aria-current=move || (active_section.get() == id).then_some("location")
                    >
                        {label}
                    </a>
                </li>
            }
        })
        .collect_view();

    view! {
        <Title text=format!("{} {} · 组件图鉴 · Yggdrasil", spec.label, spec.name)/>
        <article class="showcase showcase-detail">
            <nav class="breadcrumb" aria-label="面包屑">
                <a href=Route::About.to_string()>"关于"</a>
                <span aria-hidden="true">"/"</span>
                <a
                    href=Route::ComponentShowcase.to_string()
                    on:click=move |_| {
                        filters
                            .update(|state| {
                                state.return_to = Some(gallery_slug.clone());
                                state.selected = Some(gallery_slug.clone());
                            });
                    }
                >
                    "组件图鉴"
                </a>
                <span aria-hidden="true">"/"</span>
                <span aria-current="page">{spec.label.clone()}</span>
            </nav>
            <div class="showcase-detail-layout">
                <div class="showcase-detail-main">
                    <header class="showcase-detail-header page-enter">
                        <div class="showcase-detail-title">
                            <h1>{spec.label.clone()}</h1>
                            <code>{spec.name.clone()}</code>
                            <span class="chip">{group_label(&spec.group)}</span>
                        </div>
                        <p class="showcase-detail-lead">{spec.description.clone()}</p>
                    </header>
                    <section
                        id="preview"
                        class="showcase-live-panel page-enter"
                        style="--enter-index: 1"
                        aria-labelledby="showcase-preview-title"
                    >
                        <div class="showcase-live-toolbar">
                            <h2 id="showcase-preview-title">"预览"</h2>
                            <div class="showcase-live-actions">{state_control} {reset_button}</div>
                        </div>
                        <div class=stage_class>
                            {move || {
                                reset_generation.track();
                                view! {
                                    <div class="showcase-preview-instance">
                                        <ComponentPreview
                                            slug=slug.clone()
                                            detail=true
                                            checked=checked
                                            danger=danger
                                            alert_variant=alert_variant
                                            alert_message=alert_message
                                        />
                                    </div>
                                }
                            }}
                        </div>
                        {alert_field}
                        {move || {
                            status()
                                .map(|status| view! { <div class="showcase-live-status">{status}</div> })
                        }}
                    </section>
                    <section
                        id="props"
                        class="showcase-detail-section"
                        aria-labelledby="showcase-props-title"
                    >
                        <h2 id="showcase-props-title">"属性"</h2>
                        {props_section}
                    </section>
                    {keys_section}
                    <section
                        id="usage"
                        class="showcase-detail-section"
                        aria-labelledby="showcase-usage-title"
                    >
                        <div class="showcase-section-head">
                            <h2 id="showcase-usage-title">"用法"</h2>
                            <code class="showcase-source">{spec.source.clone()}</code>
                        </div>
                        {api_note}
                        <figure class="showcase-code-panel">
                            <div class="showcase-code-toolbar">
                                <span>{usage_label}</span>
                                <Button
                                    variant=ButtonVariant::Ghost
                                    size=ButtonSize::Sm
                                    icon=Signal::derive(move || {
                                        Some(if copied.get() { IconName::Check } else { IconName::Copy })
                                    })
                                    onclick=on_copy
                                >
                                    {move || if copied.get() { "已复制" } else { "复制" }}
                                </Button>
                            </div>
                            <pre>
                                <code>{move || highlighted_view(&snippet())}</code>
                            </pre>
                        </figure>
                    </section>
                    <nav class="showcase-neighbors" aria-label="相邻组件">
                        {previous_link} {next_link}
                    </nav>
                </div>
                <aside class="showcase-toc" aria-label="本页目录">
                    <p class="showcase-toc-label">"本页"</p>
                    <ol>{toc}</ol>
                    <p class="showcase-toc-states">{format!("状态：{}", spec.states)}</p>
                </aside>
            </div>
        </article>
    }
    .into_any()
}

#[component]
fn ComponentPreview(
    slug: String,
    detail: bool,
    #[prop(optional)] checked: Option<RwSignal<bool>>,
    #[prop(optional)] danger: Option<RwSignal<bool>>,
    #[prop(optional)] alert_variant: Option<RwSignal<String>>,
    #[prop(optional)] alert_message: Option<RwSignal<String>>,
) -> impl IntoView {
    let spec = showcase()
        .components
        .iter()
        .find(|item| item.slug == slug)
        .expect("预览必须对应图鉴条目");
    let name = spec.name.clone();
    // P5 §8.2：预览分派的公共入口统一声明增强/灯箱资源——detail 详情页不渲染
    // ShowcaseGallery，全局注入移除后这里必须自带（按 src 去重，画廊页幂等）。
    let preview = match preview_route(&slug).expect("图鉴条目必须有专属预览分派") {
        PreviewRoute::Live => match (checked, danger, alert_variant, alert_message) {
            (Some(checked), Some(danger), Some(alert_variant), Some(alert_message)) => view! {
                <live_previews::LiveComponentPreview
                    name=name
                    detail=detail
                    checked=checked
                    danger=danger
                    alert_variant=alert_variant
                    alert_message=alert_message
                />
            }
            .into_any(),
            _ => {
                view! { <live_previews::LiveComponentPreview name=name detail=detail/> }.into_any()
            }
        },
        PreviewRoute::Scene => {
            view! { <live_previews::SceneComponentPreview name=name/> }.into_any()
        }
        PreviewRoute::Business => {
            business_previews::preview(&slug, detail).expect("业务预览必须实现")
        }
        PreviewRoute::LayoutArticle => {
            layout_article_previews::preview(&slug, detail).expect("布局与文章预览必须实现")
        }
        PreviewRoute::Comment => {
            comment_previews::preview(&slug, detail).expect("评论预览必须实现")
        }
        PreviewRoute::Browser => {
            browser_previews::preview(&slug, detail).expect("浏览器预览必须实现")
        }
    };
    view! {
        <crate::components::enhance_assets::EnhanceScript/>
        <crate::components::enhance_assets::LightboxAssets/>
        {preview}
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewRoute {
    Live,
    Scene,
    Business,
    LayoutArticle,
    Comment,
    Browser,
}

fn preview_route(slug: &str) -> Option<PreviewRoute> {
    Some(match slug {
        "checkbox" | "toggle-switch" | "form-select" | "radio" | "form-input" | "form-textarea"
        | "time-picker" | "form-label" | "segmented-control" | "pagination" | "table-pager"
        | "tooltip" | "popover" | "modal-shell" | "confirm-dialog" | "alert-box"
        | "status-badge" | "status-dot" | "tag-chip" | "user-avatar" | "image-placeholder"
        | "icon" | "button" | "icon-button" | "number-input" | "menu" | "toast"
        | "bottom-sheet" | "side-sheet" | "breadcrumb" | "search-palette" | "admin-palette"
        | "empty-state" | "load-error" | "composer" | "composer-input" | "composer-send" => {
            PreviewRoute::Live
        }
        "post-cover" => PreviewRoute::Scene,
        "asset-picker-modal" | "asset-upload-modal" | "code-runner" | "sql-result-table" => {
            PreviewRoute::Business
        }
        "breadcrumbs" | "post-header" | "post-meta" | "post-content" | "post-footer"
        | "post-nav-links" | "post-toc" => PreviewRoute::LayoutArticle,
        "comment-card-shell"
        | "comment-form"
        | "comment-item"
        | "comment-list"
        | "pending-comment-item"
        | "comment-section" => PreviewRoute::Comment,
        "tiptap-editor" | "code-mirror-editor" | "xterm-terminal" | "lightbox" => {
            PreviewRoute::Browser
        }
        _ => return None,
    })
}

fn is_local_demo(slug: &str) -> bool {
    matches!(
        preview_route(slug),
        Some(PreviewRoute::Business | PreviewRoute::Comment | PreviewRoute::Browser)
    )
}

fn is_wide_preview(slug: &str) -> bool {
    matches!(
        preview_route(slug),
        Some(
            PreviewRoute::Business
                | PreviewRoute::LayoutArticle
                | PreviewRoute::Comment
                | PreviewRoute::Browser
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::Path;

    #[test]
    fn large_embedded_documents_stay_out_of_wasm() {
        // 两份文档只供服务端渲染与 server function 下发，不能随 WASM 发给每位访客。
        for (path, source, embed) in [
            (
                "components_showcase",
                include_str!("components_showcase.rs"),
                "static SHOWCASE: LazyLock",
            ),
            (
                "api/writing_guide",
                include_str!("../api/writing_guide.rs"),
                "pub const SKILL_SOURCE",
            ),
        ] {
            let before = source
                .split_once(embed)
                .unwrap_or_else(|| panic!("{path} should embed its document"))
                .0;
            assert!(
                before.trim_end().ends_with("#[cfg(feature = \"server\")]"),
                "{path} must gate its embedded document behind the server feature"
            );
        }
    }

    fn public_component_names(dir: &Path, names: &mut HashSet<String>) {
        for entry in std::fs::read_dir(dir).expect("读取组件源码目录") {
            let path = entry.expect("组件目录项").path();
            if path.is_dir() {
                // 骨架屏保留在业务页面中，不收录到公开图鉴。
                if path.file_name().is_some_and(|name| name == "skeletons") {
                    continue;
                }
                public_component_names(&path, names);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path).expect("读取组件源码");
                for line in source.lines() {
                    if let Some(rest) = line.trim_start().strip_prefix("pub fn ") {
                        let name: String = rest
                            .chars()
                            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                            .collect();
                        if name.starts_with(char::is_uppercase) {
                            names.insert(name);
                        }
                    }
                }
            }
        }
    }

    const EXCLUDED_LAYOUT_COMPONENTS: [(&str, &str, &str); 5] = [
        ("AdminLayout", "admin-layout", "admin_layout.rs"),
        ("FrontendLayout", "frontend-layout", "frontend_layout.rs"),
        ("Header", "header", "header.rs"),
        ("Footer", "footer", "footer.rs"),
        ("SearchIconLink", "search-icon-link", "header.rs"),
    ];

    #[test]
    fn catalog_covers_public_components_and_has_unique_routes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/components");
        let mut exported = HashSet::new();
        public_component_names(&root, &mut exported);
        let catalog: HashSet<&str> = SHOWCASE
            .components
            .iter()
            .map(|component| component.name.as_str())
            .collect();
        let missing: Vec<_> = exported
            .iter()
            .filter(|name| {
                !catalog.contains(name.as_str())
                    && !EXCLUDED_LAYOUT_COMPONENTS
                        .iter()
                        .any(|(excluded, _, _)| name.as_str() == *excluded)
            })
            .collect();
        assert!(missing.is_empty(), "图鉴缺少公开组件: {missing:?}");

        let mut slugs = HashSet::new();
        let groups: HashSet<&str> = SHOWCASE
            .groups
            .iter()
            .map(|group| group[0].as_str())
            .collect();
        for component in &SHOWCASE.components {
            assert!(
                slugs.insert(component.slug.as_str()),
                "重复路径: {}",
                component.slug
            );
            assert!(
                groups.contains(component.group.as_str()),
                "未知分类: {}",
                component.group
            );
            assert!(
                !component.code.trim().is_empty(),
                "缺少用法: {}",
                component.name
            );
            assert!(
                preview_route(&component.slug).is_some(),
                "缺少真实预览分派: {}",
                component.slug
            );
        }
    }

    #[test]
    fn catalog_excludes_layouts_but_keeps_production_components() {
        assert!(SHOWCASE.groups.iter().all(|group| group[0] != "layout"));
        assert!(SHOWCASE
            .components
            .iter()
            .all(|item| item.group != "layout"));
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/components");
        for (name, slug, file) in EXCLUDED_LAYOUT_COMPONENTS {
            assert!(!SHOWCASE
                .components
                .iter()
                .any(|item| item.name == name || item.slug == slug));
            assert_eq!(preview_route(slug), None, "布局预览不应保留: {slug}");
            assert!(layout_article_previews::preview(slug, false).is_none());
            assert!(layout_article_previews::preview(slug, true).is_none());
            let source = std::fs::read_to_string(root.join(file)).expect("正式布局组件必须保留");
            assert!(source.contains(&format!("pub fn {name}(")));
        }
    }

    #[test]
    fn catalog_excludes_skeletons_and_their_preview_routes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/components/skeletons");
        let mut skeletons = HashSet::new();
        public_component_names(&root, &mut skeletons);
        assert!(!skeletons.is_empty(), "业务骨架屏组件必须保留");
        for component in &SHOWCASE.components {
            assert!(!skeletons.contains(&component.name));
            assert!(!component.source.starts_with("src/components/skeletons/"));
        }
        for slug in [
            "skeleton-box",
            "skeleton-card",
            "skeleton-table",
            "delayed-skeleton",
            "home-skeleton",
            "post-detail-body",
            "tag-posts-loading",
        ] {
            assert_eq!(preview_route(slug), None, "骨架屏预览不应保留: {slug}");
        }
        assert!(SHOWCASE.groups.iter().all(|group| group[0] == "all"
            || SHOWCASE
                .components
                .iter()
                .any(|item| item.group == group[0])));
    }

    #[test]
    fn highlighted_usage_preserves_every_snippet() {
        let snippets = SHOWCASE
            .components
            .iter()
            .map(|component| component.code.clone())
            .chain([true, false].into_iter().flat_map(|checked| {
                [true, false].map(move |danger| checkbox_snippet(checked, danger))
            }))
            .chain(
                ["success", "error", "default"]
                    .into_iter()
                    .map(|variant| alert_box_snippet(variant, alert_box_preset_message(variant))),
            );

        for snippet in snippets {
            let tokens = highlight_snippet(&snippet);
            assert_eq!(
                tokens.iter().map(|token| token.text).collect::<String>(),
                snippet
            );
            assert!(tokens.iter().any(|token| token.class.is_some()));
        }
    }

    #[test]
    fn highlighted_usage_keeps_strings_and_comments_intact() {
        let code = "let text = \"中文 \\\"// still a string\\\"\";\n// 注释\n";
        let tokens = highlight_snippet(code);
        assert!(tokens
            .iter()
            .any(|token| token.class == Some("string") && token.text.contains("中文")));
        assert!(tokens
            .iter()
            .any(|token| token.class == Some("comment") && token.text == "// 注释"));
        assert_eq!(
            tokens.iter().map(|token| token.text).collect::<String>(),
            code
        );
    }
}
