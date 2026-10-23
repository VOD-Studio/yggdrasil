//! 管理后台「代码运行器」页面（`/admin/runner`）。
//!
//! 作者在写作时可在此沙箱快速试运行代码（验证围栏 ` ```lang runnable ` 的预期输出），
//! 而无需进入文章渲染后才能运行。沙箱使用与读者相同的 StartExec / GetExecResult
//! 接口，受同一套资源钳制约束（admin 跳过速率限制，见 `start_exec`）。
//!
//! 左侧为语言分段与运行器，右侧为资源上限与临时覆盖。样式见
//! styles/pages/admin-runner.css。

use crate::components::admin_layout::AdminPageHead;
use dioxus::prelude::*;

use crate::components::code_runner::CodeRunner;
use crate::components::forms::{FormLabel, FormTextarea};
use crate::components::ui::{SegmentedControl, SegmentedItem, StatusDot};
use crate::infra::runner_config::ResourceLimits;
use crate::router::Route;

const OVERRIDES_PLACEHOLDER: &str = "{\"timeout_secs\": 3}";
/// 受支持的语言集合（与 LANGUAGES 注册表 / CODE_RUNNER_LANGUAGES 对齐）。
const SUPPORTED_LANGS: &[(&str, &str)] = &[
    ("python", "Python"),
    ("node", "Node.js"),
    ("go", "Go"),
    ("rust", "Rust"),
    ("bun", "Bun · TS"),
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
    let mut limits = use_signal(|| None::<crate::models::settings::RunnerSettings>);
    use_hook(move || {
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            if let Ok(s) = crate::api::settings::get_runner_settings().await {
                limits.set(Some(s));
            }
        });
    });

    // overrides 解析用 use_memo 承载：render 体只读不写（Dioxus render purity）。
    let parsed = use_memo(move || {
        let raw = overrides_json();
        match serde_json::from_str::<ResourceLimits>(raw.trim()) {
            Ok(o) => (Some(o), String::new()),
            Err(_) if raw.trim().is_empty() => (None, String::new()),
            Err(_) => (None, "JSON 格式不对，已忽略".to_string()),
        }
    });
    let (overrides, override_error) = (parsed.read().0.clone(), parsed.read().1.clone());
    let caps = limits();

    rsx! {
        div { class: "admin-runner",
            AdminPageHead {
                title: "代码运行器",
                actions: rsx! {
                    StatusDot { label: "Docker 沙箱", tone: "success" }
                },
                "在沙箱里试运行代码，确认文章里可运行代码块的预期输出。"
            }
            div { class: "runner-admin-grid",
                section { class: "runner-admin-main", aria_label: "编辑与输出",
                    SegmentedControl {
                        class: "runner-admin-langs",
                        aria_label: "语言",
                        items: SUPPORTED_LANGS
                            .iter()
                            .map(|(v, l)| SegmentedItem::new(v.to_string(), *l))
                            .collect::<Vec<_>>(),
                        value: lang(),
                        on_change: move |l: String| {
                            if l != lang() {
                                source.set(default_source(&l));
                                lang.set(l);
                            }
                        },
                    }
                    // 切语言时经 std::iter::once keyed remount：CodeRunner 的防重复初始化
                    // 守卫与非响应式 String prop 让它不会就地换语言，卸载重建最直接。
                    for lang_key in std::iter::once(lang().clone()) {
                        div { key: "{lang_key}", class: "runner-admin-runner",
                            CodeRunner {
                                source: source(),
                                language: lang(),
                                overrides: overrides.clone(),
                                instance_id: 0,
                            }
                        }
                    }
                }
                aside { class: "admin-panel runner-admin-side", aria_label: "运行配置",
                    h2 { "资源上限" }
                    if let Some(c) = caps {
                        dl {
                            dt { "超时" }
                            dd { "{c.max_timeout_secs} 秒" }
                            dt { "内存" }
                            dd { "{c.max_memory_mb} MB" }
                            dt { "CPU" }
                            dd { {format!("{:.1} 核", c.max_cpu_cores)} }
                            dt { "网络" }
                            dd { if c.allow_network { "开启" } else { "关闭" } }
                        }
                    } else {
                        p { class: "runner-admin-hint", "正在读取…" }
                    }
                    div {
                        FormLabel { label: "临时覆盖（JSON，可选）", html_for: "runner-overrides".to_string() }
                        FormTextarea {
                            id: "runner-overrides",
                            class: "field field-textarea settings-mono",
                            rows: 4,
                            placeholder: OVERRIDES_PLACEHOLDER,
                            invalid: !override_error.is_empty(),
                            value: format!("{overrides_json}"),
                            oninput: move |v: String| overrides_json.set(v),
                        }
                        if override_error.is_empty() {
                            p { class: "runner-admin-hint",
                                "可覆盖 cpu_cores、memory_mb、timeout_secs、output_bytes、allow_network，仍受上限约束。"
                            }
                        } else {
                            p { class: "runner-admin-hint", "data-error": "true", role: "alert", "{override_error}" }
                        }
                    }
                    Link { class: "runner-admin-link", to: Route::SiteSettingsPage {}, "在设置里调整上限" }
                }
            }
        }
    }
}
