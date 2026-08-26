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

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::code_runner::execute::start_exec_stream;
#[cfg(not(target_arch = "wasm32"))]
use crate::api::code_runner::execute::{get_exec_result, start_exec};
use crate::api::code_runner::{ExecRequest, ExecResult, ExecStatus};
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::{BTN_PRIMARY_SM, SPINNER_SVG};
use crate::infra::runner_config::ResourceLimits;
#[cfg(not(target_arch = "wasm32"))]
use crate::utils::time::sleep_ms;

/// 轮询间隔（毫秒）。仅 server 端 run_code 占位路径使用。
#[cfg(not(target_arch = "wasm32"))]
const POLL_INTERVAL_MS: u32 = 500;
/// 轮询最大次数兜底，避免任务卡在 Running 状态时无限轮询。
#[cfg(not(target_arch = "wasm32"))]
const MAX_POLLS: u32 = 240; // 500ms * 240 = 120s 上限

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
/// `mut` 信号在 WASM 与 server 两套 run_code（#[cfg] 分支）中的 .set() 调用点不同，
/// 导致任一目标构建都有一组 mut binding 被判为 unused。两端都 allow 放行。
#[component]
#[allow(unused_mut)]
pub fn CodeRunner(
    source: String,
    language: String,
    overrides: Option<ResourceLimits>,
    instance_id: usize,
) -> Element {
    let mut running = use_signal(|| false);
    let mut stage = use_signal(String::new);
    let mut exit_info = use_signal(String::new);
    let mut error_msg = use_signal(String::new);
    // 输出区可见性：点运行后置 true，控制输出区按需出现（而非页面加载就显示空区）。
    let mut show_output = use_signal(|| false);
    // 是否已收到首个输出 chunk：收到后骨架屏消失，露出终端实时渲染。
    let mut has_output = use_signal(|| false);

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

    // xterm.js 终端实例句柄（仅 WASM）：声明在 cfg block 外的组件作用域，
    // 使 run_code 的 WASM 版闭包能捕获它。server 构建整行不存在。
    #[cfg(target_arch = "wasm32")]
    let mut term_handle: Signal<Option<crate::bridges::xterm::TerminalHandle>> =
        use_signal(|| None);

    // 编辑器是否已挂载就绪。声明在 cfg 块外，使 SSR 端也能读取：
    // SSR 与 hydration 完成前为 false → 容器内渲染骨架屏；CodeMirror 挂载后置 true
    // → 骨架屏从 DOM 移除，露出真实编辑器。
    let mut editor_ready = use_signal(|| false);

    // Vim 模式状态（通过 localStorage 持久化偏好，默认开启）
    let mut vim_enabled = use_signal(|| {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(val)) = storage.get_item("yggdrasil-code-runner-vim") {
                        return val == "true";
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
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    let _ = storage.set_item("yggdrasil-code-runner-vim", &next.to_string());
                }
            }
        }
    };

    // —— CodeMirror 挂载（仅 WASM）——
    // 范式镜像 src/pages/admin/system.rs 的 SQL 控制台与 src/pages/admin/write.rs 的 Tiptap。
    #[cfg(target_arch = "wasm32")]
    {
        use crate::bridges::codemirror;
        use crate::theme::{use_resolved_theme, ResolvedTheme};
        use wasm_bindgen::closure::Closure;

        let mut editor_handle: Signal<Option<codemirror::EditorHandle>> = use_signal(|| None);
        // 在 cfg 块顶层（渲染路径）取主题 memo，move 进下方 effect 闭包；闭包内只
        // 调用 memo（resolved()）读值——调用 memo 本身不是 hook，use_resolved_theme
        // 这个 hook 必须在组件体顶层调用，不能放进 use_effect 闭包（dx check 会报）。
        let resolved_theme = use_resolved_theme();

        // 首次挂载：构造 closure + options，create 后存进 editor_handle。
        // 用 resolved() 读取主题作为初始值（同时订阅，但主题切换由下方独立 effect 处理）。
        let mount_language = language.clone();
        let mount_container_id = container_id.clone();
        use_effect(move || {
            if editor_handle.read().is_some() {
                return; // 防重复 init
            }

            // onChange 回写到 source_signal（编辑器内容 = 唯一真源）。
            let on_change = Closure::new({
                let mut sig = source_signal;
                move |v: String| sig.set(v)
            });
            let on_ready = Closure::new(|| {});
            // CodeRunner 不使用 Ctrl+Enter 运行快捷键（它有自己的运行按钮），
            // 但 EditorHandle 签名要求该闭包，传 no-op 满足生命周期。
            let on_run_shortcut = Closure::new(|| {});

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

            if let Ok(Some(inst)) = codemirror::get_module().create(&mount_container_id, &opts) {
                let handle =
                    codemirror::EditorHandle::new(inst, on_change, on_ready, on_run_shortcut);
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
        // 表现为「圆形还没展开到代码块,代码块就整体瞬切」。is-theme-transitioning
        // 期间跳过,让 VT 事件负责换肤;动画结束后此 effect 会因 resolved 信号变化重跑
        // (此时 is-theme-transitioning 已移除),做一次幂等的兜底同步。
        use_effect(move || {
            #[cfg(target_arch = "wasm32")]
            {
                let transitioning = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.document_element())
                    .map(|el| el.class_list().contains("is-theme-transitioning"))
                    .unwrap_or(false);
                if transitioning {
                    return;
                }
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

        // 监听 vim 模式切换并同步。
        use_effect(move || {
            let enabled = vim_enabled();
            if let Some(h) = editor_handle.read().as_ref() {
                h.instance().set_vim(enabled);
            }
        });

        // 组件卸载时销毁 CodeMirror 实例（EditorHandle::drop → instance.destroy）。
        use_drop(move || {
            editor_handle.set(None);
            editor_ready.set(false);
        });
    }

    // —— xterm.js 终端挂载（仅 WASM，输出专用）——
    // 范式镜像 CodeMirror 挂载：get_module().create() → TerminalHandle，use_drop 销毁。
    #[cfg(target_arch = "wasm32")]
    {
        use crate::bridges::xterm;
        use crate::theme::{use_resolved_theme, ResolvedTheme};
        use wasm_bindgen::closure::Closure;

        // 在 cfg 块顶层（渲染路径）取主题 memo，move 进下方 effect 闭包；闭包内只
        // 调用 memo（resolved_theme()）读值——调用 memo 本身不是 hook。
        let resolved_theme = use_resolved_theme();

        // 首次挂载：构造 onReady 闭包 + XtermOptions，create 后存进 term_handle。
        // 订阅 show_output：输出区在 show_output 变 true（用户点运行）后才渲染进 DOM，
        // 容器此前不存在；读 show_output 建立订阅，使其变 true 时重跑本 effect 完成挂载。
        let mount_container_id = output_container_id.clone();
        use_effect(move || {
            if term_handle.read().is_some() {
                return; // 防重复 init
            }
            if !show_output() {
                return; // 输出区未显示，容器不在 DOM，等 show_output 变 true 再挂载
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

            if let Ok(Some(inst)) = xterm::get_module().create(&mount_container_id, &opts) {
                let handle = xterm::TerminalHandle::new(inst, on_ready);
                term_handle.set(Some(handle));
            }
        });

        // 主题切换时同步终端主题。
        // VT 动画期间跳过(同 CodeMirror 的 use_effect,见上方注释)。
        use_effect(move || {
            #[cfg(target_arch = "wasm32")]
            {
                let transitioning = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.document_element())
                    .map(|el| el.class_list().contains("is-theme-transitioning"))
                    .unwrap_or(false);
                if transitioning {
                    return;
                }
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

    // —— run_code：同步段取 signal 当前值，move 进 spawn ——
    let run_language = language.clone();
    let run_overrides = overrides.clone();

    // WASM 版：start_exec_stream → EventSource SSE 实时写入 xterm.js 终端；
    // SSE 不可用时降级到轮询 get_exec_result（writeAll 整段写入）。
    #[cfg(target_arch = "wasm32")]
    let run_code = {
        let mut running = running;
        let mut stage = stage;
        let mut exit_info = exit_info;
        let mut error_msg = error_msg;
        let mut source_signal = source_signal;
        let mut term_handle = term_handle;
        let mut show_output = show_output;
        let mut has_output = has_output;
        let run_language = run_language.clone();
        let run_overrides = run_overrides.clone();
        move |_| {
            if running() {
                return;
            }
            running.set(true);
            show_output.set(true);
            has_output.set(false);
            stage.set("提交中...".to_string());
            error_msg.set(String::new());

            // 清空终端，准备新一轮输出。
            if let Some(h) = term_handle.read().as_ref() {
                h.instance().clear();
            }

            let run_source = source_signal.read().clone();
            let req = ExecRequest {
                language: run_language.clone(),
                source: run_source,
                overrides: run_overrides.clone(),
            };

            spawn(async move {
                let task_id = match start_exec_stream(req).await {
                    Ok(id) => id,
                    Err(e) => {
                        running.set(false);
                        let msg = e.to_string();
                        stage.set(msg.clone());
                        error_msg.set(msg);
                        return;
                    }
                };
                stage.set("运行中".to_string());

                // 启动 SSE：用原生 EventSource 消费流，回调写入终端与 signal。
                // 若 EventSource 创建失败，降级到轮询。
                if sse_consumer::start_sse(
                    &task_id,
                    &term_handle,
                    &mut running,
                    &mut exit_info,
                    &mut error_msg,
                    &mut has_output,
                )
                .is_err()
                {
                    // 降级轮询
                    sse_consumer::poll_result(
                        &task_id,
                        &mut running,
                        &mut stage,
                        &mut exit_info,
                        &mut error_msg,
                        &term_handle,
                        &mut has_output,
                    )
                    .await;
                }
            });
        }
    };

    // Server 版（占位，组件在前端运行）：保留轮询逻辑使双目标都能编译。
    #[cfg(not(target_arch = "wasm32"))]
    let run_code = {
        let mut running = running;
        let mut stage = stage;
        let mut exit_info = exit_info;
        let mut error_msg = error_msg;
        let mut source_signal = source_signal;
        let mut show_output = show_output;
        let run_language = run_language.clone();
        let run_overrides = run_overrides.clone();
        move |_| {
            if running() {
                return;
            }
            running.set(true);
            show_output.set(true);
            stage.set("提交中...".to_string());
            exit_info.set(String::new());
            error_msg.set(String::new());

            let run_source = source_signal.read().clone();
            let req = ExecRequest {
                language: run_language.clone(),
                source: run_source,
                overrides: run_overrides.clone(),
            };

            spawn(async move {
                match start_exec(req).await {
                    Ok(task_id) => {
                        let mut polls = 0u32;
                        loop {
                            polls += 1;
                            sleep_ms(POLL_INTERVAL_MS).await;
                            match get_exec_result(task_id.clone()).await {
                                Ok(task) => {
                                    stage.set(task.stage.clone());
                                    let terminal = task.status != ExecStatus::Queued
                                        && task.status != ExecStatus::Running;
                                    if terminal {
                                        running.set(false);
                                        if let Some(res) = task.result {
                                            apply_exec_outcome(
                                                &mut exit_info,
                                                &mut error_msg,
                                                &res,
                                            );
                                        }
                                        break;
                                    }
                                    if polls >= MAX_POLLS {
                                        running.set(false);
                                        stage.set("查询超时".to_string());
                                        error_msg.set("轮询超时，请重试".to_string());
                                        break;
                                    }
                                }
                                Err(_) => {
                                    running.set(false);
                                    stage.set("结果获取异常".to_string());
                                    error_msg.set("结果获取异常".to_string());
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        running.set(false);
                        let msg = e.to_string();
                        stage.set(msg.clone());
                        error_msg.set(msg);
                    }
                }
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
                    disabled: running(),
                    onclick: run_code,
                    if running() {
                        span {
                            class: "inline-block w-3.5 h-3.5 text-[var(--color-paper-theme)]",
                            dangerous_inner_html: SPINNER_SVG,
                        }
                        "{stage()}"
                    } else {
                        "运行"
                    }
                }
            }
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
                    div { class: "flex justify-between items-center px-4 py-2 text-xs text-[var(--color-paper-tertiary)] border-b border-[var(--color-paper-border)] bg-[var(--color-paper-code-block)]",
                        span { class: "font-medium uppercase tracking-wide", "输出" }
                        span { "{exit_info()}" }
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

/// 把 ExecStatus 映射成中文标签。
fn status_label(status: &ExecStatus) -> String {
    match status {
        ExecStatus::Queued => "排队中".to_string(),
        ExecStatus::Running => "运行中".to_string(),
        ExecStatus::Success => "成功".to_string(),
        ExecStatus::Timeout => "超时".to_string(),
        ExecStatus::OomKilled => "内存超限".to_string(),
        ExecStatus::Error => "运行错误".to_string(),
        ExecStatus::Failed => "系统失败".to_string(),
        ExecStatus::RateLimited => "请求过频".to_string(),
    }
}

/// 把执行结果（耗时/状态）写入 exit_info / error_msg：成功清空错误提示，
/// 非成功态回填状态中文标签。非 WASM 占位轮询路径与 WASM 端 SSE 降级轮询
/// （sse_consumer::poll_result）共用同一套结果展示逻辑，避免重复维护。
fn apply_exec_outcome(
    exit_info: &mut Signal<String>,
    error_msg: &mut Signal<String>,
    res: &ExecResult,
) {
    exit_info.set(format!(
        "耗时: {}ms · 状态: {}",
        res.duration_ms,
        status_label(&res.status)
    ));
    if res.status == ExecStatus::Success {
        error_msg.set(String::new());
    } else {
        error_msg.set(status_label(&res.status));
    }
}

// —— WASM-only 辅助函数：SSE 消费 + 轮询兜底 ——
// start_sse 用原生 EventSource 消费 SSE 流，回调写入 xterm.js 终端与 signal。
// poll_result 是降级路径：轮询 get_exec_result，拿到完整结果后 writeAll 写入终端。
#[cfg(target_arch = "wasm32")]
mod sse_consumer {
    use dioxus::prelude::*;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use web_sys::{EventSource, MessageEvent};

    use crate::api::code_runner::execute::get_exec_result;
    use crate::api::code_runner::ExecStatus;
    use crate::bridges::xterm::TerminalHandle;
    use crate::utils::time::sleep_ms;

    /// SSE done 事件的 JSON payload。
    #[derive(serde::Deserialize)]
    struct DonePayload {
        exit_code: Option<i64>,
        oom_killed: bool,
        timed_out: bool,
        duration_ms: u64,
        /// 系统级错误消息（镜像缺失 / daemon 不可达）。有值时优先于退出码展示。
        #[serde(default)]
        error: Option<String>,
    }

    /// 启动 EventSource 消费 SSE 流。
    ///
    /// 回调写入 xterm.js 终端（stdout/stderr）与 signal（exit_info/error_msg）。
    /// 返回后 spawn block 的 future 即结束——EventSource 自行维持连接直到 done/error。
    ///
    /// 返回 Err 表示 EventSource 创建失败（如 URL 非法），调用方应降级到轮询。
    pub fn start_sse(
        task_id: &str,
        term_handle: &Signal<Option<TerminalHandle>>,
        running: &mut Signal<bool>,
        exit_info: &mut Signal<String>,
        error_msg: &mut Signal<String>,
        has_output: &mut Signal<bool>,
    ) -> Result<(), JsValue> {
        let url = format!("/api/exec/stream?task_id={task_id}");
        let es = EventSource::new(&url)?;

        // stdout 事件 → 终端 writeStdout + 标记已有输出（骨架屏消失）
        let term_for_stdout = *term_handle;
        let mut has_output_for_stdout = *has_output;
        let on_stdout = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            if let Some(s) = e.data().as_string() {
                if let Some(h) = term_for_stdout.read().as_ref() {
                    h.instance().write_stdout(&s);
                }
                has_output_for_stdout.set(true);
            }
        });
        es.add_event_listener_with_callback("stdout", on_stdout.as_ref().unchecked_ref())?;
        on_stdout.forget();

        // stderr 事件 → 终端 writeStderr（红色）+ 标记已有输出
        let term_for_stderr = *term_handle;
        let mut has_output_for_stderr = *has_output;
        let on_stderr = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            if let Some(s) = e.data().as_string() {
                if let Some(h) = term_for_stderr.read().as_ref() {
                    h.instance().write_stderr(&s);
                }
                has_output_for_stderr.set(true);
            }
        });
        es.add_event_listener_with_callback("stderr", on_stderr.as_ref().unchecked_ref())?;
        on_stderr.forget();

        // done 事件 → 解析终态，设 signal，关闭连接
        let mut running_clone = *running;
        let mut exit_info_clone = *exit_info;
        let mut error_msg_clone = *error_msg;
        let es_for_done = es.clone();
        let on_done = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            let payload: DonePayload = e
                .data()
                .as_string()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(DonePayload {
                    exit_code: None,
                    oom_killed: false,
                    timed_out: false,
                    duration_ms: 0,
                    error: None,
                });

            // 系统级错误（镜像缺失 / daemon 不可达）优先于退出码展示——
            // 此类错误下 exit_code 无意义，直接显示可操作消息。
            let (info, err) = if let Some(msg) = payload.error {
                (
                    format!("耗时: {}ms · 状态: 系统错误", payload.duration_ms),
                    msg,
                )
            } else if payload.timed_out {
                (
                    format!("耗时: {}ms · 状态: 超时", payload.duration_ms),
                    "超时".to_string(),
                )
            } else if payload.oom_killed {
                (
                    format!("耗时: {}ms · 状态: 内存超限", payload.duration_ms),
                    "内存超限".to_string(),
                )
            } else if payload.exit_code == Some(0) {
                (
                    format!("耗时: {}ms · 状态: 成功", payload.duration_ms),
                    String::new(),
                )
            } else {
                (
                    format!(
                        "耗时: {}ms · 退出码: {} · 运行错误",
                        payload.duration_ms,
                        payload.exit_code.unwrap_or(-1)
                    ),
                    "运行错误".to_string(),
                )
            };
            exit_info_clone.set(info);
            error_msg_clone.set(err);
            running_clone.set(false);
            es_for_done.close();
        });
        es.add_event_listener_with_callback("done", on_done.as_ref().unchecked_ref())?;
        on_done.forget();

        // error 事件 → EventSource 连接异常，关闭并标记错误
        let mut running_clone = *running;
        let mut error_msg_clone = *error_msg;
        let es_for_error = es.clone();
        let on_error = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
            error_msg_clone.set("连接异常，请重试".to_string());
            running_clone.set(false);
            es_for_error.close();
        });
        es.add_event_listener_with_callback("error", on_error.as_ref().unchecked_ref())?;
        on_error.forget();

        Ok(())
    }

    /// 轮询兜底路径：SSE 不可用时用 get_exec_result 轮询，
    /// 拿到完整结果后 writeAll 整段写入终端。
    pub async fn poll_result(
        task_id: &str,
        running: &mut Signal<bool>,
        stage: &mut Signal<String>,
        exit_info: &mut Signal<String>,
        error_msg: &mut Signal<String>,
        term_handle: &Signal<Option<TerminalHandle>>,
        has_output: &mut Signal<bool>,
    ) {
        let mut polls = 0u32;
        loop {
            polls += 1;
            sleep_ms(500).await;
            match get_exec_result(task_id.to_string()).await {
                Ok(task) => {
                    stage.set(task.stage.clone());
                    let terminal =
                        task.status != ExecStatus::Queued && task.status != ExecStatus::Running;
                    if terminal {
                        running.set(false);
                        if let Some(res) = task.result {
                            super::apply_exec_outcome(exit_info, error_msg, &res);
                            // 整段写入终端
                            if let Some(h) = term_handle.read().as_ref() {
                                h.instance().write_all(&res.stdout, &res.stderr);
                            }
                            has_output.set(true);
                        }
                        break;
                    }
                    if polls >= 240 {
                        running.set(false);
                        stage.set("查询超时".to_string());
                        error_msg.set("轮询超时，请重试".to_string());
                        break;
                    }
                }
                Err(_) => {
                    running.set(false);
                    stage.set("结果获取异常".to_string());
                    error_msg.set("结果获取异常".to_string());
                    break;
                }
            }
        }
    }
}
