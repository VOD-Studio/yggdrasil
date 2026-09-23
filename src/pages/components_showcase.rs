//! 项目自写组件的公开图鉴与独立用法页。
//!
//! 目录、文案和属性说明来自本模块的静态数据；预览直接渲染项目里的组件。
use std::sync::LazyLock;

use chrono::{DateTime, Utc};
#[cfg(target_arch = "wasm32")]
use dioxus::html::InteractionLocation;
use dioxus::prelude::*;
use serde::Deserialize;

use crate::models::post::{Post, PostListItem, PostNav, PostStatus};
use crate::router::Route;

#[derive(Clone, Default, PartialEq)]
pub struct ShowcaseFilters {
    pub category: String,
    pub search: String,
    pub return_to: Option<String>,
}

#[derive(Deserialize)]
struct ShowcaseData {
    groups: Vec<[String; 2]>,
    components: Vec<ComponentSpec>,
}

#[derive(Deserialize)]
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
    #[serde(default)]
    kind: String,
}

static SHOWCASE: LazyLock<ShowcaseData> = LazyLock::new(|| {
    serde_json::from_str(include_str!("components_showcase_data.json"))
        .expect("组件图鉴数据必须是有效 JSON")
});

fn group_label(group: &str) -> &str {
    SHOWCASE
        .groups
        .iter()
        .find(|item| item[0] == group)
        .map(|item| item[1].as_str())
        .unwrap_or("全部组件")
}

