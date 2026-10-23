//! 面包屑组件
//!
//! 文章标题上方的「文章 / 所在栏目」：「文章」回到首页，栏目取文章的第一个标签。

use leptos::prelude::*;

use super::{full_reload_rel, tag_href};

/// 面包屑导航组件。
///
/// Props：
/// - `tag`：所在栏目（文章的第一个标签），没有时只显示「文章」
/// - `full_reload`：链接走整页加载而非客户端路由（默认 false）。原版仅在 admin
///   预览布局内传 true，规避 dioxus 0.7.10 跨 layout 导航的 suspense 卸载 bug。
/// - `sample`：图鉴展示非跳转的样例链接；正式文章默认 false。
#[component]
pub fn Breadcrumbs(
    #[prop(optional_no_strip)] tag: Option<String>,
    #[prop(optional)] full_reload: bool,
    #[prop(optional)] sample: bool,
) -> impl IntoView {
    let home = if sample {
        view! { <a href="#" on:click=|event| event.prevent_default()>"文章"</a> }.into_any()
    } else {
        let vt_return = (!full_reload).then_some("true");
        view! {
            <a href="/" rel=full_reload_rel(full_reload) data-vt-return=vt_return>
                "文章"
            </a>
        }
        .into_any()
    };
    let tag_link = tag.map(|tag| {
        let link = if sample {
            view! { <a href="#" on:click=|event| event.prevent_default()>{tag}</a> }.into_any()
        } else {
            let href = tag_href(&tag);
            view! { <a href=href rel=full_reload_rel(full_reload)>{tag}</a> }.into_any()
        };
        view! {
            <span class="post-crumbs-sep" aria-hidden="true">"/"</span>
            {link}
        }
    });

    view! {
        <nav class="post-crumbs" aria-label="面包屑">
            {home}
            {tag_link}
        </nav>
    }
}
