//! 评论表单组件
//!
//! 提供发表评论与回复评论的表单，包含昵称、邮箱、网站、内容与反垃圾蜜罐字段。

use dioxus::prelude::*;

use crate::api::comments::create_comment;
use crate::components::comments::section::CommentContext;
use crate::components::forms::AlertBox;
use crate::components::ui::{UserAvatar, BTN_PRIMARY_SM, SPINNER_SVG};
use crate::utils::comment_storage::{self, PendingComment};

/// 辅助向评论输入框追加/插入常用 Markdown 语法片段。
fn insert_markdown_snippet(
    mut content_md: Signal<String>,
    prefix: &str,
    suffix: &str,
    placeholder: &str,
) {
    let mut text = content_md();
    if text.is_empty() {
        text.push_str(prefix);
        text.push_str(placeholder);
        text.push_str(suffix);
    } else {
        if !text.ends_with(' ') && !text.ends_with('\n') {
            text.push(' ');
        }
        text.push_str(prefix);
        text.push_str(placeholder);
        text.push_str(suffix);
    }
    content_md.set(text);
}

/// 评论表单组件，用于顶层评论或回复评论。
///
/// Props：
/// - `post_id`：所属文章 ID
/// - `parent_id`：回复目标评论 ID，`None` 表示顶层评论
/// - `parent_indent`：回复时父评论的缩进像素值，用于用负 margin 把表单拉回内容区左边缘
///
/// 关键事件：
/// - 挂载时从本地存储恢复上次填写的作者信息
/// - 提交时校验必填项与蜜罐字段
/// - 提交成功后清空内容、保存作者信息、添加待审核评论并触发列表刷新
#[component]
pub fn CommentForm(post_id: i32, parent_id: Option<i64>, parent_indent: Option<i32>) -> Element {
    let ctx: CommentContext = use_context();
    let mut active_reply = ctx.active_reply;
    let mut refresh_trigger = ctx.refresh_trigger;
    let mut pending_comments = ctx.pending_comments;
    let viewer = ctx.current_user;

    let mut author_name = use_signal(String::new);
    let mut author_email = use_signal(String::new);
    let mut author_url = use_signal(String::new);
    let mut content_md = use_signal(String::new);
    let mut honeypot = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut message = use_signal(|| Option::<(String, &'static str)>::None);
    let mut loaded = use_signal(|| false);

    // 首次挂载时从本地存储加载作者信息
    use_effect(move || {
        if loaded() {
            return;
        }
        loaded.set(true);
        if let Some(info) = comment_storage::load_author() {
            author_name.set(info.name);
            author_email.set(info.email);
            author_url.set(info.url);
        }
    });

    // 回复表单：当前未激活回复时隐藏
    if let Some(pid) = parent_id {
        if active_reply() != Some(pid) {
            return rsx! {};
        }
    }

    let is_reply = parent_id.is_some();

    // 用于区分顶层表单与多个回复表单的 id 后缀，保证页面内 label/for 关联唯一。
    let id_suffix = match parent_id {
        Some(pid) => pid.to_string(),
        None => "root".to_string(),
    };

    // 回复表单抵消父评论缩进，让表单回到内容区左边缘，避免深层回复时被越挤越右。
    let negative_margin = match (is_reply, parent_indent) {
        (true, Some(px)) if px > 0 => format!("margin-left: -{px}px;"),
        _ => String::new(),
    };

    let mut do_submit = move || {
        if submitting() {
            return;
        }

        let post_id = post_id;
        let parent_id = parent_id;
        let is_anon = viewer().is_none();
        // 登录用户的身份字段由服务端从会话推导，表单值仅匿名路径使用。
        let (name, email, url_val) = if is_anon {
            (author_name(), author_email(), author_url())
        } else {
            (String::new(), String::new(), String::new())
        };
        let content = content_md();
        let hp = honeypot();

        // 蜜罐被填充则直接丢弃
        if !hp.is_empty() {
            return;
        }

        if content.trim().is_empty() {
            message.set(Some(("请填写评论内容".to_string(), "error")));
            return;
        }
        if is_anon && (name.trim().is_empty() || email.trim().is_empty()) {
            message.set(Some(("请填写昵称和邮箱".to_string(), "error")));
            return;
        }
        submitting.set(true);
        message.set(None);
        spawn(async move {
            let result = create_comment(
                post_id,
                parent_id,
                name.clone(),
                email.clone(),
                if url_val.trim().is_empty() {
                    None
                } else {
                    Some(url_val.clone())
                },
                content.clone(),
                hp.clone(),
            )
            .await;
            submitting.set(false);
            match result {
                Ok(resp) => {
                    if resp.success {
                        // 登录评论直发 approved：不写本地待审核
                        // 存储，列表刷新后即按正式状态展示。
                        if is_anon {
                            comment_storage::save_author(&name, &email, &url_val);
                            if let Some(comment_id) = resp.comment_id {
                                let avatar_url = resp.avatar_url.unwrap_or_default();
                                let depth = resp.depth.unwrap_or(0);
                                let now = chrono::Utc::now().to_rfc3339();
                                let pending = PendingComment {
                                    id: comment_id,
                                    parent_id,
                                    depth,
                                    author_name: name.clone(),
                                    author_url: if url_val.trim().is_empty() {
                                        None
                                    } else {
                                        Some(url_val)
                                    },
                                    avatar_url,
                                    content_md: content,
                                    created_at: now.clone(),
                                    stored_at: now,
                                };
                                comment_storage::save_pending_comment(post_id, pending.clone());
                                pending_comments.write().push(pending);
                            }
                        }
                        content_md.set(String::new());
                        message.set(Some((resp.message, "success")));
                        if parent_id.is_some() {
                            active_reply.set(None);
                        }
                        refresh_trigger.set(!refresh_trigger());
                    } else {
                        message.set(Some((resp.message, "error")));
                    }
                }
                Err(_) => {
                    message.set(Some(("提交失败，请稍后重试".to_string(), "error")));
                }
            }
        });
    };

    rsx! {
        div {
            class: if is_reply { "mt-3" } else { "" },
            style: "{negative_margin}",
            role: "form",
            aria_label: if is_reply { "回复评论" } else { "发表评论" },

            if let Some((msg, variant)) = message() {
                div { class: "mb-3", aria_live: "polite",
                    AlertBox { message: msg, variant }
                }
            }

            // 一体化聚焦卡片容器 (All-in-One Focus Card)
            div { class: "rounded-2xl bg-[var(--color-paper-entry)] border border-[var(--color-paper-border)]/60 shadow-xs focus-within:border-[var(--color-paper-accent)]/60 focus-within:ring-2 focus-within:ring-[var(--color-paper-accent)]/20 transition-all duration-200 overflow-hidden",
                // 头部区域：登录用户身份行或访客轻量三栏输入行
                if let Some(user) = viewer() {
                    {
                        let label = user.display_label().to_string();
                        let action = if is_reply { "回复" } else { "发表评论" };
                        rsx! {
                            div { class: "flex items-center justify-between px-4 py-2.5 bg-[var(--color-paper-theme)]/40 border-b border-[var(--color-paper-border)]/40 text-xs text-paper-secondary",
                                div { class: "flex items-center gap-2.5",
                                    UserAvatar {
                                        name: label.clone(),
                                        avatar_url: user.avatar_url.clone(),
                                        class: "w-6 h-6 rounded-full text-xs ring-1 ring-[var(--color-paper-border)]/60 shrink-0",
                                    }
                                    span { class: "text-paper-secondary",
                                        "以 "
                                        span { class: "font-semibold text-paper-primary", "{label}" }
                                        " 的身份{action}"
                                    }
                                    span { class: "hidden sm:inline-flex items-center px-1.5 py-0.5 rounded-full text-[10px] font-medium bg-[var(--color-paper-accent)]/15 text-[var(--color-paper-accent)]",
                                        "已登录"
                                    }
                                }
                                div { class: "flex items-center gap-1 text-paper-tertiary",
                                    span { class: "hidden sm:inline text-[11px]", "Markdown 语法就绪" }
                                }
                            }
                        }
                    }
                } else {
                    div { class: "grid grid-cols-1 sm:grid-cols-3 divide-y sm:divide-y-0 sm:divide-x divide-[var(--color-paper-border)]/40 bg-[var(--color-paper-theme)]/30 border-b border-[var(--color-paper-border)]/40",
                        div { class: "relative",
                            input {
                                id: "comment-name-{id_suffix}",
                                class: "w-full px-3.5 py-2 bg-transparent text-sm text-paper-primary placeholder:text-paper-tertiary focus:outline-none focus:bg-[var(--color-paper-entry)]/60 transition-colors",
                                r#type: "text",
                                placeholder: "昵称 *",
                                aria_label: "昵称",
                                value: "{author_name}",
                                disabled: submitting(),
                                oninput: move |e| author_name.set(e.value()),
                            }
                        }
                        div { class: "relative",
                            input {
                                id: "comment-email-{id_suffix}",
                                class: "w-full px-3.5 py-2 bg-transparent text-sm text-paper-primary placeholder:text-paper-tertiary focus:outline-none focus:bg-[var(--color-paper-entry)]/60 transition-colors",
                                r#type: "email",
                                placeholder: "邮箱 * (保密)",
                                aria_label: "邮箱",
                                value: "{author_email}",
                                disabled: submitting(),
                                oninput: move |e| author_email.set(e.value()),
                            }
                        }
                        div { class: "relative",
                            input {
                                id: "comment-url-{id_suffix}",
                                class: "w-full px-3.5 py-2 bg-transparent text-sm text-paper-primary placeholder:text-paper-tertiary focus:outline-none focus:bg-[var(--color-paper-entry)]/60 transition-colors",
                                r#type: "url",
                                placeholder: "网站 (https://，可选)",
                                aria_label: "网站",
                                value: "{author_url}",
                                disabled: submitting(),
                                oninput: move |e| author_url.set(e.value()),
                            }
                        }
                    }
                }

                // 编辑区 (Textarea)
                div { class: "relative bg-transparent p-3 sm:p-4",
                    textarea {
                        id: "comment-content-{id_suffix}",
                        class: "w-full bg-transparent text-sm text-paper-primary placeholder:text-paper-tertiary resize-y min-h-[100px] sm:min-h-[110px] focus:outline-none leading-relaxed block relative z-10",
                        placeholder: if is_reply { "写下你的回复... (支持 Markdown 语法与代码块)" } else { "写下你的想法... (支持 Markdown 语法与代码块)" },
                        aria_label: "评论内容",
                        value: "{content_md}",
                        disabled: submitting(),
                        oninput: move |e| content_md.set(e.value()),
                        onkeydown: move |e: KeyboardEvent| {
                            if (e.modifiers().ctrl() || e.modifiers().meta()) && e.key() == Key::Enter {
                                do_submit();
                            }
                        },
                    }
                    img {
                        src: "/images/xiantiaoxiaogou_input_bg.webp",
                        alt: "",
                        class: "absolute bottom-2 right-2 w-20 sm:w-24 opacity-15 dark:opacity-20 pointer-events-none select-none z-0",
                    }
                }

                // 蜜罐字段：对普通用户隐藏，用于拦截简单机器人（仅匿名渲染；
                // 登录用户的身份由会话保证，服务端也跳过蜜罐校验）。
                if viewer().is_none() {
                    textarea {
                        class: "hidden",
                        aria_hidden: "true",
                        tabindex: "-1",
                        value: "{honeypot}",
                        oninput: move |e| honeypot.set(e.value()),
                    }
                }

                // 底部操作与快捷栏 (Action Toolbar)
                div { class: "flex items-center justify-between px-3.5 py-2.5 bg-[var(--color-paper-theme)]/40 border-t border-[var(--color-paper-border)]/40 text-xs",
                    // 左侧：Markdown 辅助工具图标
                    div { class: "flex items-center gap-1 text-paper-tertiary",
                        button {
                            r#type: "button",
                            class: "p-1.5 rounded-md hover:text-paper-primary hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                            title: "粗体 (**text**)",
                            aria_label: "插入粗体",
                            onclick: move |_| {
                                insert_markdown_snippet(content_md, "**", "**", "粗体文本");
                            },
                            svg {
                                class: "w-3.5 h-3.5",
                                fill: "currentColor",
                                view_box: "0 0 24 24",
                                path { d: "M15.6 10.79c.97-.67 1.65-1.77 1.65-2.79 0-2.26-1.75-4-4-4H7v14h7.04c2.09 0 3.71-1.7 3.71-3.79 0-1.52-.86-2.82-2.15-3.42zM10 6.5h3c.83 0 1.5.67 1.5 1.5s-.67 1.5-1.5 1.5h-3v-3zm3.5 9H10v-3h3.5c.83 0 1.5.67 1.5 1.5s-.67 1.5-1.5 1.5z" }
                            }
                        }
                        button {
                            r#type: "button",
                            class: "p-1.5 rounded-md hover:text-paper-primary hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                            title: "斜体 (*text*)",
                            aria_label: "插入斜体",
                            onclick: move |_| {
                                insert_markdown_snippet(content_md, "*", "*", "斜体文本");
                            },
                            svg {
                                class: "w-3.5 h-3.5",
                                fill: "currentColor",
                                view_box: "0 0 24 24",
                                path { d: "M10 4v3h2.21l-3.42 8H6v3h8v-3h-2.21l3.42-8H18V4z" }
                            }
                        }
                        button {
                            r#type: "button",
                            class: "p-1.5 rounded-md hover:text-paper-primary hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                            title: "行内代码 (`code`)",
                            aria_label: "插入代码",
                            onclick: move |_| {
                                insert_markdown_snippet(content_md, "`", "`", "代码");
                            },
                            svg {
                                class: "w-3.5 h-3.5",
                                fill: "currentColor",
                                view_box: "0 0 24 24",
                                path { d: "M9.4 16.6L4.8 12l4.6-4.6L8 6l-6 6 6 6 1.4-1.4zm5.2 0l4.6-4.6-4.6-4.6L16 6l6 6-6 6-1.4-1.4z" }
                            }
                        }
                        button {
                            r#type: "button",
                            class: "p-1.5 rounded-md hover:text-paper-primary hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                            title: "引用 (> quote)",
                            aria_label: "插入引用",
                            onclick: move |_| {
                                insert_markdown_snippet(content_md, "\n> ", "\n", "引用内容");
                            },
                            svg {
                                class: "w-3.5 h-3.5",
                                fill: "currentColor",
                                view_box: "0 0 24 24",
                                path { d: "M6 17h3l2-4V7H5v6h3zm8 0h3l2-4V7h-6v6h3z" }
                            }
                        }
                        button {
                            r#type: "button",
                            class: "p-1.5 rounded-md hover:text-paper-primary hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                            title: "链接 ([text](url))",
                            aria_label: "插入链接",
                            onclick: move |_| {
                                insert_markdown_snippet(content_md, "[", "](https://example.com)", "链接文字");
                            },
                            svg {
                                class: "w-3.5 h-3.5",
                                fill: "currentColor",
                                view_box: "0 0 24 24",
                                path { d: "M3.9 12c0-1.71 1.39-3.1 3.1-3.1h4V7H7c-2.76 0-5 2.24-5 5s2.24 5 5 5h4v-1.9H7c-1.71 0-3.1-1.39-3.1-3.1zM8 13h8v-2H8v2zm9-6h-4v1.9h4c1.71 0 3.1 1.39 3.1 3.1s-1.39 3.1-3.1 3.1h-4V17h4c2.76 0 5-2.24 5-5s-2.24-5-5-5z" }
                            }
                        }
                    }

                    // 右侧：快捷键提示 + 取消（若为回复）+ 提交按钮
                    div { class: "flex items-center gap-2",
                        span { class: "hidden sm:inline-block text-[11px] text-paper-tertiary font-mono mr-1", "Ctrl + ↵" }
                        if is_reply {
                            button {
                                r#type: "button",
                                class: "px-3 py-1.5 text-xs text-paper-secondary hover:text-paper-primary hover:bg-[var(--color-paper-entry)] rounded-full transition-colors cursor-pointer",
                                onclick: move |_| active_reply.set(None),
                                "取消"
                            }
                        }
                        button {
                            r#type: "button",
                            class: "{BTN_PRIMARY_SM}",
                            class: if submitting() { "opacity-60 cursor-not-allowed pointer-events-none" } else { "" },
                            disabled: submitting(),
                            onclick: move |_| {
                                do_submit();
                            },
                            if submitting() {
                                span { class: "inline-flex items-center gap-1.5",
                                    span { class: "inline-block", dangerous_inner_html: "{SPINNER_SVG}" }
                                    "提交中…"
                                }
                            } else if is_reply {
                                "回复"
                            } else {
                                "发表评论"
                            }
                        }
                    }
                }
            }
        }
    }
}
