//! CodeMirror resources belong to the component scope.
use crate::bridges::library::{library_ready, LibraryResource};
use dioxus::prelude::*;
/// 编辑器挂载与同步。返回编辑器句柄，供「还原」直接写回原文。
///
/// - `vim_mode`：Vim 开启时为当前模式（normal / insert / visual …），关闭时为 `None`；
/// - `on_run`：⌘↵ / Ctrl↵ 在任何模式下运行。
#[allow(clippy::too_many_arguments)]
pub(super) fn use_editor(
    language: String,
    container_id: String,
    source_signal: Signal<String>,
    vim_enabled: Signal<bool>,
    mut vim_mode: Signal<Option<String>>,
    on_run: Callback<()>,
    mut editor_ready: Signal<bool>,
    editor_library: LibraryResource,
    // 激活门控（P2）：视口接近或用户主动交互后为 true。库加载与实例创建都被它
    // 门控——这里是双保险的创建侧；读取它使 effect 在激活翻转时重跑。
    activated: Signal<bool>,
) -> Signal<Option<crate::bridges::codemirror::EditorHandle>> {
    use crate::bridges::codemirror;
    use crate::theme::{use_resolved_theme, ResolvedTheme};
    use wasm_bindgen::closure::Closure;

    let mut editor_handle: Signal<Option<codemirror::EditorHandle>> = use_signal(|| None);
    // 在 cfg 块顶层（渲染路径）取主题 memo，move 进下方 effect 闭包；闭包内只
    // 调用 memo（resolved()）读值——调用 memo 本身不是 hook，use_resolved_theme
    // 这个 hook 必须在组件体顶层调用，不能放进 use_effect 闭包（dx check 会报）。
    let resolved_theme = use_resolved_theme();

    // 激活后首次挂载：构造 closure + options，create 后存进 editor_handle。
    // 用 resolved() 读取主题作为初始值（同时订阅，但主题切换由下方独立 effect 处理）。
    let mount_language = language;
    let mount_container_id = container_id;
    use_effect(move || {
        if editor_handle.read().is_some() {
            return; // 防重复 init
        }
        if !activated() {
            return; // 未激活（视口未接近且无交互）：不创建实例
        }
        if !library_ready(editor_library) {
            return;
        }

        // onChange 回写到 source_signal（编辑器内容 = 唯一真源）。
        let on_change = Closure::new({
            let mut sig = source_signal;
            move |v: String| sig.set(v)
        });
        let on_ready = Closure::new(|| {});
        let on_run_shortcut = Closure::new(move || on_run.call(()));
        let on_vim_mode_change = Closure::new(move |mode: String| vim_mode.set(Some(mode)));

        let theme_name = if resolved_theme() == ResolvedTheme::Dark {
            "dark"
        } else {
            "light"
        };

        let opts = codemirror::EditorOptions::new();
        opts.set_language(&mount_language);
        opts.set_theme(theme_name);
        opts.set_vim(*vim_enabled.read());
        opts.set_value(&source_signal.read());
        opts.set_on_change(&on_change);
        opts.set_on_ready(&on_ready);
        opts.set_on_run_shortcut(&on_run_shortcut);
        opts.set_on_vim_mode_change(&on_vim_mode_change);

        if let Ok(Some(inst)) = codemirror::get_module().create(&mount_container_id, &opts) {
            let handle = codemirror::EditorHandle::new(inst, on_change, on_ready, on_run_shortcut)
                .with_vim_mode(on_vim_mode_change);
            editor_handle.set(Some(handle));
            editor_ready.set(true);
        }
    });

    // 主题切换（含 System 模式下系统偏好变化）时同步编辑器主题。
    //
    // VT 动画期间跳过:手动点击主题按钮时,__startThemeTransition 已在 VT 回调内
    // 通过 'yggdrasil:theme-change' 事件同步调了 setTheme(出现在 NEW 快照里)。
    // 但 use_effect 在 theme.set(next) 后立即触发——早于 VT 回调(异步),会直接改
    // 实时 DOM 的编辑器背景。VT 动画播的是伪元素快照,实时 DOM 改动会穿透伪元素,
    // 表现为「圆形还没展开到代码块,代码块就整体瞬切」。主题过渡期间跳过,让 VT
    // 事件负责换肤;动画结束后此 effect 会因 resolved 信号变化重跑,做一次幂等的兜底同步。
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        if crate::utils::js::is_theme_transitioning() {
            return;
        }
        if let Some(h) = editor_handle.read().as_ref() {
            h.instance()
                .set_theme(if resolved_theme() == ResolvedTheme::Dark {
                    "dark"
                } else {
                    "light"
                });
        }
    });

    // 监听 vim 模式切换并同步；关闭时清掉状态条的模式。
    use_effect(move || {
        let enabled = vim_enabled();
        if !enabled {
            vim_mode.set(None);
        }
        if let Some(h) = editor_handle.read().as_ref() {
            h.instance().set_vim(enabled);
        }
    });

    // 组件卸载时销毁 CodeMirror 实例（EditorHandle::drop → instance.destroy）。
    use_drop(move || {
        editor_handle.set(None);
        editor_ready.set(false);
    });

    editor_handle
}
