//! 文章编辑器页面。
//!
//! 提供新建文章与编辑文章两种模式，使用基于 Tiptap 的富文本编辑器。
//! 编辑器通过 [`crate::tiptap_bridge`] 的 wasm-bindgen 绑定在 WASM 前端初始化，
//! 并与 `window.TiptapEditor` 实例交互，实现 Markdown 内容回填、图片上传与组件卸载时的清理。

// prelude 在 WASM 构建里直接使用（use_signal/Signal/Element 等）；
// server 构建里 #[component] 宏会重新导出这些符号导致 native 报 unused，故 allow。
#[allow(unused_imports)]
use dioxus::prelude::*;

// 仅在 WASM 前端使用的类型转换与文章 API。
#[cfg(target_arch = "wasm32")]
use crate::api::posts::{
    create_post, get_post_by_id, update_post, CreatePostResponse, SinglePostResponse,
};
#[cfg(target_arch = "wasm32")]
use crate::tiptap_bridge::{consume_upload_event, upload_image_file, EditorHandle};
// 共享上传状态类型：两端都编译（rsx 在 server SSR 时也要渲染这些结构）。
use crate::components::forms::{FormInput, FormSelect, INPUT_INLINE_CLASS};
use crate::components::ui::{LoadingButton, BTN_CLOSE_ICON, BTN_PRIMARY_SM};
use crate::components::write_skeleton::WriteSkeleton;
use crate::models::post::Post;
use crate::pages::admin::asset_picker::{AssetPickerModal, AssetSelection};
use crate::router::Route;
use crate::tiptap_bridge::{UploadErrorEntry, UploadsInFlight};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;
// 封面图上传：从 Dioxus 事件拿底层 web_sys::File，以及 paste 事件的原始 web 事件。
// 这三个 trait 仅在 WASM 端可用（dioxus-web 提供），通过 dioxus 的 re-export 访问：
// - HasFileData：evt.files()（FormEvent / DragEvent 取文件）— dioxus::html
// - WebFileExt：file.get_web_file()（FileData 取底层 web_sys::File）— dioxus::web
// - WebEventExt：evt.try_as_web_event()（ClipboardEvent 取原始 web 事件）— dioxus::web
#[cfg(target_arch = "wasm32")]
use crate::utils::js::invoke_optional_global;
#[cfg(target_arch = "wasm32")]
use dioxus::html::HasFileData;
#[cfg(target_arch = "wasm32")]
use dioxus::web::{WebEventExt, WebFileExt};

/// 新建文章页面组件。
///
/// 内部委托给 `write_editor`，以 `None` 表示新建模式。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn Write() -> Element {
    write_editor(None)
}

/// 编辑文章页面组件。
///
/// `id` 为要编辑的文章 ID，内部委托给 `write_editor` 加载现有数据。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn WriteEdit(id: i32) -> Element {
    write_editor(Some(id))
}

