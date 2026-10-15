//! CodeRunner 组件实现：源码 + 运行按钮 + SSE 流式输出 + xterm.js 终端。
//!
//! 编辑器挂载：组件在 WASM 端按自身 `container_id` 调用
//! `codemirror::get_module().create(...)` 挂载 CodeMirror，`onChange`
//! 回写到内部 `source_signal`，`use_drop` 时销毁实例。范式镜像 SQL 控制台
//!（`src/pages/admin/system.rs`）与 Tiptap 编辑器（`src/pages/admin/write.rs`）。
//!
//! 输出渲染：WASM 端按 `output_container_id` 调用 `xterm::get_module().create(...)`
//! 挂载 xterm.js 终端（输出专用，无 stdin），SSE stdout/stderr 事件实时写入。
//! SSE 不可用时降级到轮询 get_exec_result，整段写入终端（writeAll）。

#[cfg(target_arch = "wasm32")]
mod editor;
mod execution;
mod state;
#[cfg(target_arch = "wasm32")]
mod stream;
#[cfg(target_arch = "wasm32")]
mod terminal;
use crate::api::code_runner::ExecRequest;
use crate::bridges::library::{use_browser_library, LibraryLoadError};
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::{BTN_PRIMARY_SM, SPINNER_SVG};
use crate::infra::runner_config::ResourceLimits;
use dioxus::prelude::*;

/// Fixed local output for callers that demonstrate the runner without execution.
#[derive(Clone, PartialEq)]
pub struct RunnerExample {
    pub stdout: String,
    pub stderr: String,
    pub status: String,
    pub error: String,
}

