//! 后台「站点配置」页面（分区化重构）。
//!
//! 采用左侧分类导航 + 右侧内容的布局（2026 admin UX 最佳实践），将全部站点级
//! 配置集中到一处。分区按功能域组织：
//!
//! - **站点**：页脚 GitHub 链接等公开配置（前台可见）
//! - **安全**：CSRF 可信源、cookie Secure、代理层数、并发会话上限（即时生效）
//! - **缓存**：图片磁盘缓存容量与保留策略（即时生效）
//! - **备份**：自动备份调度与保留份数（即时生效，备份操作仍在 /admin/system）
//! - **回收站**：自动清理开关与保留天数（即时生效）
//! - **上传**：素材上传弹窗并发数（即时生效）
//! - **系统**：只读展示启动时配置（数据库、日志、Docker 等，需重启生效）
//!
//! 分区切换用 `use_signal`（不深链 / 不走分页路由），与 system 页面的 tab 模式一致。
//! 各分区拆分为独立子模块，状态完全独立、互不共享——切换分区时父组件用 `key`
//! 强制卸载旧分区组件，其内部 signal 随之销毁。
//!
//! 仅 WASM 前端交互（照 mcp.rs / friends.rs 的 `#[cfg(target_arch = "wasm32")]`
//! 门控模式）。

mod backup_section;
mod cache_section;
mod image_section;
mod security_section;
mod site_section;
mod ratelimit_section;
mod runner_section;
mod system_section;
mod trash_section;
mod upload_section;

use dioxus::prelude::*;

use backup_section::BackupSection;
use cache_section::CacheSection;
use image_section::ImageSection;
use ratelimit_section::RateLimitSection;
use runner_section::RunnerSection;
use security_section::SecuritySection;
use site_section::SiteSection;
use system_section::SystemSection;
use trash_section::TrashSection;
use upload_section::UploadSection;

/// 设置页面的分区枚举。
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SettingsSection {
    /// 站点公开配置（页脚 GitHub 链接等）。
    Site,
    /// 安全配置（CSRF / cookie / 代理 / 会话）。
    Security,
    /// 缓存配置（图片磁盘缓存）。
    Cache,
    /// 限流配置（各接口速率限制）。
    RateLimit,
    /// 图片处理配置（WebP 编码 / 尺寸上限）。
    Image,
    /// 代码运行器配置（沙箱资源限制）。
    Runner,
    /// 自动备份调度。
    Backup,
    /// 回收站自动清理。
    Trash,
    /// 素材上传并发。
    Upload,
    /// 系统启动配置（只读）。
    System,
}

impl SettingsSection {
    fn as_str(&self) -> &'static str {
        match self {
            SettingsSection::Site => "site",
            SettingsSection::Security => "security",
            SettingsSection::Cache => "cache",
            SettingsSection::RateLimit => "ratelimit",
            SettingsSection::Image => "image",
            SettingsSection::Runner => "runner",
            SettingsSection::Backup => "backup",
            SettingsSection::Trash => "trash",
            SettingsSection::Upload => "upload",
            SettingsSection::System => "system",
        }
    }

    /// 分区显示名称。
    fn label(&self) -> &'static str {
        match self {
            SettingsSection::Site => "站点",
            SettingsSection::Security => "安全",
            SettingsSection::Cache => "缓存",
            SettingsSection::RateLimit => "限流",
            SettingsSection::Image => "图片",
            SettingsSection::Runner => "运行器",
            SettingsSection::Backup => "备份",
            SettingsSection::Trash => "回收站",
            SettingsSection::Upload => "上传",
            SettingsSection::System => "系统",
        }
    }

    /// 分区图标（Feather 线框 SVG path d 属性）。
    fn icon_path(&self) -> &'static str {
        match self {
            SettingsSection::Site => "M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z M9 22V12h6v10",
            SettingsSection::Security => "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z",
            SettingsSection::Cache => "M21 8v13H3V8 M1 3h22v5H1z M10 12h4",
            SettingsSection::RateLimit => "M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2z M12 6v6l4 2",
            SettingsSection::Image => "M21 15l-5-5L5 21 M18 21H3V3h18v9z M8.5 8.5a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0z",
            SettingsSection::Runner => "M16 18l6-6-6-6 M8 6l-6 6 6 6",
            SettingsSection::Backup => "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4 M7 10l5 5 5-5 M12 15V3",
            SettingsSection::Trash => "M3 6h18 M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2 M10 11v6 M14 11v6",
            SettingsSection::Upload => "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4 M17 8l-5-5-5 5 M12 3v12",
            SettingsSection::System => "M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z M22 6l-10 7L2 6",
        }
    }

    /// 所有分区，按导航顺序。
    fn all() -> [SettingsSection; 10] {
        [
            SettingsSection::Site,
            SettingsSection::Security,
            SettingsSection::Cache,
            SettingsSection::RateLimit,
            SettingsSection::Image,
            SettingsSection::Runner,
            SettingsSection::Backup,
            SettingsSection::Trash,
            SettingsSection::Upload,
            SettingsSection::System,
        ]
    }
}

