//! CodeRunner 组件实现：可编辑源码 + 运行按钮 + SSE 流式输出 + xterm.js 终端。
//!
//! 标题栏：语言、「可运行」、改过后的「已修改 / 还原」、「圆点 + 文字」运行状态、
//! Vim 开关、复制与运行（窄屏把 Vim、复制、还原收进「⋯」菜单）。编辑器挂载前显示
//! 服务端高亮好的代码。
//!
//! 分包边界：本文件的 [`CodeRunner`] 是外壳，留在调用方所在的包——静态代码占位、标题栏、
//! 容器、输出区与 `use_near_viewport` 激活判断；激活后的编辑器 + 终端 + 执行逻辑抽成
//! `body::code_runner_body`（`#[leptos::lazy]`），在 `<Suspense>` 内经
//! `Suspend::new(async move { code_runner_body(..).await })` 加载。外壳与分包只通过
//! `Send + Sync` 的信号通信（状态信号与命令信号，见 `execution`），JS 句柄全在分包里。
//!
//! 激活门控（P2）：挂载后并不立即加载 CodeMirror、分包或创建编辑器实例；容器接近
//! 视口（约一屏提前量），或用户点击静态代码 / 运行按钮时，才触发库加载、分包加载与
//! 实例创建（`use_near_viewport` 单向激活，滚出视口不销毁）。激活前容器渲染服务端
//! 高亮占位（`highlighted`，SSR 与 hydration 输出一致；`.code-runner-static`）；用户在
//! 编辑器就绪前点击「运行」时保留意图（pending_run），就绪后自动执行一次。
//! xterm 输出终端保持「输出出现后才加载」的既有门控，激活绝不触发代码执行。
//!
//! 输出渲染：分包按 `output_container_id` 调用 `xterm::get_module().create(...)`
//! 挂载 xterm.js 终端（输出专用，无 stdin），SSE stdout/stderr 事件实时写入。
//! SSE 不可用时降级到轮询 get_exec_result，整段写入终端（writeAll）。

mod body;
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
use leptos::prelude::*;

