//! 项目自写组件的公开图鉴与独立用法页。
//!
//! 目录、文案和属性说明来自本模块的静态数据；预览直接渲染项目里的组件。
#[cfg(feature = "server")]
use std::sync::LazyLock;
#[cfg(not(feature = "server"))]
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
#[cfg(target_arch = "wasm32")]
use dioxus::html::{InteractionElementOffset, InteractionLocation};
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use dioxus::web::WebEventExt;
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

use crate::components::forms::FormInput;
use crate::components::ui::{
    button_class, Button, ButtonSize, ButtonVariant, FilterTabs, Icon, IconName,
};
use crate::models::post::{Post, PostListItem, PostStatus};
use crate::router::Route;

mod browser_previews;
mod business_previews;
mod comment_previews;
mod layout_article_previews;

#[derive(Clone, Default, PartialEq)]
pub struct ShowcaseFilters {
    pub category: String,
    pub search: String,
    /// 宽屏详情栏当前展示的组件；`None` 时取筛选结果的第一项。
    pub selected: Option<String>,
    pub return_to: Option<String>,
}

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

#[server]
pub(crate) async fn get_showcase_data() -> Result<ShowcaseData, ServerFnError> {
    Ok(SHOWCASE.clone())
}

/// 客户端会话内只请求一次；首屏直接复用 SSR 水合数据。
async fn load_showcase() -> Result<ShowcaseData, ServerFnError> {
    #[cfg(not(feature = "server"))]
    if let Some(data) = CLIENT_SHOWCASE.get() {
        return Ok(data.clone());
    }
    get_showcase_data().await
}

/// 加载图鉴数据后再渲染子页面，使目录、详情与预览可同步读取 `showcase()`。
#[component]
fn ShowcaseDataGate(children: Element) -> Element {
    let response = use_server_future(load_showcase)?;
    match response.read().as_ref() {
        Some(Ok(_data)) =>
        {
            #[cfg(not(feature = "server"))]
            if CLIENT_SHOWCASE.get().is_none() {
                let _ = CLIENT_SHOWCASE.set(_data.clone());
            }
        }
        Some(Err(error)) => return Err(error.clone().into()),
        None => return rsx! {},
    }
    children
}

#[component]
pub fn ComponentShowcase() -> Element {
    rsx! {
        ShowcaseDataGate { ShowcaseGallery {} }
    }
}

#[component]
pub fn ComponentDetail(component: String) -> Element {
    rsx! {
        // 按组件重建详情，切换相邻组件时预览状态、复制提示与目录高亮都回到初始值。
        ShowcaseDataGate { ShowcaseDetail { key: "{component}", component } }
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
        "let mut checked = use_signal(|| {checked});\n\nrsx! {{\n    label {{\n        Checkbox {{\n            checked: checked(),\n            onchange: move |value| checked.set(value),{}\n        }}\n        \"记住这个小小的选择\"\n    }}\n}}",
        if danger { "\n            danger: true," } else { "" },
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
        "rsx! {{\n    AlertBox {{\n        message: \"{escaped_message}\",\n        variant: \"{variant}\",\n    }}\n}}"
    )
}

#[component]
fn AlertBoxDemo(
    detail: bool,
    mut alert_variant: Signal<String>,
    mut alert_message: Signal<String>,
) -> Element {
    let current_variant = alert_variant();
    let variant_str: &'static str = match current_variant.as_str() {
        "error" => "error",
        "default" => "default",
        _ => "success",
    };

    rsx! {
        div { class: "showcase-alert-demo",
            // 详情页由预览工具栏切换状态，这里只在目录卡片里给出切换按钮。
            if !detail { div { class: "showcase-alert-switcher", role: "tablist", aria_label: "切换提示状态",
                button {
                    r#type: "button",
                    class: if variant_str == "success" { "state-success" } else { "" },
                    aria_pressed: "{variant_str == \"success\"}",
                    onclick: move |_| {
                        alert_variant.set("success".to_string());
                        alert_message.set(alert_box_preset_message("success").to_string());
                    },
                    span { class: "dot dot-success" }
                    "成功"
                }
                button {
                    r#type: "button",
                    class: if variant_str == "error" { "state-error" } else { "" },
                    aria_pressed: "{variant_str == \"error\"}",
                    onclick: move |_| {
                        alert_variant.set("error".to_string());
                        alert_message.set(alert_box_preset_message("error").to_string());
                    },
                    span { class: "dot dot-error" }
                    "错误"
                }
                button {
                    r#type: "button",
                    class: if variant_str == "default" { "state-default" } else { "" },
                    aria_pressed: "{variant_str == \"default\"}",
                    onclick: move |_| {
                        alert_variant.set("default".to_string());
                        alert_message.set(alert_box_preset_message("default").to_string());
                    },
                    span { class: "dot dot-default" }
                    "普通"
                }
            } }
            div {
                class: "showcase-alert-box-target",
                role: "button",
                tabindex: 0,
                title: "点击循环切换状态",
                aria_label: "点击循环切换状态",
                onclick: move |_| {
                    let next_v = alert_box_next_variant(&alert_variant());
                    alert_variant.set(next_v.to_string());
                    alert_message.set(alert_box_preset_message(next_v).to_string());
                },
                onkeydown: move |evt: KeyboardEvent| {
                    if evt.key() == Key::Enter {
                        let next_v = alert_box_next_variant(&alert_variant());
                        alert_variant.set(next_v.to_string());
                        alert_message.set(alert_box_preset_message(next_v).to_string());
                    }
                },
                crate::components::forms::AlertBox {
                    message: alert_message(),
                    variant: variant_str,
                }
            }
        }
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

#[component]
fn ShowcaseGallery() -> Element {
    let mut filters = use_context::<Signal<ShowcaseFilters>>();

    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        if let Some(window) = web_sys::window() {
            crate::utils::js::invoke_optional_global(&window, "__initShowcaseMasonry", &[]);
        }
    });
    #[cfg(target_arch = "wasm32")]
    use_drop(move || {
        if let Some(window) = web_sys::window() {
            crate::utils::js::invoke_optional_global(&window, "__disposeShowcaseMasonry", &[]);
        }
    });

    let state = filters();
    let active_group = if state.category.is_empty() {
        "all"
    } else {
        state.category.as_str()
    };
    let query = state.search.trim().to_lowercase();
    let shown: Vec<&ComponentSpec> = showcase()
        .components
        .iter()
        .filter(|spec| {
            (active_group == "all" || spec.group == active_group)
                && (query.is_empty()
                    || spec.name.to_lowercase().contains(&query)
                    || spec.label.to_lowercase().contains(&query)
                    || spec.summary.to_lowercase().contains(&query))
        })
        .collect();
    let total = showcase().components.len();
    let interactive_count = showcase()
        .components
        .iter()
        .filter(|spec| spec.preview_mode == PreviewMode::Interactive)
        .count();
    // 选中项被筛掉时，详情栏落到当前结果的第一项。
    let selected = state
        .selected
        .as_deref()
        .and_then(|slug| shown.iter().find(|spec| spec.slug == slug))
        .or(shown.first())
        .map(|spec| spec.slug.clone());

    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        let return_to = { filters.read().return_to.clone() };
        #[cfg(target_arch = "wasm32")]
        if let Some(slug) = return_to {
            if let Some(target) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id(&format!("showcase-{slug}")))
            {
                target.scroll_into_view();
                filters.write().return_to = None;
            }
        }
    });

    rsx! {
        document::Title { "组件图鉴 · Yggdrasil" }
        article { class: "showcase showcase-gallery",
            Link { class: "showcase-up", to: Route::About {},
                Icon { name: IconName::ArrowLeft, size: 14 }
                "关于"
            }
            header { class: "showcase-header page-enter",
                div {
                    h1 { class: "showcase-title", "组件图鉴" }
                    p { class: "showcase-intro tabular-nums",
                        "{total} 个组件，都来自正式页面，其中 {interactive_count} 个可以直接操作。"
                    }
                }
                label { class: "showcase-search",
                    Icon { name: IconName::Search, size: 15 }
                    FormInput {
                        class: Some("showcase-search-input"),
                        r#type: "search",
                        aria_label: Some("搜索组件"),
                        placeholder: "组件名或用途",
                        value: state.search,
                        oninput: move |event: String| filters.write().search = event,
                    }
                }
            }
            nav { class: "showcase-categories page-enter", style: "--enter-index: 1", aria_label: "组件分类",
                for group in showcase().groups.iter() {
                    {
                        let id = group[0].clone();
                        let pressed = active_group == id;
                        let amount = if id == "all" {
                            total
                        } else {
                            showcase().components.iter().filter(|spec| spec.group == id).count()
                        };
                        let label = if id == "all" { "全部" } else { group[1].as_str() };
                        rsx! {
                            button {
                                key: "{id}",
                                r#type: "button",
                                class: if pressed { "chip is-active" } else { "chip" },
                                aria_pressed: "{pressed}",
                                onclick: move |_| filters.write().category = id.clone(),
                                "{label}"
                                span { class: "chip-count", "{amount}" }
                            }
                        }
                    }
                }
            }
            div { class: "showcase-catalog page-enter", style: "--enter-index: 2",
                div { class: "showcase-grid",
                    for spec in shown.iter() {
                        {
                            let slug = spec.slug.clone();
                            let card_slug = slug.clone();
                            let is_selected = selected.as_deref() == Some(slug.as_str());
                            rsx! {
                                article {
                                    class: if is_selected { "showcase-card is-selected" } else { "showcase-card" },
                                    id: "showcase-{slug}",
                                    key: "{slug}",
                                    onclick: move |event: MouseEvent| {
                                        #[cfg(not(target_arch = "wasm32"))]
                                        let _ = event;
                                        #[cfg(target_arch = "wasm32")]
                                        {
                                            let interactive = event.try_as_web_event()
                                                .and_then(|event| event.target())
                                                .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                                                .and_then(|element| element.closest(
                                                    "a, button, input, select, textarea, label, summary, [contenteditable], [role=button], [role=tab], .cm-editor, .xterm"
                                                ).ok().flatten())
                                                .is_some();
                                            if interactive {
                                                return;
                                            }
                                        }
                                        if inspector_visible() {
                                            filters.write().selected = Some(card_slug.clone());
                                        } else {
                                            navigator().push(Route::ComponentDetail { component: card_slug.clone() });
                                        }
                                    },
                                    div { class: "showcase-card-preview showcase-card-preview--{slug}",
                                        ComponentPreview { slug: slug.clone(), detail: false }
                                    }
                                    div { class: "showcase-card-caption",
                                        div { class: "showcase-card-title",
                                            h3 {
                                                Link { to: Route::ComponentDetail { component: slug.clone() }, "{spec.label}" }
                                            }
                                            code { "{spec.name}" }
                                        }
                                        p { "{spec.summary}" }
                                    }
                                }
                            }
                        }
                    }
                    if shown.is_empty() {
                        p { class: "showcase-no-results", "没有找到这个组件，换个名字或用途试试。" }
                    }
                }
                if let Some(slug) = selected {
                    ShowcaseInspector { key: "{slug}", slug }
                }
            }
        }
    }
}

