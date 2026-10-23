// MIGRATION_STUB
use leptos::prelude::*;
use leptos_router::{lazy_route, LazyRoute};

pub struct FriendsRoute;

#[lazy_route]
impl LazyRoute for FriendsRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <p>"迁移中"</p> }.into_any()
    }
}

/// 页头标题与说明；骨架屏复用，数据到达前后不跳动。
///
/// `count` 缺省时不渲染人数行（骨架屏）；可传值、`Option` 或信号。
#[component]
pub fn FriendsHeader(#[prop(optional, into)] count: MaybeProp<usize>) -> impl IntoView {
    view! {
        <header class="friends-header page-enter">
            <div>
                <h1 class="friends-title">"友链"</h1>
                <p class="friends-intro">"一些认真记录、持续创造的朋友。"</p>
            </div>
            {move || {
                count
                    .get()
                    .map(|count| {
                        view! {
                            <p class="friends-count tabular-nums">
                                {format!("{count} 位朋友 · 按添加顺序")}
                            </p>
                        }
                    })
            }}
        </header>
    }
}