/// 代码运行器组件。
///
/// Props：
/// - `source`：初始源码（首次挂载用于初始化编辑器；之后编辑器内容是唯一真源）。
/// - `language`：语言标识（python / node 等）。
/// - `overrides`：可选资源限制覆盖。
/// - `instance_id`：实例在父级片段序列中的索引，用作 CodeMirror 容器 id 后缀。
///   必须是 **SSR/hydration 确定性**的值（如父组件 `for (i, ..)` 的索引 `i`）——
///   Dioxus hydration 不传递 use_hook 状态，任何在 use_hook 内基于运行时状态
///   （时间戳 / 随机 / ScopeId）生成的 id，在 SSR 与 hydration 两端会不一致，
///   导致 CodeMirror `create()` 在 hydration 时找不到 SSR 渲染的容器元素。
///
#[component]
#[allow(unused_mut)]
pub fn CodeRunner(
    source: String,
    language: String,
    overrides: Option<ResourceLimits>,
    instance_id: usize,
    #[props(default)] example_output: Option<RunnerExample>,
) -> Element {
    let execution = execution::use_execution(example_output.clone());
    let show_output = execution.show_output;
    let has_output = execution.has_output;
    let running = use_memo(move || execution.state.read().is_running());
    let stage = use_memo(move || execution.state.read().stage().to_string());
    let exit_info = use_memo(move || execution.state.read().summary().to_string());
    let error_msg = use_memo(move || execution.state.read().error().to_string());
    let editor_library = use_browser_library("codemirror", || true);
    let terminal_library = use_browser_library("xterm", move || *show_output.read());
    // 编辑器内容的唯一真源；初始化为 prop 值。
    // C4 修复：删去 source_prop_signal。旧代码在 render body 里对它做镜像 .set()
    // （if source != *source_prop_signal.read() { .set(...) }），这是项目
    // dioxus-render-purity skill 明令禁止的反模式，且会触发一次额外渲染。
    // Dioxus 0.7 的 prop 本身是响应式的——use_effect 闭包内捕获 prop 副本即可在
    // prop 变化时（如 admin 切换语言重置示例代码）自动重跑，无需中间 signal。
    let mut source_signal = use_signal(|| source.clone());

    // CodeMirror 容器 id：直接由确定性 prop 派生（不进 use_hook）。
    // instance_id 由父组件从纯函数片段解析的索引传入，SSR 与 hydration 同一 content_html
    // → 同一片段序列 → 同一索引 → 同一 id，故 hydration 时 create() 能找到 SSR 渲染的容器。
    let container_id = format!("code-runner-{instance_id}");

    // xterm.js 输出终端容器 id：同样的确定性派生逻辑，SSR 与 hydration 一致。
    let output_container_id = format!("code-runner-output-{instance_id}");

    // 编辑器是否已挂载就绪。声明在 cfg 块外，使 SSR 端也能读取：
    // SSR 与 hydration 完成前为 false → 容器内渲染骨架屏；CodeMirror 挂载后置 true
    // → 骨架屏从 DOM 移除，露出真实编辑器。
    let editor_ready = use_signal(|| false);

    // Vim 模式状态（通过 localStorage 持久化偏好，默认开启）
    #[cfg(target_arch = "wasm32")]
    let example_mode = example_output.is_some();
    let mut vim_enabled = use_signal(|| {
        #[cfg(target_arch = "wasm32")]
        {
            if !example_mode {
                if let Some(window) = web_sys::window() {
                    if let Ok(Some(storage)) = window.local_storage() {
                        if let Ok(Some(val)) = storage.get_item("yggdrasil-code-runner-vim") {
                            return val == "true";
                        }
                    }
                }
            }
        }
        true
    });

    let toggle_vim = move |_| {
        let next = !vim_enabled();
        vim_enabled.set(next);
        #[cfg(target_arch = "wasm32")]
        {
            if !example_mode {
                if let Some(window) = web_sys::window() {
                    if let Ok(Some(storage)) = window.local_storage() {
                        let _ = storage.set_item("yggdrasil-code-runner-vim", &next.to_string());
                    }
                }
            }
        }
    };

    #[cfg(target_arch = "wasm32")]
    {
        editor::use_editor(
            language.clone(),
            container_id.clone(),
            source_signal,
            vim_enabled,
            editor_ready,
            editor_library,
        );
        terminal::use_terminal(
            output_container_id.clone(),
            example_output.clone(),
            show_output,
            terminal_library,
            execution.terminal,
        );
    }
    let run_example = example_output.clone();
    let run_language = language.clone();
    let run_code = move |_| {
        if running() || !editor_ready() {
            return;
        }
        if let Some(example) = &run_example {
            execution.show_example(example);
        } else {
            execution.start(ExecRequest {
                language: run_language.clone(),
                source: source_signal(),
                overrides: overrides.clone(),
            });
        }
    };
    rsx! {
        div { class: "rounded-2xl overflow-hidden border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] my-[var(--content-gap-paper)]",
            // 顶栏：语言标签 + 运行按钮
            div { class: "flex justify-between items-center px-4 py-2.5 border-b border-[var(--color-paper-border)] bg-[var(--color-paper-theme)]",
                div { class: "flex items-center gap-3",
                    span { class: "w-2 h-2 rounded-full bg-[var(--color-paper-accent)]" }
                    span { class: "font-mono text-sm font-semibold text-[var(--color-paper-primary)]",
                        "{language}"
                    }
                    button {
                        class: format!(
                            "text-[10px] px-1.5 py-0.5 rounded border transition cursor-pointer {}",
                            if vim_enabled() {
                                "bg-[var(--color-paper-accent)]/15 text-[var(--color-paper-accent)] border-[var(--color-paper-accent)]/30 font-semibold"
                            } else {
                                "bg-transparent text-[var(--color-paper-tertiary)] border-[var(--color-paper-border)] hover:text-[var(--color-paper-primary)]"
                            },
                        ),
                        onclick: toggle_vim,
                        "Vim"
                    }
                }
                button {
                    class: format!("{BTN_PRIMARY_SM} gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"),
                    disabled: running() || !editor_ready(),
                    onclick: run_code,
                    if running() {
                        span {
                            class: "inline-block w-3.5 h-3.5 text-[var(--color-paper-theme)]",
                            dangerous_inner_html: SPINNER_SVG,
                        }
                        "{stage()}"
                    } else {
                        if example_output.is_some() { "播放输出示例" } else { "运行" }
                    }
                }
            }
            LibraryLoadError { library: editor_library }
            div {
                id: "{container_id}",
                class: "code-runner-editor font-mono text-sm relative",

                // 骨架屏：CodeMirror 尚未挂载就绪时（SSR + hydration 完成前）显示。
                // editor_ready 由挂载 effect 置 true 后，此处 if 分支消失，骨架屏从 DOM 移除。
                // 用绝对定位覆盖在（始终存在的）容器上方，不影响 CodeMirror 的 getElementById 挂载。
                if !editor_ready() {
                    div { class: "absolute inset-0 flex flex-col justify-center gap-2.5 px-4 py-4 bg-[var(--color-paper-code-block)]",
                        // 代码行占位条：递减宽度模拟代码缩进，贴合等宽字体语境。
                        SkeletonBox { class: "h-3 rounded", style: Some("width: 90%") }
                        SkeletonBox { class: "h-3 rounded", style: Some("width: 70%") }
                        SkeletonBox { class: "h-3 rounded", style: Some("width: 55%") }
                        SkeletonBox { class: "h-3 rounded", style: Some("width: 85%") }
                        SkeletonBox { class: "h-3 rounded", style: Some("width: 40%") }
                    }
                }
            }
            // 输出区：用户点运行后才显示（show_output）。
            // running 时显示骨架屏占位（等首个 chunk 到达）；有内容后 xterm 终端渲染。
            if show_output() {
                div { class: "border-t border-[var(--color-paper-border)]",
                    LibraryLoadError { library: terminal_library }
                    div { class: "flex justify-between items-center px-4 py-2 text-xs text-[var(--color-paper-tertiary)] border-b border-[var(--color-paper-border)] bg-[var(--color-paper-code-block)]",
                        span { class: "font-medium uppercase tracking-wide", if example_output.is_some() { "输出示例" } else { "输出" } }
                        div { class: "flex items-center gap-3",
                            span { "{exit_info()}" }
                            if example_output.is_some() {
                                button { r#type: "button", onclick: move |_| execution.clear(), "清空" }
                            }
                        }
                    }
                    div {
                        id: "{output_container_id}",
                        class: "px-2 py-2 bg-[var(--color-paper-code-block)] max-h-80 overflow-hidden text-xs relative min-h-24",
                        // running 且尚未收到首个 chunk 时，显示骨架屏占位。
                        if running() && !has_output() {
                            div { class: "absolute inset-0 flex flex-col justify-center gap-2.5 px-4 py-4 bg-[var(--color-paper-code-block)]",
                                SkeletonBox { class: "h-3 rounded", style: Some("width: 70%") }
                                SkeletonBox { class: "h-3 rounded", style: Some("width: 55%") }
                                SkeletonBox { class: "h-3 rounded", style: Some("width: 85%") }
                            }
                        }
                    }
                }
            }
            // 错误提示
            if !error_msg().is_empty() {
                div { class: "px-4 py-2.5 border-t border-[var(--color-paper-border)] text-xs text-red-500 dark:text-red-400 bg-red-50 dark:bg-red-900/10",
                    {error_msg()}
                }
            }
        }
    }
}
