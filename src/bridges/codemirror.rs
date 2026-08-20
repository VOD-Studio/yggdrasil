//! CodeMirror 编辑器的 wasm-bindgen 绑定层。
//!
//! 封装与 `window.CodeMirrorEditor`（IIFE 暴露的全局对象字面量）的全部交互，
//! 严格镜像 [`crate::tiptap_bridge`] 的结构：共享纯数据类型双目标编译，
//! wasm-bindgen extern + `EditorHandle` 仅在 WASM 前端编译（server 构建无 window）。
//!
//! 与 tiptap 一样，`CodeMirrorEditor` 是 IIFE 挂在 window 上的**对象字面量**
//! （`{ create }`），不是函数——因此用 `js_sys::Reflect::get` 做属性访问拿到，
//! 不能用 wasm-bindgen 的 extern fn（那会被编成函数调用，"not a function"）。

use serde::{Deserialize, Serialize};

/// SQL 补全用 schema 数据，由 `get_db_schema` server function 填充。
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct SqlSchema {
    pub tables: Vec<SqlTable>,
}

/// 单张表的补全数据：表名 + 列名列表。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SqlTable {
    pub name: String,
    pub columns: Vec<String>,
}

// ============================================================================
// 以下全部仅在 WASM 前端编译：wasm-bindgen extern + EditorHandle + 闭包。
// 放在 #[cfg] 子模块内，避免 server 构建尝试编译引用 JS 对象的 extern。
// ============================================================================
#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    // —— window.CodeMirrorEditor 模块对象 ——
    //
    // CodeMirrorEditor 是 IIFE 产物挂在 window 上的对象字面量（含 create 方法），
    // 不是函数。wasm-bindgen 对 `fn get_module() -> T` 形式的 extern 会生成
    // `window.CodeMirrorEditor()`（函数调用），会因 "not a function" 失败。
    // 因此用 js_sys::Reflect::get 做属性访问拿到模块对象，再 unchecked_into。
    #[wasm_bindgen]
    extern "C" {
        /// `window.CodeMirrorEditor` 模块对象的 Rust 映射（IIFE 产物挂在 window 上的对象字面量）。
        /// 不是函数——通过 [`get_module`] 用 Reflect::get 取属性而非 extern fn 调用拿到。
        pub type CodeMirrorEditorModule;

        /// 调用 `CodeMirrorEditor.create(containerId, opts)`。
        /// 找不到容器返回 null（被 Option 捕获）；构造失败抛异常（被 catch 捕获）。
        #[wasm_bindgen(method, catch)]
        pub fn create(
            this: &CodeMirrorEditorModule,
            container_id: &str,
            opts: &EditorOptions,
        ) -> Result<Option<EditorInstance>, JsValue>;
    }

    /// 读取 `window.CodeMirrorEditor`（IIFE 默认导出，顶层 var 即 window 属性）。
    /// 用 Reflect::get 做属性访问——extern fn 形式会被 wasm-bindgen 编成函数调用。
    ///
    /// 用 unchecked_into 而非 dyn_into：CodeMirrorEditor 是 JS 对象字面量，
    /// 不是 wasm-bindgen 注册的构造函数实例，dyn_into 的 instanceof 检查必然失败。
    /// unchecked_into 只做编译期类型标注，不做运行时校验
    /// （Reflect.get 已保证拿到的是目标对象）。
    pub fn get_module() -> CodeMirrorEditorModule {
        let window = web_sys::window().expect("no window");
        let val = js_sys::Reflect::get(&window, &"CodeMirrorEditor".into())
            .expect("window.CodeMirrorEditor missing");
        val.unchecked_into::<CodeMirrorEditorModule>()
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

        /// 热切换语言（python/node/javascript/sql，Compartment.reconfigure）。
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
        /// 用 `new()` 创建空对象后通过 setter 链式设置字段。
        pub type EditorOptions;

        /// 构造一个空的 EditorOptions，随后用各 setter 填充。
        #[wasm_bindgen(constructor)]
        pub fn new() -> EditorOptions;

        /// 语言（默认 'sql'）。
        #[wasm_bindgen(method, setter, js_name = language)]
        pub fn set_language(this: &EditorOptions, v: &str);

        /// 主题：'light'（Catppuccin Latte）或 'dark'（Catppuccin Mocha）。
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

        /// Ctrl/Cmd + Enter 快捷键回调（SQL 控制台触发执行）。
        #[wasm_bindgen(method, setter, js_name = onRunShortcut)]
        pub fn set_on_run_shortcut(this: &EditorOptions, cb: &Closure<dyn FnMut()>);
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
            }
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
