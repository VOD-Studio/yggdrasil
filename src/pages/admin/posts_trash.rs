// MIGRATION_STUB
use leptos::prelude::*;
use leptos_router::{lazy_route, LazyRoute};

pub struct PostsTrashRoute;

#[lazy_route]
impl LazyRoute for PostsTrashRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <p>"迁移中"</p> }.into_any()
    }
}
