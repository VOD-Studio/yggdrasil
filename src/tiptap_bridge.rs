//! Tiptap 编辑器的 wasm-bindgen 绑定层。
//!
//! 封装与 `window.TiptapEditor`（IIFE 暴露的全局对象）的全部交互，
//! 替代旧版 `js_sys::eval` 字符串拼贴 + window 全局变量通信。
//!
//! wasm-bindgen extern 与 `EditorHandle`、上传 closure 等**仅在 WASM 前端**编译
//! （server 构建无 `window`）；共享的纯数据类型 `UploadsInFlight`/`UploadErrorEntry`
//! 在两端都编译，供 `write.rs` 在 rsx 中渲染上传状态。

/// 当前编辑器内进行中的上传计数（来自 onUploadEvent 的 counts 快照）。
#[derive(Clone, Copy, Default)]
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
// 放在 #[cfg] 子模块内，避免 server 构建尝试编译引用 JS 对象的 extern。
// ============================================================================
#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use super::{UploadErrorEntry, UploadsInFlight};
    // WritableExt 提供 .write()（Signal 在 Copy 语义下不需要 mut 绑定）。
    use dioxus::prelude::WritableExt;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    // —— window.TiptapEditor 模块对象 ——
    //
    // TiptapEditor 是 IIFE 产物挂在 window 上的模块对象（含 create 等方法），
    // 不是函数。wasm-bindgen 对 `fn get_module() -> T` 形式的 extern 会生成
    // `window.TiptapEditor()`（函数调用），会因"not a function"失败。
    // 因此用 js_sys::Reflect::get 做属性访问拿到模块对象，再 dyn_into。
    #[wasm_bindgen]
    extern "C" {
        /// `window.TiptapEditor` 模块对象的 Rust 映射（IIFE 产物挂在 window 上的对象字面量）。
        /// 不是函数——通过 [`get_module`] 用 Reflect::get 取属性而非 extern fn 调用拿到。
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
    /// 用 Reflect::get 做属性访问——extern fn 形式会被 wasm-bindgen 编成函数调用。
    ///
    /// 用 unchecked_into 而非 dyn_into：TiptapEditor 是 JS 对象字面量，
    /// 不是 wasm-bindgen 注册的构造函数实例，dyn_into 的 instanceof 检查必然失败。
    /// unchecked_into 只做编译期类型标注，不做运行时校验（Reflect.get 已保证拿到的是目标对象）。
    pub fn get_module() -> TiptapEditorModule {
        let window = web_sys::window().expect("no window");
        let val = js_sys::Reflect::get(&window, &"TiptapEditor".into())
            .expect("window.TiptapEditor missing");
        val.unchecked_into::<TiptapEditorModule>()
    }

    // —— 编辑器实例（TiptapEditorInstance）——
    #[wasm_bindgen]
    extern "C" {
        /// `TiptapEditor.create` 返回的编辑器实例对象，承载 ProseMirror 编辑器与上传协调器。
        pub type EditorInstance;

        /// 富文本模式下返回 ProseMirror 的 Markdown；源码模式下返回 textarea 内容。
        #[wasm_bindgen(method, js_name = getMarkdown)]
        pub fn get_markdown(this: &EditorInstance) -> String;

        /// 用 Markdown 内容回填编辑器（emitUpdate: false，不触发 onUpdate）。
        #[wasm_bindgen(method, js_name = setMarkdown)]
        pub fn set_markdown(this: &EditorInstance, content: &str);

        /// 按 uploadId 删除上传节点（revoke blob + 删 pending）。供宿主"×关闭"调用。
        #[wasm_bindgen(method, js_name = removeUploadByUploadId)]
        pub fn remove_upload_by_upload_id(this: &EditorInstance, upload_id: &str) -> bool;

        /// 工具栏图片按钮入口：把文件交给 coordinator 走占位符上传
        /// （与粘贴/拖放同一条路径）。
        #[wasm_bindgen(method, js_name = insertUploading)]
        pub fn insert_uploading(this: &EditorInstance, file: web_sys::File);

        /// 从素材库批量插入图片：`json` 为 `[{ "src", "alt"? }]` 串
        /// （`AssetSelection` 的 serde 序列化形状）。JS 侧在 slash 命令删除
        /// `/素材` 文本后停留的光标位置一次事务插入全部图片块；非法载荷 no-op。
        #[wasm_bindgen(method, js_name = insertImagesFromLibrary)]
        pub fn insert_images_from_library(this: &EditorInstance, json: &str);

        /// 销毁编辑器，释放 JS 侧资源。
        #[wasm_bindgen(method)]
        pub fn destroy(this: &EditorInstance);
    }

    // —— EditorOptions：用 builder 模式（setter）构造 JS 对象 ——
    #[wasm_bindgen]
    extern "C" {
        /// 传给 `TiptapEditor.create` 的配置对象，对应 JS 侧的 EditorOptions。
        /// 用 `new()` 创建空对象后通过 setter 链式设置字段。
        pub type EditorOptions;

        /// 构造一个空的 EditorOptions，随后用各 setter 填充回调与占位文案。
        #[wasm_bindgen(constructor)]
        pub fn new() -> EditorOptions;

        /// 编辑器无内容时的占位文案。
        #[wasm_bindgen(method, setter, js_name = placeholder)]
        pub fn set_placeholder(this: &EditorOptions, v: &str);

        /// 编辑器变体：`"full"`（默认，后台文章编辑器）或 `"comment"`
        /// （评论区精简子集：气泡菜单 + Placeholder，无标题/表格/斜杠命令等）。
        #[wasm_bindgen(method, setter, js_name = variant)]
        pub fn set_variant(this: &EditorOptions, v: &str);

        /// 文档变更回调（ProseMirror transaction 提交时触发，参数为最新 Markdown）。
        #[wasm_bindgen(method, setter, js_name = onUpdate)]
        pub fn set_on_update(this: &EditorOptions, cb: &Closure<dyn FnMut(String)>);

        /// JS 侧 onImageUpload: (file: File) => Promise<string>。
        /// Rust closure 返回 js_sys::Promise，由 future_to_promise 包装。
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

        /// JS 侧 onRunCode: (opts: RunCodeOptsJs) => Promise<string>。
        /// Rust closure 返回 js_sys::Promise，内部调 start_exec + 轮询 get_exec_result。
        #[wasm_bindgen(method, setter, js_name = onRunCode)]
        pub fn set_on_run_code(
            this: &EditorOptions,
            cb: &Closure<dyn Fn(RunCodeOptsJs) -> js_sys::Promise>,
        );

        /// JS 侧 onPickFromLibrary: () => void。
        /// slash 命令「素材库」触发（此时 /命令 文本已被 JS 删除、光标停在删除位置）；
        /// Rust 侧打开 AssetPickerModal，确认后由 insertImagesFromLibrary 回填。
        #[wasm_bindgen(method, setter, js_name = onPickFromLibrary)]
        pub fn set_on_pick_from_library(this: &EditorOptions, cb: &Closure<dyn FnMut()>);
    }

    // —— 上传事件（JS UploadEvent 的 Rust 映射）——
    #[wasm_bindgen]
    extern "C" {
        /// 上传协调器 emit 的事件对象，对应 JS 侧 UploadCoordinator 发出的事件。
        /// 由 `onUploadEvent` 回调回传 Rust，[`consume_upload_event`] 据其更新 UI 状态。
        #[derive(Clone)]
        pub type UploadEventJs;

        /// 事件种类：`"uploading"` / `"success"` / `"error"` / `"removed"`。
        #[wasm_bindgen(method, getter)]
        pub fn kind(this: &UploadEventJs) -> String;

        /// 本次上传的唯一标识（前端生成，用于关联上传节点与失败提示条目）。
        #[wasm_bindgen(method, getter, js_name = uploadId)]
        pub fn upload_id(this: &UploadEventJs) -> String;

        /// 上传文件名（用于失败提示展示）。
        #[wasm_bindgen(method, getter, js_name = fileName)]
        pub fn file_name(this: &UploadEventJs) -> String;

        /// 失败原因（仅 `error` 事件有值，成功/移除事件为 None）。
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

    // —— RunCodeOptsJs：onRunCode 回调参数（JS 侧传给 Rust 的纯数据对象）——
    /// JS 侧传给 onRunCode 的参数对象，Rust 侧读取 getter。
    /// language 是纯语言名（如 "python"，前端已 extractLang 提取）；overridesJson 是 overrides 的 JSON 字符串（可能为空）。
    #[wasm_bindgen]
    extern "C" {
        pub type RunCodeOptsJs;

        /// 纯语言名（前端 extractLang 从完整 info string 提取，如 "python"）。
        #[wasm_bindgen(method, getter)]
        pub fn language(this: &RunCodeOptsJs) -> String;

        /// 代码块文本内容。
        #[wasm_bindgen(method, getter)]
        pub fn source(this: &RunCodeOptsJs) -> String;

        /// overrides 的 JSON 字符串（如 `{"timeout_secs":10}`），空串表示无 overrides。
        /// 前端从 info string 提取（大括号部分），Rust 用 serde_json 反序列化。
        #[wasm_bindgen(method, getter, js_name = overridesJson)]
        pub fn overrides_json(this: &RunCodeOptsJs) -> String;
    }

    // —— EditorHandle：实例 + closure 统一生命周期 ——

    /// 持有编辑器实例 + 其全部 closure，统一生命周期。
    ///
    /// drop 时先 destroy JS 实例（释放 ProseMirror 资源），随后 closure 字段
    /// 按声明逆序自动 drop（释放 wasm-bindgen 回调表）。比 `Closure::forget`
    /// （永久泄漏）干净——closure 严格随编辑器实例同生共死。
    pub struct EditorHandle {
        instance: EditorInstance,
        _on_update: Closure<dyn FnMut(String)>,
        _on_image_upload: Closure<dyn Fn(web_sys::File) -> js_sys::Promise>,
        _on_ready: Closure<dyn FnMut()>,
        _on_upload_event: Closure<dyn FnMut(UploadEventJs)>,
        _on_run_code: Closure<dyn Fn(RunCodeOptsJs) -> js_sys::Promise>,
        _on_pick_from_library: Closure<dyn FnMut()>,
    }

    impl EditorHandle {
        /// 聚合编辑器实例与回调 closure，使其共用同一生命周期。
        ///
        /// 调用方负责先用各 setter 把 closure 装进 [`EditorOptions`]、`create` 出实例后，
        /// 再把实例与这些 closure 一并交由本函数持有。返回的 [`EditorHandle`] 一旦
        /// drop，会先 `destroy` 实例、再按字段逆序 drop closure。
        pub fn new(
            instance: EditorInstance,
            on_update: Closure<dyn FnMut(String)>,
            on_image_upload: Closure<dyn Fn(web_sys::File) -> js_sys::Promise>,
            on_ready: Closure<dyn FnMut()>,
            on_upload_event: Closure<dyn FnMut(UploadEventJs)>,
            on_run_code: Closure<dyn Fn(RunCodeOptsJs) -> js_sys::Promise>,
            on_pick_from_library: Closure<dyn FnMut()>,
        ) -> Self {
            Self {
                instance,
                _on_update: on_update,
                _on_image_upload: on_image_upload,
                _on_ready: on_ready,
                _on_upload_event: on_upload_event,
                _on_run_code: on_run_code,
                _on_pick_from_library: on_pick_from_library,
            }
        }

        /// 访问底层编辑器实例（调 getMarkdown/setMarkdown/destroy 等）。
        pub fn instance(&self) -> &EditorInstance {
            &self.instance
        }

        /// 评论编辑器用构造：只传入 comment variant 实际使用的 4 个 closure。
        ///
        /// run_code / pick_from_library 在 comment variant 下没有触发方
        /// （无斜杠命令、无代码块 NodeView），以 no-op closure 补齐生命周期结构。
        pub fn new_comment(
            instance: EditorInstance,
            on_update: Closure<dyn FnMut(String)>,
            on_image_upload: Closure<dyn Fn(web_sys::File) -> js_sys::Promise>,
            on_ready: Closure<dyn FnMut()>,
            on_upload_event: Closure<dyn FnMut(UploadEventJs)>,
        ) -> Self {
            Self::new(
                instance,
                on_update,
                on_image_upload,
                on_ready,
                on_upload_event,
                Closure::new(|_opts: RunCodeOptsJs| js_sys::Promise::resolve(&JsValue::null())),
                Closure::new(|| {}),
            )
        }
    }

    impl Drop for EditorHandle {
        fn drop(&mut self) {
            self.instance.destroy();
        }
    }

    // —— consume_upload_event：纯逻辑 helper ——

    /// 消费单个上传事件，更新 signal（即时驱动，替代旧版 500ms 轮询）。
    ///
    /// 逻辑与旧轮询 body 一致：
    /// - error：新 id 追加提示，已存在 id 原地更新消息（重试后再失败）
    /// - success/removed：移除对应提示
    /// - counts：直接从事件读（JS 已遍历文档算好）
    ///
    /// 去重以 `upload_errors` Vec 自身为唯一数据源（用 iter().any 判重），
    /// 不再额外维护 seen_error_ids，避免两份状态需手动同步。
    pub fn consume_upload_event(
        ev: &UploadEventJs,
        mut uploads_in_flight: dioxus::prelude::Signal<UploadsInFlight>,
        mut upload_errors: dioxus::prelude::Signal<Vec<UploadErrorEntry>>,
    ) {
        let id = ev.upload_id();
        match ev.kind().as_str() {
            "error" => {
                let msg = ev.error_msg().unwrap_or_else(|| "上传失败".to_string());
                // 已存在同 id（重试后再失败）：原地更新消息；否则追加。
                let mut errors = upload_errors.write();
                if let Some(entry) = errors.iter_mut().find(|e| e.id == id) {
                    entry.message = msg;
                } else {
                    errors.push(UploadErrorEntry {
                        id: id.clone(),
                        file_name: ev.file_name(),
                        message: msg,
                    });
                }
            }
            "success" | "removed" => {
                upload_errors.write().retain(|e| e.id != id);
            }
            _ => {}
        }
        // counts 直接从事件读（JS 已算好）
        let c = ev.counts();
        uploads_in_flight.set(UploadsInFlight {
            uploading: c.uploading(),
            error: c.error(),
        });
    }

    // —— make_upload_closure：Rust fetch 上传 ——

    /// 通用图片上传：multipart POST 指定端点，解析 {success, url, error}。
    ///
    /// 由封面图上传（spawn 直接 await）与 Tiptap 编辑器上传（make_upload_closure 包 Promise）共用；
    /// fetch + 契约解析样板收敛在 [`crate::utils::web_upload::post_multipart_file`]
    /// （与备份导入共享），此处只取 `url` 字段。
    ///
    /// 行为：
    /// - credentials: same-origin（携带 session cookie）
    /// - 字段名 'image'（与服务端 upload.rs 对齐）
    /// - 成功 → Ok(url)；失败 → Err(服务端中文 error，或状态码兜底)
    ///
    /// 构造阶段的错误以 Err 返回，而非 panic，
    /// 单张坏文件不应导致整个上传流程崩溃。
    async fn upload_file_to(url: &str, file: web_sys::File) -> Result<String, String> {
        let data = crate::utils::web_upload::post_multipart_file(url, "image", &file).await?;
        let url = data["url"].as_str().unwrap_or("");
        if url.is_empty() {
            // success=true 但 url 为空：服务端契约异常，按失败处理
            return Err("上传成功但未返回图片地址".to_string());
        }
        Ok(url.to_string())
    }

    /// admin 图片上传（POST /api/upload，需 admin 会话）。
    pub async fn upload_image_file(file: web_sys::File) -> Result<String, String> {
        upload_file_to("/api/upload", file).await
    }

    /// 评论图片上传（POST /api/comments/upload，允许匿名，IP 双层限流）。
    pub async fn upload_comment_image_file(file: web_sys::File) -> Result<String, String> {
        upload_file_to("/api/comments/upload", file).await
    }

    /// 创建 Tiptap 图片上传 closure：内部复用 [`upload_image_file`]，包装成 JS Promise。
    ///
    /// 返回的 closure 签名 `(File) -> Promise` 对应 JS `onImageUpload`。
    pub fn make_upload_closure() -> Closure<dyn Fn(web_sys::File) -> js_sys::Promise> {
        Closure::new(move |file: web_sys::File| -> js_sys::Promise {
            wasm_bindgen_futures::future_to_promise(async move {
                upload_image_file(file)
                    .await
                    .map(|url| js_sys::JsString::from(url).into())
                    .map_err(|msg| js_sys::Error::new(&msg).into())
            })
        })
    }

    /// 创建评论区 Tiptap 图片上传 closure：走匿名可用的评论端点
    /// （[`upload_comment_image_file`]），其余语义与 [`make_upload_closure`] 一致。
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

    // —— make_run_code_closure：编辑器内运行代码 ——

    /// 把 ExecTask 格式化为结果字符串（供编辑器结果区展示）。
    fn format_run_result(task: &crate::api::code_runner::ExecTask) -> String {
        use crate::api::code_runner::ExecStatus;
        let status_label = match task.status {
            ExecStatus::Success => "Success",
            ExecStatus::Error => "Error",
            ExecStatus::Timeout => "Timeout",
            ExecStatus::OomKilled => "OOM",
            ExecStatus::Failed => "Failed",
            _ => "Unknown",
        };
        match &task.result {
            Some(res) => {
                let mut out = format!("状态: {} · 耗时: {}ms", status_label, res.duration_ms);
                if !res.stdout.is_empty() {
                    out.push_str("\nStdout:\n");
                    out.push_str(&res.stdout);
                }
                if !res.stderr.is_empty() {
                    out.push_str("\nStderr:\n");
                    out.push_str(&res.stderr);
                }
                out
            }
            None => format!("状态: {} · {}", status_label, task.stage),
        }
    }

    /// 创建「编辑器内运行代码」closure：内部调 start_exec + 轮询 get_exec_result，
    /// 把格式化结果字符串回传 JS。
    ///
    /// 返回的 closure 签名 `(RunCodeOptsJs) -> Promise` 对应 JS `onRunCode`。
    /// JS 侧 NodeView await Promise，拿到字符串直接填进结果区 DOM。
    ///
    /// 注意：info string 的解析（提取语言名 + overrides JSON）在前端 extractLang 完成，
    /// Rust 收到的 language 已是纯语言名（如 "python"），不依赖 server-only 的 languages 模块。
    pub fn make_run_code_closure() -> Closure<dyn Fn(RunCodeOptsJs) -> js_sys::Promise> {
        Closure::new(move |opts: RunCodeOptsJs| -> js_sys::Promise {
            wasm_bindgen_futures::future_to_promise(async move {
                use crate::api::code_runner::{execute, ExecRequest, ExecStatus};
                use crate::infra::runner_config::ResourceLimits;

                let language = opts.language();
                let source = opts.source();
                let overrides_json = opts.overrides_json();

                // 反序列化 overrides JSON（前端已提取大括号部分；空串视为 None）
                let overrides = if overrides_json.trim().is_empty() {
                    None
                } else {
                    match serde_json::from_str::<ResourceLimits>(&overrides_json) {
                        Ok(o) => Some(o),
                        Err(_) => None, // 畸形 JSON 静默降级为无 overrides
                    }
                };

                let req = ExecRequest {
                    language,
                    source,
                    overrides,
                };

                match execute::start_exec(req).await {
                    Ok(task_id) => {
                        let poll_interval = 500;
                        // 500ms * 60 = 30s 上限（编辑器内运行是写作辅助，比 reader 的 120s 短）
                        for _ in 0..60 {
                            crate::utils::time::sleep_ms(poll_interval).await;
                            match execute::get_exec_result(task_id.clone()).await {
                                Ok(task) => {
                                    let terminal = task.status != ExecStatus::Queued
                                        && task.status != ExecStatus::Running;
                                    if terminal {
                                        let s = format_run_result(&task);
                                        return Ok(js_sys::JsString::from(s).into());
                                    }
                                }
                                Err(_) => {
                                    return Err(js_sys::Error::new("结果获取异常").into());
                                }
                            }
                        }
                        Err(js_sys::Error::new("轮询超时，请重试").into())
                    }
                    Err(e) => Err(js_sys::Error::new(&e.to_string()).into()),
                }
            })
        })
    }
}

/// 将 WASM 子模块中的桥接类型与函数重导出到 crate 根，供 `write.rs` 直接引用。
/// server 构建剥离该子模块，故此重导出仅对 WASM 前端生效。
#[cfg(target_arch = "wasm32")]
pub use wasm::{
    consume_upload_event, get_module, make_comment_upload_closure, make_run_code_closure,
    make_upload_closure, upload_image_file, EditorHandle, EditorOptions, UploadEventJs,
};
