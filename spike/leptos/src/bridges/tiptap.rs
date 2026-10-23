//! Tiptap 编辑器的 wasm-bindgen 绑定层（评论区子集）。
//!
//! 封装与 `window.TiptapEditor`（IIFE 暴露的全局对象）的全部交互。
//! 移植自主 crate `src/bridges/tiptap.rs`，只保留评论编辑器（comment variant）
//! 用到的部分：后台写作页专属的运行代码、素材库、往返守卫与 admin 上传不在原型范围。
//!
//! wasm-bindgen extern 与 `EditorHandle`、上传 closure 等**仅在 WASM 前端**编译
//! （server 构建无 `window`）；共享的纯数据类型 `UploadsInFlight`/`UploadErrorEntry`
//! 在两端都编译，供视图渲染上传状态。

/// 当前编辑器内进行中的上传计数（来自 onUploadEvent 的 counts 快照）。
#[derive(Clone, Copy, Default, PartialEq)]
pub struct UploadsInFlight {
    pub uploading: u32,
    pub error: u32,
}

/// 顶部堆叠的上传失败提示条目。
#[derive(Clone, PartialEq)]
pub struct UploadErrorEntry {
    pub id: String,
    pub file_name: String,
    pub message: String,
}

// ============================================================================
// 以下全部仅在 WASM 前端编译：wasm-bindgen extern + EditorHandle + 上传 closure。
// ============================================================================
#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use super::{UploadErrorEntry, UploadsInFlight};
    use leptos::prelude::{RwSignal, Set, Update};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    // —— window.TiptapEditor 模块对象 ——
    //
    // TiptapEditor 是 IIFE 产物挂在 window 上的模块对象（含 create 等方法），
    // 不是函数；用 js_sys::Reflect::get 做属性访问拿到模块对象。
    #[wasm_bindgen]
    extern "C" {
        /// `window.TiptapEditor` 模块对象的 Rust 映射。
        pub type TiptapEditorModule;

        /// 调用 `TiptapEditor.create(containerId, options)`。
        /// 找不到容器返回 null（被 Option 捕获）；构造失败抛异常（被 catch 捕获）。
        #[wasm_bindgen(method, catch)]
        pub fn create(
            this: &TiptapEditorModule,
            container_id: &str,
            opts: &EditorOptions,
        ) -> Result<Option<EditorInstance>, JsValue>;
    }

    /// 读取 `window.TiptapEditor`（IIFE 默认导出，顶层 var 即 window 属性）。
    ///
    /// 用 unchecked_into 而非 dyn_into：TiptapEditor 是 JS 对象字面量，
    /// instanceof 检查必然失败。调用方先等待 use_browser_library 就绪。
    pub fn get_module() -> TiptapEditorModule {
        let window = web_sys::window().expect("no window: get_module 只能在浏览器 wasm32 前端调用");
        let val = js_sys::Reflect::get(&window, &"TiptapEditor".into()).expect(
            "window.TiptapEditor missing: /tiptap/editor.js 未加载，检查该静态资源是否随构建产物部署",
        );
        val.unchecked_into::<TiptapEditorModule>()
    }

    // —— 编辑器实例（TiptapEditorInstance）——
    #[wasm_bindgen]
    extern "C" {
        /// `TiptapEditor.create` 返回的编辑器实例对象。
        pub type EditorInstance;

        /// 富文本模式下返回 ProseMirror 的 Markdown；源码模式下返回 textarea 内容。
        #[wasm_bindgen(method, js_name = getMarkdown)]
        pub fn get_markdown(this: &EditorInstance) -> String;

        /// 用 Markdown 内容回填编辑器（emitUpdate: false，不触发 onUpdate）。
        /// 未注入 `checkRoundtrip` 时同步完成，Promise 可直接丢弃。
        #[wasm_bindgen(method, js_name = setMarkdown)]
        pub fn set_markdown(this: &EditorInstance, content: &str) -> js_sys::Promise;

        /// 工具栏图片按钮入口：把文件交给 coordinator 走占位符上传
        /// （与粘贴/拖放同一条路径）。
        #[wasm_bindgen(method, js_name = insertUploading)]
        pub fn insert_uploading(this: &EditorInstance, file: web_sys::File);

        /// 销毁编辑器，释放 JS 侧资源。
        #[wasm_bindgen(method)]
        pub fn destroy(this: &EditorInstance);
    }

    // —— EditorOptions：用 builder 模式（setter）构造 JS 对象 ——
    #[wasm_bindgen]
    extern "C" {
        /// 传给 `TiptapEditor.create` 的配置对象，对应 JS 侧的 EditorOptions
        /// （tiptap bundle 把该类挂到 `window.EditorOptions`）。
        pub type EditorOptions;

        /// 构造一个空的 EditorOptions，随后用各 setter 填充回调与占位文案。
        #[wasm_bindgen(constructor)]
        pub fn new() -> EditorOptions;

        /// 编辑器无内容时的占位文案。
        #[wasm_bindgen(method, setter, js_name = placeholder)]
        pub fn set_placeholder(this: &EditorOptions, v: &str);

        /// 编辑器变体：`"full"`（后台文章编辑器）或 `"comment"`（评论区精简子集）。
        #[wasm_bindgen(method, setter, js_name = variant)]
        pub fn set_variant(this: &EditorOptions, v: &str);

        /// 文档变更回调（参数为最新 Markdown）。
        #[wasm_bindgen(method, setter, js_name = onUpdate)]
        pub fn set_on_update(this: &EditorOptions, cb: &Closure<dyn FnMut(String)>);

        /// JS 侧 onImageUpload: (file: File) => Promise<string>。
        #[wasm_bindgen(method, setter, js_name = onImageUpload)]
        pub fn set_on_image_upload(
            this: &EditorOptions,
            cb: &Closure<dyn Fn(web_sys::File) -> js_sys::Promise>,
        );

        /// 编辑器就绪回调（init 末尾同步触发一次）。
        #[wasm_bindgen(method, setter, js_name = onReady)]
        pub fn set_on_ready(this: &EditorOptions, cb: &Closure<dyn FnMut()>);

        /// 上传事件回调（coordinator.emit 时触发，携带 counts）。
        #[wasm_bindgen(method, setter, js_name = onUploadEvent)]
        pub fn set_on_upload_event(this: &EditorOptions, cb: &Closure<dyn FnMut(UploadEventJs)>);
    }

    // —— 上传事件（JS UploadEvent 的 Rust 映射）——
    #[wasm_bindgen]
    extern "C" {
        /// 上传协调器 emit 的事件对象。
        #[derive(Clone)]
        pub type UploadEventJs;

        /// 事件种类：`"uploading"` / `"success"` / `"error"` / `"removed"`。
        #[wasm_bindgen(method, getter)]
        pub fn kind(this: &UploadEventJs) -> String;

        /// 本次上传的唯一标识。
        #[wasm_bindgen(method, getter, js_name = uploadId)]
        pub fn upload_id(this: &UploadEventJs) -> String;

        /// 上传文件名（用于失败提示展示）。
        #[wasm_bindgen(method, getter, js_name = fileName)]
        pub fn file_name(this: &UploadEventJs) -> String;

        /// 失败原因（仅 `error` 事件有值）。
        #[wasm_bindgen(method, getter, js_name = errorMsg)]
        pub fn error_msg(this: &UploadEventJs) -> Option<String>;

        /// 当前文档内全部上传节点的实时计数快照。
        #[wasm_bindgen(method, getter)]
        pub fn counts(this: &UploadEventJs) -> UploadCountsJs;
    }

    #[wasm_bindgen]
    extern "C" {
        /// 上传计数快照（JS 侧遍历文档节点统计后随事件下发）。
        #[derive(Clone)]
        pub type UploadCountsJs;

        /// 进行中的上传数量。
        #[wasm_bindgen(method, getter)]
        pub fn uploading(this: &UploadCountsJs) -> u32;

        /// 失败的上传数量。
        #[wasm_bindgen(method, getter)]
        pub fn error(this: &UploadCountsJs) -> u32;
    }

    // —— EditorHandle：实例 + closure 统一生命周期 ——

    /// 持有评论编辑器实例 + 其全部 closure，统一生命周期。
    ///
    /// drop 时先 destroy JS 实例，随后 closure 字段自动 drop（释放 wasm-bindgen
    /// 回调表），closure 严格随编辑器实例同生共死。
    pub struct EditorHandle {
        instance: EditorInstance,
        _on_update: Closure<dyn FnMut(String)>,
        _on_image_upload: Closure<dyn Fn(web_sys::File) -> js_sys::Promise>,
        _on_ready: Closure<dyn FnMut()>,
        _on_upload_event: Closure<dyn FnMut(UploadEventJs)>,
    }

    impl EditorHandle {
        /// 评论编辑器用构造：传入 comment variant 实际使用的 4 个 closure。
        pub fn new_comment(
            instance: EditorInstance,
            on_update: Closure<dyn FnMut(String)>,
            on_image_upload: Closure<dyn Fn(web_sys::File) -> js_sys::Promise>,
            on_ready: Closure<dyn FnMut()>,
            on_upload_event: Closure<dyn FnMut(UploadEventJs)>,
        ) -> Self {
            Self {
                instance,
                _on_update: on_update,
                _on_image_upload: on_image_upload,
                _on_ready: on_ready,
                _on_upload_event: on_upload_event,
            }
        }

        /// 访问底层编辑器实例（调 getMarkdown/setMarkdown/destroy 等）。
        pub fn instance(&self) -> &EditorInstance {
            &self.instance
        }
    }

    impl Drop for EditorHandle {
        fn drop(&mut self) {
            self.instance.destroy();
        }
    }

    // —— consume_upload_event：纯逻辑 helper ——

    /// 消费单个上传事件，更新 signal。
    ///
    /// - error：新 id 追加提示，已存在 id 原地更新消息（重试后再失败）
    /// - success/removed：移除对应提示
    /// - counts：直接从事件读（JS 已遍历文档算好）
    pub fn consume_upload_event(
        ev: &UploadEventJs,
        uploads_in_flight: RwSignal<UploadsInFlight>,
        upload_errors: RwSignal<Vec<UploadErrorEntry>>,
    ) {
        let id = ev.upload_id();
        match ev.kind().as_str() {
            "error" => {
                let msg = ev.error_msg().unwrap_or_else(|| "上传失败".to_string());
                let file_name = ev.file_name();
                upload_errors.update(|errors| {
                    if let Some(entry) = errors.iter_mut().find(|e| e.id == id) {
                        entry.message = msg;
                    } else {
                        errors.push(UploadErrorEntry {
                            id: id.clone(),
                            file_name,
                            message: msg,
                        });
                    }
                });
            }
            "success" | "removed" => {
                upload_errors.update(|errors| errors.retain(|e| e.id != id));
            }
            _ => {}
        }
        let c = ev.counts();
        uploads_in_flight.set(UploadsInFlight {
            uploading: c.uploading(),
            error: c.error(),
        });
    }

    // —— 上传：Rust fetch ——

    /// 以 multipart 表单 POST 单个文件，解析服务端统一 JSON 契约
    /// （移植自主 crate `utils::web_upload::post_multipart_file`）。
    async fn post_multipart_file(
        url: &str,
        field_name: &str,
        file: &web_sys::File,
    ) -> Result<serde_json::Value, String> {
        let form = web_sys::FormData::new().map_err(|_| "无法构造上传表单".to_string())?;
        form.append_with_blob(field_name, file)
            .map_err(|_| "无法附加文件".to_string())?;

        // credentials same-origin 携带 session cookie
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
        init.set_body(form.as_ref());
        init.set_credentials(web_sys::RequestCredentials::SameOrigin);

        let request = web_sys::Request::new_with_str_and_init(url, &init)
            .map_err(|_| "无法构造上传请求".to_string())?;

        let window = web_sys::window().expect("multipart 上传仅在浏览器环境发生");
        let promise = window.fetch_with_request(&request);

        let resp_val = wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|_| "网络连接中断，请检查网络后重试".to_string())?;
        let resp: web_sys::Response = resp_val
            .dyn_into()
            .map_err(|_| "没有收到服务器的响应".to_string())?;

        let text_promise = resp
            .text()
            .map_err(|_| "没有收到完整的服务器响应".to_string())?;
        let text_val = wasm_bindgen_futures::JsFuture::from(text_promise)
            .await
            .map_err(|_| "没有收到完整的服务器响应".to_string())?;
        let text = text_val.as_string().unwrap_or_default();
        let data: serde_json::Value =
            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);

        if data["success"].as_bool() == Some(true) {
            Ok(data)
        } else {
            // 失败：优先用服务端中文 error；没有 JSON 时多半是反向代理拦下的，按状态码说明。
            Err(data["error"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| match resp.status() {
                    413 => "文件超过服务器允许的大小".to_string(),
                    status => format!("服务器返回了错误（HTTP {status}）"),
                }))
        }
    }

    /// 通用图片上传：multipart POST 指定端点，取 `url` 字段。
    async fn upload_file_to(url: &str, file: web_sys::File) -> Result<String, String> {
        let data = post_multipart_file(url, "image", &file).await?;
        let url = data["url"].as_str().unwrap_or("");
        if url.is_empty() {
            return Err("上传成功但未返回图片地址".to_string());
        }
        Ok(url.to_string())
    }

    /// 评论图片上传（POST /api/comments/upload，允许匿名）。
    ///
    /// 原型服务端未实现该端点，运行时会得到 404 失败提示。
    pub async fn upload_comment_image_file(file: web_sys::File) -> Result<String, String> {
        upload_file_to("/api/comments/upload", file).await
    }

    /// 创建评论区 Tiptap 图片上传 closure：签名 `(File) -> Promise<string>`
    /// 对应 JS `onImageUpload`。
    pub fn make_comment_upload_closure() -> Closure<dyn Fn(web_sys::File) -> js_sys::Promise> {
        Closure::new(move |file: web_sys::File| -> js_sys::Promise {
            wasm_bindgen_futures::future_to_promise(async move {
                upload_comment_image_file(file)
                    .await
                    .map(|url| js_sys::JsString::from(url).into())
                    .map_err(|msg| js_sys::Error::new(&msg).into())
            })
        })
    }
}

/// 将 WASM 子模块中调用方按名字引用的桥接项重导出到模块根。
#[cfg(target_arch = "wasm32")]
pub use wasm::{
    consume_upload_event, get_module, make_comment_upload_closure, EditorHandle, EditorOptions,
    UploadEventJs,
};