/// 文章编辑器核心组件，支持新建（`post_id == None`）与编辑模式。
///
/// 负责：
/// - 编辑模式下通过 server function 拉取文章数据；
/// - 在 WASM 前端通过 tiptap_bridge 的 closure 回调初始化 Tiptap 富文本编辑器；
/// - 编辑模式下将 Markdown 内容回填到编辑器；
/// - 提交时读取编辑器 Markdown、校验并调用 create_post / update_post；
/// - 组件卸载时销毁 Tiptap 实例（EditorHandle::drop 自动 destroy + 释放 closure）。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn write_editor(post_id: Option<i32>) -> Element {
    let is_edit = post_id.is_some();

    // 文章元信息表单字段。
    let mut title = use_signal(|| "".to_string());
    let mut summary = use_signal(|| "".to_string());
    let mut slug = use_signal(|| "".to_string());
    let mut tags = use_signal(|| "".to_string());
    let mut cover_image = use_signal(|| "".to_string());
    // 封面上传中状态：由子组件 CoverUploader 写入，本组件在 on_submit 读取以拦截保存。
    // 无需 mut：本组件只读取（L254）与传递（L452），写入都在 CoverUploader 内部，
    // 而 Dioxus Signal 是 Copy 类型，.set() 不要求 mut 绑定。
    let cover_uploading = use_signal(|| false);
    // 正文素材选择弹窗：slash 命令「素材库」经 onPickFromLibrary 回调打开（多选模式）；
    // body_picker_uploading 承载弹窗内上传中状态，供 on_submit 拦截（语义同 cover_uploading）。
    let body_picker_visible = use_signal(|| false);
    let body_picker_uploading = use_signal(|| false);
    let mut status = use_signal(|| "published".to_string());
    let mut content = use_signal(|| "".to_string());
    // 页面与编辑器加载、保存、错误、成功等状态。
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut editor_content_set = use_signal(|| false);
    let mut has_backfilled = use_signal(|| false);
    let mut load_error = use_signal(|| None::<String>);

    // 编辑模式：用于暂存从服务端加载的文章数据。
    let mut edit_post = use_signal(|| None::<Post>);

    // WASM 前端：编辑器实例句柄（持有实例 + closure，统一生命周期）。
    #[cfg(target_arch = "wasm32")]
    let mut editor: Signal<Option<EditorHandle>> = use_signal(|| None);
    // WASM 前端：编辑器就绪标志（onReady 回调驱动，替代 __tiptap_ready 轮询）。
    #[cfg(target_arch = "wasm32")]
    let ready = use_signal(|| false);

    // 上传状态：当前进行中计数（保存拦截）+ 顶部失败提示堆叠（用户手动关闭）
    let uploads_in_flight = use_signal(UploadsInFlight::default);
    let upload_errors: Signal<Vec<UploadErrorEntry>> = use_signal(Vec::new);

    // 编辑模式：文章数据加载完成后，将字段回填到表单信号。
    use_effect(move || {
        if !is_edit || has_backfilled() {
            return;
        }
        if let Some(ref post) = edit_post() {
            has_backfilled.set(true);
            title.set(post.title.clone());
            summary.set(post.summary.clone().unwrap_or_default());
            slug.set(post.slug.clone());
            tags.set(post.tags.join(", "));
            cover_image.set(post.cover_image.clone().unwrap_or_default());
            status.set(post.status.as_str().to_string());
            content.set(post.content_md.clone());
        }
    });

    // 编辑模式：仅在 WASM 前端通过 server function 加载文章详情。
    use_effect(move || {
        if is_edit {
            #[cfg(target_arch = "wasm32")]
            if let Some(id) = post_id {
                spawn(async move {
                    match get_post_by_id(id).await {
                        Ok(SinglePostResponse { post: Some(post) }) => {
                            edit_post.set(Some(post));
                        }
                        Ok(SinglePostResponse { post: None }) => {
                            load_error.set(Some("文章不存在".to_string()));
                        }
                        Err(e) => {
                            load_error.set(Some(format!("加载失败: {}", e)));
                        }
                    }
                });
            }
        }
    });

    // 组件卸载时清理 Tiptap 实例：EditorHandle::drop 会 destroy 编辑器并释放全部 closure。
    #[cfg(target_arch = "wasm32")]
    use_drop(move || {
        editor.set(None);
    });

    // Tiptap 编辑器初始化：构造 closure + EditorOptions，调用 bridge.create。
    // 替代旧版 eval initEditor 脚本 + 100ms 就绪轮询。
    // use_effect 在首次渲染后跑，此时 #tiptap-editor 容器已挂载。
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        // 编辑模式：等数据加载完再初始化（避免空内容覆盖回填）。
        // 回填内容直接从 edit_post（唯一可信源）取，不走 content signal——
        // content 由独立的回填 effect 写入，两者在 edit_post 变更同一 tick 触发时
        // 没有保证的先后顺序：本 effect 可能先于回填 effect 跑，读到仍是空串，
        // 导致 setMarkdown 被跳过、编辑器空白（且 editor_content_set=true 阻止重试）。
        if is_edit && edit_post().is_none() {
            return;
        }
        // 防重复 init（effect 可能多次触发）
        if editor.read().is_some() {
            return;
        }

        // —— 构造 closure ——
        // 用 FnMut：Dioxus Signal 的 write/set 接收 &mut self，回调需可变借用捕获的 signal。
        let on_update = Closure::new({
            let mut content = content;
            move |md: String| content.set(md)
        });
        let on_ready = Closure::new({
            let mut ready = ready;
            move || ready.set(true)
        });
        let on_image_upload = crate::tiptap_bridge::make_upload_closure();
        let on_upload_event = Closure::new({
            let uploads_in_flight = uploads_in_flight;
            let upload_errors = upload_errors;
            move |ev: crate::tiptap_bridge::UploadEventJs| {
                consume_upload_event(&ev, uploads_in_flight, upload_errors);
            }
        });
        // 运行代码 closure：start_exec + 轮询，结果字符串回填 JS 结果区 DOM。
        let on_run_code = crate::tiptap_bridge::make_run_code_closure();
        // slash 命令「素材库」closure：打开正文素材选择弹窗
        // （JS 侧已删掉 /命令 文本；选中回填由弹窗 on_select 完成）。
        let on_pick_from_library = Closure::new({
            let mut body_picker_visible = body_picker_visible;
            move || body_picker_visible.set(true)
        });

        // —— 构造 options ——
        let opts = crate::tiptap_bridge::EditorOptions::new();
        opts.set_placeholder("在此输入内容...");
        opts.set_on_update(&on_update);
        opts.set_on_ready(&on_ready);
        opts.set_on_image_upload(&on_image_upload);
        opts.set_on_upload_event(&on_upload_event);
        opts.set_on_run_code(&on_run_code);
        opts.set_on_pick_from_library(&on_pick_from_library);

        // —— create（同步返回；找不到容器返回 None，构造失败抛异常）——
        match crate::tiptap_bridge::get_module().create("tiptap-editor", &opts) {
            Ok(Some(inst)) => {
                // 编辑模式回填：create 成功立即回填（实例已创建，时机确定）。
                // 直接从 edit_post 取 content_md：本 effect 已在上面 guard 了 edit_post.is_none()，
                // 而 edit_post 是异步加载后一次性写入、之后只读的真值源，读取它没有竞态。
                // 用 editor_content_set 防重复回填（effect 重跑时跳过）。
                if is_edit && !editor_content_set() {
                    if let Some(post) = edit_post() {
                        let md = &post.content_md;
                        if !md.is_empty() {
                            inst.set_markdown(md);
                        }
                    }
                    editor_content_set.set(true);
                }
                let handle = EditorHandle::new(
                    inst,
                    on_update,
                    on_image_upload,
                    on_ready,
                    on_upload_event,
                    on_run_code,
                    on_pick_from_library,
                );
                editor.set(Some(handle));
            }
            Ok(None) => {
                load_error.set(Some("编辑器容器未就绪".to_string()));
                loading.set(false);
            }
            Err(e) => {
                load_error.set(Some(format!("编辑器初始化错误: {:?}", e)));
                loading.set(false);
            }
        }
    });

    // 编辑器就绪后解除 loading（onReady 回调设 ready=true）。
    // 独立 effect 避免 on_ready 闭包与 loading 写权限在同一作用域冲突。
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        if ready() {
            loading.set(false);
        }
    });

    // 非 WASM（server SSR）：无编辑器，直接解除 loading。
    #[cfg(not(target_arch = "wasm32"))]
    use_effect(move || {
        loading.set(false);
    });

    // 提交表单：校验标题与内容，读取 Tiptap 编辑器 Markdown，调用 create_post 或 update_post。
    let mut on_submit = move |_| {
        // 上传未完成/失败拦截：有占位符时阻止保存
        let in_flight = uploads_in_flight.read();
        if in_flight.uploading > 0 || in_flight.error > 0 {
            let msg = if in_flight.uploading > 0 {
                format!(
                    "有 {} 张图片正在上传，请等待完成后再保存",
                    in_flight.uploading
                )
            } else {
                format!(
                    "有 {} 张图片上传失败，请移除或重试后再保存",
                    in_flight.error
                )
            };
            error.set(Some(msg));
            return;
        }
        drop(in_flight);

        // 封面图上传中拦截：防止保存半成品（cover_uploading 由子组件 CoverUploader 写入）。
        if cover_uploading() {
            error.set(Some("封面图正在上传，请等待完成后再保存".to_string()));
            return;
        }

        // 正文素材弹窗内上传中拦截（与封面同一语义）。
        if body_picker_uploading() {
            error.set(Some("图片正在上传，请等待完成后再保存".to_string()));
            return;
        }

        if title().trim().is_empty() {
            error.set(Some("标题不能为空".to_string()));
        }

        // 仅在 WASM 前端读取编辑器内容并发起保存请求。
        #[cfg(target_arch = "wasm32")]
        if !title().trim().is_empty() {
            // 通过 EditorHandle 实例读取 Markdown；句柄未就绪时退回 content signal。
            let md = if let Some(handle) = &*editor.read() {
                handle.instance().get_markdown()
            } else {
                content()
            };

            // 兜底：扫描残留的上传占位符标记（轮询窗口期漏判防护）
            // 检测 ![](blob:...) 形式的泄漏图片 src，而非裸 "blob:" 字符串，
            // 避免误伤合法讨论 blob URL 的代码块/正文。
            if md.contains("](blob:") || md.contains("data-upload-state") {
                error.set(Some("检测到未完成上传的图片，请处理后保存".to_string()));
                return;
            }

            if md.trim().is_empty() {
                error.set(Some("内容不能为空".to_string()));
                return;
            }

            // 将逗号分隔的标签字符串转换为列表。
            // 同时支持半角/全角逗号与分号，避免中文输入法下的全角标点被误并入单个标签。
            let tags_list: Vec<String> = tags()
                .split([',', '，', ';', '；'])
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();

            let slug_opt = if slug().trim().is_empty() {
                None
            } else {
                Some(slug().trim().to_string())
            };

            let summary_opt = if summary().trim().is_empty() {
                None
            } else {
                Some(summary().trim().to_string())
            };

            let cover_image_opt = if cover_image().trim().is_empty() {
                None
            } else {
                Some(cover_image().trim().to_string())
            };

            saving.set(true);
            error.set(None);

            if let Some(id) = post_id {
                // 编辑模式：调用 update_post
                spawn(async move {
                    match update_post(
                        id,
                        title().trim().to_string(),
                        slug_opt,
                        summary_opt,
                        md,
                        status(),
                        tags_list,
                        cover_image_opt,
                    )
                    .await
                    {
                        Ok(CreatePostResponse { success: true, .. }) => {
                            saving.set(false);
                            let _ = dioxus::router::navigator().push(Route::Posts {});
                        }
                        Ok(CreatePostResponse {
                            success: false,
                            message,
                            ..
                        }) => {
                            saving.set(false);
                            error.set(Some(message));
                        }
                        Err(e) => {
                            saving.set(false);
                            error.set(Some(format!("更新失败: {}", e)));
                        }
                    }
                });
            } else {
                // 新建模式：调用 create_post
                spawn(async move {
                    match create_post(
                        title().trim().to_string(),
                        slug_opt,
                        summary_opt,
                        md,
                        status(),
                        tags_list,
                        cover_image_opt,
                    )
                    .await
                    {
                        Ok(CreatePostResponse { success: true, .. }) => {
                            saving.set(false);
                            let _ = dioxus::router::navigator().push(Route::Posts {});
                        }
                        Ok(CreatePostResponse {
                            success: false,
                            message,
                            ..
                        }) => {
                            saving.set(false);
                            error.set(Some(message));
                        }
                        Err(e) => {
                            saving.set(false);
                            error.set(Some(format!("保存失败: {}", e)));
                        }
                    }
                });
            }
        }

        // 非 WASM（SSR hydration 前等场景）：明确提示无法保存，而非静默无反应。
        #[cfg(not(target_arch = "wasm32"))]
        if !title().trim().is_empty() {
            error.set(Some("此页面需要 JavaScript 才能保存".to_string()));
        }
    };

    rsx! {
        // 根容器:flex 分区布局。layout 给 write 的 main 是 flex 容器(无 padding/不滚动),
        // 这里拆成 [内容区 flex-1 overflow-y-auto] + [底栏 flex-shrink-0] 两块。
        // 底栏作为 main 直接子元素永远贴卡片底沿,不随内容滚动,也无需 sticky。
        div { class: "animate-page-enter relative flex flex-col w-full min-h-0 flex-1",
            if loading() {
                div { class: "absolute inset-0 z-10 bg-paper-theme flex flex-col", WriteSkeleton {} }
            }

            // 两栏容器:flex-1 分配空间,自身不滚动(min-h-0),滚动职责下放给左右两栏。
            div { class: "flex-1 min-h-0 flex",
                // 左栏(主写作区):flex-1 撑满宽度,min-w-0 防止长标题/代码块撑破 flex,
                // overflow-y-auto 独立滚动。
                div { class: "flex-1 min-w-0 min-h-0 overflow-y-auto px-6 py-8 flex flex-col",
                    // 标题输入
                    input {
                        class: "w-full text-4xl md:text-5xl font-extrabold bg-transparent text-[var(--color-paper-primary)] placeholder-[var(--color-paper-tertiary)] focus:outline-none tracking-tight leading-tight",
                        placeholder: "输入文章标题...",
                        value: "{title}",
                        oninput: move |evt| title.set(evt.value()),
                    }

                    // 错误和成功提示
                    if let Some(err) = load_error() {
                        div { class: "flex-shrink-0 px-4 py-2 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-2xl text-sm border border-red-100 dark:border-red-900/30 mb-2",
                            "{err}"
                        }
                    }

                    if let Some(err) = error() {
                        div { class: "flex-shrink-0 px-4 py-2 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-2xl text-sm border border-red-100 dark:border-red-900/30 mb-2",
                            "{err}"
                        }
                    }

                    // 上传失败提示：多条堆叠，×关闭同时删除编辑器内失败占位符（避免孤儿）
                    for err in upload_errors().clone() {
                        div {
                            key: "{err.id}",
                            class: "flex-shrink-0 flex items-center justify-between gap-3 px-4 py-2 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-2xl text-sm border border-red-100 dark:border-red-900/30 mb-2",
                            span { "图片上传失败: {err.file_name} — {err.message}" }
                            button {
                                class: "{BTN_CLOSE_ICON}",
                                aria_label: "关闭提示",
                                onclick: {
                                    // 捕获 owned id，避免借用临时值
                                    let id = err.id.clone();
                                    let mut upload_errors = upload_errors;
                                    move |_| {
                                        // 关闭提示同时删除编辑器内失败占位符（避免孤儿）
                                        #[cfg(target_arch = "wasm32")]
                                        if let Some(handle) = &*editor.read() {
                                            handle.instance().remove_upload_by_upload_id(&id);
                                        }
                                        upload_errors.write().retain(|e| e.id != id);
                                    }
                                },
                                "×"
                            }
                        }
                    }

                    // 编辑器区域:flex-1 撑满左栏剩余高度,不再硬编码 60vh。
                    // 编辑器内部 .tiptap-editor/.ProseMirror 均为 height:100% + overflow-y:auto,
                    // 容器变高编辑器自动跟着变高。min-h-[400px] 保证窗口过矮时仍可用。
                    div { class: "flex-1 min-h-[400px] flex flex-col my-4",
                        div {
                            class: "relative group flex-1 min-h-0 w-full border border-[var(--color-paper-border)] rounded-2xl overflow-hidden bg-[var(--color-paper-entry)] shadow-sm",
                            id: "tiptap-editor",
                            img {
                                src: "/images/xiantiaoxiaogou_input_bg.webp",
                                alt: "",
                                class: "absolute bottom-2 right-2 w-24 opacity-10 pointer-events-none z-0",
                            }
                        }
                    }
                } // 左栏闭合

                // 右栏(侧边栏):分节式布局,每节带小标题,节间用细分隔线分隔。
                // 透明背景(与左栏共用 paper-theme 底),靠 border-l 和分隔线划分区域。
                div { class: "w-80 flex-shrink-0 min-h-0 overflow-y-auto border-l border-[var(--color-paper-border)] flex flex-col",
                    // Slug 节
                    div {
                        class: "animate-row-enter p-6 border-b border-[var(--color-paper-border)]",
                        style: "animation-delay: 60ms",
                        label { class: "block text-xs font-semibold uppercase tracking-wide text-[var(--color-paper-tertiary)] mb-3",
                            "链接"
                        }
                        FormInput {
                            r#type: "text",
                            placeholder: "自动生成",
                            value: slug(),
                            oninput: move |v: String| slug.set(v),
                        }
                    }
                    // 标签节
                    div {
                        class: "animate-row-enter p-6 border-b border-[var(--color-paper-border)]",
                        style: "animation-delay: 120ms",
                        label { class: "block text-xs font-semibold uppercase tracking-wide text-[var(--color-paper-tertiary)] mb-3",
                            "标签"
                        }
                        FormInput {
                            r#type: "text",
                            placeholder: "逗号分隔...",
                            value: tags(),
                            oninput: move |v: String| tags.set(v),
                        }
                    }
                    // 摘要节
                    div {
                        class: "animate-row-enter p-6 border-b border-[var(--color-paper-border)]",
                        style: "animation-delay: 180ms",
                        label { class: "block text-xs font-semibold uppercase tracking-wide text-[var(--color-paper-tertiary)] mb-3",
                            "摘要"
                        }
                        textarea {
                            class: "w-full text-sm bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] placeholder-[var(--color-paper-tertiary)] focus:outline-none border border-[var(--color-paper-border)] focus:border-[var(--color-paper-primary)] rounded-2xl px-3 py-2 transition-all resize-none leading-relaxed",
                            placeholder: "选填...",
                            rows: "3",
                            value: "{summary}",
                            oninput: move |evt| summary.set(evt.value()),
                        }
                    }
                    // 封面图节
                    div {
                        class: "animate-row-enter p-6",
                        style: "animation-delay: 240ms",
                        label { class: "block text-xs font-semibold uppercase tracking-wide text-[var(--color-paper-tertiary)] mb-3",
                            "封面图"
                        }
                        CoverUploader { cover_image, cover_uploading }
                    }
                } // 右栏闭合
            } // 两栏容器闭合

            // 底部操作栏 - flex 分区布局的贴底块:作为 main 直接子元素(flex-shrink-0),
            // 永远贴卡片底沿,不随内容区滚动。无需 sticky,不会跳动。
            // px-6 py-4 与内容区水平对齐;border-t 与页头分割线视觉一致。
            div { class: "flex-shrink-0 px-6 py-4 flex items-center gap-4 border-t border-[var(--color-paper-border)] bg-[var(--color-paper-theme)]",
                button {
                    class: "px-6 py-2 rounded-full text-sm font-medium text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors cursor-pointer",
                    onclick: move |_| {
                        let _ = dioxus::router::navigator().push(Route::Posts {});
                    },
                    "取消"
                }
                div { class: "w-px h-5 bg-[var(--color-paper-border)]" }
                // 文章状态下拉（胶囊触发器；贴底栏，面板自动向上展开）
                FormSelect {
                    trigger_class: Some(
                        "inline-flex w-auto cursor-pointer select-none text-left text-sm font-medium pl-4 pr-8 py-2 rounded-full border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] hover:bg-[var(--color-paper-theme)] focus:outline-none focus:border-paper-accent focus:ring-1 focus:ring-paper-accent/30 transition-colors duration-200",
                    ),
                    value: status(),
                    options: vec![
                        ("draft".to_string(), "存为草稿"),
                        ("published".to_string(), "直接发布"),
                    ],
                    onchange: move |v| status.set(v),
                }
                div { class: "w-px h-5 bg-[var(--color-paper-border)]" }
                LoadingButton {
                    label: if is_edit { "更新文章".to_string() } else { "发布文章".to_string() },
                    loading: saving(),
                    onclick: move |_| on_submit(()),
                }
            } // 底部操作栏闭合

            // 正文素材选择 modal（多选模式）：slash 命令「素材库」→ onPickFromLibrary 打开。
            // 确认后把选中集序列化为桥接契约 [{src,alt?}]，由 insertImagesFromLibrary
            // 在 slash 删除 /命令 文本后停留的光标处一次事务批量插入。
            AssetPickerModal {
                visible: body_picker_visible,
                cover_uploading: body_picker_uploading,
                title: "选择图片",
                multi: true,
                on_select: move |picks: Vec<AssetSelection>| {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(handle) = &*editor.read() {
                        if let Ok(json) = serde_json::to_string(&picks) {
                            handle.instance().insert_images_from_library(&json);
                        }
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = picks;
                },
            }
        }
    }
}

