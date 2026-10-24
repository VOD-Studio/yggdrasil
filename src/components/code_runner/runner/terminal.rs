//! Terminal resources belong to the component scope.
use super::{
    execution::{Execution, TerminalCommand},
    stream::TerminalSlot,
    RunnerExample,
};
use crate::bridges::library::{library_ready, LibraryResource};
use crate::theme::ResolvedTheme;
use leptos::prelude::*;

/// 终端挂载与同步。`terminal_ready` 在终端创建成功后置 `true`，
/// 供执行 Effect 等待（终端句柄本身不是响应式的）。
#[allow(clippy::too_many_arguments)]
pub(super) fn use_terminal(
    output_container_id: String,
    example_output: Option<RunnerExample>,
    execution: Execution,
    terminal_library: LibraryResource,
    term_handle: TerminalSlot,
    terminal_ready: RwSignal<bool>,
    terminal_failed: RwSignal<bool>,
    mount_retry: RwSignal<u32>,
    resolved_theme: Memo<ResolvedTheme>,
) {
    use crate::bridges::xterm;
    use wasm_bindgen::closure::Closure;

    let show_output = execution.show_output;

    // 首次挂载：构造 onReady 闭包 + XtermOptions，create 后存进 term_handle。
    // 订阅 show_output：输出区在 show_output 变 true（用户点运行）后才渲染进 DOM，
    // 容器此前不存在；读 show_output 建立订阅，使其变 true 时重跑本 Effect 完成挂载。
    Effect::new(move |_| {
        mount_retry.track(); // 订阅：点重试即重跑
        if term_handle.with_value(|handle| handle.is_some()) {
            return; // 防重复 init
        }
        if !show_output.get() {
            return; // 输出区未显示，容器不在 DOM，等 show_output 变 true 再挂载
        }
        if !library_ready(terminal_library) {
            return;
        }

        let on_ready = Closure::new(|| {});
        let theme_name = if resolved_theme.get_untracked() == ResolvedTheme::Dark {
            "dark"
        } else {
            "light"
        };

        let opts = xterm::XtermOptions::new();
        opts.set_theme(theme_name);
        opts.set_font_size(13);
        opts.set_on_ready(&on_ready);

        match xterm::get_module().create(&output_container_id, &opts) {
            Ok(Some(inst)) => {
                if let Some(example) = &example_output {
                    inst.write_all(&example.stdout, &example.stderr);
                }
                let handle = xterm::TerminalHandle::new(inst, on_ready);
                term_handle.set_value(Some(handle));
                terminal_ready.set(true);
                terminal_failed.set(false);
            }
            Ok(None) => terminal_failed.set(true),
            Err(error) => {
                web_sys::console::error_1(&error);
                terminal_failed.set(true);
            }
        }
    });

    // 外壳发来的命令（清空 / 写入示例输出）。终端未挂载时丢弃：示例在挂载时已按
    // `example_output` 写入，清空则无内容可清。
    Effect::new(move |_| {
        let Some(command) = execution.terminal_command.get() else {
            return;
        };
        let _ = execution.terminal_command.try_set(None);
        term_handle.with_value(|handle| {
            if let Some(handle) = handle.as_ref() {
                match &command {
                    TerminalCommand::Clear => handle.instance().clear(),
                    TerminalCommand::WriteAll { stdout, stderr } => {
                        handle.instance().write_all(stdout, stderr)
                    }
                }
            }
        });
    });

    // 主题切换时同步终端主题。
    // VT 动画期间跳过：主题过渡的视图过渡回调负责换肤，动画结束后本 Effect 因 resolved
    // 信号变化重跑，做一次幂等兜底。
    Effect::new(move |_| {
        let dark = resolved_theme.get() == ResolvedTheme::Dark;
        if crate::utils::js::is_theme_transitioning() {
            return;
        }
        term_handle.with_value(|handle| {
            if let Some(handle) = handle.as_ref() {
                handle
                    .instance()
                    .set_theme(if dark { "dark" } else { "light" });
            }
        });
    });

    // 组件卸载时销毁终端（TerminalHandle::drop → instance.destroy）。
    on_cleanup(move || {
        let _ = term_handle.try_set_value(None);
    });
}
