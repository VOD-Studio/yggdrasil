//! 空列表、无搜索结果等状态共用的图标、文案与行动入口。

use dioxus::prelude::*;

use crate::components::ui::{Icon, IconName, BTN_SECONDARY};

/// 空状态行动按钮。跳转或重试由调用方负责。
#[derive(Props, Clone, PartialEq)]
pub struct EmptyStateAction {
    #[props(into)]
    pub label: String,
    pub onclick: EventHandler<()>,
}

/// 展示线条图标、标题与说明，可选一个行动按钮。
///
/// 图标默认为收件箱；无搜索结果可传 `IconName::SearchX`，缺图可传 `IconName::ImageOff`。
#[component]
pub fn EmptyState(
    #[props(into, default = "还没有文章".to_string())] title: String,
    #[props(into, default = String::new())] description: String,
    #[props(default = IconName::Inbox)] icon: IconName,
    #[props(default)] action: Option<EmptyStateAction>,
) -> Element {
    rsx! {
        section { class: "empty-state",
            span { class: "empty-state-icon", Icon { name: icon, size: 20 } }
            h2 { class: "empty-state-title", "{title}" }
            if !description.is_empty() {
                p { class: "empty-state-description", "{description}" }
            }
            if let Some(act) = action {
                button {
                    class: "{BTN_SECONDARY} empty-state-action",
                    r#type: "button",
                    onclick: move |_| act.onclick.call(()),
                    "{act.label}"
                }
            }
        }
    }
}