/// 封面上传子组件。
///
/// 封装封面图的全部状态与交互：拖拽/粘贴/选择文件上传、URL 输入、预览、移除。
/// 通过两个 signal 与父组件双向绑定：
/// - `cover_image`：子组件写入最终 URL，父组件读取用于保存；
/// - `cover_uploading`：子组件上传时置 true，父组件在 `on_submit` 读取以拦截
///   上传中的保存（避免封面 URL 尚未落定就提交导致封面丢失）。
/// 其余上传中间态（error/url/drag）对本组件私有。
///
/// 从 `write_editor` 抽取以降低 god component 复杂度（见 dioxus-render-purity skill）。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn CoverUploader(cover_image: Signal<String>, cover_uploading: Signal<bool>) -> Element {
    // 封面图上传状态：错误消息、URL 输入框展开、拖拽高亮（cover_uploading 由父组件传入）。
    let mut cover_error = use_signal(|| None::<String>);
    let mut cover_url_mode = use_signal(|| false);
    let mut cover_drag_active = use_signal(|| false);
    // 封面 URL 输入框的临时值（确认前不直接写入 cover_image，避免半截 URL 触发预览加载）。
    let mut cover_url_input = use_signal(|| "".to_string());
    // 素材选择 modal 显隐。
    let mut picker_visible = use_signal(|| false);
    // 封面预览图加载状态：cover_image 变化时置 false，img onload 置 true。
    // 从素材库选择/拖拽替换/URL 输入后，预览图 ?w=600 版本未缓存需重新加载，
    // 加载期间显示骨架占位，避免空白无反馈（与上传中的骨架语义独立）。
    let mut cover_img_loaded = use_signal(|| true);

    // 封面图上传：spawn 一个 async 调用 upload_image_file。
    // 三条入口（file input / drop / paste）收敛成拿到 web_sys::File 后统一调用此闭包。
    // 仅在 WASM 端有意义（upload_image_file 与 spawn 都依赖 WASM 运行时），
    // server SSR 不渲染上传逻辑，故整体 cfg-gate 避免引用 wasm-only 符号。
    #[cfg(target_arch = "wasm32")]
    let mut spawn_cover_upload = move |file: web_sys::File| {
        cover_uploading.set(true);
        cover_error.set(None);
        spawn(async move {
            match upload_image_file(file).await {
                Ok(url) => {
                    cover_image.set(url);
                }
                Err(msg) => {
                    cover_error.set(Some(msg));
                }
            }
            cover_uploading.set(false);
        });
    };
    // 灯箱绑定 + 预览图加载占位：封面有图时点击放大查看（复用全局 lightbox.js）。
    // 订阅 cover_image：图片出现/更换后 effect 重跑，data-lb-bound 守卫保证幂等。
    // 新 src 待加载时置 cover_img_loaded=false，由 img onload 回填 true。
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let cv = cover_image();
        if cv.is_empty() {
            cover_img_loaded.set(true);
            return;
        }
        cover_img_loaded.set(false);
        let window = web_sys::window()
            .expect("CoverUploader use_effect 仅在 WASM 浏览器上下文执行：无 window");
        let selectors = js_sys::Array::of1(&".cover-preview".into());
        let selectors_val = js_sys::Object::from(selectors).into();
        let _ = js_sys::Reflect::set(&window, &"__lightboxSelectors".into(), &selectors_val);
        invoke_optional_global(&window, "__initLightbox", &[selectors_val]);
    });

    rsx! {
        // 封面图上传区：空态矮横条（不挤压编辑器），有图时展开成 21:9 超宽预览。
        // 21:9 与首页卡片封面统一比例，比 16:9 更扁，适合宽屏横幅式封面。
        // 容器统一绑定拖拽与粘贴事件；内部按 cover_image / cover_uploading 切换空态、上传中、预览。
        div {
            class: "relative w-full border border-dashed rounded-2xl overflow-hidden transition-all duration-200 group/cover",
            class: "cover-preview",
            // 空态矮横条；有图/上传中展开成 21:9。
            class: if cover_image().is_empty() && !cover_uploading() { "h-14" } else { "aspect-[21/9]" },
            class: if cover_drag_active() { "border-[var(--color-paper-primary)] bg-[var(--color-paper-entry)]" } else if cover_image().is_empty() { "border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] hover:border-[var(--color-paper-primary)]" } else { "border-[var(--color-paper-border)] bg-[var(--color-paper-entry)]" },

            // 整个容器可接收拖拽与粘贴（ondragover 必须 prevent_default，否则浏览器直接打开文件）。
            ondragover: move |evt| {
                evt.prevent_default();
                if !cover_uploading() {
                    cover_drag_active.set(true);
                }
            },
            ondragenter: move |evt| {
                evt.prevent_default();
            },
            ondragleave: move |_| {
                cover_drag_active.set(false);
            },
            ondrop: move |evt| {
                evt.prevent_default();
                if !cover_uploading() {
                    #[cfg(target_arch = "wasm32")]
                    {
                        if let Some(file) = evt.files().into_iter().next() {
                            if let Some(web_file) = file.get_web_file() {
                                spawn_cover_upload(web_file);
                            }
                        }
                    }
                }
            },
            onpaste: move |evt| {
                evt.prevent_default();
                if !cover_uploading() {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use wasm_bindgen::JsCast;
                        if let Some(raw) = evt.try_as_web_event() {
                            if let Some(ce) = raw.dyn_ref::<web_sys::ClipboardEvent>() {
                                if let Some(dt) = ce.clipboard_data() {
                                    if let Some(file_list) = dt.files() {
                                        if let Some(file) = file_list.item(0) {
                                            spawn_cover_upload(file);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },

            // —— 上传中：骨架占位 + 文案 ——
            if cover_uploading() {
                div { class: "absolute inset-0 flex flex-col items-center justify-center gap-3 bg-[var(--color-paper-tertiary)]/30 animate-pulse",
                    span { class: "text-sm font-medium text-[var(--color-paper-secondary)]",
                        "正在上传..."
                    }
                }
            }

            // —— 有图：预览（点击放大）+ 拖拽替换 + hover 工具栏 ——
            if !cover_image().is_empty() && !cover_uploading() {
                // 预览图：/uploads/ 路径加 ?w=600 缩略，外链 URL 原样用。
                // cursor-zoom-in 提示可点击放大；lightbox-single 标记为单张，
                // 由全局 lightbox.js 接管点击放大；data-src 为原图 URL。
                img {
                    class: "absolute inset-0 w-full h-full object-cover cursor-zoom-in lightbox-single",
                    // data-src：原图 URL，lightbox.js 点击放大时读取（originalUrl 剥离 /uploads/ 的 ?w= 查询）。
                    "data-src": "{cover_image()}",
                    src: {
                        let cv = cover_image();
                        if cv.starts_with("/uploads/") {
                            let base = cv.split('?').next().unwrap_or(&cv);
                            if cv.contains('?') {
                                format!("{}&w=600", base)
                            } else {
                                format!("{}?w=600", base)
                            }
                        } else {
                            cv
                        }
                    },
                    alt: "封面预览",
                    // 加载完成清除骨架占位；失败也清除并提示（避免骨架永驻）。
                    onload: move |_| cover_img_loaded.set(true),
                    onerror: move |_| {
                        cover_img_loaded.set(true);
                        cover_error.set(Some("封面图加载失败，请检查 URL".to_string()));
                    },
                }
                // 预览图加载中骨架占位（素材库选择/拖拽替换/URL 切换后，?w=600 未缓存时）。
                if !cover_img_loaded() {
                    div { class: "absolute inset-0 animate-pulse bg-[var(--color-paper-tertiary)]/40 pointer-events-none" }
                }
                // 拖拽替换高亮：有图时拖入文件，提示"松开以更换"。
                if cover_drag_active() {
                    div { class: "absolute inset-0 flex items-center justify-center bg-black/50 pointer-events-none",
                        span { class: "text-sm font-medium text-white drop-shadow-md",
                            "松开以更换封面"
                        }
                    }
                }
                // hover 工具栏：拖拽时隐藏（由拖拽替换高亮接管），避免两个覆盖层叠加。
                // pointer-events-none 容器穿透点击到预览图触发灯箱，按钮设 pointer-events-auto。
                if !cover_drag_active() {
                    div { class: "absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/60 to-transparent opacity-0 group-hover/cover:opacity-100 transition-opacity flex items-center justify-center gap-1 py-2 pointer-events-none",
                        // 更换：label 包裹隐藏 file input，点击触发文件选择。
                        label { class: "pointer-events-auto px-3 py-1.5 rounded-full text-xs font-medium text-white/90 hover:bg-white/15 cursor-pointer transition-colors",
                            "更换"
                            input {
                                r#type: "file",
                                accept: "image/jpeg,image/png,image/gif,image/webp",
                                class: "hidden",
                                onchange: move |evt| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        if let Some(file) = evt.files().into_iter().next() {
                                            if let Some(web_file) = file.get_web_file() {
                                                spawn_cover_upload(web_file);
                                            }
                                        }
                                    }
                                },
                            }
                        }
                        // 从素材库选择。
                        button {
                            class: "pointer-events-auto px-3 py-1.5 rounded-full text-xs font-medium text-white/90 hover:bg-white/15 transition-colors cursor-pointer",
                            onclick: move |_| picker_visible.set(true),
                            "素材库"
                        }
                        // 移除封面。
                        button {
                            class: "pointer-events-auto px-3 py-1.5 rounded-full text-xs font-medium text-white/90 hover:bg-white/15 transition-colors cursor-pointer",
                            onclick: move |_| {
                                cover_image.set(String::new());
                                cover_error.set(None);
                                cover_url_mode.set(false);
                                cover_url_input.set(String::new());
                            },
                            "移除"
                        }
                    }
                }
            }

            // —— 空态：横向矮横条，图标+提示+URL 链 ——
            if cover_image().is_empty() && !cover_uploading() {
                // label 包裹整个空态：点击天然触发隐藏的 file input，无需 JS。
                label { class: "absolute inset-0 flex flex-row items-center gap-3 cursor-pointer px-4 text-left",
                    // 上传图标（Feather 风格线框，与项目现有图标体系一致）。
                    svg {
                        class: "w-5 h-5 shrink-0 text-[var(--color-paper-secondary)]",
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.8",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                        polyline { points: "17 8 12 3 7 8" }
                        line {
                            x1: "12",
                            y1: "3",
                            x2: "12",
                            y2: "15",
                        }
                    }
                    span { class: "text-sm font-medium text-[var(--color-paper-secondary)] shrink-0",
                        "拖拽 / 粘贴 / 点击上传"
                    }
                    // 从素材库选择：阻止 label 默认 file 触发，打开选择 modal。
                    span {
                        class: "text-sm font-medium text-[var(--color-paper-tertiary)] hover:text-[var(--color-paper-primary)] transition-colors ml-auto shrink-0",
                        onclick: move |evt| {
                            evt.prevent_default();
                            evt.stop_propagation();
                            picker_visible.set(true);
                        },
                        "素材库"
                    }
                    // URL 文字链：阻止 label 的默认 file 触发，切换到 URL 输入模式。
                    span {
                        class: "text-sm font-medium text-[var(--color-paper-tertiary)] hover:text-[var(--color-paper-primary)] transition-colors shrink-0",
                        onclick: move |evt| {
                            evt.prevent_default();
                            evt.stop_propagation();
                            cover_url_mode.set(true);
                            cover_url_input.set(cover_image());
                        },
                        "输入链接"
                    }
                    // 隐藏的 file input，由 label 点击触发。
                    input {
                        r#type: "file",
                        accept: "image/jpeg,image/png,image/gif,image/webp",
                        class: "hidden",
                        onchange: move |evt| {
                            #[cfg(target_arch = "wasm32")]
                            {
                                if let Some(file) = evt.files().into_iter().next() {
                                    if let Some(web_file) = file.get_web_file() {
                                        spawn_cover_upload(web_file);
                                    }
                                }
                            }
                            // 注意：未重置 input.value，重复选择同一文件不会再次触发 onchange。
                            // 这是 file input 的通用行为，封面场景影响可忽略。
                        },
                    }
                }
            }
        }

        // —— URL 输入模式（内联展开，空态时叠加在容器外，避免与拖拽区争抢点击）——
        if cover_url_mode() && cover_image().is_empty() {
            div { class: "flex items-center gap-2 mt-2",
                FormInput {
                    r#type: "url",
                    placeholder: "粘贴图片链接...",
                    value: cover_url_input(),
                    class: INPUT_INLINE_CLASS,
                    oninput: move |v: String| cover_url_input.set(v),
                    onkeydown: move |evt: KeyboardEvent| {
                        if evt.key() == Key::Enter {
                            let v = cover_url_input().trim().to_string();
                            if !v.is_empty() {
                                cover_image.set(v);
                                cover_error.set(None);
                                cover_url_mode.set(false);
                            }
                        }
                    },
                }
                button {
                    class: "shrink-0 {BTN_PRIMARY_SM}",
                    onclick: move |_| {
                        let v = cover_url_input().trim().to_string();
                        if !v.is_empty() {
                            cover_image.set(v);
                            cover_error.set(None);
                            cover_url_mode.set(false);
                        }
                    },
                    "确认"
                }
                button {
                    class: "shrink-0 px-4 py-1.5 rounded-full text-sm font-medium text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                    onclick: move |_| {
                        cover_url_mode.set(false);
                        cover_url_input.set(String::new());
                    },
                    "取消"
                }
            }
        }

        // 封面上传失败提示：复用页面红色条风格。
        if let Some(err) = cover_error() {
            div { class: "flex items-center justify-between gap-3 px-4 py-2 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-2xl text-sm border border-red-100 dark:border-red-900/30 mt-2",
                span { "封面图: {err}" }
                button {
                    class: "{BTN_CLOSE_ICON}",
                    aria_label: "关闭提示",
                    onclick: move |_| cover_error.set(None),
                    "×"
                }
            }
        }

        // 素材选择 modal：选中回填封面 URL（上传新图成功同样回填）。
        AssetPickerModal {
            visible: picker_visible,
            cover_uploading,
            on_select: move |picks: Vec<AssetSelection>| {
                // 单选模式：载荷恰含一个元素。
                if let Some(first) = picks.into_iter().next() {
                    cover_image.set(first.url);
                    cover_error.set(None);
                }
            },
        }
    }
}
