//! 管理后台「代码试运行」页面。
//!
//! 作者在写作时可在此沙箱快速试运行代码（验证围栏 ` ```lang runnable ` 的预期输出），
//! 而无需进入文章渲染后才能运行。沙箱使用与读者相同的 StartExec / GetExecResult
//! 接口，受同一套资源钳制约束（admin 跳过速率限制，见 `start_exec`）。
//!
//! 仅 WASM 前端交互；语言在受支持集合内切换。

use dioxus::prelude::*;

use crate::components::code_runner::CodeRunner;
use crate::components::forms::FormInput;
use crate::infra::runner_config::ResourceLimits;
/// 受支持的语言集合（与 LANGUAGES 注册表 / CODE_RUNNER_LANGUAGES 对齐）。
const SUPPORTED_LANGS: &[(&str, &str)] = &[
    ("python", "Python"),
    ("node", "Node.js"),
    ("go", "Go"),
    ("rust", "Rust"),
    ("bun", "Bun (TS)"),
];
/// 默认示例源码（按语言）。
fn default_source(lang: &str) -> String {
    match lang {
        "python" => "print('Hello from author sandbox')\nfor i in range(3):\n    print(f'line {i}')\n".to_string(),
        "node" => "console.log('Hello from author sandbox');\n[0,1,2].forEach(i => console.log(`line ${i}`));\n".to_string(),
        "go" => "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"Hello from author sandbox\")\n\tfor i := 0; i < 3; i++ {\n\t\tfmt.Printf(\"line %d\\n\", i)\n\t}\n}\n".to_string(),
        "rust" => "fn main() {\n    println!(\"Hello from author sandbox\");\n    for i in 0..3 {\n        println!(\"line {}\", i);\n    }\n}\n".to_string(),
        // bun 跑 TypeScript：示例用 TS 类型注解体现语言特性。
        "bun" => "const greeting: string = 'Hello from author sandbox';\nconsole.log(greeting);\n[0, 1, 2].forEach((i: number) => console.log(`line ${i}`));\n".to_string(),
        _ => String::new(),
    }
}

