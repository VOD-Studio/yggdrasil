//! 待审核评论项组件
//!
//! 展示用户刚提交、尚未通过审核的评论占位项，
//! 视觉上使用较低的透明度并标注"审核中"状态。

use leptos::prelude::*;

use crate::components::comments::card::CommentCardShell;
use crate::utils::comment_storage::{render_pending_content, PendingComment};
use crate::utils::time::format_relative_time_iso;

/// 待审核评论项组件。
///
/// Props：
/// - `comment`：待审核评论数据
/// - `post_id`：所属文章 ID（当前未使用，保留用于未来扩展）
///
/// 展示内容包括：作者头像/链接、基于创建时间动态计算的相对时间、审核中徽章、Markdown 渲染内容。
/// 孤儿评论深度会被修正为 0。
#[component]
pub fn PendingCommentItem(comment: PendingComment, post_id: i32) -> impl IntoView {
    let _ = post_id;
    // 孤儿评论（parent_id 为 None 但 depth > 0）按顶层展示
    let depth = if comment.parent_id.is_none() && comment.depth > 0 {
        0
    } else {
        comment.depth
    };

    let content_html = render_pending_content(&comment.content_md);
    // 基于创建时间实时计算相对时间，避免"刚刚"永久显示。
    let relative_time = format_relative_time_iso(&comment.created_at);

    let author_element = match &comment.author_url {
        Some(url) if !url.is_empty() => view! {
            <a class="comment-author" href=url.clone() rel="nofollow noopener" target="_blank">
                {comment.author_name.clone()}
            </a>
        }
        .into_any(),
        _ => view! { <span class="comment-author">{comment.author_name.clone()}</span> }.into_any(),
    };

    let status_badge = view! {
        <span
            class="comment-badge is-pending"
            title="刚提交的评论只有你自己能看到，审核通过后对所有人显示。"
        >
            "审核中"
        </span>
    }
    .into_any();

    let timestamp = view! {
        <time class="comment-time" datetime=comment.created_at.clone() title=comment.created_at.clone()>
            {relative_time}
        </time>
    }
    .into_any();

    view! {
        <CommentCardShell
            depth=depth
            muted=true
            avatar_url=comment.avatar_url.clone()
            author_name=comment.author_name.clone()
            author_element=author_element
            author_badge=().into_any()
            timestamp=timestamp
            status_badge=status_badge
            content_html=content_html
        />
    }
}
