//! 空列表、无搜索结果等状态共用的图标、文案与行动入口。

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::components::ui::{Icon, IconName, BTN_SECONDARY};

/// 空状态行动按钮。跳转或重试由调用方负责。
#[derive(Clone)]
pub struct EmptyStateAction {
    pub label: String,
    pub onclick: Callback<MouseEvent>,
}

/// 展示线条图标、标题与说明，可选一个行动按钮。
///
/// 图标默认为收件箱；无搜索结果可传 `IconName::SearchX`，缺图可传 `IconName::ImageOff`。
#[component]
pub fn EmptyState(
    #[prop(into, default = "还没有文章".to_string())] title: String,
    #[prop(optional, into)] description: String,
    #[prop(default = IconName::Inbox)] icon: IconName,
    #[prop(optional)] action: Option<EmptyStateAction>,
) -> impl IntoView {
    view! {
        <section class="empty-state">
            <span class="empty-state-icon">
                <Icon name=icon size=20/>
            </span>
            <h2 class="empty-state-title">{title}</h2>
            {(!description.is_empty())
                .then(|| view! { <p class="empty-state-description">{description}</p> })}
            {action
                .map(|act| {
                    let onclick = act.onclick;
                    view! {
                        <button
                            class=format!("{BTN_SECONDARY} empty-state-action")
                            type="button"
                            on:click=move |e| onclick.run(e)
                        >
                            {act.label}
                        </button>
                    }
                })}
        </section>
    }
}
