//! 单条评论项组件
//!
//! 展示已审核通过的评论，支持展开/收起回复表单。

use dioxus::prelude::*;

use crate::components::comments::form::CommentForm;
use crate::components::comments::section::CommentContext;
use crate::components::ui::BADGE_BASE;
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

    let indent = depth.min(6) * 24;

    let is_replying = active_reply() == Some(comment.id);
    let show_reply = depth < 20;

    // 作者名展示为链接或普通文本
    let author_element = match &comment.author_url {
        Some(url) if !url.is_empty() => rsx! {
            a {
                href: "{url}",
                rel: "nofollow noopener",
                target: "_blank",
                class: "font-medium text-paper-primary hover:text-paper-accent transition-colors",
                "{comment.author_name}"
            }
        },
        _ => rsx! {
            span { class: "font-medium text-paper-primary", "{comment.author_name}" }
        },
    };

    rsx! {
        div { class: "py-4", style: "margin-left: {indent}px",

            div { class: "flex gap-3",
                img {
                    src: "{comment.avatar_url}",
                    alt: "{comment.author_name} 的头像",
                    loading: "lazy",
                    decoding: "async",
                    class: "w-8 h-8 rounded-full shrink-0 mt-0.5 bg-gray-200 dark:bg-gray-800",
                }

                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-1.5 text-sm mb-1.5 flex-wrap",
                        {author_element}
                        if comment.is_author {
                            span { class: "{BADGE_BASE} bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300",
                                "作者"
                            }
                        }
                        span { class: "text-paper-tertiary", "·" }
                        span {
                            class: "text-paper-tertiary",
                            title: "{comment.created_at_iso}",
                            "{comment.created_at}"
                        }
                    }

                    div {
                        class: "prose prose-sm dark:prose-invert max-w-none text-paper-secondary md-content",
                        dangerous_inner_html: comment.content_html.as_deref().unwrap_or(""),
                    }

                    div { class: "flex items-center gap-3 mt-2",
                        if show_reply {
                            button {
                                class: "text-xs text-paper-tertiary hover:text-paper-accent transition-colors cursor-pointer",
                                aria_label: "回复 {comment.author_name} 的评论",
                                onclick: move |_| {
                                    if is_replying {
                                        active_reply.set(None);
                                    } else {
                                        active_reply.set(Some(comment.id));
                                    }
                                },
                                if is_replying {
                                    "取消回复"
                                } else {
                                    "回复"
                                }
                            }
                        }

                    }

                    if is_replying {
                        CommentForm {
                            post_id,
                            parent_id: Some(comment.id),
                            parent_indent: Some(indent),
                        }
                    }
                }
            }
        }
    }
}
