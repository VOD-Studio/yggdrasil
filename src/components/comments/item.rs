//! 单条评论项组件
//!
//! 展示已审核通过的评论，支持展开/收起回复表单。

use dioxus::prelude::*;

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

    let is_child = depth > 0;
    let is_replying = active_reply() == Some(comment.id);
    let show_reply = depth < 20;

    // 作者名展示为链接或普通文本
    let author_element = match &comment.author_url {
        Some(url) if !url.is_empty() => rsx! {
            a {
                href: "{url}",
                rel: "nofollow noopener",
                target: "_blank",
                class: "font-semibold text-paper-primary hover:text-paper-accent transition-colors",
            }
        },
        _ => rsx! {
            span { class: "font-semibold text-paper-primary", "{comment.author_name}" }
        },
    };

    rsx! {
        div {
            class: "py-3.5 transition-colors",
            class: if is_child { "border-l-2 border-[var(--color-paper-border)]/50 pl-3.5 sm:pl-4.5 my-1" } else { "" },
            style: if depth > 1 { format!("margin-left: {}px;", (depth.min(5) - 1) * 16) } else { String::new() },

            div { class: "flex items-start gap-3",
                img {
                    src: "{comment.avatar_url}",
                    alt: "{comment.author_name} 的头像",
                    loading: "lazy",
                    decoding: "async",
                    class: "w-8 h-8 rounded-full shrink-0 object-cover ring-1 ring-[var(--color-paper-border)]/60 bg-[var(--color-paper-entry)] mt-0.5",
                }

                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-2 text-xs mb-1 flex-wrap",
                        {author_element}
                        if comment.is_author {
                            span { class: "inline-flex items-center px-1.5 py-0.5 rounded-full text-[10px] font-medium bg-[var(--color-paper-accent)]/15 text-[var(--color-paper-accent)]",
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
                        class: "prose prose-sm dark:prose-invert max-w-none text-paper-secondary md-content leading-relaxed",
                        dangerous_inner_html: comment.content_html.as_deref().unwrap_or(""),
                    }

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

                    if is_replying {
                        div { class: "mt-2 pt-1",
                            CommentForm {
                                post_id,
                                parent_id: Some(comment.id),
                                parent_indent: None,
                            }
                        }
                    }
                }
            }
        }
    }
}
