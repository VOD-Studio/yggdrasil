//! Terminal resources belong to the component scope.
use super::RunnerExample;
use crate::bridges::library::{library_ready, LibraryResource};
use crate::bridges::xterm::TerminalHandle;
use dioxus::prelude::*;
pub(super) fn use_terminal(
    output_container_id: String,
    example_output: Option<RunnerExample>,
    show_output: Signal<bool>,
    terminal_library: LibraryResource,
    mut term_handle: Signal<Option<TerminalHandle>>,
    mut terminal_failed: Signal<bool>,
    mount_retry: Signal<u32>,
) {
    use crate::bridges::xterm;
    use crate::theme::{use_resolved_theme, ResolvedTheme};
    use wasm_bindgen::closure::Closure;

    // 在 cfg 块顶层（渲染路径）取主题 memo，move 进下方 effect 闭包；闭包内只
    // 调用 memo（resolved_theme()）读值——调用 memo 本身不是 hook。
    let resolved_theme = use_resolved_theme();

    // 首次挂载：构造 onReady 闭包 + XtermOptions，create 后存进 term_handle。
    // 订阅 show_output：输出区在 show_output 变 true（用户点运行）后才渲染进 DOM，
    // 容器此前不存在；读 show_output 建立订阅，使其变 true 时重跑本 effect 完成挂载。
    let mount_container_id = output_container_id;
    let mount_example = example_output;
    use_effect(move || {
        let _ = mount_retry(); // 订阅：点重试即重跑
        if term_handle.read().is_some() {
            return; // 防重复 init
        }
        if !show_output() {
            return; // 输出区未显示，容器不在 DOM，等 show_output 变 true 再挂载
        }
        if !library_ready(terminal_library) {
            return;
        }

        let on_ready = Closure::new(|| {});
        let theme_name = if resolved_theme() == ResolvedTheme::Dark {
            "dark"
        } else {
            "light"
        };

        let opts = xterm::XtermOptions::new();
        opts.set_theme(theme_name);
        opts.set_font_size(13);
        opts.set_on_ready(&on_ready);

        match xterm::get_module().create(&mount_container_id, &opts) {
            Ok(Some(inst)) => {
                if let Some(example) = &mount_example {
                    inst.write_all(&example.stdout, &example.stderr);
                }
                let handle = xterm::TerminalHandle::new(inst, on_ready);
                term_handle.set(Some(handle));
                terminal_failed.set(false);
            }
            Ok(None) => terminal_failed.set(true),
            Err(error) => {
                web_sys::console::error_1(&error);
                terminal_failed.set(true);
            }
        }
    });

    // 主题切换时同步终端主题。
    // VT 动画期间跳过(同 CodeMirror 的 use_effect,见上方注释)。
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        if crate::utils::js::is_theme_transitioning() {
            return;
        }
        if let Some(h) = term_handle.read().as_ref() {
            h.instance()
                .set_theme(if resolved_theme() == ResolvedTheme::Dark {
                    "dark"
                } else {
                    "light"
                });
        }
    });

    // 组件卸载时销毁终端（TerminalHandle::drop → instance.destroy）。
    use_drop(move || {
        term_handle.set(None);
    });
}