/// 管理后台代码试运行页面。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
pub fn Runner() -> Element {
    let mut lang = use_signal(|| "python".to_string());
    // 语言切换时刷新示例源码（首次进入也有默认值）。
    let mut source = use_signal(|| default_source("python"));
    let mut overrides_json = use_signal(String::new);

    // overrides 解析用 use_memo 承载：render 体只读不写（Dioxus render purity），
    // 避免 render 期间 .set() override_error。畸形 JSON 标记在 memo 返回值里。
    let parsed = use_memo(move || {
        let raw = overrides_json();
        match serde_json::from_str::<ResourceLimits>(raw.trim()) {
            Ok(o) => (Some(o), String::new()),
            Err(_) => {
                if raw.trim().is_empty() {
                    (None, String::new())
                } else {
                    (None, "overrides JSON 格式错误，已忽略".to_string())
                }
            }
        }
    });
    let (overrides, override_error) = (parsed.read().0.clone(), parsed.read().1.clone());

    rsx! {
        div { class: "animate-page-enter w-full max-w-7xl mx-auto space-y-8",
            // 页头：标题 + 副标题 + 运行引擎指示胶囊
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70",
                div {
                    h1 { class: "text-3xl sm:text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                        "代码试运行沙箱"
                    }
                    p { class: "text-sm text-[var(--color-paper-secondary)] mt-1.5",
                        "在线运行与调试多语言代码，实时验证文章内交互代码块的预期输出"
                    }
                }
                div { class: "flex items-center gap-2.5",
                    div { class: "inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-full text-xs font-mono bg-[var(--color-paper-entry)] text-[var(--color-paper-secondary)] border border-[var(--color-paper-border)]/70 shadow-2xs",
                        span { class: "w-1.5 h-1.5 rounded-full bg-[var(--color-paper-accent)]" }
                        span { "Docker 沙箱环境" }
                    }
                }
            }

            // 配置卡片：语言切换 + 资源覆盖
            div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 p-6 sm:p-8 flex flex-col gap-6",
                // 卡片标题
                div { class: "flex items-center gap-2.5 border-b border-[var(--color-paper-border)]/60 pb-4",
                    svg {
                        class: "w-5 h-5 text-[var(--color-paper-accent)]",
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M4 17l6-6-6-6" }
                        path { d: "M12 19h8" }
                    }
                    h2 { class: "text-lg sm:text-xl font-bold text-[var(--color-paper-primary)]",
                        "运行环境与配置"
                    }
                }

                // 语言选择胶囊群
                div { class: "flex flex-col gap-2.5",
                    label { class: "text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)]",
                        "编程语言"
                    }
                    div { class: "flex flex-wrap gap-2.5",
                        for (idx, (l, label_text)) in SUPPORTED_LANGS.iter().enumerate() {
                            button {
                                key: "{l}",
                                class: format!(
                                    "animate-row-enter inline-flex items-center gap-1.5 px-4 py-1.5 text-xs font-medium rounded-full transition-all cursor-pointer {}",
                                    if lang() == *l {
                                        "bg-[var(--color-paper-accent)] text-[var(--color-paper-theme)] shadow-2xs font-semibold"
                                    } else {
                                        "text-[var(--color-paper-secondary)] bg-[var(--color-paper-entry)] hover:bg-[var(--color-paper-theme)] hover:text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]/70"
                                    },
                                ),
                                style: "animation-delay: {idx * 40}ms",
                                onclick: {
                                    let ll = (*l).to_string();
                                    move |_| {
                                        if ll != lang() {
                                            lang.set(ll.clone());
                                            source.set(default_source(&ll));
                                        }
                                    }
                                },
                                span { "{label_text}" }
                            }
                        }
                    }
                }

                // 资源覆盖（JSON）
                div { class: "flex flex-col gap-2",
                    div { class: "flex items-center justify-between",
                        label { class: "text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)]",
                            "资源限制覆盖 (JSON, 可选)"
                        }
                        span { class: "text-[11px] text-[var(--color-paper-tertiary)] font-mono",
                            "timeout_secs / memory_mb / cpu_cores"
                        }
                    }
                    FormInput {
                        r#type: "text",
                        placeholder: "如 {{\"timeout_secs\":10,\"memory_mb\":512}}",
                        value: overrides_json(),
                        mono: true,
                        oninput: move |v: String| overrides_json.set(v),
                    }
                    if !override_error.is_empty() {
                        p { class: "text-xs text-red-500 dark:text-red-400 font-medium", "{override_error}" }
                    } else {
                        p { class: "text-xs text-[var(--color-paper-tertiary)] leading-normal",
                            "支持覆盖 cpu_cores / memory_mb / timeout_secs / output_bytes / allow_network；最终仍受服务端全局上限约束"
                        }
                    }
                }
            }
            // 强制 remount 切换语言：CodeRunner 挂载 use_effect 的「防重复 init」守卫
            // （editor_handle.is_some() → return）阻止 CodeMirror 重建到新语言；且 plain
            // String prop 非响应式，内部 use_effect 不会因 prop 变化重跑（与 post_detail 翻页
            // remount 同根因，见 src/pages/post_detail.rs:110-115 注释）。key 绑定语言 →
            // keyed diff 卸载旧实例（use_drop 销毁 CodeMirror/xterm）→ 挂载新实例，mount effect
            // 以新 language + 新 source 初始化编辑器。输出区随 remount 重置（切语言本应清空旧输出）。
            // 切语言时 CodeRunner 经 std::iter::once keyed remount 卸载/重建（详见上注释），
            // 包一层 animate-section-enter div：随 remount 重挂载，每次切语言重播
            // 300ms 淡入位移（与设置页分区切换同款动画）。
            for lang_key in std::iter::once(lang().clone()) {
                div { key: "{lang_key}", class: "animate-section-enter",
                    CodeRunner {
                        source: source(),
                        language: lang(),
                        overrides: overrides.clone(),
                        instance_id: 0,
                    }
                }
            }
        }
    }
}
