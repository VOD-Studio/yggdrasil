// MIGRATION_STUB
use crate::components::ui::{Icon, IconName};
use crate::routes::Route;
use leptos::prelude::*;
use leptos_router::{lazy_route, LazyRoute};

pub struct ChangelogRoute;

#[lazy_route]
impl LazyRoute for ChangelogRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <p>"迁移中"</p> }.into_any()
    }
}

/// 页头：返回关于、标题、说明与版本数。骨架屏复用，数据到达前后不跳动。
///
/// `total` / `since` 缺省时不渲染版本数行（骨架屏）；可传值、`Option` 或信号。
#[component]
pub fn ChangelogHeader(
    #[prop(optional, into)] total: MaybeProp<usize>,
    #[prop(optional, into)] since: MaybeProp<String>,
) -> impl IntoView {
    view! {
        <a class="changelog-back" href=Route::About.to_string()>
            <Icon name=IconName::ArrowLeft size=14/>
            "关于"
        </a>
        <header class="changelog-header page-enter">
            <div>
                <h1 class="changelog-title">"更新日志"</h1>
                <p class="changelog-intro">"每个版本新增、改进和修复了什么。"</p>
            </div>
            {move || {
                total
                    .get()
                    .map(|total| {
                        view! {
                            <p class="changelog-count tabular-nums">
                                {format!("{total} 个版本")}
                                {move || since.get().map(|since| format!(" · {since} 起"))}
                            </p>
                        }
                    })
            }}
        </header>
    }
}
