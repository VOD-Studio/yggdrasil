//! 单条评论项组件
//!
//! 展示已审核通过的评论，支持展开/收起回复表单。

use dioxus::prelude::*;

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
///   卸载——否则 `CommentForm` 内的草稿 `use_signal`（`content_md` 等）会随组件
///   卸载被销毁，取消回复或切换回复目标时静默丢失草稿
#[component]
pub fn CommentItem(comment: PublicComment, post_id: i32) -> Element {
    let ctx: CommentContext = use_context();
    let mut active_reply = ctx.active_reply;

    // 孤儿评论按顶层展示
    let depth = if comment.parent_id.is_none() && comment.depth > 0 {
        0
    } else {
        comment.depth
    };

    let is_replying = active_reply() == Some(comment.id);
    let show_reply = depth < 20;
    // 本评论卡片自身的缩进像素值（与 CommentCardShell 的 style 计算一致），
    // 传给回复表单用于抵消缩进、把表单拉回内容区左边缘。
    let indent_px = if depth > 1 {
        (depth.min(5) - 1) * 16
    } else {
        0
    };

    let author_element = match &comment.author_url {
        Some(url) if !url.is_empty() => rsx! {
            a { class: "comment-author", href: "{url}", rel: "nofollow noopener", target: "_blank",
                "{comment.author_name}"
            }
        },
        _ => rsx! {
            span { class: "comment-author", "{comment.author_name}" }
        },
    };

    let author_badge = if comment.is_author {
        rsx! { span { class: "comment-badge", "作者" } }
    } else {
        rsx! {}
    };

    let timestamp = rsx! {
        time { class: "comment-time", datetime: "{comment.created_at_iso}", title: "{comment.created_at_iso}",
            "{comment.created_at}"
        }
    };

    rsx! {
        CommentCardShell {
            depth,
            avatar_url: comment.avatar_url.clone(),
            author_name: comment.author_name.clone(),
            author_element,
            author_badge,
            timestamp,
            status_badge: rsx! {},
            content_html: comment.content_html.clone().unwrap_or_default(),
            content_extra_class: "md-content",

            if show_reply {
                div { class: "comment-actions",
                    button {
                        class: "comment-reply",
                        r#type: "button",
                        aria_expanded: "{is_replying}",
                        aria_label: if is_replying { "收起回复" } else { "回复 {comment.author_name} 的评论" },
                        onclick: move |_| {
                            if is_replying {
                                active_reply.set(None);
                            } else {
                                active_reply.set(Some(comment.id));
                            }
                        },
                        if is_replying { "收起回复" } else { "回复" }
                    }
                }
            }

            div {
                class: if is_replying { "comment-reply-form" } else { "hidden" },
                CommentForm {
                    post_id,
                    parent_id: Some(comment.id),
                    parent_indent: Some(indent_px),
                }
            }
        }
    }
}
