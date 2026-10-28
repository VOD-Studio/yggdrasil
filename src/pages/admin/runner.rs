//! 管理后台「代码运行器」页面（`/admin/runner`）。
//!
//! 作者在写作时可在此沙箱快速试运行代码（验证围栏 ` ```lang runnable ` 的预期输出），
//! 而无需进入文章渲染后才能运行。沙箱使用与读者相同的 StartExec / GetExecResult
//! 接口，受同一套资源钳制约束（admin 跳过速率限制，见 `start_exec`）。
//!
//! 左侧为语言分段与运行器，右侧为资源上限与临时覆盖。样式见
//! styles/pages/admin-runner.css。

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::{lazy_route, LazyRoute};

use crate::components::admin_layout::AdminPageHead;
use crate::components::code_runner::CodeRunner;
use crate::components::forms::{FormLabel, FormTextarea};
use crate::components::ui::{SegmentedControl, SegmentedItem, StatusDot};
use crate::infra::runner_config::ResourceLimits;
use crate::models::settings::RunnerSettings;
use crate::routes::Route;

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

/// 解析「临时覆盖」JSON：空串视为无覆盖，格式不对时忽略并给出提示。
fn parse_overrides(raw: &str) -> (Option<ResourceLimits>, String) {
    match serde_json::from_str::<ResourceLimits>(raw.trim()) {
        Ok(o) => (Some(o), String::new()),
        Err(_) if raw.trim().is_empty() => (None, String::new()),
        Err(_) => (None, "JSON 格式不对，已忽略".to_string()),
    }
}

/// 代码运行器路由（无 SSR 数据，资源上限在客户端读取）。
pub struct RunnerRoute;

#[lazy_route]
impl LazyRoute for RunnerRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <Runner/> }.into_any()
    }
}

/// 管理后台代码试运行页面。
#[component]
fn Runner() -> impl IntoView {
    let lang = RwSignal::new("python".to_string());
    // 语言切换时刷新示例源码（首次进入也有默认值）。
    let source = RwSignal::new(default_source("python"));
    let overrides_json = RwSignal::new(String::new());
    let limits = RwSignal::new(None::<RunnerSettings>);
    // 仅客户端读取资源上限（Effect 不在服务端运行）。
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(s) = crate::api::settings::get_runner_settings().await {
                let _ = limits.try_set(Some(s));
            }
        });
    });

    let parsed = Memo::new(move |_| overrides_json.with(|raw| parse_overrides(raw)));
    let override_error = Signal::derive(move || parsed.with(|(_, error)| error.clone()));

    view! {
        <div class="admin-runner">
            <AdminPageHead
                title="代码运行器"
                actions=|| view! { <StatusDot label="Docker 沙箱" tone="success"/> }
            >
                "在沙箱里试运行代码，确认文章里可运行代码块的预期输出。"
            </AdminPageHead>
            <div class="runner-admin-grid">
                <section class="runner-admin-main" aria-label="编辑与输出">
                    <SegmentedControl
                        class="runner-admin-langs"
                        aria_label="语言"
                        items={
                            SUPPORTED_LANGS
                                .iter()
                                .map(|(v, l)| SegmentedItem::new(v.to_string(), *l))
                                .collect::<Vec<_>>()
                        }
                        value=lang
                        on_change=move |l: String| {
                            if l != lang.get_untracked() {
                                source.set(default_source(&l));
                                lang.set(l);
                            }
                        }
                    />
                    // 切语言时按语言重建：CodeRunner 的防重复初始化守卫与非响应式
                    // prop 让它不会就地换语言，卸载重建最直接。
                    {move || {
                        let language = lang.get();
                        let overrides = parsed.with_untracked(|(o, _)| o.clone());
                        view! {
                            <div class="runner-admin-runner">
                                <CodeRunner
                                    source=source.get_untracked()
                                    language=language
                                    overrides=overrides
                                    instance_id=0
                                />
                            </div>
                        }
                    }}
                </section>
                <aside class="admin-panel runner-admin-side" aria-label="运行配置">
                    <h2>"资源上限"</h2>
                    {move || match limits.get() {
                        Some(c) => {
                            view! {
                                <dl>
                                    <dt>"超时"</dt>
                                    <dd>{format!("{} 秒", c.max_timeout_secs)}</dd>
                                    <dt>"内存"</dt>
                                    <dd>{format!("{} MB", c.max_memory_mb)}</dd>
                                    <dt>"CPU"</dt>
                                    <dd>{format!("{:.1} 核", c.max_cpu_cores)}</dd>
                                    <dt>"网络"</dt>
                                    <dd>{if c.allow_network { "开启" } else { "关闭" }}</dd>
                                </dl>
                            }
                                .into_any()
                        }
                        None => view! { <p class="runner-admin-hint">"正在读取…"</p> }.into_any(),
                    }}
                    <div>
                        <FormLabel
                            label="临时覆盖（JSON，可选）"
                            html_for="runner-overrides".to_string()
                        />
                        <FormTextarea
                            id="runner-overrides"
                            class="field field-textarea settings-mono"
                            rows=4
                            placeholder=OVERRIDES_PLACEHOLDER
                            invalid=Signal::derive(move || !override_error.with(String::is_empty))
                            value=overrides_json
                            oninput=move |v: String| overrides_json.set(v)
                        />
                        {move || {
                            if override_error.with(String::is_empty) {
                                view! {
                                    <p class="runner-admin-hint">
                                        "可覆盖 cpu_cores、memory_mb、timeout_secs、output_bytes、allow_network，仍受上限约束。"
                                    </p>
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <p class="runner-admin-hint" data-error="true" role="alert">
                                        {move || override_error.get()}
                                    </p>
                                }
                                    .into_any()
                            }
                        }}
                    </div>
                    <a class="runner-admin-link" href=Route::SiteSettingsPage {}.to_string()>
                        "在设置里调整上限"
                    </a>
                </aside>
            </div>
        </div>
    }
}