use body::{code_runner_body, RunnerBody};
use execution::Execution;

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
/// - `overrides`：可选资源限制覆盖（可传 `ResourceLimits`、`Option<ResourceLimits>` 或省略）。
/// - `instance_id`：实例在父级片段序列中的索引，用作 CodeMirror 容器 id 后缀。
///   必须是 **SSR/hydration 确定性**的值（如父组件遍历片段的索引）——容器 id 在 SSR 与
///   水合两端必须一致，`create()` 才能在 hydration 后找到 SSR 渲染的容器元素。
/// - `highlighted`：服务端高亮好的代码 HTML，编辑器挂载前显示；缺省时显示纯文本源码
///   （可传 `String`、`Option<String>` 或省略）。
/// - `example_output`：图鉴播放作者保存的输出，不实际运行（可传 `RunnerExample`、
///   `Option<RunnerExample>` 或省略）。
#[component]
pub fn CodeRunner(
    #[prop(into)] source: String,
    #[prop(into)] language: String,
    #[prop(optional, into)] overrides: MaybeProp<ResourceLimits>,
    instance_id: usize,
    #[prop(optional, into)] highlighted: MaybeProp<String>,
    #[prop(optional, into)] example_output: MaybeProp<RunnerExample>,
) -> impl IntoView {
    let overrides = overrides.get_untracked();
    let highlighted = highlighted.get_untracked();
    let example_output = example_output.get_untracked();

    let execution = Execution::new(example_output.as_ref());
    let show_output = execution.show_output;
    let has_output = execution.has_output;
    let state = execution.state;
    let running = Memo::new(move |_| state.with(|state| state.is_running()));
    let terminal_library = use_browser_library("xterm", move || show_output.get());
    // 编辑器内容的唯一真源；初始化为 prop 值。
    let source_signal = RwSignal::new(source.clone());
    let dirty = {
        let original = source.clone();
        Memo::new(move |_| source_signal.with(|current| *current != original))
    };
    // 最近一次运行用的是不是改过的代码，决定输出区标题。
    let ran_modified = RwSignal::new(false);
    // 复制结果：Some(true) 已复制，Some(false) 失败（如非安全上下文），None 默认。
    let copied = RwSignal::new(None::<bool>);

    // CodeMirror 与 xterm 容器 id：由确定性 prop 派生，SSR 与 hydration 一致，
    // hydration 时 create() 能找到 SSR 渲染的容器。
    let container_id = format!("code-runner-{instance_id}");
    let output_container_id = format!("code-runner-output-{instance_id}");

    // 激活门控：容器接近视口（底部提前约一屏）或用户主动点击占位代码 / 运行按钮
    // 时置 true。单向激活，滚出视口不销毁已建实例；IntersectionObserver 不可用时
    // near 保持 false，由上述交互入口兜底激活。SSR 端恒为 false：占位与 hydration 首帧
    // 结构一致，可见性只在 hydration 后的 Effect 里生效。
    let near_viewport = crate::hooks::use_near_viewport(container_id.clone(), "0px 0px 100% 0px");
    let activated = RwSignal::new(false);
    Effect::new(move |_| {
        if near_viewport.get() {
            activated.set(true);
        }
    });

    // CodeMirror 库只在激活后加载；多实例共享 JS 侧 pending-map 去重，仍只发一次脚本请求。
    let editor_library = use_browser_library("codemirror", move || activated.get());

    // 用户在编辑器就绪前的运行意图；编辑器就绪后由下方 pending Effect 自动执行一次。
    let pending_run = RwSignal::new(false);

    // 编辑器是否已挂载就绪：SSR 与 hydration 完成前为 false，显示高亮占位。
    let editor_ready = RwSignal::new(false);
    // 编辑器/终端实例创建失败（库已加载但 create 报错或返回空）：提示并允许重试。
    let editor_failed = RwSignal::new(false);
    let terminal_failed = RwSignal::new(false);
    // 点「重试」自增，分包里的两个挂载 Effect 订阅它而重跑；已有句柄的那个早退。
    let mount_retry = RwSignal::new(0_u32);
    let vim_mode: RwSignal<Option<String>> = RwSignal::new(None);
    // 「还原」命令：分包里的编辑器 Effect 取走并整体替换编辑器内容。
    let editor_command: RwSignal<Option<String>> = RwSignal::new(None);

    // Vim 偏好存在本机，默认开启；没有偏好时，触屏设备默认关闭。
    // 首屏与 hydration 都按默认开启渲染，挂载后再读本机偏好，避免两端结构不一致。
    let vim_enabled = RwSignal::new(true);
    #[cfg(target_arch = "wasm32")]
    let example_mode = example_output.is_some();
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
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
            vim_enabled.set(enabled);
        }
    });

    let toggle_vim = Callback::new(move |()| {
        let next = !vim_enabled.get_untracked();
        vim_enabled.set(next);
        #[cfg(target_arch = "wasm32")]
        if !example_mode {
            if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten())
            {
                let _ = storage.set_item(VIM_STORAGE_KEY, &next.to_string());
            }
        }
    });

    let run_example = example_output.clone();
    let run_language = language.clone();
    let run = Callback::new(move |()| {
        if running.get_untracked() {
            return;
        }
        if !editor_ready.get_untracked() {
            // 用户操作早于编辑器就绪（未激活或库/分包加载中）：保留意图并立即触发
            // 激活，就绪后由 pending Effect 自动执行一次，不静默丢弃点击、不
            // 执行空代码或重复执行。
            pending_run.set(true);
            activated.set(true);
            return;
        }
        if let Some(example) = &run_example {
            execution.show_example(example);
        } else {
            ran_modified.set(dirty.get_untracked());
            execution.start(ExecRequest {
                language: run_language.clone(),
                source: source_signal.get_untracked(),
                overrides: overrides.clone(),
            });
        }
    });

    // 编辑器就绪后兑现激活前的运行意图：先清标志再启动，保证只执行一次。
    // 读取 running 建立订阅并避免在已有执行未结束时重复启动。
    Effect::new(move |_| {
        if editor_ready.get() && pending_run.get() && !running.get() {
            pending_run.set(false);
            run.run(());
        }
    });

    // 意图已保留：编辑器激活/加载期间按钮显示加载态；库加载失败时不伪装加载，
    // 由 LibraryLoadError 重试恢复。
    let awaiting_editor = move || {
        let library_failed = matches!(editor_library.read_state(), Some(Err(_)));
        pending_run.get() && !editor_ready.get() && !library_failed && !editor_failed.get()
    };

    let revert_source = source.clone();
    let revert = Callback::new(move |()| {
        editor_command.set(Some(revert_source.clone()));
        source_signal.set(revert_source.clone());
    });
    let copy = Callback::new(move |()| {
        #[cfg(target_arch = "wasm32")]
        {
            let text = source_signal.get_untracked();
            leptos::task::spawn_local(async move {
                let ok = crate::utils::js::copy_text(&text).await;
                let _ = copied.try_set(Some(ok));
                crate::utils::time::sleep_ms(2000).await;
                let _ = copied.try_set(None);
            });
        }
    });

    let example_mode_label = example_output.is_some();
    let copy_label = move || match copied.get() {
        Some(true) => "已复制",
        Some(false) => "复制失败",
        None => "复制代码",
    };
    let run_label = move || {
        if example_mode_label {
            "播放示例"
        } else if state.with(|state| state.is_finished()) {
            "再次运行"
        } else {
            "运行"
        }
    };
    let output_label = move || {
        if example_mode_label {
            "输出示例"
        } else if ran_modified.get() {
            "输出 · 修改后的代码"
        } else {
            "输出"
        }
    };
    let menu_items = Signal::derive(move || {
        let mut items = vec![MenuItem::new("copy", copy_label()).icon(IconName::Copy)];
        if dirty.get() {
            items.push(MenuItem::new("revert", "还原").icon(IconName::RotateCcw));
        }
        items.push(
            MenuItem::new(
                "vim",
                if vim_enabled.get() {
                    "关闭 Vim 键位"
                } else {
                    "开启 Vim 键位"
                },
            )
            .icon(IconName::Keyboard)
            .separated(),
        );
        items
    });
    let placeholder_html = highlighted.unwrap_or_else(|| crate::utils::html::escape_html(&source));
    // 编辑器把末尾换行显示成一个空行，占位按同样的行数撑高。
    let line_count = source.split('\n').count().max(1);
    let gutter_digits = line_count.to_string().len();
    let gutter_text: String = (1..=line_count).map(|n| format!("{n}\n")).collect();
    let placeholder_style =
        format!("--runner-lines: {line_count}; --runner-digits: {gutter_digits};");
    let language_label = language.clone();

    let body_args = RunnerBody {
        language,
        container_id: container_id.clone(),
        output_container_id: output_container_id.clone(),
        example_output: example_output.clone(),
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
        resolved_theme: crate::theme::use_resolved_theme(),
    };

    view! {
        <figure class="runner" data-tone=move || state.with(|state| state.tone().as_str())>
            <div class="runner-head">
                <div class="runner-head-start">
                    <span class="runner-lang">{language_label}</span>
                    <span class="runner-badge">"可运行"</span>
                    {move || {
                        dirty
                            .get()
                            .then(|| {
                                view! {
                                    <span class="runner-dirty">"已修改"</span>
                                    <button
                                        class="runner-revert runner-wide"
                                        type="button"
                                        on:click=move |_| revert.run(())
                                    >
                                        "还原"
                                    </button>
                                }
                            })
                    }}
                </div>
                <div class="runner-head-end">
                    <span
                        class=move || {
                            if running.get() { "runner-status is-live" } else { "runner-status" }
                        }
                        role="status"
                        aria-live="polite"
                    >
                        {move || state.with(|state| state.label())}
                    </span>
                    <button
                        class="runner-vim runner-wide"
                        type="button"
                        aria-pressed=move || vim_enabled.get().to_string()
                        title="Vim 键位，偏好保存在本机"
                        on:click=move |_| toggle_vim.run(())
                    >
                        "Vim"
                    </button>
                    <span class="runner-wide">
                        <Tooltip tip=Signal::derive(move || copy_label().to_string())>
                            <button
                                class="btn btn-sm btn-icon btn-ghost"
                                type="button"
                                aria-label=copy_label
                                on:click=move |_| copy.run(())
                            >
                                {move || {
                                    view! {
                                        <Icon
                                            name=if copied.get() == Some(true) {
                                                IconName::Check
                                            } else {
                                                IconName::Copy
                                            }
                                            size=15
                                        />
                                    }
                                }}
                            </button>
                        </Tooltip>
                    </span>
                    <span class="runner-narrow">
                        <Menu
                            label="更多操作"
                            items=menu_items
                            on_select=Callback::new(move |id: &'static str| match id {
                                "copy" => copy.run(()),
                                "revert" => revert.run(()),
                                "vim" => toggle_vim.run(()),
                                _ => {}
                            })
                        />
                    </span>
                    <Button
                        variant=ButtonVariant::Primary
                        size=ButtonSize::Sm
                        icon=IconName::Play
                        loading=Signal::derive(move || running.get() || awaiting_editor())
                        onclick=Callback::new(move |_| run.run(()))
                    >
                        {run_label}
                    </Button>
                </div>
            </div>
            <LibraryLoadError library=editor_library/>
            {move || {
                editor_failed
                    .get()
                    .then(|| {
                        view! {
                            <div
                                role="alert"
                                class="flex items-center gap-3 p-3 text-sm text-red-500 dark:text-red-400"
                            >
                                <span>"编辑器初始化失败。"</span>
                                <button
                                    type="button"
                                    class="underline underline-offset-4"
                                    on:click=move |_| {
                                        editor_failed.set(false);
                                        mount_retry.update(|n| *n += 1);
                                    }
                                >
                                    "重试"
                                </button>
                            </div>
                        }
                    })
            }}
            <div id=container_id class="runner-editor">
                // 挂载前显示服务端高亮好的代码，行号与编辑器对齐，挂载时不跳动。
                // 用绝对定位覆盖在（始终存在的）容器上方，不影响 CodeMirror 的 getElementById 挂载。
                // 点击占位提前激活编辑器，也是 IntersectionObserver 不可用时的交互兜底。
                {move || {
                    (!editor_ready.get())
                        .then(|| {
                            view! {
                                <div
                                    class="runner-placeholder code-runner-static"
                                    aria-hidden="true"
                                    title="点击启用编辑"
                                    on:click=move |_| activated.set(true)
                                    style=placeholder_style.clone()
                                >
                                    <pre class="runner-placeholder-gutter">{gutter_text.clone()}</pre>
                                    <pre class="runner-placeholder-code">
                                        <code inner_html=placeholder_html.clone()></code>
                                    </pre>
                                </div>
                            }
                        })
                }}
            </div>
            // 挂载前按 NORMAL 占位，挂载后跟随编辑器的模式，状态条出现时不跳动。
            {move || {
                vim_enabled
                    .get()
                    .then(|| {
                        let mode = if editor_ready.get() {
                            vim_mode.get().unwrap_or_default()
                        } else {
                            String::new()
                        };
                        view! { <VimStatus mode/> }
                    })
            }}
            // 输出区：第一次运行时才加载终端，加载期间两行占位。
            {move || {
                let output_container_id = output_container_id.clone();
                show_output
                    .get()
                    .then(|| {
                        view! {
                            <div class="runner-output">
                                <LibraryLoadError library=terminal_library/>
                                {move || {
                                    terminal_failed
                                        .get()
                                        .then(|| {
                                            view! {
                                                <div
                                                    role="alert"
                                                    class="flex items-center gap-3 p-3 text-sm text-red-500 dark:text-red-400"
                                                >
                                                    <span>"终端初始化失败。"</span>
                                                    <button
                                                        type="button"
                                                        class="underline underline-offset-4"
                                                        on:click=move |_| {
                                                            terminal_failed.set(false);
                                                            mount_retry.update(|n| *n += 1);
                                                        }
                                                    >
                                                        "重试"
                                                    </button>
                                                </div>
                                            }
                                        })
                                }}
                                <div class="runner-output-head">
                                    <span>{output_label}</span>
                                    <div class="runner-output-meta">
                                        <span class="tabular-nums">
                                            {move || state.with(|state| state.meta())}
                                        </span>
                                        {example_mode_label
                                            .then(|| {
                                                view! {
                                                    <button
                                                        type="button"
                                                        on:click=move |_| execution.clear()
                                                    >
                                                        "清空"
                                                    </button>
                                                }
                                            })}
                                    </div>
                                </div>
                                <div id=output_container_id class="runner-terminal">
                                    {move || {
                                        (running.get() && !has_output.get())
                                            .then(|| {
                                                view! {
                                                    <div class="runner-terminal-skeleton">
                                                        <SkeletonBox class="h-2.5" style="width: 40%"/>
                                                        <SkeletonBox class="h-2.5" style="width: 64%"/>
                                                    </div>
                                                }
                                            })
                                    }}
                                </div>
                                {move || {
                                    state
                                        .with(|state| state.note().map(|note| (note, state.retryable())))
                                        .map(|(note, retryable)| {
                                            view! {
                                                <div class="runner-note">
                                                    <span>{note}</span>
                                                    {retryable
                                                        .then(|| {
                                                            view! {
                                                                <button
                                                                    class="runner-retry"
                                                                    type="button"
                                                                    on:click=move |_| run.run(())
                                                                >
                                                                    "重试"
                                                                </button>
                                                            }
                                                        })}
                                                </div>
                                            }
                                        })
                                }}
                            </div>
                        }
                    })
            }}
            // 激活后才加载分包：挂载编辑器与终端、接管执行。分包不渲染 DOM。
            <Suspense fallback=|| ()>
                {move || {
                    let args = body_args.clone();
                    activated
                        .get()
                        .then(|| Suspend::new(async move { code_runner_body(args).await }))
                }}
            </Suspense>
        </figure>
    }
}

