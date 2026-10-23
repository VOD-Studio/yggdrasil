//! 单条评论项组件
//!
//! 展示已审核通过的评论，支持展开/收起回复表单。

use leptos::prelude::*;

use crate::components::comments::card::CommentCardShell;
use crate::components::comments::form::CommentForm;
use crate::components::comments::section::CommentContext;
use crate::models::comment::PublicComment;

/// 单条已审核评论组件。
///
/// Props：
/// - `comment`：已审核评论数据
/// - `post_id`：所属文章 ID
///
/// 关键行为：
/// - 点击"回复"按钮切换该评论下方的回复表单
/// - 最大递归深度限制为 20，超过后隐藏回复按钮
/// - 回复表单始终挂载（仅用 `hidden` 类切换可见性），不随 `is_replying` 结构性
///   卸载——否则 `CommentForm` 内的草稿信号（`content_md` 等）会随组件
///   卸载被销毁，取消回复或切换回复目标时静默丢失草稿
#[component]
pub fn CommentItem(comment: PublicComment, post_id: i32) -> impl IntoView {
    let ctx = expect_context::<CommentContext>();
    let active_reply = ctx.active_reply;

    // 孤儿评论按顶层展示
    let depth = if comment.parent_id.is_none() && comment.depth > 0 {
        0
    } else {
        comment.depth
    };

    let comment_id = comment.id;
    let is_replying = Memo::new(move |_| active_reply.get() == Some(comment_id));
    let show_reply = depth < 20;
    // 本评论卡片自身的缩进像素值（与 CommentCardShell 的 style 计算一致），
    // 传给回复表单用于抵消缩进、把表单拉回内容区左边缘。
    let indent_px = if depth > 1 {
        (depth.min(5) - 1) * 16
    } else {
        0
    };

    let author_element = match &comment.author_url {
        Some(url) if !url.is_empty() => view! {
            <a class="comment-author" href=url.clone() rel="nofollow noopener" target="_blank">
                {comment.author_name.clone()}
            </a>
        }
        .into_any(),
        _ => view! { <span class="comment-author">{comment.author_name.clone()}</span> }.into_any(),
    };

    let author_badge = if comment.is_author {
        view! { <span class="comment-badge">"作者"</span> }.into_any()
    } else {
        ().into_any()
    };

    let timestamp = view! {
        <time
            class="comment-time"
            datetime=comment.created_at_iso.clone()
            title=comment.created_at_iso.clone()
        >
            {comment.created_at.clone()}
        </time>
    }
    .into_any();

    let reply_label = format!("回复 {} 的评论", comment.author_name);

    view! {
        <CommentCardShell
            depth=depth
            avatar_url=comment.avatar_url.clone()
            author_name=comment.author_name.clone()
            author_element=author_element
            author_badge=author_badge
            timestamp=timestamp
            status_badge=().into_any()
            content_html=comment.content_html.clone().unwrap_or_default()
            content_extra_class="md-content"
        >
            {show_reply
                .then(|| {
                    view! {
                        <div class="comment-actions">
                            <button
                                class="comment-reply"
                                type="button"
                                aria-expanded=move || is_replying.get().to_string()
                                aria-label=move || {
                                    if is_replying.get() {
                                        "收起回复".to_string()
                                    } else {
                                        reply_label.clone()
                                    }
                                }
                                on:click=move |_| {
                                    if is_replying.get_untracked() {
                                        active_reply.set(None);
                                    } else {
                                        active_reply.set(Some(comment_id));
                                    }
                                }
                            >
                                {move || if is_replying.get() { "收起回复" } else { "回复" }}
                            </button>
                        </div>
                    }
                })}

            <div class=move || if is_replying.get() { "comment-reply-form" } else { "hidden" }>
                <CommentForm
                    post_id=post_id
                    parent_id=Some(comment_id)
                    parent_indent=Some(indent_px)
                />
            </div>
        </CommentCardShell>
    }
}
