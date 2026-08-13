//! 系统启动配置分区（只读展示）。
//!
//! 展示启动时读取、不可运行时修改的配置：数据库连接、日志级别、Docker 等。
//! 这些值在进程启动前就需要（配置 pool/runtime 本身），无法迁移到 DB 面板。
//! 2026 最佳实践：密钥与基础设施配置保留在 env，不放自定义 DB 表。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::settings::get_system_info;
#[cfg(target_arch = "wasm32")]
use crate::components::ui::ADMIN_CARD_CLASS;
#[cfg(target_arch = "wasm32")]
use crate::models::settings::SystemInfo;

/// 系统启动配置分区组件（只读）。
#[allow(non_snake_case)]
#[component]
pub fn SystemSection() -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        let info: Signal<Option<SystemInfo>> = use_signal(|| None);
        let loading = use_signal(|| true);

        use_effect(move || {
            #[cfg(target_arch = "wasm32")]
            {
                let mut info = info;
                let mut loading = loading;
                spawn(async move {
                    match get_system_info().await {
                        Ok(i) => info.set(Some(i)),
                        Err(_) => info.set(None),
                    }
                    loading.set(false);
                });
            }
        });

        rsx! {
            div { class: "space-y-6",
                if loading() {
                    div { class: "{ADMIN_CARD_CLASS} p-8 text-[var(--color-paper-secondary)] animate-pulse", "加载中…" }
                } else if let Some(i) = info() {
                    div { class: "{ADMIN_CARD_CLASS} p-6 md:p-8 flex flex-col gap-6",
                        div { class: "flex items-center gap-3",
                            span { class: "inline-flex items-center justify-center w-10 h-10 rounded-full bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]",
                                svg { class: "w-5 h-5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z" }
                                    polyline { points: "22,6 12,13 2,6" }
                                }
                            }
                            div {
                                h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]", "系统启动配置" }
                                p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                    "以下配置在进程启动时读取，修改后需重启服务生效。密钥类配置仅展示是否已设置。"
                                }
                            }
                        }

                        div { class: "divide-y divide-[var(--color-paper-border)]",
                            SystemInfoRow { label: "数据库连接 (DATABASE_URL)", value: i.database_url_masked, hint: "启动时必需，不可运行时修改" }
                            SystemInfoRow { label: "日志级别 (RUST_LOG)", value: i.rust_log.clone(), hint: "tracing 过滤器，修改需重启" }
                            SystemInfoRow { label: "连接池大小 (DB_POOL_SIZE)", value: i.db_pool_size.to_string(), hint: "deadpool 连接池，修改需重启" }
                            SystemInfoRow { label: "查询超时 (STATEMENT_TIMEOUT_SECS)", value: format!("{} 秒", i.statement_timeout_secs), hint: "烤进连接池 options，修改需重启" }
                            SystemInfoRow { label: "SSR 缓存时长 (SSR_CACHE_SECS)", value: format!("{} 秒", i.ssr_cache_secs), hint: "Dioxus 增量渲染配置，修改需重启" }
                            SystemInfoRow { label: "响应压缩 (COMPRESSION_ALGORITHMS)", value: if i.compression_algorithms.is_empty() { "off".to_string() } else { i.compression_algorithms.clone() }, hint: "CompressionLayer 启动时构建" }
                            SystemInfoRow { label: "版本响应头 (EXPOSE_VERSION_HEADERS)", value: if i.expose_version_headers { "开启".to_string() } else { "关闭".to_string() }, hint: "中间件层启动时挂载" }
                            SystemInfoRow { label: "Docker Socket", value: i.docker_socket_path.clone(), hint: "代码运行器 Docker 连接" }
                            SystemInfoRow { label: "MCP 加密主密钥", value: if i.mcp_token_enc_key_set { "已设置".to_string() } else { "未设置".to_string() }, hint: "AES-GCM-256 令牌加密，轮换会使旧令牌无法解密" }
                            SystemInfoRow { label: "启动迁移超时 (MIGRATE_STARTUP_TIMEOUT_SECS)", value: format!("{} 秒", i.migrate_startup_timeout_secs), hint: "仅启动期间生效" }
                            SystemInfoRow { label: "系统采样间隔 (SYSINFO_SAMPLE_SECS)", value: format!("{} 秒", i.sysinfo_sample_secs), hint: "采样器循环启动时捕获" }
                        }
                    }
                } else {
                    div { class: "{ADMIN_CARD_CLASS} p-8 text-[var(--color-paper-secondary)]", "无法加载系统信息" }
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        rsx! { div { class: "p-8 text-[var(--color-paper-secondary)]", "系统配置（前端渲染）" } }
    }
}

/// 单行系统信息（标签 + 值 + 提示）。
#[allow(non_snake_case)]
#[component]
fn SystemInfoRow(label: String, value: String, hint: String) -> Element {
    rsx! {
        div { class: "py-3 flex flex-col sm:flex-row sm:items-center gap-1 sm:gap-4",
            div { class: "sm:w-64 flex-shrink-0",
                span { class: "text-sm font-medium text-[var(--color-paper-primary)]", "{label}" }
            }
            div { class: "flex-1 flex flex-col gap-0.5",
                span { class: "text-sm text-[var(--color-paper-secondary)] font-mono break-all", "{value}" }
                span { class: "text-xs text-[var(--color-paper-tertiary)]", "{hint}" }
            }
        }
    }
}
