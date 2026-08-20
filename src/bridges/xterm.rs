//! xterm.js 终端桥接：输出专用（无 stdin），配合 SSE 流式渲染容器 stdout/stderr。
//!
//! 镜像 `codemirror_bridge.rs` 范式：
//! - `window.XtermTerminal` 是 IIFE 产物挂在 window 上的对象字面量（含 create 方法），
//!   不是函数。用 `js_sys::Reflect::get` 做属性访问拿到模块对象，再 `unchecked_into`。
//! - `XtermOptions` 是 class（非 interface），TS 擦除后存活，wasm 侧能 `new`。
//! - `TerminalHandle` 持有实例 + onReady 闭包，Drop → destroy()。
//!
//! WASM-only：所有 extern 与 handle 都在 `#[cfg(target_arch = "wasm32")] mod wasm` 内，
//! server 构建整体剥离。无跨目标共享数据（纯渲染层）。

#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    // —— window.XtermTerminal 模块对象 ——
    //
    // XtermTerminal 是 IIFE 产物挂在 window 上的对象字面量（含 create 方法），
    // 不是函数。wasm-bindgen 对 `fn get_module() -> T` 形式的 extern 会生成
    // `window.XtermTerminal()`（函数调用），会因 "not a function" 失败。
    // 因此用 js_sys::Reflect::get 做属性访问拿到模块对象，再 unchecked_into。
    #[wasm_bindgen]
    extern "C" {
        /// `window.XtermTerminal` 模块对象的 Rust 映射（IIFE 产物挂在 window 上的对象字面量）。
        /// 不是函数——通过 [`get_module`] 用 Reflect::get 取属性而非 extern fn 调用拿到。
        pub type XtermTerminalModule;

        /// 调用 `XtermTerminal.create(containerId, opts)`。
        /// 找不到容器返回 null（被 Option 捕获）；构造失败抛异常（被 catch 捕获）。
        #[wasm_bindgen(method, catch)]
        pub fn create(
            this: &XtermTerminalModule,
            container_id: &str,
            opts: &XtermOptions,
        ) -> Result<Option<TerminalInstance>, JsValue>;
    }

    /// 读取 `window.XtermTerminal`（IIFE 默认导出，顶层 var 即 window 属性）。
    /// 用 Reflect::get 做属性访问——extern fn 形式会被 wasm-bindgen 编成函数调用。
    ///
    /// 用 unchecked_into 而非 dyn_into：XtermTerminal 是 JS 对象字面量，
    /// 不是 wasm-bindgen 注册的构造函数实例，dyn_into 的 instanceof 检查必然失败。
    /// unchecked_into 只做编译期类型标注，不做运行时校验
    /// （Reflect.get 已保证拿到的是目标对象）。
    pub fn get_module() -> XtermTerminalModule {
        let window = web_sys::window().expect("no window");
        let val = js_sys::Reflect::get(&window, &"XtermTerminal".into())
            .expect("window.XtermTerminal missing");
        val.unchecked_into::<XtermTerminalModule>()
    }

    // —— 终端实例（TerminalInstance）——
    #[wasm_bindgen]
    extern "C" {
        /// `XtermTerminal.create` 返回的终端实例对象，承载 xterm.js Terminal。
        pub type TerminalInstance;

        /// 流式写入 stdout 块（SSE stdout 事件）。
        #[wasm_bindgen(method, js_name = writeStdout)]
        pub fn write_stdout(this: &TerminalInstance, data: &str);

        /// 流式写入 stderr 块（SSE stderr 事件），红色显示。
        #[wasm_bindgen(method, js_name = writeStderr)]
        pub fn write_stderr(this: &TerminalInstance, data: &str);

        /// 整段写入（轮询兜底路径）：清屏后重写 stdout + stderr。
        #[wasm_bindgen(method, js_name = writeAll)]
        pub fn write_all(this: &TerminalInstance, stdout: &str, stderr: &str);

        /// 热切换主题（Catppuccin Latte/Mocha）。
        #[wasm_bindgen(method, js_name = setTheme)]
        pub fn set_theme(this: &TerminalInstance, theme: &str);

        /// 重新计算列宽以适配容器（如父容器尺寸变化时调用）。
        #[wasm_bindgen(method)]
        pub fn fit(this: &TerminalInstance);

        /// 清屏（新一轮运行前调用）。
        #[wasm_bindgen(method)]
        pub fn clear(this: &TerminalInstance);

        /// 销毁终端，释放 JS 侧资源。
        #[wasm_bindgen(method)]
        pub fn destroy(this: &TerminalInstance);
    }

    // —— XtermOptions：用 builder 模式（setter）构造 JS 对象 ——
    #[wasm_bindgen]
    extern "C" {
        /// 传给 `XtermTerminal.create` 的配置对象，对应 JS 侧的 XtermOptions。
        /// 用 `new()` 创建空对象后通过 setter 链式设置字段。
        pub type XtermOptions;

        /// 构造一个空的 XtermOptions，随后用各 setter 填充。
        #[wasm_bindgen(constructor)]
        pub fn new() -> XtermOptions;

        /// 主题：'light'（Catppuccin Latte）或 'dark'（Catppuccin Mocha）。
        #[wasm_bindgen(method, setter, js_name = theme)]
        pub fn set_theme(this: &XtermOptions, v: &str);

        /// 字号（默认 13）。
        #[wasm_bindgen(method, setter, js_name = fontSize)]
        pub fn set_font_size(this: &XtermOptions, v: u32);

        /// 终端就绪回调（构造末尾同步触发一次）。
        #[wasm_bindgen(method, setter, js_name = onReady)]
        pub fn set_on_ready(this: &XtermOptions, cb: &Closure<dyn FnMut()>);
    }

    /// 终端实例句柄：持有 instance + onReady 闭包，Drop 时销毁实例并释放闭包。
    ///
    /// 闭包字段 `_` 前缀表示仅用于保持生命周期——它被注入 JS 后，JS 侧持有
    /// 函数引用；只要 [`TerminalHandle`] 存活，闭包就不会被回收。Drop 时随结构释放。
    pub struct TerminalHandle {
        instance: TerminalInstance,
        _on_ready: Closure<dyn FnMut()>,
    }

    impl TerminalHandle {
        /// 调用方须先把 on_ready set 进 XtermOptions，再 create，
        /// 然后把返回的 instance + 同一 closure 一起传入 new。
        pub fn new(instance: TerminalInstance, on_ready: Closure<dyn FnMut()>) -> Self {
            Self {
                instance,
                _on_ready: on_ready,
            }
        }

        /// 借用底层实例，供宿主调 writeStdout/writeStderr/setTheme 等。
        pub fn instance(&self) -> &TerminalInstance {
            &self.instance
        }
    }

    impl Drop for TerminalHandle {
        fn drop(&mut self) {
            // 销毁 JS 侧终端；随后 _on_ready 字段释放，释放 wasm-bindgen 函数表槽位。
            self.instance.destroy();
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
