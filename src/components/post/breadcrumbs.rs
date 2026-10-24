//! 面包屑组件
//!
//! 文章标题上方的「文章 / 所在栏目」：「文章」回到首页，栏目取文章的第一个标签。

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::routes::Route;

/// 面包屑导航组件。
///
/// Props：
/// - `tag`：所在栏目（文章的第一个标签），没有时只显示「文章」；可传 `String`、
///   `Option<String>` 或省略
/// - `sample`：图鉴展示非跳转的样例链接；正式文章默认 false。
#[component]
pub fn Breadcrumbs(
    #[prop(optional, into)] tag: MaybeProp<String>,
    #[prop(optional)] sample: bool,
) -> impl IntoView {
    let home = if sample {
        view! { <a href="#" on:click=|event: MouseEvent| event.prevent_default()>"文章"</a> }
            .into_any()
    } else {
        view! { <a href=Route::Home.to_string() data-vt-return="true">"文章"</a> }.into_any()
    };
    let tag = tag.get_untracked().map(|tag| {
        let link = if sample {
            let label = tag.clone();
            view! { <a href="#" on:click=|event: MouseEvent| event.prevent_default()>{label}</a> }
                .into_any()
        } else {
            let href = Route::TagDetail { tag: tag.clone() }.to_string();
            view! { <a href=href>{tag}</a> }.into_any()
        };
        view! {
            <span class="post-crumbs-sep" aria-hidden="true">"/"</span>
            {link}
        }
    });
    view! {
        <nav class="post-crumbs" aria-label="面包屑">
            {home}
            {tag}
        </nav>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use leptos::reactive::owner::Owner;

    #[test]
    fn links_home_and_first_tag() {
        let html =
            Owner::new().with(|| view! { <Breadcrumbs tag="Rust 编程".to_string()/> }.to_html());
        assert!(html.contains(r#"href="/""#), "{html}");
        assert!(html.contains(r#"data-vt-return="true""#), "{html}");
        assert!(html.contains("/tags/Rust%20%E7%BC%96%E7%A8%8B"), "{html}");
        assert!(html.contains("post-crumbs-sep"), "{html}");
    }

    #[test]
    fn without_tag_only_shows_home() {
        let html = Owner::new().with(|| view! { <Breadcrumbs/> }.to_html());
        assert!(!html.contains("post-crumbs-sep"), "{html}");
    }
}
