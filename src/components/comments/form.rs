//! 评论表单组件
//!
//! 提供发表评论与回复评论的表单，包含昵称、邮箱、网站、内容与反垃圾蜜罐字段。

use dioxus::prelude::*;

use crate::api::comments::create_comment;
use crate::components::comments::section::CommentContext;
use crate::components::forms::{AlertBox, INPUT_CLASS};
use crate::components::ui::UserAvatar;
use crate::utils::comment_storage::{self, PendingComment};

/// 评论提交按钮样式：去掉全宽，改为内联宽度并右对齐。
///
/// 与 `BUTTON_PRIMARY_CLASS` 视觉一致，但不含 `w-full`，并把 `px-4` 加宽为 `px-6`，
/// 使按钮宽度跟随文字、更适合文章页内联场景。
const COMMENT_SUBMIT_CLASS: &str = "py-2.5 px-6 bg-paper-accent text-white font-medium rounded-full hover:brightness-110 active:scale-[0.98] transition-all duration-200 cursor-pointer";

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

    rsx! {
        div {
            class: if is_reply { "mt-3 pt-3 border-t border-gray-100 dark:border-gray-700" } else { "" },
            style: "{negative_margin}",
            role: "form",
            aria_label: if is_reply { "回复评论" } else { "发表评论" },

            if let Some((msg, variant)) = message() {
                div { aria_live: "polite",
                    AlertBox { message: msg, variant }
                }
            }

            div { class: "space-y-3",
                if let Some(user) = viewer() {
                    {
                        let label = user.display_label().to_string();
                        let action = if is_reply { "回复" } else { "发表评论" };
                        rsx! {
                            // 登录态：身份行替代昵称/邮箱/网站字段（身份由服务端会话确定）。
                            div { class: "flex items-center gap-2.5",
                                UserAvatar {
                                    name: label.clone(),
                                    avatar_url: user.avatar_url.clone(),
                                    class: "w-8 h-8 rounded-full text-sm border border-[var(--color-paper-border)]",
                                }
                                span { class: "text-sm text-paper-secondary",
                                    "以 "
                                    span { class: "font-medium text-paper-primary", "{label}" }
                                    " 的身份{action}"
                                }
                            }
                        }
                    }
                } else {
                    div { class: "grid grid-cols-1 sm:grid-cols-2 gap-3",
                        div {
                            label {
                                r#for: "comment-name-{id_suffix}",
                                class: "block text-sm font-medium text-paper-secondary mb-1",
                                "昵称 *"
                            }
                            input {
                                id: "comment-name-{id_suffix}",
                                class: INPUT_CLASS,
                                r#type: "text",
                                placeholder: "你的昵称",
                                value: "{author_name}",
                                disabled: submitting(),
                                oninput: move |e| author_name.set(e.value()),
                            }
                        }
                        div {
                            label {
                                r#for: "comment-email-{id_suffix}",
                                class: "block text-sm font-medium text-paper-secondary mb-1",
                                "邮箱 *"
                            }
                            input {
                                id: "comment-email-{id_suffix}",
                                class: INPUT_CLASS,
                                r#type: "email",
                                placeholder: "your@email.com",
                                value: "{author_email}",
                                disabled: submitting(),
                                oninput: move |e| author_email.set(e.value()),
                            }
                        }
                    }
                    div {
                        label {
                            r#for: "comment-url-{id_suffix}",
                            class: "block text-sm font-medium text-paper-secondary mb-1",
                            "网站"
                        }
                        input {
                            id: "comment-url-{id_suffix}",
                            class: INPUT_CLASS,
                            r#type: "url",
                            placeholder: "https://example.com（可选）",
                            value: "{author_url}",
                            disabled: submitting(),
                            oninput: move |e| author_url.set(e.value()),
                        }
                    }
                }

                div {
                    label {
                        r#for: "comment-content-{id_suffix}",
                        class: "block text-sm font-medium text-paper-secondary mb-1",
                        "内容 *"
                    }
                    div { class: "relative bg-paper-entry rounded-lg",
                        textarea {
                            id: "comment-content-{id_suffix}",
                            class: "{INPUT_CLASS} !bg-transparent relative z-10 peer block min-h-[100px] resize-y",
                            value: "{content_md}",
                            disabled: submitting(),
                            oninput: move |e| content_md.set(e.value()),
                        }
                        img {
                            src: "/images/xiantiaoxiaogou_input_bg.webp",
                            alt: "",
                            class: "absolute bottom-1.5 right-1.5 w-24 opacity-10 pointer-events-none z-0",
                        }
                    }
                }

                p { class: "text-xs text-paper-tertiary", "支持 Markdown 语法" }

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

                div { class: "flex justify-end",
                    button {
                        class: COMMENT_SUBMIT_CLASS,
                        disabled: submitting(),
                        onclick: move |_| {
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
                                message.set(Some(("请填写所有必填项".to_string(), "error")));
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
                                        if url_val.trim().is_empty() { None } else { Some(url_val.clone()) },
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
                                                    comment_storage::save_pending_comment(
                                                        post_id,
                                                        pending.clone(),
                                                    );
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
                                        message
                                            .set(
                                                Some(("提交失败，请稍后重试".to_string(), "error")),
                                            );
                                    }
                                }
                            });
                        },

                        if submitting() {
                            "提交中…"
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
