//! CodeMirror resources belong to the component scope.
use crate::bridges::codemirror::{self, EditorHandle};
use crate::bridges::library::{library_ready, LibraryResource};
use crate::theme::ResolvedTheme;
use leptos::prelude::*;

/// 编辑器句柄的本地存储（JS 句柄不是 `Send`，只能放 `StoredValue::new_local`）。
pub(super) type EditorSlot = StoredValue<Option<EditorHandle>, LocalStorage>;

/// 编辑器挂载与同步。
///
/// - `vim_mode`：Vim 开启时为当前模式（normal / insert / visual …），关闭时为 `None`；
/// - `on_run`：⌘↵ / Ctrl↵ 在任何模式下运行；
/// - `editor_command`：外壳发来的「把编辑器内容整体替换为」命令（还原）。
#[allow(clippy::too_many_arguments)]
pub(super) fn use_editor(
    language: String,
    container_id: String,
    source_signal: RwSignal<String>,
    vim_enabled: RwSignal<bool>,
    vim_mode: RwSignal<Option<String>>,
    on_run: Callback<()>,
    editor_ready: RwSignal<bool>,
    editor_library: LibraryResource,
    // 激活门控（P2）：视口接近或用户主动交互后为 true。库加载与实例创建都被它
    // 门控——这里是双保险的创建侧；读取它使 Effect 在激活翻转时重跑。
    activated: RwSignal<bool>,
    editor_failed: RwSignal<bool>,
    mount_retry: RwSignal<u32>,
    editor_command: RwSignal<Option<String>>,
    resolved_theme: Memo<ResolvedTheme>,
) {
    use wasm_bindgen::closure::Closure;

    let editor_handle: EditorSlot = StoredValue::new_local(None);

    // 激活后首次挂载：构造 closure + options，create 后存进 editor_handle。
    // 主题切换由下方独立 Effect 处理，这里只取初始值。
    Effect::new(move |_| {
        mount_retry.track(); // 订阅：点重试即重跑
        if editor_handle.with_value(|handle| handle.is_some()) {
            return; // 防重复 init
        }
        if !activated.get() {
            return; // 未激活（视口未接近且无交互）：不创建实例
        }
        if !library_ready(editor_library) {
            return;
        }

        // onChange 回写到 source_signal（编辑器内容 = 唯一真源）。
        let on_change = Closure::new(move |v: String| {
            let _ = source_signal.try_set(v);
        });
        let on_ready = Closure::new(|| {});
        let on_run_shortcut = Closure::new(move || on_run.run(()));
        let on_vim_mode_change = Closure::new(move |mode: String| {
            let _ = vim_mode.try_set(Some(mode));
        });

        let theme_name = if resolved_theme.get_untracked() == ResolvedTheme::Dark {
            "dark"
        } else {
            "light"
        };

        let opts = codemirror::EditorOptions::new();
        opts.set_language(&language);
        opts.set_theme(theme_name);
        opts.set_vim(vim_enabled.get_untracked());
        opts.set_value(&source_signal.get_untracked());
        opts.set_on_change(&on_change);
        opts.set_on_ready(&on_ready);
        opts.set_on_run_shortcut(&on_run_shortcut);
        opts.set_on_vim_mode_change(&on_vim_mode_change);

        match codemirror::get_module().create(&container_id, &opts) {
            Ok(Some(inst)) => {
                let handle = EditorHandle::new(inst, on_change, on_ready, on_run_shortcut)
                    .with_vim_mode(on_vim_mode_change);
                editor_handle.set_value(Some(handle));
                editor_ready.set(true);
                editor_failed.set(false);
            }
            Ok(None) => editor_failed.set(true),
            Err(error) => {
                web_sys::console::error_1(&error);
                editor_failed.set(true);
            }
        }
    });

    // 外壳的还原命令：整体替换编辑器内容（onChange 随之回写 source_signal）。
    Effect::new(move |_| {
        let Some(text) = editor_command.get() else {
            return;
        };
        let _ = editor_command.try_set(None);
        editor_handle.with_value(|handle| {
            if let Some(handle) = handle.as_ref() {
                handle.instance().set_value(&text);
            }
        });
    });

    // 主题切换（含 System 模式下系统偏好变化）时同步编辑器主题。
    //
    // VT 动画期间跳过：手动点击主题按钮时，视图过渡回调内已通过 'yggdrasil:theme-change'
    // 事件同步调了 setTheme（出现在 NEW 快照里）。本 Effect 在主题信号变化后立即触发——
    // 早于 VT 回调（异步），会直接改实时 DOM 的编辑器背景，表现为「圆形还没展开到代码块，
    // 代码块就整体瞬切」。主题过渡期间跳过，动画结束后本 Effect 会因 resolved 信号变化
    // 重跑，做一次幂等的兜底同步。
    Effect::new(move |_| {
        let dark = resolved_theme.get() == ResolvedTheme::Dark;
        if crate::utils::js::is_theme_transitioning() {
            return;
        }
        editor_handle.with_value(|handle| {
            if let Some(handle) = handle.as_ref() {
                handle
                    .instance()
                    .set_theme(if dark { "dark" } else { "light" });
            }
        });
    });

    // 监听 vim 开关并同步；关闭时清掉状态条的模式。
    Effect::new(move |_| {
        let enabled = vim_enabled.get();
        if !enabled {
            vim_mode.set(None);
        }
        editor_handle.with_value(|handle| {
            if let Some(handle) = handle.as_ref() {
                handle.instance().set_vim(enabled);
            }
        });
    });

    // 组件卸载时销毁 CodeMirror 实例（EditorHandle::drop → instance.destroy）。
    on_cleanup(move || {
        let _ = editor_handle.try_set_value(None);
        let _ = editor_ready.try_set(false);
    });
}
