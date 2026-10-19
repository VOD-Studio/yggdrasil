//! CodeRunner 组件实现：可编辑源码 + 运行按钮 + SSE 流式输出 + xterm.js 终端。
//!
//! 标题栏：语言、「可运行」、改过后的「已修改 / 还原」、「圆点 + 文字」运行状态、
//! Vim 开关、复制与运行（窄屏把 Vim、复制、还原收进「⋯」菜单）。编辑器挂载前显示
//! 服务端高亮好的代码，挂载完成前运行按钮不可用。
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
use crate::components::ui::{
    Button, ButtonSize, ButtonVariant, Icon, IconName, Menu, MenuItem, Tooltip,
};
use crate::infra::runner_config::ResourceLimits;
use dioxus::prelude::*;

/// Vim 偏好在 localStorage 中的键，所有代码块共用。
#[cfg(target_arch = "wasm32")]
const VIM_STORAGE_KEY: &str = "yggdrasil-code-runner-vim";

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
/// - `source`：初始源码（首次挂载用于初始化编辑器；之后编辑器内容是唯一真源，
///   与它不同即为「已修改」，「还原」写回它）。
/// - `language`：语言标识（python / node 等）。
/// - `overrides`：可选资源限制覆盖。
/// - `instance_id`：实例在父级片段序列中的索引，用作 CodeMirror 容器 id 后缀。
///   必须是 **SSR/hydration 确定性**的值（如父组件 `for (i, ..)` 的索引 `i`）——
///   Dioxus hydration 不传递 use_hook 状态，任何在 use_hook 内基于运行时状态
///   （时间戳 / 随机 / ScopeId）生成的 id，在 SSR 与 hydration 两端会不一致，
///   导致 CodeMirror `create()` 在 hydration 时找不到 SSR 渲染的容器元素。
/// - `highlighted`：服务端高亮好的代码 HTML，编辑器挂载前显示；缺省时显示纯文本源码。
/// - `example_output`：图鉴播放作者保存的输出，不实际运行。
#[component]
#[allow(unused_mut)]
pub fn CodeRunner(
    source: String,
    language: String,
    overrides: Option<ResourceLimits>,
    instance_id: usize,
    #[props(default)] highlighted: Option<String>,
    #[props(default)] example_output: Option<RunnerExample>,
) -> Element {
    let execution = execution::use_execution(example_output.clone());
    let show_output = execution.show_output;
    let has_output = execution.has_output;
    let state = execution.state;
    let running = use_memo(move || state.read().is_running());
    let editor_library = use_browser_library("codemirror", || true);
    let terminal_library = use_browser_library("xterm", move || *show_output.read());
    // 编辑器内容的唯一真源；初始化为 prop 值。
    let mut source_signal = use_signal(|| source.clone());
    let original = source.clone();
    let dirty = use_memo(move || *source_signal.read() != original);
    // 最近一次运行用的是不是改过的代码，决定输出区标题。
    let mut ran_modified = use_signal(|| false);
    let mut copied = use_signal(|| false);

    // CodeMirror 与 xterm 容器 id：由确定性 prop 派生（不进 use_hook），
    // SSR 与 hydration 一致，hydration 时 create() 能找到 SSR 渲染的容器。
    let container_id = format!("code-runner-{instance_id}");
    let output_container_id = format!("code-runner-output-{instance_id}");

    // 编辑器是否已挂载就绪：SSR 与 hydration 完成前为 false，显示高亮占位。
    let editor_ready = use_signal(|| false);
    let vim_mode: Signal<Option<String>> = use_signal(|| None);

    // Vim 偏好存在本机，默认开启；没有偏好时，触屏设备默认关闭。
    // 首屏与 hydration 都按默认开启渲染，挂载后再读本机偏好，避免两端结构不一致。
    #[cfg(target_arch = "wasm32")]
    let example_mode = example_output.is_some();
    let mut vim_enabled = use_signal(|| true);
    #[cfg(target_arch = "wasm32")]
    use_hook(move || {
        let preferred = web_sys::window().and_then(|window| {
            let stored = (!example_mode)
                .then(|| window.local_storage().ok().flatten())
                .flatten()
                .and_then(|storage| storage.get_item(VIM_STORAGE_KEY).ok().flatten());
            match stored {
                Some(val) => Some(val == "true"),
                None => window
                    .match_media("(pointer: coarse)")
                    .ok()
                    .flatten()
                    .map(|query| !query.matches()),
            }
        });
        if let Some(enabled) = preferred.filter(|enabled| !enabled) {
            spawn(async move { vim_enabled.set(enabled) });
        }
    });

    let mut toggle_vim = move || {
        let next = !vim_enabled();
        vim_enabled.set(next);
        #[cfg(target_arch = "wasm32")]
        {
            if !example_mode {
                if let Some(window) = web_sys::window() {
                    if let Ok(Some(storage)) = window.local_storage() {
                        let _ = storage.set_item(VIM_STORAGE_KEY, &next.to_string());
                    }
                }
            }
        }
    };

    let run_example = example_output.clone();
    let run_language = language.clone();
    let run = use_callback(move |_: ()| {
        if running() || !editor_ready() {
            return;
        }
        if let Some(example) = &run_example {
            execution.show_example(example);
        } else {
            ran_modified.set(dirty());
            execution.start(ExecRequest {
                language: run_language.clone(),
                source: source_signal(),
                overrides: overrides.clone(),
            });
        }
    });

    #[cfg(target_arch = "wasm32")]
    let editor_handle = editor::use_editor(
        language.clone(),
        container_id.clone(),
        source_signal,
        vim_enabled,
        vim_mode,
        run,
        editor_ready,
        editor_library,
    );
    #[cfg(target_arch = "wasm32")]
    terminal::use_terminal(
        output_container_id.clone(),
        example_output.clone(),
        show_output,
        terminal_library,
        execution.terminal,
    );

    let revert_source = source.clone();
    let revert = use_callback(move |_: ()| {
        #[cfg(target_arch = "wasm32")]
        if let Some(handle) = editor_handle.read().as_ref() {
            handle.instance().set_value(&revert_source);
        }
        source_signal.set(revert_source.clone());
    });
    let mut copy = move || {
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = web_sys::window() {
            let _ = window
                .navigator()
                .clipboard()
                .write_text(&source_signal.peek());
        }
        copied.set(true);
        spawn(async move {
            crate::utils::time::sleep_ms(2000).await;
            copied.set(false);
        });
    };

    let run_state = state.read().clone();
    let tone = run_state.tone().as_str();
    let status_class = if run_state.is_running() {
        "runner-status is-live"
    } else {
        "runner-status"
    };
    let run_label = if example_output.is_some() {
        "播放示例"
    } else if run_state.is_finished() {
        "再次运行"
    } else {
        "运行"
    };
    let output_label = if example_output.is_some() {
        "输出示例"
    } else if ran_modified() {
        "输出 · 修改后的代码"
    } else {
        "输出"
    };
    let note = run_state.note();
    let retryable = run_state.retryable();
    let mut menu_items = vec![MenuItem::new(
        "copy",
        if copied() {
            "已复制"
        } else {
            "复制代码"
        },
    )
    .icon(IconName::Copy)];
    if dirty() {
        menu_items.push(MenuItem::new("revert", "还原").icon(IconName::RotateCcw));
    }
    menu_items.push(
        MenuItem::new(
            "vim",
            if vim_enabled() {
                "关闭 Vim 键位"
            } else {
                "开启 Vim 键位"
            },
        )
        .icon(IconName::Keyboard)
        .separated(),
    );
    let placeholder_html = highlighted.unwrap_or_else(|| crate::utils::html::escape_html(&source));
    // 编辑器把末尾换行显示成一个空行，占位按同样的行数撑高。
    let line_count = source.split('\n').count().max(1);
    let gutter_digits = line_count.to_string().len();

    rsx! {
        figure { class: "runner", "data-tone": tone,
            div { class: "runner-head",
                div { class: "runner-head-start",
                    span { class: "runner-lang", "{language}" }
                    span { class: "runner-badge", "可运行" }
                    if dirty() {
                        span { class: "runner-dirty", "已修改" }
                        button { class: "runner-revert runner-wide", r#type: "button", onclick: move |_| revert.call(()), "还原" }
                    }
                }
                div { class: "runner-head-end",
                    span { class: status_class, role: "status", aria_live: "polite", "{run_state.label()}" }
                    button {
                        class: "runner-vim runner-wide",
                        r#type: "button",
                        aria_pressed: "{vim_enabled}",
                        title: "Vim 键位，偏好保存在本机",
                        onclick: move |_| toggle_vim(),
                        "Vim"
                    }
                    span { class: "runner-wide",
                        Tooltip { tip: if copied() { "已复制" } else { "复制代码" },
                            button {
                                class: "btn btn-sm btn-icon btn-ghost",
                                r#type: "button",
                                aria_label: if copied() { "已复制" } else { "复制代码" },
                                onclick: move |_| copy(),
                                Icon { name: if copied() { IconName::Check } else { IconName::Copy }, size: 15 }
                            }
                        }
                    }
                    span { class: "runner-narrow",
                        Menu {
                            label: "更多操作",
                            items: menu_items,
                            on_select: move |id: &'static str| match id {
                                "copy" => copy(),
                                "revert" => revert.call(()),
                                "vim" => toggle_vim(),
                                _ => {}
                            },
                        }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        size: ButtonSize::Sm,
                        icon: IconName::Play,
                        loading: running(),
                        disabled: !editor_ready(),
                        onclick: move |_| run.call(()),
                        "{run_label}"
                    }
                }
            }
            LibraryLoadError { library: editor_library }
            div {
                id: "{container_id}",
                class: "runner-editor",
                // 挂载前显示服务端高亮好的代码，行号与编辑器对齐，挂载时不跳动。
                // 用绝对定位覆盖在（始终存在的）容器上方，不影响 CodeMirror 的 getElementById 挂载。
                if !editor_ready() {
                    div {
                        class: "runner-placeholder",
                        aria_hidden: "true",
                        style: "--runner-lines: {line_count}; --runner-digits: {gutter_digits};",
                        pre { class: "runner-placeholder-gutter",
                            for n in 1..=line_count { "{n}\n" }
                        }
                        pre { class: "runner-placeholder-code",
                            code { dangerous_inner_html: "{placeholder_html}" }
                        }
                    }
                }
            }
            // 挂载前按 NORMAL 占位，挂载后跟随编辑器的模式，状态条出现时不跳动。
            if vim_enabled() {
                VimStatus { mode: if editor_ready() { vim_mode().unwrap_or_default() } else { String::new() } }
            }
            // 输出区：第一次运行时才加载终端，加载期间两行占位。
            if show_output() {
                div { class: "runner-output",
                    LibraryLoadError { library: terminal_library }
                    div { class: "runner-output-head",
                        span { "{output_label}" }
                        div { class: "runner-output-meta",
                            span { class: "tabular-nums", "{run_state.meta()}" }
                            if example_output.is_some() {
                                button { r#type: "button", onclick: move |_| execution.clear(), "清空" }
                            }
                        }
                    }
                    div { id: "{output_container_id}", class: "runner-terminal",
                        if running() && !has_output() {
                            div { class: "runner-terminal-skeleton",
                                SkeletonBox { class: "h-2.5", style: Some("width: 40%") }
                                SkeletonBox { class: "h-2.5", style: Some("width: 64%") }
                            }
                        }
                    }
                    if let Some(note) = note {
                        div { class: "runner-note",
                            span { "{note}" }
                            if retryable {
                                button { class: "runner-retry", r#type: "button", onclick: move |_| run.call(()), "重试" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Vim 状态条：当前模式与该模式下最常用的键。
#[component]
fn VimStatus(mode: String) -> Element {
    let (label, hint) = match mode.as_str() {
        "insert" => ("INSERT", "Esc 回到普通模式 · ⌘↵ 运行"),
        "visual" => ("VISUAL", "y 复制选区 · Esc 取消"),
        "replace" => ("REPLACE", "Esc 回到普通模式"),
        _ => ("NORMAL", "i 插入 · v 选择 · ⌘↵ 运行"),
    };
    rsx! {
        div { class: "runner-vim-status", "data-mode": "{label}",
            span { class: "runner-vim-mode", "-- {label} --" }
            span { class: "runner-vim-hint", "{hint}" }
        }
    }
}