#[component]
pub fn ComponentShowcase() -> Element {
    let mut filters = use_context::<Signal<ShowcaseFilters>>();
    let state = filters();
    let active_group = if state.category.is_empty() {
        "all"
    } else {
        state.category.as_str()
    };
    let query = state.search.trim().to_lowercase();
    let shown: Vec<&ComponentSpec> = SHOWCASE
        .components
        .iter()
        .filter(|spec| {
            (active_group == "all" || spec.group == active_group)
                && (query.is_empty()
                    || spec.name.to_lowercase().contains(&query)
                    || spec.label.to_lowercase().contains(&query))
        })
        .collect();
    let count = shown.len();

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
            nav { class: "showcase-breadcrumb", aria_label: "面包屑",
                Link { to: Route::About {}, "← 关于这里" }
                span { aria_hidden: "true", "/" }
                span { aria_current: "page", "组件图鉴" }
            }
            header { class: "showcase-hero",
                div {
                    p { class: "showcase-eyebrow", span { class: "showcase-dot" } "YGGDRASIL / INTERFACE ATLAS" }
                    h1 { "组件图鉴" small { "↗ 01—{SHOWCASE.components.len()}" } }
                    p { class: "showcase-lead", "一枚按钮，一次勾选，一点恰好的回应。" br {} "这里收藏着，让 Yggdrasil 长成自己的界面细节。" }
                    div { class: "showcase-hero-meta",
                        span { "24 枚基础组件" }
                        span { "59 枚场景组件" }
                        span { "可以触碰的设计" }
                    }
                }
                ShowcaseStillLife {}
            }
            div { class: "showcase-intro-line",
                p { "好的界面，藏在每一个小小的细节里。" }
                a { href: "#showcase-collection", "开始翻阅 ↓" }
            }
            div { class: "showcase-catalog", id: "showcase-collection",
                aside { class: "showcase-sidebar",
                    p { class: "showcase-sidebar-label", "沿着目录，慢慢翻阅" }
                    label { class: "showcase-search",
                        span { aria_hidden: "true", "⌕" }
                        input {
                            r#type: "search",
                            aria_label: "搜索组件",
                            placeholder: "搜索组件…",
                            value: state.search,
                            oninput: move |event| filters.write().search = event.value(),
                        }
                    }
                    nav { class: "showcase-categories", aria_label: "组件分类",
                        for group in SHOWCASE.groups.iter() {
                            {
                                let id = group[0].clone();
                                let label = group[1].clone();
                                let selected = active_group == id;
                                let amount = if id == "all" { SHOWCASE.components.len() } else {
                                    SHOWCASE.components.iter().filter(|spec| spec.group == id).count()
                                };
                                rsx! {
                                    button {
                                        key: "{id}",
                                        r#type: "button",
                                        class: if selected { "is-active" } else { "" },
                                        aria_pressed: "{selected}",
                                        onclick: move |_| filters.write().category = id.clone(),
                                        "{label}" small { "{amount:02}" }
                                    }
                                }
                            }
                        }
                    }
                    p { class: "showcase-sidebar-note", "设计，发生在使用之中。" br {} "试着勾选、切换或展开。" br {} "每个细节，都有自己的回应。" }
                }
                div { class: "showcase-collection",
                    div { class: "showcase-collection-heading",
                        h2 { "{group_label(active_group)}" }
                        small { "{count:02} SPECIMENS" }
                    }
                    div { class: "showcase-grid",
                        for spec in shown.iter() {
                            {
                                let slug = spec.slug.clone();
                                let name = spec.name.clone();
                                rsx! {
                                    article { class: "showcase-card", id: "showcase-{slug}", key: "{slug}",
                                        div { class: "showcase-card-preview",
                                            span { class: "showcase-card-index", "{spec.name.to_uppercase()} / {group_label(&spec.group)}" }
                                            ComponentPreview { name: name.clone(), detail: false }
                                        }
                                        div { class: "showcase-card-caption",
                                            div { class: "showcase-card-title", h3 { "{spec.label}" } code { "{spec.name}" } }
                                            p { "{spec.summary}" }
                                            div { class: "showcase-card-foot",
                                                span { "{spec.states}" }
                                                Link { to: Route::ComponentDetail { component: slug.clone() }, "查看用法 ↗" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if count == 0 {
                            p { class: "showcase-no-results", "没有找到这个组件，换个名字试试。" }
                        }
                    }
                    div { class: "showcase-composition",
                        div { h3 { "当枝叶，组成风景。" } p { "素材、文章、评论与编辑器，组成更完整的界面。" } }
                        span { "素材选择 · 文章卡片 · 评论区 · 页面骨架" }
                    }
                }
            }
            footer { class: "showcase-closing", em { "细节有回应，界面有温度。" } span { "YGGDRASIL · A COLLECTION OF SMALL DETAILS" } }
        }
    }
}

#[component]
pub fn ComponentDetail(component: String) -> Element {
    let mut filters = use_context::<Signal<ShowcaseFilters>>();
    let mut checked = use_signal(|| true);
    let mut danger = use_signal(|| false);
    #[allow(unused_mut)]
    let mut copied = use_signal(|| false);
    let Some((index, spec)) = SHOWCASE
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
                h1 { "这枚组件尚未收录。" }
                Link { to: Route::ComponentShowcase {}, "返回组件图鉴 →" }
            }
        };
    };
    let snippet = if spec.name == "Checkbox" {
        format!(
            "let mut checked = use_signal(|| {});\n\nrsx! {{\n    label {{\n        Checkbox {{\n            checked: checked(),\n            onchange: move |value| checked.set(value),{}\n        }}\n        \"记住这个小小的选择\"\n    }}\n}}",
            checked(),
            if danger() { "\n            danger: true," } else { "" },
        )
    } else {
        spec.code.clone()
    };
    #[cfg(target_arch = "wasm32")]
    let snippet_for_copy = snippet.clone();
    let previous = index.checked_sub(1).map(|i| &SHOWCASE.components[i]);
    let next = SHOWCASE.components.get(index + 1);

    rsx! {
        document::Title { "{spec.label} {spec.name} · 组件图鉴 · Yggdrasil" }
        article { class: "showcase showcase-detail",
            nav { class: "showcase-breadcrumb", aria_label: "面包屑",
                Link { to: Route::About {}, "关于这里" }
                span { aria_hidden: "true", "/" }
                Link { to: Route::ComponentShowcase {}, onclick: move |_| filters.write().return_to = Some(component.clone()), "组件图鉴" }
                span { aria_hidden: "true", "/" }
                span { aria_current: "page", "{spec.label}" }
            }
            div { class: "showcase-detail-layout",
                aside { class: "showcase-detail-sidebar",
                    Link { class: "showcase-back", to: Route::ComponentShowcase {}, onclick: move |_| filters.write().return_to = Some(spec.slug.clone()), "← 返回组件图鉴" }
                    nav { aria_label: "组件目录",
                        for group in SHOWCASE.groups.iter().skip(1) {
                            section {
                                h2 { "{group[1]}" }
                                for item in SHOWCASE.components.iter().filter(|item| item.group == group[0]) {
                                    Link {
                                        key: "{item.slug}",
                                        class: if item.slug == spec.slug { "is-current" } else { "" },
                                        aria_current: if item.slug == spec.slug { "page" } else { "false" },
                                        to: Route::ComponentDetail { component: item.slug.clone() },
                                        "{item.name}"
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "showcase-detail-content",
                    label { class: "showcase-mobile-select",
                        "组件目录"
                        select {
                            aria_label: "切换组件",
                            value: "{spec.slug}",
                            onchange: move |event| { navigator().push(Route::ComponentDetail { component: event.value() }); },
                            for item in SHOWCASE.components.iter() {
                                option { value: "{item.slug}", "{item.name} · {item.label}" }
                            }
                        }
                    }
                    header { class: "showcase-detail-heading",
                        div {
                            p { class: "showcase-eyebrow", span { class: "showcase-dot" } "{group_label(&spec.group)} / COMPONENT ATLAS" }
                            h1 { "{spec.label}" code { "{spec.name}" } }
                            p { "{spec.description}" }
                        }
                        span { class: "showcase-detail-number", aria_hidden: "true", "{index + 1:02}" }
                    }
                    nav { class: "showcase-detail-tabs", aria_label: "本页章节",
                        a { href: "#preview", "交互预览" }
                        a { href: "#usage", "用法" }
                        a { href: "#properties", "属性" }
                        if !spec.keys.is_empty() { a { href: "#keyboard", "操作方式" } }
                    }
                    section { class: "showcase-detail-section", id: "preview",
                        div { class: "showcase-section-heading", h2 { "亲手试一试。" } span { "01 / LIVE PREVIEW" } }
                        div { class: "showcase-live-panel",
                            div { class: "showcase-live-toolbar", span { "● {spec.name}" }
                                button { r#type: "button", onclick: move |_| { checked.set(true); danger.set(false); }, "↻ 恢复默认" }
                            }
                            div { class: "showcase-live-workarea",
                                div { class: "showcase-live-preview",
                                    ComponentPreview { name: spec.name.clone(), detail: true, checked, danger }
                                }
                                div { class: "showcase-live-controls",
                                    if spec.name == "Checkbox" {
                                        p { "PROPERTIES / 调整属性" }
                                        div { class: "showcase-property-row", span { "checked" }
                                            crate::components::forms::ToggleSwitch { checked: checked(), aria_label: Some("设置 checked".to_string()), ontoggle: move |_| checked.toggle() }
                                        }
                                        div { class: "showcase-property-row", span { "danger" }
                                            crate::components::forms::ToggleSwitch { checked: danger(), aria_label: Some("设置 danger".to_string()), ontoggle: move |_| danger.toggle() }
                                        }
                                        small { "调整属性，看看细节如何回应。" }
                                    } else {
                                        p { if spec.kind == "reference" { "CONTEXT / 使用场景" } else { "STATES / 示例状态" } }
                                        div { class: "showcase-state-chips",
                                            for state in spec.states.split(" · ") { span { "{state}" } }
                                        }
                                        small { if spec.kind == "reference" { "场景组件需要页面数据；这里展示其用途和接口。" } else { "在左侧预览中体验这枚组件。" } }
                                    }
                                }
                            }
                            div { class: "showcase-live-status", if spec.name == "Checkbox" { "checked: {checked()}　 danger: {danger()}" } else if spec.kind == "reference" { "COMPONENT REFERENCE · {spec.source}" } else { "INTERACTIVE SPECIMEN · 跟随全站主题" } }
                        }
                    }
                    section { class: "showcase-detail-section", id: "usage",
                        div { class: "showcase-section-heading", h2 { "用法" } span { "02 / USAGE" } }
                        p { class: "showcase-description", if spec.kind == "reference" { "下方展示组件接口；完整场景需要业务数据或页面上下文。" } else { "组件的调用片段；状态与回调由父组件提供。" } }
                        div { class: "showcase-code-panel",
                            div { class: "showcase-code-toolbar",
                                span { if spec.kind == "reference" { if spec.group == "browser" { "TYPESCRIPT · 模块入口" } else { "RUST · 组件接口" } } else { "RUST · DIOXUS RSX" } }
                                button {
                                    r#type: "button",
                                    aria_label: "复制代码",
                                    onclick: move |_| {
                                        #[cfg(target_arch = "wasm32")]
                                        let text = snippet_for_copy.clone();
                                        #[cfg(target_arch = "wasm32")]
                                        spawn(async move {
                                            if let Some(window) = web_sys::window() {
                                                let clipboard = window.navigator().clipboard();
                                                if wasm_bindgen_futures::JsFuture::from(clipboard.write_text(&text)).await.is_ok() {
                                                    copied.set(true);
                                                }
                                            }
                                        });
                                    },
                                    if copied() { "✓ 已复制" } else { "复制代码 ↗" }
                                }
                            }
                            pre { code {
                                if spec.name == "Checkbox" {
                                    span { class: "showcase-code-keyword", "let mut" }
                                    " checked = "
                                    span { class: "showcase-code-function", "use_signal" }
                                    "(|| "
                                    span { class: "showcase-code-literal", "{checked()}" }
                                    ");\n\n"
                                    span { class: "showcase-code-function", "rsx!" }
                                    " {{\n    "
                                    span { class: "showcase-code-keyword", "label" }
                                    " {{\n        "
                                    span { class: "showcase-code-type", "Checkbox" }
                                    " {{\n            "
                                    span { class: "showcase-code-property", "checked" }
                                    ": checked(),\n            "
                                    span { class: "showcase-code-property", "onchange" }
                                    ": "
                                    span { class: "showcase-code-keyword", "move" }
                                    " |value| checked.set(value),"
                                    if danger() {
                                        "\n            "
                                        span { class: "showcase-code-property", "danger" }
                                        ": "
                                        span { class: "showcase-code-literal", "true" }
                                        ","
                                    }
                                    "\n        }}\n        "
                                    span { class: "showcase-code-string", "\"记住这个小小的选择\"" }
                                    "\n    }}\n}}"
                                } else {
                                    "{snippet}"
                                }
                            } }
                        }
                    }
                    section { class: "showcase-detail-section", id: "properties",
                        div { class: "showcase-section-heading", h2 { "主要属性" } span { "03 / PROPERTIES" } }
                        div { class: "showcase-props-wrap",
                            table { thead { tr { th { "属性 / 类型" } th { "默认值" } th { "说明" } } }
                                tbody {
                                    for prop in spec.props.iter() {
                                        tr { td { code { "{prop[0]}" } small { "{prop[1]}" } } td { "{prop[2]}" } td { "{prop[3]}" } }
                                    }
                                }
                            }
                        }
                    }
                    if !spec.keys.is_empty() {
                        section { class: "showcase-detail-section", id: "keyboard",
                            div { class: "showcase-section-heading", h2 { "操作方式" } span { "04 / INTERACTION" } }
                            div { class: "showcase-key-guide",
                                for item in spec.keys.iter() { span { kbd { "{item[0]}" } "{item[1]}" } }
                            }
                        }
                    }
                    nav { class: "showcase-neighbors", aria_label: "相邻组件",
                        if let Some(item) = previous {
                            Link { to: Route::ComponentDetail { component: item.slug.clone() }, span { "←" } div { small { "上一枚组件" } strong { "{item.label} · {item.name}" } } }
                        } else {
                            Link { to: Route::ComponentShowcase {}, span { "←" } div { small { "全部收藏" } strong { "返回组件图鉴" } } }
                        }
                        if let Some(item) = next {
                            Link { to: Route::ComponentDetail { component: item.slug.clone() }, div { small { "下一枚组件" } strong { "{item.label} · {item.name}" } } span { "→" } }
                        } else {
                            Link { to: Route::ComponentShowcase {}, div { small { "继续翻阅" } strong { "返回组件图鉴" } } span { "→" } }
                        }
                    }
                    footer { class: "showcase-detail-colophon", span { "YGGDRASIL · 每一个细节，都有回应。" } code { "{spec.source}" } }
                }
            }
        }
    }
}

#[component]
fn ShowcaseStillLife() -> Element {
    rsx! {
        div { class: "showcase-still-life", aria_hidden: "true",
            div { class: "showcase-orbit" }
            div { class: "showcase-specimen",
                span { "A SMALL INTERACTION" }
                strong { "让细节，轻轻生长。" }
                div { "一点恰好的回应" i { class: "showcase-specimen-switch" } }
            }
            span { class: "showcase-float-mark", "✓ 刚刚好" }
            span { class: "showcase-float-code", "Checkbox {{ checked: true }}" }
            span { class: "showcase-life-caption", "FORM · FEEDBACK · FEELING" }
        }
    }
}

#[component]
fn ComponentPreview(
    name: String,
    detail: bool,
    #[props(default)] checked: Option<Signal<bool>>,
    #[props(default)] danger: Option<Signal<bool>>,
) -> Element {
    if let Some(spec) = SHOWCASE.components.iter().find(|item| item.name == name) {
        if spec.kind == "reference" {
            if spec.group == "page-loading" {
                return rsx! { PageSkeletonPreview { name: name.clone() } };
            }
            if matches!(
                name.as_str(),
                "PostCard"
                    | "PostHeader"
                    | "PostMeta"
                    | "PostNavLinks"
                    | "Breadcrumbs"
                    | "PostCover"
                    | "CommentCardShell"
                    | "SqlResultTable"
                    | "SearchIconLink"
            ) {
                return rsx! { SceneComponentPreview { name: name.clone() } };
            }
            return rsx! { CompositeSpecimen { name: name.clone(), group: spec.group.clone(), label: spec.label.clone(), source: spec.source.clone() } };
        }
    }
    rsx! { LiveComponentPreview { name, detail, checked, danger } }
}

#[component]
fn LiveComponentPreview(
    name: String,
    detail: bool,
    #[props(default)] checked: Option<Signal<bool>>,
    #[props(default)] danger: Option<Signal<bool>>,
) -> Element {
    let local_checked = use_signal(|| true);
    let local_danger = use_signal(|| false);
    let mut checked = checked.unwrap_or(local_checked);
    let danger = danger.unwrap_or(local_danger);
    let mut switch_on = use_signal(|| true);
    let mut selected_radio = use_signal(|| "public".to_string());
    let mut select_value = use_signal(|| "leaf".to_string());
    let mut time_value = use_signal(|| "08:30".to_string());
    let mut input_value = use_signal(String::new);
    let mut tab_value = use_signal(|| "all".to_string());
    let mut page = use_signal(|| 1i32);
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
            crate::components::forms::FormSelect {
                value: select_value(),
                options: vec![("leaf".to_string(), "每一片叶子"), ("design".to_string(), "关于设计"), ("daily".to_string(), "日常与灵感")],
                onchange: move |value| select_value.set(value),
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
        "Tooltip" => rsx! {
            crate::components::ui::Tooltip { tip: "一条恰到好处的说明", button { class: "showcase-demo-button secondary", "停留片刻" } }
        },
        "Popover" => rsx! {
            div { class: "showcase-popover-example",
                button { class: "showcase-demo-button secondary", onclick: move |event: MouseEvent| {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let coords = event.client_coordinates();
                        popover_x.set(coords.x as i32);
                        popover_y.set(coords.y as i32);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = event;
                    popover_open.toggle();
                }, "试试浮层 ↗" }
                crate::components::ui::Popover {
                    open: popover_open(), anchor_x: popover_x(), anchor_y: popover_y(), placement: "top", align: "center",
                    on_close: move |_| popover_open.set(false),
                    div { class: "showcase-popover-content",
                        p { "保留这片小小的灵感？" }
                        button { class: "showcase-demo-button", onclick: move |_| popover_open.set(false), "保留" }
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
            crate::components::forms::AlertBox { message: "已保存，新的想法正在生长。", variant: "success" }
        },
        "CollapsibleSettingsCard" => rsx! {
            crate::components::ui::CollapsibleSettingsCard {
                title: "自动备份", summary: "每天 08:30 留下一份记录", enabled: true,
                div { class: "showcase-collapse-content", "这里放置详细设置。" }
            }
        },
        "StatusBadge" => rsx! {
            div { class: "showcase-preview-row",
                crate::components::ui::StatusBadge { color_class: "bg-green-500/10 text-green-600 dark:text-green-400", label: "已发布" }
                crate::components::ui::StatusBadge { color_class: "bg-paper-tertiary/20 text-paper-secondary", label: "草稿" }
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
        "SproutPlaceholder" => rsx! {
            crate::components::ui::SproutPlaceholder { class: "w-10 h-10 text-paper-accent" }
        },
        "EmptyState" => rsx! {
            div { class: "showcase-empty-preview",
                crate::components::empty_state::EmptyState { title: "这里，等待第一颗种子。", description: "写下一个新的开始。" }
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
        _ => rsx! { span { "组件预览即将到来" } },
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
            rsx! { crate::components::skeletons::tags_skeleton::TagDetailSkeleton { tag: "设计".to_string() } }
        }
        "TagPostsLoading" => {
            rsx! { crate::components::skeletons::tags_skeleton::TagPostsLoading {} }
        }
        "TagsSkeleton" => rsx! { crate::components::skeletons::tags_skeleton::TagsSkeleton {} },
        "WriteSkeleton" => rsx! { crate::components::skeletons::write_skeleton::WriteSkeleton {} },
        _ => {
            rsx! { crate::components::skeletons::atoms::SkeletonBox { class: "w-40 h-3 rounded" } }
        }
    };
    rsx! { div { class: "showcase-skeleton-window",
        div { class: "showcase-skeleton-window-bar", span {} span {} span {} }
        div { class: "showcase-skeleton-scale", {skeleton} }
    } }
}

/// 有业务模型或权限要求的组件展示其实际所属场景和源码，避免触发公开页写操作。
#[component]
fn CompositeSpecimen(name: String, group: String, label: String, source: String) -> Element {
    let motif = match group.as_str() {
        "post" => "✦",
        "comments" => "❞",
        "assets" => "▣",
        "layout" => "◫",
        "tools" => "⌘",
        "browser" => "⌘",
        _ => "✳",
    };
    rsx! {
        div { class: "showcase-reference-art", aria_label: "{label}的场景示意",
            span { class: "showcase-reference-kicker", "{group_label(&group)} / COMPONENT" }
            span { class: "showcase-reference-motif", aria_hidden: "true", "{motif}" }
            strong { "{label}" }
            code { "{name}" }
            span { class: "showcase-reference-source", "{source}" }
        }
    }
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
        "PostHeader" => {
            rsx! { crate::components::post::post_header::PostHeader { post: post.clone() } }
        }
        "PostMeta" => rsx! { crate::components::post::post_meta::PostMeta { post: post.clone() } },
        "PostNavLinks" => rsx! {
            crate::components::post::post_nav_links::PostNavLinks {
                prev: Some(PostNav { title: "一片叶子".to_string(), slug: "showcase-previous".to_string() }),
                next: Some(PostNav { title: "下一圈年轮".to_string(), slug: "showcase-next".to_string() }),
            }
        },
        "Breadcrumbs" => {
            rsx! { crate::components::post::breadcrumbs::Breadcrumbs { title: post.title.clone() } }
        }
        "PostCover" => {
            rsx! { crate::components::post::post_cover::PostCover { src: "/images/xiaotiaoxiaogou_01.webp".to_string() } }
        }
        "CommentCardShell" => rsx! {
            crate::components::comments::card::CommentCardShell {
                depth: 0,
                avatar_url: String::new(),
                author_name: "叶子".to_string(),
                author_element: rsx! { span { "叶子" } },
                author_badge: rsx! { span { "作者" } },
                timestamp: rsx! { time { "刚刚" } },
                status_badge: rsx! {},
                content_html: "<p>一片叶子，也记得来时的风。</p>".to_string(),
                div { "回复" }
            }
        },
        "SqlResultTable" => rsx! {
            crate::components::sql_result_table::SqlResultTable {
                result: crate::api::database::sql_console::SqlResult {
                    columns: vec!["id".to_string(), "title".to_string(), "published".to_string()],
                    rows: vec![vec![serde_json::json!(1), serde_json::json!("让想法生根"), serde_json::json!(true)]],
                    statement_type: "Select".to_string(),
                    ..Default::default()
                }
            }
        },
        "SearchIconLink" => rsx! { crate::components::header::SearchIconLink {} },
        _ => rsx! {},
    };
    rsx! {
        div { class: "showcase-scene-window", aria_label: "{name} 的静态样例",
            div { class: "showcase-skeleton-window-bar", span {} span {} span {} }
            div { class: "showcase-scene-canvas", inert: "true", {inner} }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::Path;

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
        }
    }
}
