//! 启动配置分区（只读）：进程启动前就要用到、不能运行时修改的配置。
//!
//! 数据库连接、日志级别、Docker 等值保留在环境变量里，这里只展示；密钥类只显示
//! 是否已设置。

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::kit::SettingsCard;
use super::SettingsSection;
use crate::api::settings::get_system_info;
use crate::models::settings::SystemInfo;

/// 一行只读配置：名称、环境变量、当前值与说明。
fn rows(i: &SystemInfo) -> Vec<(&'static str, &'static str, String, &'static str)> {
    let on_off = |b: bool| if b { "开启" } else { "关闭" }.to_string();
    vec![
        (
            "数据库连接",
            "DATABASE_URL",
            i.database_url_masked.clone(),
            "启动时必需",
        ),
        ("日志级别", "RUST_LOG", i.rust_log.clone(), "tracing 过滤器"),
        (
            "连接池大小",
            "DB_POOL_SIZE",
            i.db_pool_size.to_string(),
            "deadpool 连接池",
        ),
        (
            "查询超时",
            "STATEMENT_TIMEOUT_SECS",
            format!("{} 秒", i.statement_timeout_secs),
            "写进连接池选项",
        ),
        (
            "SSR 缓存时长",
            "SSR_CACHE_SECS",
            format!("{} 秒", i.ssr_cache_secs),
            "增量渲染缓存",
        ),
        (
            "响应压缩",
            "COMPRESSION_ALGORITHMS",
            if i.compression_algorithms.is_empty() {
                "关闭".into()
            } else {
                i.compression_algorithms.clone()
            },
            "压缩层启动时构建",
        ),
        (
            "版本响应头",
            "EXPOSE_VERSION_HEADERS",
            on_off(i.expose_version_headers),
            "中间件启动时挂载",
        ),
        (
            "Docker Socket",
            "DOCKER_SOCKET_PATH",
            i.docker_socket_path.clone(),
            "代码运行器连接 Docker",
        ),
        (
            "MCP 加密主密钥",
            "MCP_TOKEN_ENC_KEY",
            if i.mcp_token_enc_key_set {
                "已设置".into()
            } else {
                "未设置".into()
            },
            "轮换会使旧令牌无法解密",
        ),
        (
            "启动迁移超时",
            "MIGRATE_STARTUP_TIMEOUT_SECS",
            format!("{} 秒", i.migrate_startup_timeout_secs),
            "只在启动期间生效",
        ),
        (
            "系统采样间隔",
            "SYSINFO_SAMPLE_SECS",
            format!("{} 秒", i.sysinfo_sample_secs),
            "采样循环启动时读取",
        ),
        (
            "日志查看器级别",
            "LOG_VIEWER_LEVEL",
            i.log_viewer_level.clone(),
            "/admin/logs 入库级别，独立于 RUST_LOG",
        ),
        (
            "备份导入上限",
            "BACKUP_IMPORT_MAX_MB",
            format!("{} MB", i.backup_import_max_mb),
            "反代 body 上限需同步放大",
        ),
        (
            "旧发布资源目录",
            "YGG_PREVIOUS_PUBLIC_DIR",
            if i.previous_public_dir.is_empty() {
                "未设置".into()
            } else {
                i.previous_public_dir.clone()
            },
            "滚动发布时保留旧 hash 资源",
        ),
    ]
}

/// 启动配置分区组件。
#[component]
pub fn SystemSection() -> impl IntoView {
    let info = RwSignal::new(None::<SystemInfo>);
    let failed = RwSignal::new(false);
    // Effect 只在浏览器里运行：服务端渲染不请求。
    Effect::new(move |loaded: Option<()>| {
        if loaded.is_some() {
            return;
        }
        spawn_local(async move {
            match get_system_info().await {
                Ok(i) => info.set(Some(i)),
                Err(_) => failed.set(true),
            }
        });
    });
    let ready = Signal::derive(move || info.with(|i| i.is_some()));
    view! {
        <SettingsCard
            section=SettingsSection::System
            description="进程启动时读取的环境变量，这里只展示；修改后需重启服务。"
            ready=ready
            load_failed=failed
        >
            {move || {
                info.get()
                    .map(|i| {
                        view! {
                            <dl class="settings-readonly">
                                {rows(&i)
                                    .into_iter()
                                    .map(|(label, env, value, hint)| {
                                        view! {
                                            <div class="settings-readonly-row">
                                                <dt>
                                                    <span>{label}</span>
                                                    <code>{env}</code>
                                                </dt>
                                                <dd>
                                                    <span class="settings-mono">{value}</span>
                                                    <small>{hint}</small>
                                                </dd>
                                            </div>
                                        }
                                    })
                                    .collect_view()}
                            </dl>
                        }
                    })
            }}
        </SettingsCard>
    }
}
