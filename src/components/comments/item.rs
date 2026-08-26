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
    let indent_px = if depth > 1 { (depth.min(5) - 1) * 16 } else { 0 };

    // 作者名展示为链接或普通文本
    let author_element = match &comment.author_url {
        Some(url) if !url.is_empty() => rsx! {
            a {
                href: "{url}",
                rel: "nofollow noopener",
                target: "_blank",
                class: "font-semibold text-paper-primary hover:text-paper-accent transition-colors",
                "{comment.author_name}"
            }
        },
        _ => rsx! {
            span { class: "font-semibold text-paper-primary", "{comment.author_name}" }
        },
    };

    let author_badge = if comment.is_author {
        rsx! {
            span { class: "inline-flex items-center px-1.5 py-0.5 rounded-full text-[10px] font-medium bg-[var(--color-paper-accent)]/15 text-[var(--color-paper-accent)]",
                "作者"
            }
        }
    } else {
        rsx! {}
    };

    let timestamp = rsx! {
        span {
            class: "text-paper-tertiary",
            title: "{comment.created_at_iso}",
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

            div { class: "flex items-center gap-3 mt-2",
                if show_reply {
                    button {
                        class: "inline-flex items-center gap-1 text-xs font-medium text-paper-tertiary hover:text-paper-accent hover:bg-[var(--color-paper-entry)] px-2 py-1 rounded-md transition-all cursor-pointer",
                        class: if is_replying { "text-[var(--color-paper-accent)] bg-[var(--color-paper-accent)]/10" } else { "" },
                        aria_label: "回复 {comment.author_name} 的评论",
                        onclick: move |_| {
                            if is_replying {
                                active_reply.set(None);
                            } else {
                                active_reply.set(Some(comment.id));
                            }
                        },
                        svg {
                            class: "w-3.5 h-3.5",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M3 10h10a5 5 0 015 5v2m-15-7l4-4m-4 4l4 4",
                            }
                        }
                        if is_replying {
                            "取消回复"
                        } else {
                            "回复"
                         }
                    }
                }
            }

            div {
                class: if is_replying { "mt-2 pt-1" } else { "hidden" },
                CommentForm {
                    post_id,
                    parent_id: Some(comment.id),
                    parent_indent: Some(indent_px),
                }
            }
        }
    }
}
