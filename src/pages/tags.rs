// MIGRATION_STUB
use crate::components::ui::{Icon, IconName};
use crate::routes::Route;
use leptos::prelude::*;
use leptos_router::{lazy_route, LazyRoute};

pub struct TagRoute;

#[lazy_route]
impl LazyRoute for TagRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <p>"迁移中"</p> }.into_any()
    }
}

/// 路由兜底和实际页面共用开场；标签名无需等待文章请求。
#[component]
pub fn TagIntro(#[prop(into)] tag: String) -> impl IntoView {
    view! {
        <header class="archive-header page-enter">
            <a class="archive-back" href=Route::Archives.to_string()>
                <Icon name=IconName::ArrowLeft size=14/>
                "全部归档"
            </a>
            <h1 class="archive-title">
                <span class="archive-title-hash" aria-hidden="true">
                    "# "
                </span>
                {tag}
            </h1>
        </header>
    }
}
