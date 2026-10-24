//! 文章页眉组件
//!
//! 面包屑、衬线标题、导语、元信息与标签，末尾一条发丝线与正文分开。

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::components::post::breadcrumbs::Breadcrumbs;
use crate::components::post::post_meta::PostMeta;
use crate::components::ui::{StatusBadge, TagChip};
use crate::models::post::{PostHeaderData, PostStatus};
use crate::routes::Route;

/// 文章页眉组件。
///
/// Props：
/// - `post`：页眉展示所需的最小字段集（P4.2：不携带完整正文，调用方用
///   `PostHeaderData::from(&post)` 构造）
/// - `sample`：图鉴展示非跳转的样例链接；正式文章默认 false。
#[component]
pub fn PostHeader(post: PostHeaderData, #[prop(optional)] sample: bool) -> impl IntoView {
    let summary = post.summary.clone().filter(|s| !s.trim().is_empty());
    let is_draft = post.status == PostStatus::Draft;
    let post_id = post.id.to_string();
    let title = post.title.clone();
    let tags = post.tags.clone();
    let first_tag = tags.first().cloned();

    let tag_list = (!tags.is_empty()).then(|| {
        let items = tags
            .into_iter()
            .map(|tag| {
                let chip = if sample {
                    view! {
                        <a class="chip" href="#" on:click=|event: MouseEvent| event.prevent_default()>
                            {tag}
                        </a>
                    }
                    .into_any()
                } else {
                    let to = Route::TagDetail { tag: tag.clone() }.to_string();
                    view! { <TagChip label=tag to/> }.into_any()
                };
                view! { <li>{chip}</li> }
            })
            .collect_view();
        view! { <ul class="post-tags" aria-label="文章标签">{items}</ul> }
    });

    view! {
        <header class="post-header">
            <Breadcrumbs tag=first_tag sample/>
            <h1 class="post-title" data-vt-post-id=post_id data-vt-role="title">
                {title}
                {is_draft
                    .then(|| {
                        view! { <StatusBadge color_class="badge-warning" label="草稿" class="post-draft"/> }
                    })}
            </h1>
            {summary.map(|summary| view! { <p class="post-lede">{summary}</p> })}
            <PostMeta post=post/>
            {tag_list}
        </header>
    }
}