/// Vim 状态条：当前模式与该模式下最常用的键。
#[component]
fn VimStatus(mode: String) -> impl IntoView {
    let (label, hint) = match mode.as_str() {
        "insert" => ("INSERT", "Esc 回到普通模式 · ⌘↵ 运行"),
        "visual" => ("VISUAL", "y 复制选区 · Esc 取消"),
        "replace" => ("REPLACE", "Esc 回到普通模式"),
        _ => ("NORMAL", "i 插入 · v 选择 · ⌘↵ 运行"),
    };
    view! {
        <div class="runner-vim-status" data-mode=label>
            <span class="runner-vim-mode">{format!("-- {label} --")}</span>
            <span class="runner-vim-hint">{hint}</span>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use leptos::reactive::owner::Owner;

    fn render(view: impl FnOnce() -> AnyView) -> String {
        Owner::new().with(|| {
            crate::theme::provide_theme();
            view().to_html()
        })
    }

    #[test]
    fn inactive_runner_renders_static_code_without_output_or_editor() {
        let html = render(|| {
            view! {
                <CodeRunner
                    source="print('hi')\nprint(2)"
                    language="python"
                    instance_id=1usize
                    highlighted="<span>hi</span>".to_string()
                />
            }
            .into_any()
        });
        // 激活前：静态高亮占位 + 确定性容器 id；没有输出区。
        assert!(html.contains("code-runner-static"), "{html}");
        assert!(html.contains(r#"id="code-runner-1""#), "{html}");
        assert!(html.contains("<span>hi</span>"), "{html}");
        assert!(html.contains("可运行"), "{html}");
        assert!(html.contains("NORMAL"), "{html}");
        assert!(!html.contains("runner-output"), "{html}");
        assert!(!html.contains("cm-editor"), "{html}");
    }

    #[test]
    fn example_runner_shows_saved_output_without_running() {
        let html = render(|| {
            view! {
                <CodeRunner
                    source="print(1)"
                    language="python"
                    instance_id=2usize
                    example_output=RunnerExample {
                        stdout: "1\n".into(),
                        stderr: String::new(),
                        status: "退出码 0".into(),
                        error: String::new(),
                    }
                />
            }
            .into_any()
        });
        assert!(html.contains("播放示例"), "{html}");
        assert!(html.contains("输出示例"), "{html}");
        assert!(html.contains(r#"id="code-runner-output-2""#), "{html}");
        assert!(html.contains("清空"), "{html}");
    }

    #[test]
    fn placeholder_gutter_counts_lines() {
        let html = render(|| {
            view! { <CodeRunner source="a\nb\nc" language="python" instance_id=3usize/> }.into_any()
        });
        assert!(
            html.contains("--runner-lines: 3; --runner-digits: 1;"),
            "{html}"
        );
    }
}
