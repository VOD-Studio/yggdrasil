//! 文章页眉组件
//!
//! 面包屑、衬线标题、导语、元信息与标签，末尾一条发丝线与正文分开。

use leptos::prelude::*;

use super::{full_reload_rel, tag_href};
use crate::components::post::breadcrumbs::Breadcrumbs;
use crate::components::post::post_meta::PostMeta;
use crate::components::ui::StatusBadge;
use crate::models::post::{PostHeaderData, PostStatus};

/// 文章页眉组件。
///
/// Props：
/// - `post`：页眉展示所需的最小字段集（P4.2：不携带完整正文，调用方用
///   `PostHeaderData::from(&post)` 构造）
/// - `full_reload`：链接走整页加载（默认 false），原版仅 admin 预览布局使用。
/// - `sample`：图鉴展示非跳转的样例链接；正式文章默认 false。
#[component]
pub fn PostHeader(
    post: PostHeaderData,
    #[prop(optional)] full_reload: bool,
    #[prop(optional)] sample: bool,
) -> impl IntoView {
    let summary = post.summary.clone().filter(|s| !s.trim().is_empty());
    let draft = (post.status == PostStatus::Draft).then(|| {
        view! { <StatusBadge color_class="badge-warning" label="草稿" class="post-draft"/> }
    });
    let tags = (!post.tags.is_empty()).then(|| {
        let items = post
            .tags
            .iter()
            .cloned()
            .map(|tag| {
                let link = if sample {
                    view! {
                        <a class="chip" href="#" on:click=|event| event.prevent_default()>{tag}</a>
                    }
                    .into_any()
                } else {
                    let href = tag_href(&tag);
                    view! {
                        <a class="chip" href=href rel=full_reload_rel(full_reload)>{tag}</a>
                    }
                    .into_any()
                };
                view! { <li>{link}</li> }
            })
            .collect_view();
        view! { <ul class="post-tags" aria-label="文章标签">{items}</ul> }
    });

    view! {
        <header class="post-header">
            <Breadcrumbs tag=post.tags.first().cloned() full_reload=full_reload sample=sample/>
            <h1 class="post-title" data-vt-post-id=post.id.to_string() data-vt-role="title">
                {post.title.clone()}
                {draft}
            </h1>
            {summary.map(|summary| view! { <p class="post-lede">{summary}</p> })}
            <PostMeta post=post.clone()/>
            {tags}
        </header>
    }
}