/// 目录右侧的详情栏：说明、状态、属性与用法，并给出详情页入口。
#[component]
fn ShowcaseInspector(slug: String) -> Element {
    let spec = showcase()
        .components
        .iter()
        .find(|item| item.slug == slug)
        .expect("详情栏必须对应图鉴条目");
    let tokens = highlight_snippet(&spec.code);

    rsx! {
        aside { class: "showcase-inspector", aria_label: "组件详情",
            div { class: "showcase-inspector-head",
                div { class: "showcase-inspector-title",
                    h2 { "{spec.label}" }
                    code { "{spec.name}" }
                }
                p { "{spec.description}" }
                p { class: "showcase-inspector-states", "状态：{spec.states}" }
            }
            if !spec.props.is_empty() {
                section { class: "showcase-inspector-section",
                    h3 { "属性" }
                    dl { class: "showcase-inspector-props",
                        for (index, prop) in spec.props.iter().enumerate() {
                            div { key: "{index}",
                                dt {
                                    code { "{prop[0]}" }
                                    span { "{prop[1]}" }
                                }
                                dd { "{prop[3]}" }
                            }
                        }
                    }
                }
            }
            section { class: "showcase-inspector-section",
                h3 { "用法" }
                div { class: "showcase-code-panel",
                    pre {
                        code {
                            for (index, token) in tokens.iter().enumerate() {
                                span { key: "{index}", class: token.class.unwrap_or(""), "{token.text}" }
                            }
                        }
                    }
                }
                p { class: "showcase-inspector-source", "{spec.source}" }
            }
            div { class: "showcase-inspector-foot",
                Link {
                    class: button_class(ButtonVariant::Secondary, ButtonSize::Sm),
                    to: Route::ComponentDetail { component: spec.slug.clone() },
                    "打开详情页"
                    Icon { name: IconName::ArrowRight, size: 14 }
                }
            }
        }
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

#[component]
fn ShowcaseDetail(component: String) -> Element {
    let mut filters = use_context::<Signal<ShowcaseFilters>>();
    let mut checked = use_signal(|| false);
    let mut danger = use_signal(|| false);
    let mut alert_variant = use_signal(|| "success".to_string());
    let mut alert_message = use_signal(|| alert_box_preset_message("success").to_string());
    let mut reset_generation = use_signal(|| 0_u32);
    #[allow(unused_mut)]
    let mut copied = use_signal(|| false);
    #[allow(unused_mut)]
    let mut active_section = use_signal(|| "preview");

    // 目录高亮最后一个越过页头的章节；滚到底时高亮最后一节，短章节也能被选中。
    #[cfg(target_arch = "wasm32")]
    {
        use crate::hooks::event_listener::use_event_listener;
        use_event_listener(web_sys::window, "scroll", move || {
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
            if *active_section.peek() != current {
                active_section.set(current);
            }
        });
    }

    let Some((index, spec)) = showcase()
        .components
        .iter()
        .enumerate()
        .find(|(_, spec)| spec.slug == component)
    else {
        #[cfg(feature = "server")]
        dioxus::fullstack::FullstackContext::commit_http_status(
            http::StatusCode::NOT_FOUND,
            Some("Component Not Found".to_string()),
        );
        return rsx! {
            document::Title { "组件未找到 · Yggdrasil" }
            div { class: "showcase showcase-missing",
                h1 { "图鉴里没有这个组件" }
                p { "地址可能写错了，或者组件已经改名。" }
                Link {
                    class: button_class(ButtonVariant::Secondary, ButtonSize::Md),
                    to: Route::ComponentShowcase {},
                    "返回组件图鉴"
                }
            }
        };
    };
    let snippet = if spec.name == "Checkbox" {
        checkbox_snippet(checked(), danger())
    } else if spec.name == "AlertBox" {
        alert_box_snippet(&alert_variant(), &alert_message())
    } else {
        spec.code.clone()
    };
    #[cfg(target_arch = "wasm32")]
    let snippet_for_copy = snippet.clone();
    let highlighted_snippet = highlight_snippet(&snippet);
    let interactive = spec.preview_mode == PreviewMode::Interactive;
    let stage_class = match (interactive, is_wide_preview(&spec.slug)) {
        (false, true) => "showcase-live-preview is-static is-wide",
        (false, false) => "showcase-live-preview is-static",
        (true, true) => "showcase-live-preview is-wide",
        (true, false) => "showcase-live-preview",
    };
    let status = if spec.name == "Checkbox" {
        Some(format!("checked: {} · danger: {}", checked(), danger()))
    } else if spec.name == "AlertBox" {
        Some(format!(
            "variant: \"{}\" · message: \"{}\"",
            alert_variant(),
            alert_message()
        ))
    } else if is_local_demo(&spec.slug) {
        Some("本地演示，内容不会保存".to_string())
    } else {
        None
    };
    let sections: Vec<(&str, &str)> = DETAIL_SECTIONS
        .into_iter()
        .filter(|(id, _)| *id != "keys" || !spec.keys.is_empty())
        .collect();
    let previous = index.checked_sub(1).map(|i| &showcase().components[i]);
    let next = showcase().components.get(index + 1);
    let gallery_slug = spec.slug.clone();

    rsx! {
        document::Title { "{spec.label} {spec.name} · 组件图鉴 · Yggdrasil" }
        article { class: "showcase showcase-detail",
            nav { class: "breadcrumb", aria_label: "面包屑",
                Link { to: Route::About {}, "关于" }
                span { aria_hidden: "true", "/" }
                Link {
                    to: Route::ComponentShowcase {},
                    onclick: move |_| {
                        let mut filters = filters.write();
                        filters.return_to = Some(gallery_slug.clone());
                        filters.selected = Some(gallery_slug.clone());
                    },
                    "组件图鉴"
                }
                span { aria_hidden: "true", "/" }
                span { aria_current: "page", "{spec.label}" }
            }
            div { class: "showcase-detail-layout",
                div { class: "showcase-detail-main",
                    header { class: "showcase-detail-header page-enter",
                        div { class: "showcase-detail-title",
                            h1 { "{spec.label}" }
                            code { "{spec.name}" }
                            span { class: "chip", "{group_label(&spec.group)}" }
                        }
                        p { class: "showcase-detail-lead", "{spec.description}" }
                    }
                    section {
                        id: "preview",
                        class: "showcase-live-panel page-enter",
                        style: "--enter-index: 1",
                        aria_labelledby: "showcase-preview-title",
                        div { class: "showcase-live-toolbar",
                            h2 { id: "showcase-preview-title", "预览" }
                            div { class: "showcase-live-actions",
                                if spec.name == "Checkbox" {
                                    FilterTabs {
                                        items: vec![("default", "默认"), ("checked", "已选"), ("danger", "危险态")],
                                        active_value: checkbox_state(checked(), danger()).to_string(),
                                        aria_label: Some("预览状态"),
                                        class: "",
                                        on_change: move |state: String| {
                                            checked.set(state != "default");
                                            danger.set(state == "danger");
                                        },
                                    }
                                } else if spec.name == "AlertBox" {
                                    FilterTabs {
                                        items: vec![("success", "成功"), ("error", "错误"), ("default", "普通")],
                                        active_value: alert_variant(),
                                        aria_label: Some("预览状态"),
                                        class: "",
                                        on_change: move |variant: String| {
                                            alert_message.set(alert_box_preset_message(&variant).to_string());
                                            alert_variant.set(variant);
                                        },
                                    }
                                }
                                if interactive {
                                    Button {
                                        variant: ButtonVariant::Ghost,
                                        size: ButtonSize::Sm,
                                        icon: Some(IconName::RotateCcw),
                                        class: "showcase-reset".to_string(),
                                        onclick: move |_| {
                                            checked.set(false);
                                            danger.set(false);
                                            alert_variant.set("success".to_string());
                                            alert_message.set(alert_box_preset_message("success").to_string());
                                            reset_generation.set(reset_generation().wrapping_add(1));
                                        },
                                        "恢复默认"
                                    }
                                }
                            }
                        }
                        div { class: "{stage_class}",
                            for generation in std::iter::once(reset_generation()) {
                                div { key: "{spec.slug}-{generation}", class: "showcase-preview-instance",
                                    ComponentPreview {
                                        slug: spec.slug.clone(),
                                        detail: true,
                                        checked,
                                        danger,
                                        alert_variant,
                                        alert_message,
                                    }
                                }
                            }
                        }
                        if spec.name == "AlertBox" {
                            label { class: "showcase-live-field",
                                span { "message" }
                                FormInput {
                                    r#type: "text",
                                    placeholder: "输入自定义提示文案…",
                                    value: alert_message(),
                                    oninput: move |value| alert_message.set(value),
                                }
                            }
                        }
                        if let Some(status) = status {
                            div { class: "showcase-live-status", "{status}" }
                        }
                    }
                    section { id: "props", class: "showcase-detail-section", aria_labelledby: "showcase-props-title",
                        h2 { id: "showcase-props-title", "属性" }
                        if spec.props.is_empty() {
                            p { class: "showcase-detail-note", "这个组件不接收属性。" }
                        } else {
                            div { class: "showcase-props-wrap",
                                table {
                                    thead {
                                        tr {
                                            th { "名称" }
                                            th { "类型" }
                                            th { "默认值" }
                                            th { "说明" }
                                        }
                                    }
                                    tbody {
                                        for (index, prop) in spec.props.iter().enumerate() {
                                            tr { key: "{index}",
                                                td { code { "{prop[0]}" } }
                                                td { code { class: "showcase-prop-type", "{prop[1]}" } }
                                                td {
                                                    if prop[2] == "必填" {
                                                        span { class: "showcase-prop-required", "必填" }
                                                    } else {
                                                        code { class: "showcase-prop-default", "{prop[2]}" }
                                                    }
                                                }
                                                td { "{prop[3]}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !spec.keys.is_empty() {
                        section { id: "keys", class: "showcase-detail-section", aria_labelledby: "showcase-keys-title",
                            h2 { id: "showcase-keys-title", "键盘" }
                            dl { class: "showcase-key-list",
                                for (index, item) in spec.keys.iter().enumerate() {
                                    div { key: "{index}",
                                        dt { kbd { "{item[0]}" } }
                                        dd { "{item[1]}" }
                                    }
                                }
                            }
                        }
                    }
                    section { id: "usage", class: "showcase-detail-section", aria_labelledby: "showcase-usage-title",
                        div { class: "showcase-section-head",
                            h2 { id: "showcase-usage-title", "用法" }
                            code { class: "showcase-source", "{spec.source}" }
                        }
                        if spec.usage_kind == UsageKind::Api {
                            p { class: "showcase-detail-note", "下方列出组件接口；正式调用由所在页面提供数据与回调。" }
                        }
                        figure { class: "showcase-code-panel",
                            div { class: "showcase-code-toolbar",
                                span {
                                    match spec.usage_kind {
                                        UsageKind::Rust => "Dioxus RSX",
                                        UsageKind::Api => "Rust 接口",
                                        UsageKind::Router => "路由配置",
                                        UsageKind::Browser => "TypeScript",
                                    }
                                }
                                Button {
                                    variant: ButtonVariant::Ghost,
                                    size: ButtonSize::Sm,
                                    icon: Some(if copied() { IconName::Check } else { IconName::Copy }),
                                    onclick: move |_| {
                                        #[cfg(target_arch = "wasm32")]
                                        {
                                            let text = snippet_for_copy.clone();
                                            spawn(async move {
                                                if crate::utils::js::copy_text(&text).await {
                                                    copied.set(true);
                                                    crate::utils::time::sleep_ms(1600).await;
                                                    copied.set(false);
                                                }
                                            });
                                        }
                                    },
                                    if copied() { "已复制" } else { "复制" }
                                }
                            }
                            pre {
                                code {
                                    for (token_index, token) in highlighted_snippet.iter().enumerate() {
                                        span { key: "{token_index}", class: token.class.unwrap_or(""), "{token.text}" }
                                    }
                                }
                            }
                        }
                    }
                    nav { class: "showcase-neighbors", aria_label: "相邻组件",
                        if let Some(item) = previous {
                            Link { to: Route::ComponentDetail { component: item.slug.clone() },
                                span { class: "showcase-neighbor-hint", "← 上一个" }
                                span { class: "showcase-neighbor-name", "{item.label} " code { "{item.name}" } }
                            }
                        }
                        if let Some(item) = next {
                            Link { class: "is-next", to: Route::ComponentDetail { component: item.slug.clone() },
                                span { class: "showcase-neighbor-hint", "下一个 →" }
                                span { class: "showcase-neighbor-name", "{item.label} " code { "{item.name}" } }
                            }
                        }
                    }
                }
                aside { class: "showcase-toc", aria_label: "本页目录",
                    p { class: "showcase-toc-label", "本页" }
                    ol {
                        for (id, label) in sections {
                            li { key: "{id}",
                                a {
                                    href: "#{id}",
                                    class: if active_section() == id { "is-active" } else { "" },
                                    aria_current: (active_section() == id).then_some("location"),
                                    "{label}"
                                }
                            }
                        }
                    }
                    p { class: "showcase-toc-states", "状态：{spec.states}" }
                }
            }
        }
    }
}

#[component]
fn ComponentPreview(
    slug: String,
    detail: bool,
    #[props(default)] checked: Option<Signal<bool>>,
    #[props(default)] danger: Option<Signal<bool>>,
    #[props(default)] alert_variant: Option<Signal<String>>,
    #[props(default)] alert_message: Option<Signal<String>>,
) -> Element {
    let spec = showcase()
        .components
        .iter()
        .find(|item| item.slug == slug)
        .expect("预览必须对应图鉴条目");
    match preview_route(&slug).expect("图鉴条目必须有专属预览分派") {
        PreviewRoute::Live => {
            rsx! {
                LiveComponentPreview {
                    name: spec.name.clone(),
                    detail,
                    checked,
                    danger,
                    alert_variant,
                    alert_message,
                }
            }
        }
        PreviewRoute::Skeleton => rsx! { PageSkeletonPreview { name: spec.name.clone() } },
        PreviewRoute::Scene => rsx! { SceneComponentPreview { name: spec.name.clone() } },
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
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewRoute {
    Live,
    Skeleton,
    Scene,
    Business,
    LayoutArticle,
    Comment,
    Browser,
}

fn preview_route(slug: &str) -> Option<PreviewRoute> {
    Some(match slug {
        "checkbox"
        | "toggle-switch"
        | "form-select"
        | "radio"
        | "loading-button"
        | "form-input"
        | "time-picker"
        | "form-label"
        | "filter-tabs"
        | "pagination"
        | "table-pager"
        | "tooltip"
        | "popover"
        | "modal-shell"
        | "alert-box"
        | "collapsible-settings-card"
        | "status-badge"
        | "tag-chip"
        | "user-avatar"
        | "image-placeholder"
        | "icon"
        | "button"
        | "icon-button"
        | "number-input"
        | "menu"
        | "toast"
        | "bottom-sheet"
        | "breadcrumb"
        | "search-palette"
        | "empty-state"
        | "load-error"
        | "skeleton-box"
        | "skeleton-card"
        | "skeleton-table"
        | "delayed-skeleton" => PreviewRoute::Live,
        "admin-comments-table-skeleton"
        | "admin-comments-skeleton"
        | "archive-skeleton"
        | "assets-skeleton"
        | "changelog-skeleton"
        | "comment-list-skeleton"
        | "admin-dashboard-skeleton"
        | "friends-admin-list-skeleton"
        | "friends-admin-skeleton"
        | "friends-skeleton"
        | "home-skeleton"
        | "home-posts-skeleton"
        | "logs-skeleton"
        | "mcp-skeleton"
        | "post-card-skeleton"
        | "post-detail-body"
        | "post-detail-skeleton"
        | "post-preview-skeleton"
        | "posts-table-skeleton"
        | "posts-skeleton"
        | "posts-trash-table-skeleton"
        | "posts-trash-skeleton"
        | "profile-skeleton"
        | "runner-skeleton"
        | "search-skeleton"
        | "settings-admin-skeleton"
        | "system-skeleton"
        | "archive-body-skeleton"
        | "tag-detail-skeleton"
        | "write-skeleton"
        | "writing-guide-skeleton" => PreviewRoute::Skeleton,
        "post-cover" | "post-card" => PreviewRoute::Scene,
        "asset-picker-modal" | "asset-upload-modal" | "code-runner" | "sql-result-table" => {
            PreviewRoute::Business
        }
        "breadcrumbs" | "search-icon-link" | "admin-layout" | "footer" | "frontend-layout"
        | "header" | "post-header" | "post-meta" | "post-content" | "post-footer"
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
    slug == "admin-layout"
        || slug == "header"
        || slug == "search-icon-link"
        || slug == "frontend-layout"
        || slug == "footer"
        || matches!(
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

#[component]
fn LiveComponentPreview(
    name: String,
    detail: bool,
    #[props(default)] checked: Option<Signal<bool>>,
    #[props(default)] danger: Option<Signal<bool>>,
    #[props(default)] alert_variant: Option<Signal<String>>,
    #[props(default)] alert_message: Option<Signal<String>>,
) -> Element {
    let local_checked = use_signal(|| true);
    let local_danger = use_signal(|| false);
    let mut checked = checked.unwrap_or(local_checked);
    let danger = danger.unwrap_or(local_danger);
    let local_alert_variant = use_signal(|| "success".to_string());
    let local_alert_message = use_signal(|| "已保存，新的想法正在生长。".to_string());
    let alert_variant = alert_variant.unwrap_or(local_alert_variant);
    let alert_message = alert_message.unwrap_or(local_alert_message);
    let mut switch_on = use_signal(|| true);
    let mut selected_radio = use_signal(|| "public".to_string());
    let mut select_value = use_signal(|| "leaf".to_string());
    let mut time_value = use_signal(|| "08:30".to_string());
    let mut input_value = use_signal(String::new);
    let mut tab_value = use_signal(|| "all".to_string());
    let mut page = use_signal(|| 1i32);
    let mut number_value = use_signal(|| 14i64);
    let mut clicked = use_signal(|| false);
    let mut popover_open = use_signal(|| false);
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
    let mut popover_x = use_signal(|| 550i32);
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
    let mut popover_y = use_signal(|| 330i32);
    let mut modal_open = use_signal(|| false);
    let mut modal_closing = use_signal(|| false);
    let mut filter = use_signal(|| false);

    match name.as_str() {
        "Checkbox" => rsx! {
            div { class: "showcase-preview-stack",
                label { class: if danger() { "showcase-check-row danger" } else { "showcase-check-row" },
                    crate::components::ui::Checkbox { checked: checked(), danger: danger(), onchange: move |value| checked.set(value) }
                    if detail { "记住这个小小的选择" } else { "记住这个小小的选择" }
                }
                if !detail {
                    label { class: "showcase-check-row",
                        crate::components::ui::Checkbox { checked: filter(), onchange: move |value| filter.set(value) }
                        "接收新的生长消息"
                    }
                    label { class: "showcase-check-row danger",
                        crate::components::ui::Checkbox { checked: true, danger: true, onchange: move |_| {} }
                        "确认移除这条记录"
                    }
                }
            }
        },
        "Radio" => rsx! {
            div { class: "showcase-preview-stack", role: "radiogroup", aria_label: "分享范围",
                for (value, label) in [("public", "公开分享"), ("private", "仅自己可见")] {
                    label { class: "showcase-check-row",
                        crate::components::ui::Radio { name: "showcase-visibility".to_string(), value: value.to_string(), checked: selected_radio() == value, onchange: move |value| selected_radio.set(value) }
                        "{label}"
                    }
                }
                label { class: "showcase-check-row",
                    crate::components::ui::Radio { name: "showcase-visibility".to_string(), value: "unavailable".to_string(), checked: false, disabled: true, onchange: move |_| {} }
                    "暂不可用"
                }
            }
        },
        "ToggleSwitch" => rsx! {
            div { class: "showcase-preview-stack",
                div { class: "showcase-switch-row", span { "接收更新" }
                    crate::components::forms::ToggleSwitch { checked: switch_on(), ontoggle: move |_| switch_on.toggle() }
                }
                div { class: "showcase-switch-row", span { "自动备份" }
                    crate::components::forms::ToggleSwitch { checked: filter(), ontoggle: move |_| filter.toggle() }
                }
            }
        },
        "FormSelect" => rsx! {
            div { class: "showcase-preview-stack",
                div { class: "showcase-preview-field",
                    crate::components::forms::FormLabel { label: "素材排序", html_for: Some("showcase-sort".to_string()) }
                    crate::components::forms::FormSelect {
                        id: Some("showcase-sort".to_string()),
                        value: select_value(),
                        options: vec![("leaf".to_string(), "最新上传"), ("design".to_string(), "体积最大"), ("daily".to_string(), "引用最多"), ("name".to_string(), "按文件名")],
                        disabled_options: vec![("name".to_string(), "需要先重建索引")],
                        onchange: move |value| select_value.set(value),
                    }
                }
                div { class: "showcase-preview-field",
                    crate::components::forms::FormLabel { label: "主题标签（多于 8 项时可筛选）", html_for: Some("showcase-tag".to_string()) }
                    crate::components::forms::FormSelect {
                        id: Some("showcase-tag".to_string()),
                        value: tab_value(),
                        options: vec![
                            ("all".to_string(), "全部标签"), ("rust".to_string(), "Rust"), ("dioxus".to_string(), "Dioxus"),
                            ("pg".to_string(), "PostgreSQL"), ("wasm".to_string(), "WebAssembly"), ("design".to_string(), "设计"),
                            ("arch".to_string(), "架构"), ("note".to_string(), "读书笔记"), ("emu".to_string(), "模拟器"),
                            ("mcp".to_string(), "MCP"),
                        ],
                        onchange: move |value| tab_value.set(value),
                    }
                }
            }
        },
        "TimePicker" => rsx! {
            crate::components::forms::TimePicker { value: time_value(), onchange: move |value| time_value.set(value) }
        },
        "FormInput" => rsx! {
            crate::components::forms::FormInput {
                r#type: "text",
                placeholder: "一个新的开始…",
                value: input_value(),
                oninput: move |value| input_value.set(value),
            }
        },
        "FormLabel" => rsx! {
            div { class: "showcase-preview-field",
                crate::components::forms::FormLabel { label: "文章标题", html_for: Some("showcase-title".to_string()) }
                crate::components::forms::FormInput {
                    id: Some("showcase-title".to_string()), r#type: "text", placeholder: "点击标签也能聚焦这里",
                    value: input_value(), oninput: move |value| input_value.set(value),
                }
            }
        },
        "Button" => rsx! {
            div { class: "flex flex-col gap-4",
                div { class: "showcase-preview-row flex-wrap",
                    crate::components::ui::Button { onclick: move |_| clicked.set(true), if clicked() { "已发布" } else { "发布文章" } }
                    crate::components::ui::Button { variant: crate::components::ui::ButtonVariant::Secondary, icon: crate::components::ui::IconName::Save, "保存草稿" }
                    crate::components::ui::Button { variant: crate::components::ui::ButtonVariant::Outline, "预览" }
                    crate::components::ui::Button { variant: crate::components::ui::ButtonVariant::Ghost, "取消" }
                    crate::components::ui::Button { variant: crate::components::ui::ButtonVariant::Danger, "永久删除" }
                    crate::components::ui::Button { variant: crate::components::ui::ButtonVariant::Secondary, loading: true, "处理中" }
                }
                div { class: "showcase-preview-row flex-wrap items-center",
                    crate::components::ui::Button { size: crate::components::ui::ButtonSize::Xs, variant: crate::components::ui::ButtonVariant::Secondary, "XS" }
                    crate::components::ui::Button { size: crate::components::ui::ButtonSize::Sm, variant: crate::components::ui::ButtonVariant::Secondary, "SM" }
                    crate::components::ui::Button { variant: crate::components::ui::ButtonVariant::Secondary, "MD" }
                    crate::components::ui::Button { size: crate::components::ui::ButtonSize::Lg, variant: crate::components::ui::ButtonVariant::Secondary, "LG" }
                    crate::components::ui::Button { variant: crate::components::ui::ButtonVariant::DangerSoft, icon: crate::components::ui::IconName::Trash, "清空回收站" }
                }
            }
        },
        "IconButton" => rsx! {
            div { class: "showcase-preview-row",
                crate::components::ui::IconButton { icon: crate::components::ui::IconName::Copy, label: "复制链接" }
                crate::components::ui::IconButton { icon: crate::components::ui::IconName::Search, label: "搜索", shortcut: "⌘K", variant: crate::components::ui::ButtonVariant::Secondary }
                crate::components::ui::IconButton {
                    icon: crate::components::ui::IconName::PanelLeft, label: "收起侧栏",
                    pressed: Some(clicked()), onclick: move |_| clicked.set(!clicked()),
                }
                crate::components::ui::IconButton { icon: crate::components::ui::IconName::Trash, label: "删除", disabled: true }
            }
        },
        "NumberInput" => rsx! {
            div { class: "showcase-preview-field",
                crate::components::forms::FormLabel { label: "日志保留", html_for: Some("showcase-retention".to_string()) }
                crate::components::forms::NumberInput {
                    id: Some("showcase-retention".to_string()),
                    value: number_value(), min: 1, max: 90, unit: "天",
                    hint: "超过保留期的日志由后台任务自动清理。".to_string(),
                    onchange: move |v| number_value.set(v),
                }
            }
        },
        "Menu" => rsx! {
            div { class: "flex flex-col items-center gap-3",
                crate::components::ui::Menu {
                    label: "更多操作",
                    items: vec![
                        crate::components::ui::MenuItem::new("edit", "编辑").icon(crate::components::ui::IconName::Pencil),
                        crate::components::ui::MenuItem::new("copy", "复制链接").icon(crate::components::ui::IconName::Link).shortcut("⌘C"),
                        crate::components::ui::MenuItem::new("archive", "归档").icon(crate::components::ui::IconName::Archive).disabled("已发布的文章不能归档"),
                        crate::components::ui::MenuItem::new("trash", "移入回收站").icon(crate::components::ui::IconName::Trash).danger().separated(),
                    ],
                    on_select: move |id: &'static str| input_value.set(id.to_string()),
                }
                if !input_value().is_empty() {
                    p { class: "text-xs text-fg-3 font-mono", "on_select: {input_value}" }
                }
            }
        },
        "ToastRegion" => rsx! {
            ToastDemo {}
        },
        "BottomSheet" => rsx! {
            BottomSheetDemo {}
        },
        "Breadcrumb" => rsx! {
            crate::components::ui::Breadcrumb::<Route> {
                items: vec![
                    crate::components::ui::Crumb::link("关于", Route::About {}),
                    crate::components::ui::Crumb::link("组件图鉴", Route::ComponentShowcase {}),
                    crate::components::ui::Crumb::current("复选框"),
                ],
            }
        },
        "SearchPalette" => rsx! {
            SearchPaletteDemo {}
        },
        "LoadingButton" => rsx! {
            div { class: "showcase-preview-row",
                crate::components::ui::LoadingButton { label: if clicked() { "已保存" } else { "保存设置" }, loading: false, onclick: move |_| clicked.set(true) }
                crate::components::ui::LoadingButton { label: "处理中", loading: true, onclick: move |_| {} }
            }
        },
        "FilterTabs" => rsx! {
            crate::components::ui::FilterTabs {
                items: vec![("all", "全部"), ("published", "已发布"), ("draft", "草稿")],
                active_value: tab_value(), on_change: move |value| tab_value.set(value),
            }
        },
        "Pagination" => rsx! {
            crate::components::ui::Pagination::<Route> {
                variant: "admin", current_page: page(), total: 80, per_page: 10, unit: "篇",
                on_prev: move |_| page.set((page() - 1).max(1)),
                on_next: move |_| page.set((page() + 1).min(8)),
                compact: true,
            }
        },
        "TablePager" => rsx! {
            div { class: "admin-panel w-full max-w-md",
                crate::components::ui::TablePager {
                    page: page(), total: 57, per_page: 20, unit: "篇",
                    on_change: move |p| page.set(p),
                }
            }
        },
        "Tooltip" => rsx! {
            div { class: "showcase-preview-row",
                crate::components::ui::Tooltip { tip: "复制链接",
                    button { class: "btn btn-icon btn-secondary", aria_label: "复制链接",
                        crate::components::ui::Icon { name: crate::components::ui::IconName::Copy }
                    }
                }
                crate::components::ui::Tooltip { tip: "搜索", shortcut: "⌘K",
                    button { class: "btn btn-icon btn-secondary", aria_label: "搜索",
                        crate::components::ui::Icon { name: crate::components::ui::IconName::Search }
                    }
                }
                crate::components::ui::Tooltip { tip: "向右旋转", shortcut: "R", placement: "bottom",
                    button { class: "btn btn-icon btn-secondary", aria_label: "向右旋转",
                        crate::components::ui::Icon { name: crate::components::ui::IconName::RotateCw }
                    }
                }
            }
        },
        "Popover" => rsx! {
            div { class: "showcase-popover-example",
                button { class: "showcase-demo-button secondary", onclick: move |event: MouseEvent| {
                    event.stop_propagation();
                    #[cfg(target_arch = "wasm32")]
                    {
                        let coords = event.client_coordinates();
                        let offset = event.element_coordinates();
                        popover_x.set(coords.x as i32);
                        popover_y.set((coords.y - offset.y) as i32);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = event;
                    popover_open.toggle();
                }, "试试浮层 ↗" }
                crate::components::ui::Popover {
                    open: popover_open(), anchor_x: popover_x(), anchor_y: popover_y(), placement: "top", align: "center",
                    on_close: move |_| popover_open.set(false),
                    div { class: "showcase-popover-content",
                        p { "要保留这片小小的灵感吗？" }
                        div { class: "showcase-popover-actions",
                            button { class: "showcase-demo-button secondary", onclick: move |_| popover_open.set(false), "取消" }
                            button { class: "showcase-demo-button", onclick: move |_| popover_open.set(false), "保留" }
                        }
                    }
                }
            }
        },
        "ModalShell" => rsx! {
            div {
                button { class: "showcase-demo-button secondary", onclick: move |_| modal_open.set(true), "打开一扇小窗 ↗" }
                crate::components::ui::ModalShell {
                    visible: modal_open, closing: modal_closing, title: "留住一片灵感",
                    div { class: "showcase-demo-modal-content",
                        h2 { "留住一片灵感。" }
                        p { "让内容拥有一点专注的空间。" }
                        button { class: "showcase-demo-button", onclick: move |_| { modal_closing.set(true); modal_open.set(false); }, "收好了" }
                    }
                }
            }
        },
        "AlertBox" => rsx! {
            AlertBoxDemo {
                detail,
                alert_variant,
                alert_message,
            }
        },
        "CollapsibleSettingsCard" => rsx! {
            crate::components::ui::CollapsibleSettingsCard {
                title: "自动备份", summary: "每天 08:30 留下一份记录", enabled: true,
                div { class: "showcase-collapse-content", "这里放置详细设置。" }
            }
        },
        "StatusBadge" => rsx! {
            div { class: "showcase-preview-row",
                crate::components::ui::StatusBadge { color_class: "badge-success", label: "已发布" }
                crate::components::ui::StatusBadge { color_class: "", label: "草稿" }
                crate::components::ui::StatusBadge { color_class: "badge-warning", label: "待审核" }
                crate::components::ui::StatusBadge { color_class: "badge-danger", label: "3 天后清除" }
                crate::components::ui::StatusBadge { color_class: "badge-clay", label: "新" }
            }
        },
        "TagChip" => rsx! {
            div { class: "showcase-preview-row",
                crate::components::ui::TagChip { label: "设计", to: Route::TagDetail { tag: "设计".to_string() }, variant: "outline", count: 12 }
                crate::components::ui::TagChip { label: "灵感", to: Route::TagDetail { tag: "灵感".to_string() }, variant: "solid" }
            }
        },
        "UserAvatar" => rsx! {
            div { class: "showcase-preview-row",
                crate::components::ui::UserAvatar { name: "叶子", avatar_url: None, class: "w-14 h-14 rounded-full text-xl" }
                crate::components::ui::UserAvatar { name: "Yggdrasil", avatar_url: Some("/missing-avatar-for-showcase.png".to_string()), class: "w-10 h-10 rounded-full text-sm" }
            }
        },
        "ImagePlaceholder" => rsx! {
            crate::components::ui::ImagePlaceholder { class: "w-10 h-10 text-fg-3" }
        },
        "Icon" => rsx! {
            div { class: "grid grid-cols-8 gap-3 text-paper-secondary",
                for icon in crate::components::ui::IconName::ALL {
                    span { class: "grid place-items-center", title: icon.slug(),
                        crate::components::ui::Icon { name: icon, size: 18 }
                    }
                }
            }
        },
        "EmptyState" => rsx! {
            div { class: "showcase-empty-preview",
                crate::components::empty_state::EmptyState { title: "这里，等待第一颗种子。", description: "写下一个新的开始。" }
            }
        },
        "LoadError" => rsx! {
            div { class: "w-full max-w-xl",
                crate::components::ui::LoadError {
                    title: "文章暂时没能加载",
                    retrying: clicked(),
                    on_retry: move |_| clicked.set(true),
                }
            }
        },
        "SkeletonBox" => rsx! {
            div { class: "showcase-preview-stack showcase-skeleton-lines",
                crate::components::skeletons::atoms::SkeletonBox { class: "h-3 w-52 rounded" }
                crate::components::skeletons::atoms::SkeletonBox { class: "h-3 w-36 rounded" }
                crate::components::skeletons::atoms::SkeletonBox { class: "h-3 w-44 rounded" }
            }
        },
        "SkeletonCard" => rsx! {
            crate::components::skeletons::atoms::SkeletonCard { class: Some("p-5 space-y-3 w-56"),
                crate::components::skeletons::atoms::SkeletonBox { class: "w-9 h-9 rounded-full" }
                crate::components::skeletons::atoms::SkeletonBox { class: "w-full h-3 rounded" }
                crate::components::skeletons::atoms::SkeletonBox { class: "w-2/3 h-3 rounded" }
            }
        },
        "SkeletonTable" => rsx! {
            div { class: "showcase-table-preview",
                crate::components::skeletons::atoms::SkeletonTable {
                    columns: vec![
                        crate::components::skeletons::atoms::SkeletonTableColumn { header_class: "px-3 py-2", header_box_class: "w-10 h-3 rounded", cell_class: "px-3 py-2", cell_shape: crate::components::skeletons::atoms::SkeletonCellShape::Single("w-12 h-3 rounded") },
                        crate::components::skeletons::atoms::SkeletonTableColumn { header_class: "px-3 py-2", header_box_class: "w-20 h-3 rounded", cell_class: "px-3 py-2", cell_shape: crate::components::skeletons::atoms::SkeletonCellShape::Single("w-24 h-3 rounded") },
                    ], rows: 2,
                }
            }
        },
        "DelayedSkeleton" => rsx! {
            div { class: "showcase-preview-stack showcase-skeleton-lines",
                crate::components::skeletons::delayed_skeleton::DelayedSkeleton {
                    crate::components::skeletons::atoms::SkeletonBox { class: "h-3 w-48 rounded" }
                }
            }
        },
        _ => panic!("基础组件缺少预览: {name}"),
    }
}

/// 页面骨架可以脱离数据源安全渲染，图鉴因此直接展示原组件。
#[component]
fn PageSkeletonPreview(name: String) -> Element {
    let skeleton = match name.as_str() {
        "AdminCommentsTableSkeleton" => {
            rsx! { crate::components::skeletons::admin_comments_skeleton::AdminCommentsTableSkeleton {} }
        }
        "AdminCommentsSkeleton" => {
            rsx! { crate::components::skeletons::admin_comments_skeleton::AdminCommentsSkeleton {} }
        }
        "ArchiveSkeleton" => {
            rsx! { crate::components::skeletons::archive_skeleton::ArchiveSkeleton {} }
        }
        "AssetsSkeleton" => {
            rsx! { crate::components::skeletons::assets_skeleton::AssetsSkeleton {} }
        }
        "ChangelogSkeleton" => {
            rsx! { crate::components::skeletons::changelog_skeleton::ChangelogSkeleton {} }
        }
        "CommentListSkeleton" => {
            rsx! { crate::components::skeletons::comment_skeleton::CommentListSkeleton {} }
        }
        "AdminDashboardSkeleton" => {
            rsx! { crate::components::skeletons::dashboard_skeleton::AdminDashboardSkeleton {} }
        }
        "FriendsAdminListSkeleton" => {
            rsx! { crate::components::skeletons::friends_admin_skeleton::FriendsAdminListSkeleton {} }
        }
        "FriendsAdminSkeleton" => {
            rsx! { crate::components::skeletons::friends_admin_skeleton::FriendsAdminSkeleton {} }
        }
        "FriendsSkeleton" => {
            rsx! { crate::components::skeletons::friends_skeleton::FriendsSkeleton {} }
        }
        "HomeSkeleton" => rsx! { crate::components::skeletons::home_skeleton::HomeSkeleton {} },
        "HomePostsSkeleton" => {
            rsx! { crate::components::skeletons::home_skeleton::HomePostsSkeleton { current_page: 1 } }
        }
        "LogsSkeleton" => rsx! { crate::components::skeletons::logs_skeleton::LogsSkeleton {} },
        "McpSkeleton" => rsx! { crate::components::skeletons::mcp_skeleton::McpSkeleton {} },
        "PostCardSkeleton" => {
            rsx! { crate::components::skeletons::post_card_skeleton::PostCardSkeleton { has_cover: true } }
        }
        "PostDetailBody" => {
            rsx! { crate::components::skeletons::post_detail_skeleton::PostDetailBody {} }
        }
        "PostDetailSkeleton" => {
            rsx! { crate::components::skeletons::post_detail_skeleton::PostDetailSkeleton {} }
        }
        "PostPreviewSkeleton" => {
            rsx! { crate::components::skeletons::post_preview_skeleton::PostPreviewSkeleton {} }
        }
        "PostsTrashTableSkeleton" => {
            rsx! { crate::components::skeletons::posts_trash_skeleton::PostsTrashTableSkeleton {} }
        }
        "PostsTrashSkeleton" => {
            rsx! { crate::components::skeletons::posts_trash_skeleton::PostsTrashSkeleton {} }
        }
        "PostsTableSkeleton" => {
            rsx! { crate::components::skeletons::posts_skeleton::PostsTableSkeleton {} }
        }
        "PostsSkeleton" => rsx! { crate::components::skeletons::posts_skeleton::PostsSkeleton {} },
        "ProfileSkeleton" => {
            rsx! { crate::components::skeletons::profile_skeleton::ProfileSkeleton {} }
        }
        "RunnerSkeleton" => {
            rsx! { crate::components::skeletons::runner_skeleton::RunnerSkeleton {} }
        }
        "SearchSkeleton" => {
            rsx! { crate::components::skeletons::search_skeleton::SearchSkeleton {} }
        }
        "SettingsAdminSkeleton" => {
            rsx! { crate::components::skeletons::settings_admin_skeleton::SettingsAdminSkeleton {} }
        }
        "SystemSkeleton" => {
            rsx! { crate::components::skeletons::system_skeleton::SystemSkeleton {} }
        }
        "TagDetailSkeleton" => {
            rsx! { crate::components::skeletons::archive_skeleton::TagDetailSkeleton { tag: "设计".to_string() } }
        }
        "ArchiveBodySkeleton" => {
            rsx! { div { class: "archive", crate::components::skeletons::archive_skeleton::ArchiveBodySkeleton {} } }
        }
        "WriteSkeleton" => rsx! { crate::components::skeletons::write_skeleton::WriteSkeleton {} },
        "WritingGuideSkeleton" => {
            rsx! { crate::components::skeletons::writing_guide_skeleton::WritingGuideSkeleton {} }
        }
        _ => panic!("页面骨架缺少预览: {name}"),
    };
    rsx! { div { class: "showcase-skeleton-window",
        div { class: "showcase-skeleton-window-bar", span {} span {} span {} }
        div { class: "showcase-skeleton-scale", {skeleton} }
    } }
}

fn sample_post() -> Post {
    let created_at = DateTime::parse_from_rfc3339("2026-09-23T08:00:00Z")
        .expect("固定的图鉴示例日期必须有效")
        .with_timezone(&Utc);
    Post {
        id: 0,
        author_id: 0,
        title: "让想法生根".to_string(),
        slug: "showcase-example".to_string(),
        summary: Some("在时间的缝隙里，种下一些文字。".to_string()),
        content_md: "一片叶子。".to_string(),
        content_html: Some("<p>一片叶子，也记得来时的风。</p>".to_string()),
        status: PostStatus::Published,
        published_at: Some(created_at),
        created_at,
        updated_at: created_at,
        deleted_at: None,
        tags: vec!["设计".to_string()],
        cover_image: None,
        reading_time: 3,
        word_count: 620,
        toc_html: None,
        prev_post: None,
        next_post: None,
    }
}

/// 不请求服务、不修改数据的业务组件使用固定样例资料展示真实结构。
#[component]
fn SceneComponentPreview(name: String) -> Element {
    let cover_preview = name == "PostCover";
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        if !cover_preview {
            return;
        }
        if let Some(window) = web_sys::window() {
            let selectors = js_sys::Array::new();
            selectors.push(&".showcase-cover-window".into());
            let selectors = js_sys::Object::from(selectors).into();
            crate::utils::js::invoke_optional_global(&window, "__initLightbox", &[selectors]);
        }
    });
    let post = sample_post();
    let inner = match name.as_str() {
        "PostCard" => rsx! {
            crate::components::post_card::PostCard {
                post: PostListItem {
                    id: post.id,
                    author_id: post.author_id,
                    title: post.title.clone(),
                    slug: post.slug.clone(),
                    summary: post.summary.clone(),
                    status: post.status.clone(),
                    published_at: post.published_at,
                    created_at: post.created_at,
                    updated_at: post.updated_at,
                    deleted_at: None,
                    tags: post.tags.clone(),
                    cover_image: None,
                    reading_time: post.reading_time,
                    word_count: post.word_count,
                },
            }
        },
        "PostCover" => {
            rsx! { crate::components::post::post_cover::PostCover { src: "/images/samples/paper-clay.webp".to_string() } }
        }
        "SearchIconLink" => rsx! { crate::components::header::SearchIconLink {} },
        _ => panic!("场景组件缺少预览: {name}"),
    };
    rsx! {
        div { class: if cover_preview { "showcase-scene-window showcase-cover-window" } else { "showcase-scene-window" }, aria_label: "{name} 的静态样例",
            div { class: "showcase-skeleton-window-bar", span {} span {} span {} }
            div { class: "showcase-scene-canvas", inert: "true", {inner} }
        }
    }
}

/// 搜索面板演示：自带句柄与面板实例，不注册全局快捷键，不影响页面上的正式面板。
#[component]
fn SearchPaletteDemo() -> Element {
    let palette = crate::components::search_palette::use_search_palette_provider();
    rsx! {
        div { class: "showcase-preview-row",
            crate::components::ui::Button {
                variant: crate::components::ui::ButtonVariant::Secondary,
                icon: crate::components::ui::IconName::Search,
                onclick: move |_| palette.open(),
                "打开搜索面板"
            }
        }
        crate::components::search_palette::SearchPalette { shortcut: false }
    }
}

/// 提示演示：三种语气各发一条，走正式的全站提示区。
#[component]
fn ToastDemo() -> Element {
    let toast = crate::components::ui::use_toast();
    rsx! {
        div { class: "showcase-preview-row flex-wrap",
            crate::components::ui::Button {
                variant: crate::components::ui::ButtonVariant::Secondary,
                onclick: move |_| toast.show(crate::components::ui::ToastTone::Info, "草稿已自动保存"),
                "说明"
            }
            crate::components::ui::Button {
                variant: crate::components::ui::ButtonVariant::Secondary,
                onclick: move |_| toast.success("设置已保存"),
                "成功"
            }
            crate::components::ui::Button {
                variant: crate::components::ui::ButtonVariant::Secondary,
                onclick: move |_| toast.error("保存失败：网络连接中断，请稍后重试"),
                "错误"
            }
        }
    }
}

/// 图鉴里的抽屉只放演示用的按钮，选中后关闭，不跳转。
#[component]
fn BottomSheetDemo() -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        button {
            id: "showcase-sheet-trigger",
            class: "btn btn-secondary",
            r#type: "button",
            aria_haspopup: "dialog",
            aria_expanded: "{open}",
            aria_controls: "showcase-bottom-sheet",
            onclick: move |_| open.set(true),
            "打开抽屉"
        }
        crate::components::ui::BottomSheet {
            open,
            id: "showcase-bottom-sheet",
            trigger_id: "showcase-sheet-trigger",
            label: "演示抽屉",
            BottomSheetDemoBody {}
        }
    }
}

#[component]
fn BottomSheetDemoBody() -> Element {
    let sheet = crate::components::ui::use_sheet_close();
    rsx! {
        ul { class: "flex flex-col gap-0.5",
            for label in ["设计系统", "Dioxus 实践", "Rust 异步"] {
                li { key: "{label}",
                    button { class: "sheet-nav-item", r#type: "button", onclick: move |_| sheet.close(), "{label}" }
                }
            }
        }
    }
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
            .filter(|name| !catalog.contains(name.as_str()))
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
