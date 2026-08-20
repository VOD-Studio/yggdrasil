//! 评论表单组件
//!
//! 提供发表评论与回复评论的表单，包含昵称、邮箱、网站、内容与反垃圾蜜罐字段。
//! 内容编辑用 tiptap 所见即所得编辑器（comment variant——后台文章编辑器的
//! 精简子集）：选中浮出气泡菜单（B/I/S/code/link）、StarterKit 输入规则
//! （`> `、`- `、``` 等）、数学公式、图片上传（点击按钮/粘贴/拖放均走
//! coordinator 占位符上传，loading/error 态与后台完全一致）。

use dioxus::prelude::*;

use crate::api::comments::create_comment;
use crate::bridges::tiptap::{UploadErrorEntry, UploadsInFlight};
use crate::components::comments::section::CommentContext;
use crate::components::forms::AlertBox;
use crate::components::ui::{UserAvatar, BTN_PRIMARY_SM, SPINNER_SVG};
use crate::utils::comment_storage::{self, PendingComment};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;

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
// 图片上传逻辑整体 cfg(target_arch = "wasm32") 门控：server 构建下
// pending_uploads/upload_seq 的 mut、事件参数 e 均无实际用途。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
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
    // 图片上传状态（tiptap coordinator 事件驱动，与后台 write.rs 同一类型）：
    // 进行中计数用于提交门控；失败条目由编辑器内错误态兜底（重试/移除）。
    let uploads_in_flight = use_signal(UploadsInFlight::default);
    let upload_errors: Signal<Vec<UploadErrorEntry>> = use_signal(Vec::new);

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
    // 图片上传 file input 的 DOM id（label r#for 关联 + onchange 重置 value 共用）。
    let image_input_dom_id = format!("comment-image-{id_suffix}");

    // 回复表单抵消父评论缩进，让表单回到内容区左边缘，避免深层回复时被越挤越右。
    let negative_margin = match (is_reply, parent_indent) {
        (true, Some(px)) if px > 0 => format!("margin-left: -{px}px;"),
        _ => String::new(),
    };

    // tiptap 编辑器挂载容器 id（顶层表单与多个回复表单并存，按 id_suffix 隔离）。
    let editor_dom_id = format!("comment-editor-{id_suffix}");

    // 编辑器实例句柄（WASM 端）：持有 JS 实例与全部 closure，drop 时销毁。
    #[cfg(target_arch = "wasm32")]
    let mut editor_handle: Signal<Option<crate::bridges::tiptap::EditorHandle>> =
        use_signal(|| None);

    // 挂载/销毁编辑器。顶层表单立即挂载；回复表单随 active_reply 激活挂载、
    // 取消时销毁（容器 div 随 rsx 早退消失，JS 实例必须同步回收）。
    // content_md 在组件存活期内持久，重挂载时经 set_markdown 回填（取消不丢草稿）。
    #[cfg(target_arch = "wasm32")]
    let editor_dom_id_for_mount = editor_dom_id.clone();
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let editor_dom_id = editor_dom_id_for_mount.clone();
        let active = match parent_id {
            Some(pid) => active_reply() == Some(pid),
            None => true,
        };
        if !active {
            // 隐藏：销毁已挂载的编辑器（handle drop → JS destroy + closure 释放）。
            if editor_handle.peek().is_some() {
                editor_handle.set(None);
            }
            return;
        }
        // 防重复挂载（effect 可能因订阅的信号多次触发）。
        if editor_handle.peek().is_some() {
            return;
        }

        // 用 FnMut：Dioxus Signal 的 set 接收 &mut self，回调需可变借用捕获的 signal。
        let on_update = Closure::new({
            let mut content_md = content_md;
            move |md: String| content_md.set(md)
        });
        let on_ready = Closure::new(|| {});
        let on_image_upload = crate::bridges::tiptap::make_comment_upload_closure();
        let on_upload_event = Closure::new({
            let mut message = message;
            move |ev: crate::bridges::tiptap::UploadEventJs| {
                // 失败时借表单 AlertBox 同步一行错误（编辑器内错误态为主，
                // 这里兜底可见性）；成功/移除时若当前消息恰是上传错误则清除。
                match ev.kind().as_str() {
                    "error" => {
                        let msg = ev.error_msg().unwrap_or_else(|| "上传失败".to_string());
                        message.set(Some((format!("图片上传失败：{msg}"), "error")));
                    }
                    "success" | "removed" => {
                        if message
                            .peek()
                            .as_ref()
                            .is_some_and(|(m, _)| m.starts_with("图片上传失败"))
                        {
                            message.set(None);
                        }
                    }
                    _ => {}
                }
                crate::bridges::tiptap::consume_upload_event(&ev, uploads_in_flight, upload_errors);
            }
        });

        let opts = crate::bridges::tiptap::EditorOptions::new();
        opts.set_variant("comment");
        opts.set_placeholder(if is_reply {
            "写下你的回复..."
        } else {
            "写下你的想法..."
        });
        opts.set_on_update(&on_update);
        opts.set_on_ready(&on_ready);
        opts.set_on_image_upload(&on_image_upload);
        opts.set_on_upload_event(&on_upload_event);

        // create 同步返回；找不到容器返回 None（rsx 早退时容器不在 DOM）。
        match crate::bridges::tiptap::get_module().create(&editor_dom_id, &opts) {
            Ok(Some(inst)) => {
                // 草稿回填：回复表单取消再激活时恢复 content_md（组件未卸载，
                // signal 持久）。顶层表单为空字符串时 no-op。
                let draft = content_md.peek().clone();
                if !draft.is_empty() {
                    inst.set_markdown(&draft);
                }
                let handle = crate::bridges::tiptap::EditorHandle::new_comment(
                    inst,
                    on_update,
                    on_image_upload,
                    on_ready,
                    on_upload_event,
                );
                editor_handle.set(Some(handle));
            }
            Ok(None) => {
                web_sys::console::warn_1(&format!("评论编辑器容器未找到: #{editor_dom_id}").into());
            }
            Err(e) => {
                message.set(Some((format!("编辑器初始化失败: {e:?}"), "error")));
            }
        }
    });

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

        // 图片上传未完成拦截：与后台 write.rs 的保存拦截同语义——占位节点
        // 未落定前提交会丢图/留下 blob 半成品。
        let in_flight = uploads_in_flight();
        if in_flight.uploading > 0 || in_flight.error > 0 {
            let msg = if in_flight.uploading > 0 {
                format!(
                    "有 {} 张图片正在上传，请等待完成后再发表",
                    in_flight.uploading
                )
            } else {
                format!(
                    "有 {} 张图片上传失败，请重试或移除后再发表",
                    in_flight.error
                )
            };
            message.set(Some((msg, "error")));
            return;
        }
        // 防御：markdown 中检出 blob 图片 URL（异常路径，如上传事件丢失）时拦截。
        // （与 write.rs 的 blob: 检出同款兜底。）
        if content.contains("](blob:") {
            message.set(Some((
                "检测到未完成上传的图片，请处理后再发表".to_string(),
                "error",
            )));
            return;
        }

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
                        // 同步清空编辑器文档（onUpdate 会把空 markdown 回写 signal）。
                        #[cfg(target_arch = "wasm32")]
                        if let Some(handle) = &*editor_handle.peek() {
                            handle.instance().set_markdown("");
                        }
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
            // Ctrl/Cmd+Enter 提交：编辑器内按键事件冒泡到表单根（ProseMirror
            // 不消费 Mod-Enter），身份输入栏同样生效。
            onkeydown: move |e: KeyboardEvent| {
                if (e.modifiers().ctrl() || e.modifiers().meta()) && e.key() == Key::Enter {
                    do_submit();
                }
            },

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
                // 编辑区（WYSIWYG：tiptap comment variant）。SSR 输出空容器，
                // hydration 后由编辑器接管；min-height 与编辑器对齐避免 CLS。
                div { class: "relative bg-transparent",
                    div {
                        id: "{editor_dom_id}",
                        class: "comment-editor-mount min-h-[96px]",
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
                    // 左侧：图片上传（点击选择 / 直接粘贴图片到输入框）
                    div { class: "flex items-center gap-1.5 text-paper-tertiary",
                        // label + 隐藏 file input：点击天然触发文件选择对话框，无需 JS。
                        label {
                            r#for: "{image_input_dom_id}",
                            class: "p-1.5 rounded-md hover:text-paper-primary hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                            title: "上传图片（也可直接粘贴到输入框）",
                            aria_label: "上传图片",
                            svg {
                                class: "w-3.5 h-3.5",
                                fill: "currentColor",
                                view_box: "0 -960 960 960",
                                path { d: "M180-120q-24 0-42-18t-18-42v-600q0-24 18-42t42-18h600q24 0 42 18t18 42v600q0 24-18 42t-42 18H180Zm0-60h600v-600H180v600Zm56-97h489L578-473 446-302l-93-127-117 152Zm-56 97v-600 600Z" }
                            }
                        }
                        input {
                            id: "{image_input_dom_id}",
                            class: "hidden",
                            r#type: "file",
                            accept: "image/jpeg,image/png,image/gif,image/webp",
                            multiple: true,
                            onchange: {
                                // cfg 门控使 server 构建（无 editor_handle 绑定）闭包为空捕获。
                                #[cfg(target_arch = "wasm32")]
                                let input_dom_id = image_input_dom_id.clone();
                                move |e| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        use dioxus::web::WebFileExt;
                                        // 与粘贴/拖放同一条 coordinator 占位上传路径。
                                        for f in e.files() {
                                            if let Some(web_file) = f.get_web_file() {
                                                if let Some(handle) = &*editor_handle.peek() {
                                                    handle.instance().insert_uploading(web_file);
                                                }
                                            }
                                        }
                                        // 重置 value：连续选择同一文件也能再次触发 change。
                                        if let Some(el) = web_sys::window()
                                            .and_then(|w| w.document())
                                            .and_then(|d| d.get_element_by_id(&input_dom_id))
                                        {
                                            use wasm_bindgen::JsCast;
                                            if let Some(input) = el.dyn_ref::<web_sys::HtmlInputElement>() {
                                                input.set_value("");
                                            }
                                        }
                                    }
                                }
                            },
                        }
                        // 上传中指示（与后台 tiptap 的「上传中…」遮罩同文案）。
                        if uploads_in_flight().uploading > 0 {
                            span { class: "inline-flex items-center gap-1.5 text-[11px] text-paper-secondary ml-1",
                                span { class: "inline-block", dangerous_inner_html: "{SPINNER_SVG}" }
                                "图片上传中…"
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
                            // 提交中或图片上传未完成（含失败态）均禁用：占位节点未落定前
                            // 提交会丢图或残留 blob URL。
                            class: if submitting() || uploads_in_flight().uploading > 0 || uploads_in_flight().error > 0 { "opacity-60 cursor-not-allowed pointer-events-none" } else { "" },
                            disabled: submitting() || uploads_in_flight().uploading > 0 || uploads_in_flight().error > 0,
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
