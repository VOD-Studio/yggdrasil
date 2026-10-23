// MIGRATION_STUB
use crate::components::ui::{Icon, IconName};
use crate::routes::Route;
use leptos::prelude::*;
use leptos_router::{lazy_route, LazyRoute};

pub struct WritingGuideRoute;

#[lazy_route]
impl LazyRoute for WritingGuideRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <p>"迁移中"</p> }.into_any()
    }
}

/// 页头：返回关于、标题与一句说明。骨架屏复用。
#[component]
pub fn WritingHeader() -> impl IntoView {
    view! {
        <a class="writing-back" href=Route::About.to_string()>
            <Icon name=IconName::ArrowLeft size=14/>
            "关于"
        </a>
        <header class="writing-header page-enter">
            <h1 class="writing-title">"写作指南"</h1>
            <p class="writing-lead">
                "把这里的写作方式交给你的 AI：一份技能文件告诉它怎么排版，一条 MCP 连接让它能直接保存草稿。"
            </p>
        </header>
    }
}
