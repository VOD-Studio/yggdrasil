//! CodeMirror 编辑器的 wasm-bindgen 绑定层。
//!
//! 封装与 `window.CodeMirrorEditor`（IIFE 暴露的全局对象字面量）的全部交互，
//! 严格镜像 [`crate::bridges::tiptap`] 的结构：wasm-bindgen extern +
//! `EditorHandle` 仅在 WASM 前端编译（server 构建无 window）。SQL 补全用的
//! `SqlSchema`/`SqlTable` 是 server function 返回值 DTO，定义在
//! [`crate::models::sql_schema`]（不属于本文件——避免 API 层反向依赖桥接层）。
//!
//! 与 tiptap 一样，`create` 以 js_namespace 绑定为成员调用，调用时才解析全局；
//! 脚本未加载时返回 Err。
//!
//! 用法（Leptos）：挂载点用 `NodeRef` 取元素，先用 `use_browser_library(BrowserLibrary::CodeMirror, ..)` 等库就绪，
//! 再在 `Effect` 内构造 `EditorOptions` 并 `create(&element, &opts)`；`EditorHandle` 含 JS 引用
//! （非 `Send + Sync`），放进 `StoredValue::new_local`/`RwSignal::new_local`，`on_cleanup` 里 `take`
//! 出来丢弃即触发 `Drop` → `destroy`。

