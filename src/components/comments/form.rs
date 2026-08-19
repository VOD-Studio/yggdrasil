//! 评论表单组件
//!
//! 提供发表评论与回复评论的表单，包含昵称、邮箱、网站、内容与反垃圾蜜罐字段。
//! 支持图片上传：点击工具栏图片按钮或直接粘贴图片到输入框，占位符 +
//! 上传中指示与后台 tiptap 编辑器的语义一致（上传完成前禁止提交）。

use dioxus::prelude::*;

use crate::api::comments::create_comment;
use crate::components::comments::section::CommentContext;
use crate::components::forms::AlertBox;
use crate::components::ui::{UserAvatar, BTN_PRIMARY_SM, SPINNER_SVG};
use crate::utils::comment_storage::{self, PendingComment};

/// 单文件大小硬上限（5MiB），镜像服务端 `crate::utils::server::MAX_FILE_SIZE`。
#[cfg(any(test, target_arch = "wasm32"))]
const MAX_UPLOAD_BYTES: u64 = 5 * 1024 * 1024;
/// 允许的 MIME 白名单，镜像服务端 `api/upload.rs` 的 `ALLOWED_MIME_TYPES`。
#[cfg(any(test, target_arch = "wasm32"))]
const ALLOWED_MIME: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// 预校验：MIME 白名单 + 5MiB 上限。失败返回可读原因（直接展示，不发请求）。
#[cfg(any(test, target_arch = "wasm32"))]
fn validate_image_file(mime: &str, size: u64) -> Result<(), String> {
    if !ALLOWED_MIME.contains(&mime) {
        return Err("不支持的图片格式（仅 JPEG/PNG/GIF/WebP）".to_string());
    }
    if size > MAX_UPLOAD_BYTES {
        return Err("图片超过 5MB 大小限制".to_string());
    }
    Ok(())
}

/// 进行中的评论图片上传。纯数据（两端都可编译）；`web_sys::File` 句柄不进
/// signal，由 spawn 的上传任务直接持有。
#[derive(Clone, PartialEq)]
struct PendingUpload {
    /// 唯一 id（组件内单调计数器），用于完成后从列表移除。
    id: u64,
    /// 插入评论文本的占位 Markdown：`![上传中 name…](uploading-N)`。
    /// 上传成功被替换为最终图片语法，失败被移除；`uploading-N` 后缀同时是
    /// 提交防御检查（残留占位符拦截）的识别标记。
    placeholder: String,
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
    // 进行中的图片上传：占位符语义对齐后台 tiptap 编辑器（写入文本占位 +
    // 工具栏「上传中」指示 + 完成前拦截提交）。
    let mut pending_uploads: Signal<Vec<PendingUpload>> = use_signal(Vec::new);
    let mut upload_seq: Signal<u64> = use_signal(|| 0);

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

    // 图片上传（仅 WASM 端）：校验 → 光标处插入占位符 → POST /api/comments/upload →
    // 成功替换为最终图片语法 / 失败移除占位符并报错。三入口（工具栏按钮 / 文件
    // 选择 / 粘贴）收敛到这一个闭包，语义对齐后台 tiptap 的 UploadCoordinator。
    #[cfg(target_arch = "wasm32")]
    let start_upload = {
        let textarea_dom_id = format!("comment-content-{id_suffix}");
        move |file: web_sys::File| {
            let filename = file.name();
            if let Err(reason) = validate_image_file(&file.type_(), file.size() as u64) {
                message.set(Some((reason, "error")));
                return;
            }

            let id = *upload_seq.peek() + 1;
            upload_seq.set(id);
            let placeholder = format!("![上传中 {filename}…](uploading-{id})");

            // 在光标处插入占位符：走 DOM set_range_text（JS 字符串索引，避免
            // UTF-16/UTF-8 偏移错配导致的中文截断），随后从 DOM 回读完整值同步
            // signal。拿不到 DOM/选区时退化为追加到文末。
            let mut inserted = false;
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                if let Some(el) = doc.get_element_by_id(&textarea_dom_id) {
                    use wasm_bindgen::JsCast;
                    if let Ok(ta) = el.dyn_into::<web_sys::HtmlTextAreaElement>() {
                        let start = ta.selection_start().ok().flatten();
                        let end = ta.selection_end().ok().flatten();
                        if let (Some(start), Some(end)) = (start, end) {
                            // IndexSizeError 等失败时保持 inserted=false，走文末追加兜底。
                            if ta
                                .set_range_text_with_start_and_end(&placeholder, start, end)
                                .is_ok()
                            {
                                content_md.set(ta.value());
                                inserted = true;
                            }
                        }
                    }
                }
            }
            if !inserted {
                let mut text = content_md();
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&placeholder);
                content_md.set(text);
            }

            pending_uploads.write().push(PendingUpload {
                id,
                placeholder: placeholder.clone(),
            });
            message.set(None);