/// 管理后台站点配置页面（分区化布局）。
///
/// 左侧分类导航 + 右侧分区内容。分区切换用 `use_signal`，切换时用 `key`
/// 强制卸载旧分区组件。
#[component]
pub fn SiteSettingsPage() -> Element {
    let mut active = use_signal(|| SettingsSection::Site);
    let mut toast_state: Signal<Option<(String, bool)>> = use_signal(|| None);
    let toast: Callback<(String, bool)> = Callback::new(move |m| toast_state.set(Some(m)));
    // 展示信号：toast_state 驱动 is-open（展开/收起），display_* 保留最近一条消息，
    // 使退出动画（收起+淡出）期间文本不闪。
    let mut display_msg: Signal<String> = use_signal(String::new);
    let mut display_err: Signal<bool> = use_signal(|| false);
    // toast 出现时同步展示信号 + 启动 3 秒自动消失计时器。
    use_effect(move || {
        if let Some((msg, is_err)) = toast_state() {
            display_msg.set(msg.clone());
            display_err.set(is_err);
            let key = msg.clone();
            spawn(async move {
                crate::utils::time::sleep_ms(3000).await;
                // 仅当 toast 未被新消息覆盖时清除（避免误清后续 toast）
                if toast_state().map(|(m, _)| m == key).unwrap_or(false) {
                    toast_state.set(None);
                }
            });
        }
    });
    rsx! {
        // flex-1 min-h-0 作为 main 的 flex 子项获得有界高度（AdminLayout 对 settings 路由
        // 已切到 internal-scroll 变体：卡片 overflow-hidden、main 无 padding 纯 flex 容器），
        // 故内边距 px-6 py-12 由本页自带。纯 flex 约束，不用百分比——百分比会被 main 的
        // min-height:auto 循环撑大失效。页头固定，下方 flex-1 区域仅占剩余高度。
        div { class: "animate-page-enter w-full flex-1 min-h-0 flex flex-col px-6 py-12",
            // 页头（固定，不随右侧内容滚动）
            div { class: "flex-shrink-0 flex flex-col md:flex-row md:items-end justify-between gap-6 pb-6 border-b border-[var(--color-paper-border)] mb-6",
                div {
                    h1 { class: "text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                        "站点配置"
                    }
                    p { class: "text-base text-[var(--color-paper-secondary)] mt-2",
                        "管理站点公开配置、安全策略与后台行为参数"
                    }
                }
            }

            // 操作提示条（各分区共享；grid-rows 高度过渡 + 淡入淡出，3 秒自动消失）
            div {
                class: if toast_state().is_some() { "ygg-toast is-open" } else { "ygg-toast" },
                div {
                    class: if display_err() {
                        "ygg-toast-inner text-sm rounded-lg px-3 py-2 bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300"
                    } else {
                        "ygg-toast-inner text-sm rounded-lg px-3 py-2 bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300"
                    },
                    "{display_msg()}"
                }
            }

            // 左侧导航 + 右侧内容（占满剩余高度，仅右侧内容列滚动）
            div { class: "flex flex-col lg:flex-row gap-6 flex-1 min-h-0",
                // 左侧导航（固定；视口过矮时菜单自身纵向滚动）
                nav { class: "lg:w-48 flex-shrink-0 min-h-0",
                    div { class: "flex lg:flex-col gap-1 overflow-x-auto lg:overflow-x-visible lg:overflow-y-auto lg:h-full pb-2 lg:pb-0",
                        for (idx, &section) in SettingsSection::all().iter().enumerate() {
                            {
                                let is_active = active() == section;
                                let label = section.label();
                                let icon_path = section.icon_path();
                                let base = "flex items-center gap-2 px-3 py-2 rounded-xl text-sm font-medium transition-all whitespace-nowrap cursor-pointer border";
                                let color = if is_active {
                                    "bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] shadow-sm border-[var(--color-paper-border)]"
                                } else {
                                    "text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-theme)]/50 hover:text-[var(--color-paper-primary)] border-transparent"
                                };
                                rsx! {
                                    button {
                                        key: "{section.as_str()}",
                                        class: "animate-row-enter {base} {color}",
                                        style: "animation-delay: {idx * 35}ms",
                                        onclick: move |_| active.set(section),
                                        svg {
                                            class: "w-4 h-4 flex-shrink-0",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "1.8",
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            path { d: "{icon_path}" }
                                        }
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                }

                // 右侧内容（key 强制切换分区时卸载/重建；本列独立纵向滚动）。
                // rounded-2xl：overflow 裁剪沿 padding-box 圆角曲线裁切，滚动到中途时
                // 面板被截断的底角/顶角也呈现与卡片（ADMIN_CARD_CLASS 16px）一致的圆角，
                // 不再是直角切断。列无背景，静止态（顶/底）与面板自身圆角重合，无视觉变化。
                div { class: "animate-section-enter flex-1 min-w-0 min-h-0 overflow-y-auto pb-6 rounded-2xl", key: "{active().as_str()}",
                    {match active() {
                        SettingsSection::Security => rsx! { SecuritySection { toast } },
                        SettingsSection::RateLimit => rsx! { RateLimitSection { toast } },
                        SettingsSection::Site => rsx! { SiteSection { toast } },
                        SettingsSection::Cache => rsx! { CacheSection { toast } },
                        SettingsSection::Backup => rsx! { BackupSection { toast } },
                        SettingsSection::Image => rsx! { ImageSection { toast } },
                        SettingsSection::Runner => rsx! { RunnerSection { toast } },
                        SettingsSection::Trash => rsx! { TrashSection { toast } },
                        SettingsSection::Upload => rsx! { UploadSection { toast } },
                        SettingsSection::System => rsx! { SystemSection {} },
                    }}
                }
            }
        }
    }
}