// ============================================================================
// 以下全部仅在 WASM 前端编译：wasm-bindgen extern + EditorHandle + 闭包。
// 放在 #[cfg] 子模块内，避免 server 构建尝试编译引用 JS 对象的 extern。
// ============================================================================
#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    // —— window.CodeMirrorEditor.create ——
    //
    // `js_namespace` 把 `create` 绑成 `CodeMirrorEditor.create(...)` 的成员调用，
    // 全局在调用时才解析：脚本未加载时返回 Err（TypeError），而不是在绑定阶段 panic。
    #[wasm_bindgen]
    extern "C" {
        /// `CodeMirrorEditor.create(container, options)`：在元素上创建实例，替换该元素上的旧实例。
        /// 构造失败抛异常（被 catch 捕获）。
        #[wasm_bindgen(js_namespace = CodeMirrorEditor, js_name = create, catch)]
        pub fn create(
            container: &web_sys::HtmlElement,
            options: &EditorOptions,
        ) -> Result<EditorInstance, JsValue>;
    }

    // —— 编辑器实例（CodeMirrorInstance）——
    #[wasm_bindgen]
    extern "C" {
        /// `CodeMirrorEditor.create` 返回的编辑器实例对象，承载 CodeMirror EditorView。
        pub type EditorInstance;

        /// 返回当前文档全文。
        #[wasm_bindgen(method, js_name = getValue)]
        pub fn get_value(this: &EditorInstance) -> String;

        /// 替换整个文档内容（dispatch changes，触发 onChange）。
        #[wasm_bindgen(method, js_name = setValue)]
        pub fn set_value(this: &EditorInstance, s: &str);

        /// 热切换主题（Compartment.reconfigure，不重建实例）。
        #[wasm_bindgen(method, js_name = setTheme)]
        pub fn set_theme(this: &EditorInstance, theme: &str);

        /// 热切换 Vim 模式（Compartment.reconfigure，不重建实例）。
        #[wasm_bindgen(method, js_name = setVim)]
        pub fn set_vim(this: &EditorInstance, v: bool);

        /// 热切换语言（go/rust/python/node/javascript/sql，Compartment.reconfigure）。
        /// 由 CodeRunner 组件在挂载时按 data-lang 调用。
        #[wasm_bindgen(method, js_name = setLanguage)]
        pub fn set_language(this: &EditorInstance, lang: &str);

        /// 更新 SQL 补全 schema（Compartment.reconfigure）。
        /// 参数为 serde_wasm_bindgen::to_value 序列化后的 JsValue
        ///（SqlSchema 是 serde 类型，非 wasm-bindgen 类型，故不能直接传 &SqlSchema）。
        #[wasm_bindgen(method, js_name = setSchema)]
        pub fn set_schema(this: &EditorInstance, schema: &wasm_bindgen::JsValue);

        /// 让编辑器获取焦点。
        #[wasm_bindgen(method)]
        pub fn focus(this: &EditorInstance);

        /// 销毁编辑器，释放 JS 侧资源。
        #[wasm_bindgen(method)]
        pub fn destroy(this: &EditorInstance);
    }

    // —— EditorOptions：用 builder 模式（setter）构造 JS 对象 ——
    #[wasm_bindgen]
    extern "C" {
        /// 传给 `CodeMirrorEditor.create` 的配置对象，对应 JS 侧的 EditorOptions。
        /// 用 `default()` 创建空对象后通过 setter 设置字段。
        pub type EditorOptions;

        /// 语言（默认 'sql'）。
        #[wasm_bindgen(method, setter, js_name = language)]
        pub fn set_language(this: &EditorOptions, v: &str);

        /// 主题：'light' 或 'dark'（配色取自 `CODE_PALETTES`）。
        #[wasm_bindgen(method, setter, js_name = theme)]
        pub fn set_theme(this: &EditorOptions, v: &str);

        /// 是否启用 Vim keymap。
        #[wasm_bindgen(method, setter, js_name = vim)]
        pub fn set_vim(this: &EditorOptions, v: bool);

        /// SQL 补全 schema（表/列数据）。v 为 serde_wasm_bindgen::to_value 序列化结果。
        #[wasm_bindgen(method, setter, js_name = schema)]
        pub fn set_schema(this: &EditorOptions, v: &wasm_bindgen::JsValue);

        /// 初始文档内容。
        #[wasm_bindgen(method, setter, js_name = value)]
        pub fn set_value(this: &EditorOptions, v: &str);

        /// 文档变更回调（参数为最新全文）。
        #[wasm_bindgen(method, setter, js_name = onChange)]
        pub fn set_on_change(this: &EditorOptions, cb: &Closure<dyn FnMut(String)>);

        /// 编辑器就绪回调（构造末尾同步触发一次）。
        #[wasm_bindgen(method, setter, js_name = onReady)]
        pub fn set_on_ready(this: &EditorOptions, cb: &Closure<dyn FnMut()>);

        /// Ctrl/Cmd + Enter 快捷键回调（SQL 控制台、可运行代码块触发执行）。
        #[wasm_bindgen(method, setter, js_name = onRunShortcut)]
        pub fn set_on_run_shortcut(this: &EditorOptions, cb: &Closure<dyn FnMut()>);

        /// Vim 模式变化回调（normal / insert / visual / replace）。
        #[wasm_bindgen(method, setter, js_name = onVimModeChange)]
        pub fn set_on_vim_mode_change(this: &EditorOptions, cb: &Closure<dyn FnMut(String)>);
    }

    impl Default for EditorOptions {
        /// 空 JS 对象；字段只经上面的 setter 写入，JS 端按同名 interface 读取。
        fn default() -> Self {
            js_sys::Object::new().unchecked_into()
        }
    }

    /// 编辑器实例句柄：持有 instance + 所有 Closure，Drop 时销毁实例并释放闭包。
    ///
    /// 闭包字段 `_` 前缀表示仅用于保持生命周期——它们被注入 JS 后，JS 侧持有
    /// 函数引用；只要 [`EditorHandle`] 存活，闭包就不会被回收。Drop 时随结构释放。
    pub struct EditorHandle {
        instance: EditorInstance,
        _on_change: Closure<dyn FnMut(String)>,
        _on_ready: Closure<dyn FnMut()>,
        _on_run_shortcut: Closure<dyn FnMut()>,
        _on_vim_mode_change: Option<Closure<dyn FnMut(String)>>,
    }

    impl EditorHandle {
        /// 调用方须先把各 closure set 进 EditorOptions，再 create，
        /// 然后把返回的 instance + 同名 closure 一起传入 new。
        /// `on_run_shortcut` 对应 Ctrl/Cmd+Enter 回调；不用该功能时传 no-op 闭包。
        pub fn new(
            instance: EditorInstance,
            on_change: Closure<dyn FnMut(String)>,
            on_ready: Closure<dyn FnMut()>,
            on_run_shortcut: Closure<dyn FnMut()>,
        ) -> Self {
            Self {
                instance,
                _on_change: on_change,
                _on_ready: on_ready,
                _on_run_shortcut: on_run_shortcut,
                _on_vim_mode_change: None,
            }
        }

        /// 一并持有已注入 EditorOptions 的 Vim 模式回调。
        pub fn with_vim_mode(mut self, on_vim_mode_change: Closure<dyn FnMut(String)>) -> Self {
            self._on_vim_mode_change = Some(on_vim_mode_change);
            self
        }

        /// 借用底层实例，供宿主调 getValue/setTheme/setSchema 等。
        pub fn instance(&self) -> &EditorInstance {
            &self.instance
        }
    }

    impl Drop for EditorHandle {
        fn drop(&mut self) {
            // 销毁 JS 侧编辑器；随后 _on_change/_on_ready 字段按声明顺序释放，
            // 释放 wasm-bindgen 函数表槽位。
            self.instance.destroy();
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