            spawn(async move {
                let result = crate::utils::web_upload::post_multipart_file(
                    "/api/comments/upload",
                    "image",
                    &file,
                )
                .await;
                match result {
                    Ok(data) => {
                        let url = data["url"].as_str().unwrap_or("").to_string();
                        if url.is_empty() {
                            // success=true 但 url 为空：服务端契约异常，按失败处理
                            let text = content_md().replacen(&placeholder, "", 1);
                            content_md.set(text);
                            message
                                .set(Some(("图片上传失败：服务端返回异常".to_string(), "error")));
                        } else {
                            let final_md = format!("![{filename}]({url})");
                            let text = content_md().replacen(&placeholder, &final_md, 1);
                            content_md.set(text);
                        }
                    }
                    Err(err) => {
                        let text = content_md().replacen(&placeholder, "", 1);
                        content_md.set(text);
                        message.set(Some((format!("图片上传失败：{err}"), "error")));
                    }
                }
                pending_uploads.write().retain(|p| p.id != id);
            });
        }
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

        // 图片上传未完成拦截：与后台 write.rs 的保存拦截同语义——占位符还在
        // 文本里时提交会丢失图片/留下半成品。
        let in_flight = pending_uploads.read().len();
        if in_flight > 0 {
            message.set(Some((
                format!("有 {in_flight} 张图片正在上传，请等待完成后再发表"),
                "error",
            )));
            return;
        }
        // 防御：文本中残留上传占位符（异常路径，如上传任务被打断）时拦截，
        // 提示用户自行移除。（与 write.rs 的 blob: 检出同款兜底。）
        if content.contains("](uploading-") {
            message.set(Some((
                "检测到未完成上传的图片，请移除占位内容后再发表".to_string(),
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
                        // 粘贴图片上传：与后台 tiptap 编辑器一致——图片进入占位上传
                        // 流程（占位符 + 上传中指示），纯文本粘贴不拦截。
                        onpaste: {
                            // clone 进闭包：start_upload 在 onchange 里还要再用一份。
                            // cfg 门控使 server 构建（无 start_upload 绑定）闭包为空捕获。
                            #[cfg(target_arch = "wasm32")]
                            let mut start_upload = start_upload.clone();
                            move |e: ClipboardEvent| {
                                #[cfg(target_arch = "wasm32")]
                                {
                                use dioxus::web::WebEventExt;
                                use wasm_bindgen::JsCast;
                                // dioxus-web 0.7 把剪贴板事件包成通用 web_sys::Event；
                                // 真实 paste 事件底层必为 ClipboardEvent，unchecked 转换安全。
                                let Some(ev) = e.try_as_web_event() else {
                                    return;
                                };
                                let ev: &web_sys::ClipboardEvent = ev.unchecked_ref();
                                let Some(dt) = ev.clipboard_data() else {
                                    return;
                                };
                                let Some(list) = dt.files() else {
                                    return;
                                };
                                let files: Vec<web_sys::File> = (0..list.length())
                                    .filter_map(|i| list.item(i))
                                    .collect();
                                if files.is_empty() {
                                    return;
                                }
                                // 剪贴板含文件：阻止默认粘贴（防止文件名文本落入输入框）。
                                ev.prevent_default();
                                for f in files {
                                    start_upload(f);
                                }
                                }
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
                                // clone 进闭包：start_upload 在 onpaste 里已用一份。
                                // cfg 门控使 server 构建（无 start_upload 绑定）闭包为空捕获。
                                #[cfg(target_arch = "wasm32")]
                                let mut start_upload = start_upload.clone();
                                #[cfg(target_arch = "wasm32")]
                                let input_dom_id = image_input_dom_id.clone();
                                move |e| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        use dioxus::web::WebFileExt;
                                        for f in e.files() {
                                            if let Some(web_file) = f.get_web_file() {
                                                start_upload(web_file);
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
                        if !pending_uploads.read().is_empty() {
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
                            // 提交中或图片上传中均禁用：占位符未替换前提交会丢图。
                            class: if submitting() || !pending_uploads.read().is_empty() { "opacity-60 cursor-not-allowed pointer-events-none" } else { "" },
                            disabled: submitting() || !pending_uploads.read().is_empty(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_image_file_accepts_allowed_mime() {
        for mime in ALLOWED_MIME {
            assert!(validate_image_file(mime, 1024).is_ok(), "{mime} 应通过");
        }
    }

    #[test]
    fn validate_image_file_rejects_disallowed_mime() {
        assert!(validate_image_file("image/svg+xml", 1024).is_err());
        assert!(validate_image_file("image/bmp", 1024).is_err());
        assert!(validate_image_file("text/html", 1024).is_err());
        assert!(validate_image_file("", 1024).is_err());
    }

    #[test]
    fn validate_image_file_rejects_oversize() {
        assert!(validate_image_file("image/png", MAX_UPLOAD_BYTES).is_ok());
        assert!(validate_image_file("image/png", MAX_UPLOAD_BYTES + 1).is_err());
    }
}
