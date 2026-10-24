//! 评论表单组件
//!
//! 提供发表评论与回复评论的表单，包含昵称、邮箱、网站、内容与反垃圾蜜罐字段。
//! 内容编辑用 tiptap 所见即所得编辑器（comment variant——后台文章编辑器的
//! 精简子集）：选中浮出气泡菜单（B/I/S/code/link）、StarterKit 输入规则
//! （`> `、`- `、``` 等）、数学公式、图片上传（点击按钮/粘贴/拖放均走
//! coordinator 占位符上传，loading/error 态与后台完全一致）。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::bridges::library::{library_ready, use_browser_library, LibraryLoadError};
use crate::bridges::tiptap::{UploadErrorEntry, UploadsInFlight};
use crate::components::comments::section::{CommentContext, CommentSubmission};
use crate::components::forms::{AlertBox, FormInput};
use crate::components::ui::{
    ime_composing, Button, ButtonSize, ButtonVariant, Composer, ComposerSize, Icon, IconName,
    UserAvatar, SPINNER_SVG,
};
use crate::utils::comment_storage;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;

/// 顶层表单的视口观察器：表单接近视口（300px）时才加载编辑器；drop 时断开观察。
#[cfg(target_arch = "wasm32")]
struct EditorVisibility {
    observer: web_sys::IntersectionObserver,
    _callback: Closure<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for EditorVisibility {
    fn drop(&mut self) {
        self.observer.disconnect();
    }
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
/// - 提交交给 CommentContext::submit；成功后清空内容、关闭回复并触发列表刷新
///
/// 回复表单未激活时不渲染任何 DOM（与此前的早退行为一致），草稿信号随组件存活保留。
#[component]
pub fn CommentForm(
    post_id: i32,
    parent_id: Option<i64>,
    parent_indent: Option<i32>,
) -> impl IntoView {
    let ctx = expect_context::<CommentContext>();
    let active_reply = ctx.active_reply;
    let refresh_trigger = ctx.refresh_trigger;
    let viewer = ctx.current_user;
    let image_upload = ctx.image_upload;

    let author_name = RwSignal::new(String::new());
    let author_email = RwSignal::new(String::new());
    let author_url = RwSignal::new(String::new());
    let content_md = RwSignal::new(String::new());
    let honeypot = RwSignal::new(String::new());
    let submitting = RwSignal::new(false);
    let message = RwSignal::new(Option::<(String, &'static str)>::None);
    let editor_visible = RwSignal::new(false);
    // 回复表单：当前未激活回复时隐藏；顶层表单恒为激活。
    let is_active = Memo::new(move |_| match parent_id {
        Some(pid) => active_reply.get() == Some(pid),
        None => true,
    });
    let editor_library = use_browser_library("tiptap", move || match parent_id {
        Some(pid) => active_reply.get() == Some(pid),
        None => editor_visible.get(),
    });
    // 图片上传状态（tiptap coordinator 事件驱动）：
    // 进行中计数用于提交门控；失败条目由编辑器内错误态兜底（重试/移除）。
    let uploads_in_flight = RwSignal::new(UploadsInFlight::default());
    let upload_errors: RwSignal<Vec<UploadErrorEntry>> = RwSignal::new(Vec::new());

    // 首次挂载时从本地存储加载作者信息（Effect 只在浏览器执行，不影响水合）
    Effect::new(move |_| {
        if let Some(info) = comment_storage::load_author() {
            author_name.set(info.name);
            author_email.set(info.email);
            author_url.set(info.url);
        }
    });

    let is_reply = parent_id.is_some();

    // 用于区分顶层表单与多个回复表单的 id 后缀，保证页面内 label/for 关联唯一。
    let id_suffix = format!(
        "{}-{}",
        ctx.id_scope,
        parent_id.map_or_else(|| "root".to_string(), |pid| pid.to_string())
    );
    // 图片上传 file input 的 DOM id（label for 关联）。
    let image_input_dom_id = format!("comment-image-{id_suffix}");

    // 回复表单抵消父评论缩进，让表单回到内容区左边缘，避免深层回复时被越挤越右。
    let negative_margin = match (is_reply, parent_indent) {
        (true, Some(px)) if px > 0 => Some(format!("margin-left: -{px}px;")),
        _ => None,
    };

    // tiptap 编辑器挂载容器 id（顶层表单与多个回复表单并存，按 id_suffix 隔离）。
    let editor_dom_id = format!("comment-editor-{id_suffix}");

    let form_ref = NodeRef::<leptos::html::Div>::new();

    // 编辑器实例句柄（WASM 端）：持有 JS 实例与全部 closure，drop（含组件卸载时
    // StoredValue 释放）时销毁。
    #[cfg(target_arch = "wasm32")]
    let editor_handle: StoredValue<Option<crate::bridges::tiptap::EditorHandle>, LocalStorage> =
        StoredValue::new_local(None);

    // 顶层表单：表单节点挂载后用 IntersectionObserver 等其接近视口再加载编辑器。
    #[cfg(target_arch = "wasm32")]
    {
        let visibility_observer: StoredValue<Option<EditorVisibility>, LocalStorage> =
            StoredValue::new_local(None);
        Effect::new(move |_| {
            use wasm_bindgen::JsCast;
            let Some(element) = form_ref.get() else {
                return;
            };
            if is_reply
                || editor_visible.get_untracked()
                || visibility_observer.with_value(|o| o.is_some())
            {
                return;
            }
            let callback = Closure::<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>::new(
                move |entries: js_sys::Array, observer: web_sys::IntersectionObserver| {
                    if entries.iter().any(|entry| {
                        entry
                            .dyn_into::<web_sys::IntersectionObserverEntry>()
                            .is_ok_and(|entry| entry.is_intersecting())
                    }) {
                        observer.disconnect();
                        editor_visible.set(true);
                    }
                },
            );
            let options = web_sys::IntersectionObserverInit::new();
            options.set_root_margin("300px");
            match web_sys::IntersectionObserver::new_with_options(
                callback.as_ref().unchecked_ref(),
                &options,
            ) {
                Ok(observer) => {
                    observer.observe(&element);
                    visibility_observer.set_value(Some(EditorVisibility {
                        observer,
                        _callback: callback,
                    }));
                }
                Err(_) => editor_visible.set(true),
            }
        });
    }

    // 挂载/销毁编辑器。顶层表单接近视口时加载；回复表单随 active_reply 激活挂载、
    // 取消时销毁（容器 div 随视图分支消失，JS 实例必须同步回收）。
    // content_md 在组件存活期内持久，重挂载时经 set_markdown 回填（取消不丢草稿）。
    // 挂载推迟到下一帧：保证激活后的表单 DOM 已提交，容器可被找到。
    #[cfg(target_arch = "wasm32")]
    {
        let editor_dom_id = editor_dom_id.clone();
        Effect::new(move |_| {
            if !is_active.get() {
                // 隐藏：销毁已挂载的编辑器（handle drop → JS destroy + closure 释放）。
                if editor_handle.with_value(|h| h.is_some()) {
                    editor_handle.set_value(None);
                }
                return;
            }
            // 防重复挂载（effect 可能因订阅的信号多次触发）。
            if editor_handle.with_value(|h| h.is_some()) {
                return;
            }
            if !library_ready(editor_library) {
                return;
            }
            let editor_dom_id = editor_dom_id.clone();
            request_animation_frame(move || {
                // 组件已卸载、已取消回复或已挂载：放弃本次挂载。
                if is_active.try_get_untracked() != Some(true) {
                    return;
                }
                if editor_handle.try_with_value(|h| h.is_some()) != Some(false) {
                    return;
                }

                let on_update: Closure<dyn FnMut(String)> =
                    Closure::new(move |md: String| content_md.set(md));
                let on_ready: Closure<dyn FnMut()> = Closure::new(|| {});
                let on_image_upload: Closure<dyn Fn(web_sys::File) -> js_sys::Promise> =
                    if image_upload {
                        crate::bridges::tiptap::make_comment_upload_closure()
                    } else {
                        Closure::new(|_file: web_sys::File| {
                            js_sys::Promise::reject(&"评论图片上传已关闭".into())
                        })
                    };
                let on_upload_event: Closure<dyn FnMut(crate::bridges::tiptap::UploadEventJs)> =
                    Closure::new(move |ev: crate::bridges::tiptap::UploadEventJs| {
                        // 失败时借表单 AlertBox 同步一行错误（编辑器内错误态为主，
                        // 这里兜底可见性）；成功/移除时若当前消息恰是上传错误则清除。
                        match ev.kind().as_str() {
                            "error" => {
                                let msg = ev.error_msg().unwrap_or_else(|| "上传失败".to_string());
                                message.set(Some((format!("图片上传失败：{msg}"), "error")));
                            }
                            "success" | "removed"
                                if message.with_untracked(|m| {
                                    m.as_ref()
                                        .is_some_and(|(m, _)| m.starts_with("图片上传失败"))
                                }) =>
                            {
                                message.set(None);
                            }
                            _ => {}
                        }
                        crate::bridges::tiptap::consume_upload_event(
                            &ev,
                            uploads_in_flight,
                            upload_errors,
                        );
                    });

                let opts = crate::bridges::tiptap::EditorOptions::new();
                opts.set_variant("comment");
                opts.set_placeholder(if is_reply {
                    "写下你的回复…"
                } else {
                    "写下你的想法…"
                });
                opts.set_on_update(&on_update);
                opts.set_on_ready(&on_ready);
                if image_upload {
                    opts.set_on_image_upload(&on_image_upload);
                }
                opts.set_on_upload_event(&on_upload_event);

                // create 同步返回；找不到容器返回 None。
                match crate::bridges::tiptap::get_module().create(&editor_dom_id, &opts) {
                    Ok(Some(inst)) => {
                        // 草稿回填：回复表单取消再激活时恢复 content_md。
                        let draft = content_md.get_untracked();
                        if !draft.is_empty() {
                            let _ = inst.set_markdown(&draft);
                        }
                        let handle = crate::bridges::tiptap::EditorHandle::new_comment(
                            inst,
                            on_update,
                            on_image_upload,
                            on_ready,
                            on_upload_event,
                        );
                        editor_handle.set_value(Some(handle));
                    }
                    Ok(None) => {
                        web_sys::console::warn_1(
                            &format!("评论编辑器容器未找到: #{editor_dom_id}").into(),
                        );
                    }
                    Err(e) => {
                        web_sys::console::error_1(&e);
                        message.set(Some((
                            "评论编辑器加载失败，请刷新页面重试".to_string(),
                            "error",
                        )));
                    }
                }
            });
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (upload_errors, image_upload);

    let do_submit = move || {
        if submitting.get_untracked() || !editor_library.ready_untracked() {
            return;
        }

        let is_anon = viewer.with_untracked(|v| v.is_none());
        // 登录用户的身份字段由服务端从会话推导，表单值仅匿名路径使用。
        let (name, email, url_val) = if is_anon {
            (
                author_name.get_untracked(),
                author_email.get_untracked(),
                author_url.get_untracked(),
            )
        } else {
            (String::new(), String::new(), String::new())
        };
        let content = content_md.get_untracked();
        let hp = honeypot.get_untracked();

        // 图片上传未完成拦截：占位节点未落定前提交会丢图/留下 blob 半成品。
        let in_flight = uploads_in_flight.get_untracked();
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
        let request = ctx.submit.run(CommentSubmission {
            post_id,
            parent_id,
            anonymous: is_anon,
            author_name: name,
            author_email: email,
            author_url: url_val,
            content_md: content,
            honeypot: hp,
        });
        spawn_local(async move {
            let result = request.await;
            let _ = submitting.try_set(false);
            match result {
                Ok(msg) => {
                    let _ = content_md.try_set(String::new());
                    // 同步清空编辑器文档（onUpdate 会把空 markdown 回写 signal）。
                    #[cfg(target_arch = "wasm32")]
                    let _ = editor_handle.try_with_value(|h| {
                        if let Some(handle) = h {
                            let _ = handle.instance().set_markdown("");
                        }
                    });
                    let _ = message.try_set(Some((msg, "success")));
                    if parent_id.is_some() {
                        let _ = active_reply.try_set(None);
                    }
                    let _ = refresh_trigger.try_update(|v| *v = !*v);
                }
                Err(msg) => {
                    let _ = message.try_set(Some((msg, "error")));
                }
            }
        });
    };

    let form_class = if is_reply {
        "comment-form is-reply"
    } else {
        "comment-form"
    };
    let form_label = if is_reply {
        "回复评论"
    } else {
        "发表评论"
    };
    let composer_size = if is_reply {
        ComposerSize::Sm
    } else {
        ComposerSize::Md
    };
    let mount_class = if is_reply {
        "comment-editor-mount is-reply"
    } else {
        "comment-editor-mount"
    };

    move || {
        if !is_active.get() {
            return ().into_any();
        }
        let negative_margin = negative_margin.clone();
        let editor_dom_id = editor_dom_id.clone();
        let fields_suffix = id_suffix.clone();
        let image_input_dom_id = image_input_dom_id.clone();

        view! {
            <div
                class=form_class
                style=negative_margin
                role="form"
                aria-label=form_label
                node_ref=form_ref
                on:focusin=move |_| editor_visible.set(true)
                on:pointerdown=move |_| editor_visible.set(true)
                // Ctrl/Cmd+Enter 提交：编辑器内按键事件冒泡到表单根（ProseMirror
                // 不消费 Mod-Enter），身份输入栏同样生效。
                on:keydown=move |e| {
                    if !ime_composing(false, &e) && (e.ctrl_key() || e.meta_key())
                        && e.key() == "Enter"
                    {
                        do_submit();
                    }
                }
            >
                {move || {
                    message
                        .get()
                        .map(|(msg, variant)| {
                            view! {
                                <div class="mb-3" aria-live="polite">
                                    <AlertBox message=msg variant=variant/>
                                </div>
                            }
                        })
                }}
                <LibraryLoadError library=editor_library/>

                <Composer
                    stacked=true
                    size=composer_size
                    class="comment-composer"
                >
                    // 登录用户：身份行；访客的昵称、邮箱、网站放在编辑区下方。
                    {move || {
                        viewer
                            .get()
                            .map(|user| {
                                let label = user.display_label().to_string();
                                let action = if is_reply { "回复" } else { "发表评论" };
                                view! {
                                    <div class="comment-identity">
                                        <UserAvatar
                                            name=label.clone()
                                            avatar_url=user.avatar_url.clone()
                                            class="comment-identity-avatar"
                                        />
                                        <span>
                                            "以 " <strong>{label}</strong> {format!(" 的身份{action}")}
                                        </span>
                                    </div>
                                }
                            })
                    }}

                    // 编辑区（WYSIWYG：tiptap comment variant）。SSR 输出空容器，
                    // hydration 后由编辑器接管；min-height 与编辑器对齐避免 CLS。
                    <div class="comment-editor">
                        <div
                            id=editor_dom_id
                            class=mount_class
                        ></div>
                        {move || {
                            (!library_ready(editor_library))
                                .then(|| {
                                    view! {
                                        <span class="comment-editor-loading">"正在加载编辑器…"</span>
                                    }
                                })
                        }}
                    </div>

                    // 蜜罐字段：对普通用户隐藏，用于拦截简单机器人（仅匿名渲染；
                    // 登录用户的身份由会话保证，服务端也跳过蜜罐校验）。
                    {move || {
                        let fields_suffix = fields_suffix.clone();
                        viewer
                            .with(|v| v.is_none())
                            .then(move || {
                                view! {
                                    <textarea
                                        class="hidden"
                                        aria-hidden="true"
                                        tabindex="-1"
                                        prop:value=move || honeypot.get()
                                        on:input=move |e| honeypot.set(event_target_value(&e))
                                    ></textarea>
                                    <div class="comment-fields">
                                        <FormInput
                                            id=format!("comment-name-{fields_suffix}")
                                            r#type="text"
                                            placeholder="昵称"
                                            aria_label="昵称（必填）"
                                            value=author_name
                                            disabled=submitting
                                            class="comment-field"
                                            oninput=move |v: String| author_name.set(v)
                                        />
                                        <FormInput
                                            id=format!("comment-email-{fields_suffix}")
                                            r#type="email"
                                            placeholder="邮箱（不公开）"
                                            aria_label="邮箱（必填，不公开）"
                                            value=author_email
                                            disabled=submitting
                                            class="comment-field"
                                            oninput=move |v: String| author_email.set(v)
                                        />
                                        <FormInput
                                            id=format!("comment-url-{fields_suffix}")
                                            r#type="url"
                                            placeholder="网站（可选）"
                                            aria_label="网站（可选）"
                                            value=author_url
                                            disabled=submitting
                                            class="comment-field"
                                            oninput=move |v: String| author_url.set(v)
                                        />
                                    </div>
                                }
                            })
                    }}

                    <div class="comment-bar">
                        // 左侧：图片上传（点击选择 / 直接粘贴图片到输入框）与提示
                        <div class="comment-bar-start">
                            // label + 隐藏 file input：点击天然触发文件选择对话框，无需 JS。
                            {image_upload
                                .then(|| {
                                    view! {
                                        <label
                                            for=image_input_dom_id.clone()
                                            class="btn btn-sm btn-icon btn-ghost"
                                            title="上传图片（也可直接粘贴到输入框）"
                                            aria-label="上传图片"
                                        >
                                            <Icon name=IconName::Image size=16/>
                                        </label>
                                        <input
                                            id=image_input_dom_id
                                            class="hidden"
                                            type="file"
                                            accept="image/jpeg,image/png,image/gif,image/webp"
                                            multiple=true
                                            on:change=move |ev| {
                                                #[cfg(target_arch = "wasm32")]
                                                {
                                                    // 与粘贴/拖放同一条 coordinator 占位上传路径。
                                                    let input = event_target::<
                                                        web_sys::HtmlInputElement,
                                                    >(&ev);
                                                    if let Some(files) = input.files() {
                                                        for i in 0..files.length() {
                                                            if let Some(file) = files.get(i) {
                                                                editor_handle
                                                                    .with_value(|h| {
                                                                        if let Some(handle) = h {
                                                                            handle
                                                                                .instance()
                                                                                .insert_uploading(file);
                                                                        }
                                                                    });
                                                            }
                                                        }
                                                    }
                                                    // 重置 value：连续选择同一文件也能再次触发 change。
                                                    input.set_value("");
                                                }
                                                #[cfg(not(target_arch = "wasm32"))]
                                                let _ = ev;
                                            }
                                        />
                                    }
                                })}
                            {move || {
                                if uploads_in_flight.with(|u| u.uploading > 0) {
                                    view! {
                                        <span class="comment-hint">
                                            <span class="inline-block" inner_html=SPINNER_SVG></span>
                                            "图片上传中…"
                                        </span>
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <span class="comment-hint comment-hint-wide">
                                            {move || {
                                                if viewer.with(|v| v.is_none()) {
                                                    "支持 Markdown 与图片，审核后显示"
                                                } else {
                                                    "支持 Markdown 与图片 · ⌘↵ 发表"
                                                }
                                            }}
                                        </span>
                                    }
                                        .into_any()
                                }
                            }}
                        </div>

                        // 右侧：取消（若为回复）+ 提交按钮
                        <div class="comment-bar-end">
                            {is_reply
                                .then(|| {
                                    view! {
                                        <button
                                            type="button"
                                            class="btn btn-sm btn-ghost"
                                            on:click=move |_| active_reply.set(None)
                                        >
                                            "取消"
                                        </button>
                                    }
                                })}
                            // 提交中或图片上传未完成（含失败态）均禁用：占位节点未落定前
                            // 提交会丢图或残留 blob URL。
                            <Button
                                variant=ButtonVariant::Primary
                                size=ButtonSize::Sm
                                loading=submitting
                                disabled=Signal::derive(move || {
                                    !library_ready(editor_library)
                                        || uploads_in_flight.with(|u| u.uploading > 0 || u.error > 0)
                                })
                                onclick=move |_| do_submit()
                            >
                                {if is_reply { "回复" } else { "发表" }}
                            </Button>
                        </div>
                    </div>
                </Composer>
            </div>
        }
        .into_any()
    }
}
