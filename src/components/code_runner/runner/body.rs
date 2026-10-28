//! 激活后才加载的分包：CodeMirror 编辑器、xterm 终端与执行（SSE / 轮询）逻辑。
//!
//! 外壳（[`super::CodeRunner`]，留在调用方所在的包）渲染静态代码占位、标题栏、容器与
//! 输出区，并用 `use_near_viewport` 决定是否激活；激活（接近视口，或用户点击占位 /
//! 「运行」）后才在 `<Suspense>` 里加载本分包。分包函数不渲染任何 DOM：它只在外壳已经
//! 渲染好的容器上挂载编辑器与终端，并消费外壳经信号发来的命令（见 `execution`）。
//! 所有 JS 句柄（编辑器、终端、SSE 流）都由分包持有，随分包所有者清理而销毁。

use super::{execution::Execution, RunnerExample};
use crate::bridges::library::LibraryResource;
use crate::theme::ResolvedTheme;
use leptos::prelude::*;

/// 外壳传给分包的全部参数（信号与只读配置，均不含 JS 句柄）。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone)]
pub(super) struct RunnerBody {
    pub language: String,
    pub container_id: String,
    pub output_container_id: String,
    pub example_output: Option<RunnerExample>,
    pub source_signal: RwSignal<String>,
    pub vim_enabled: RwSignal<bool>,
    pub vim_mode: RwSignal<Option<String>>,
    pub run: Callback<()>,
    pub activated: RwSignal<bool>,
    pub editor_ready: RwSignal<bool>,
    pub editor_failed: RwSignal<bool>,
    pub terminal_failed: RwSignal<bool>,
    pub mount_retry: RwSignal<u32>,
    pub editor_command: RwSignal<Option<String>>,
    pub editor_library: LibraryResource,
    pub terminal_library: LibraryResource,
    pub execution: Execution,
    pub resolved_theme: Memo<ResolvedTheme>,
}

/// 分包入口：在已渲染的容器上挂载编辑器与终端并接管执行；自身不产生 DOM。
///
/// 首次调用时加载分包；服务端与未激活时从不调用。
#[leptos::lazy]
pub(super) fn code_runner_body(args: RunnerBody) -> AnyView {
    #[cfg(target_arch = "wasm32")]
    mount(args);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = args;
    ().into_any()
}

#[cfg(target_arch = "wasm32")]
fn mount(args: RunnerBody) {
    use super::state::RunState;
    use super::stream::{self, StreamHandle, TerminalSlot};
    use super::{editor, terminal};

    let RunnerBody {
        language,
        container_id,
        output_container_id,
        example_output,
        source_signal,
        vim_enabled,
        vim_mode,
        run,
        activated,
        editor_ready,
        editor_failed,
        terminal_failed,
        mount_retry,
        editor_command,
        editor_library,
        terminal_library,
        execution,
        resolved_theme,
    } = args;

    editor::use_editor(
        language,
        container_id,
        source_signal,
        vim_enabled,
        vim_mode,
        run,
        editor_ready,
        editor_library,
        activated,
        editor_failed,
        mount_retry,
        editor_command,
        resolved_theme,
    );

    let term_handle: TerminalSlot = StoredValue::new_local(None);
    let terminal_ready = RwSignal::new(false);
    terminal::use_terminal(
        output_container_id,
        example_output,
        execution,
        terminal_library,
        term_handle,
        terminal_ready,
        terminal_failed,
        mount_retry,
        resolved_theme,
    );

    // SSE 流句柄：运行结束或组件卸载时丢弃（Drop 里关闭连接并摘掉监听）。
    let stream_handle: StoredValue<Option<StreamHandle>, LocalStorage> =
        StoredValue::new_local(None);
    on_cleanup(move || {
        let _ = stream_handle.try_set_value(None);
    });
    Effect::new(move |_| {
        if !execution.state.with(RunState::is_running) {
            let _ = stream_handle.try_set_value(None);
        }
    });

    // 取走外壳提交的运行请求。终端挂载后才提交，保证输出一到就有地方写。
    // 提交后保持「准备中」，排队与容器启动由流里的 stage 事件推进。
    Effect::new(move |_| {
        if !terminal_ready.get() {
            return;
        }
        let Some(request) = execution.pending.get() else {
            return;
        };
        execution.pending.set(None);
        stream_handle.set_value(None);
        leptos::task::spawn_local(async move {
            match crate::api::code_runner::execute::start_exec_stream(request).await {
                Ok(id) => match stream::start(&id, execution, term_handle) {
                    // 组件已卸载时 try_set_value 把句柄交还，随即丢弃并关闭连接。
                    Ok(handle) => drop(stream_handle.try_set_value(Some(handle))),
                    Err(_) => stream::poll_result(&id, execution, term_handle).await,
                },
                Err(error) => {
                    let _ = execution.state.try_set(RunState::rejected(&error));
                }
            }
        });
    });
}
