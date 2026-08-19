//! 后台「站点配置」页面（滚动联动分区布局）。
//!
//! 左侧分类导航 + 右侧滚动内容的布局（2026 admin UX 最佳实践），将全部站点级
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
//! 与 tab 切换不同，全部分区**常驻挂载、纵向堆叠**在同一个滚动容器里：
//! - 点击左侧导航 → 容器 `scroll-smooth` + `scrollIntoView` 平滑滚动到对应分区；
//! - 手动滚动 → scroll-spy（复用 changelog.rs 的 getBoundingClientRect 判定线
//!   模式）实时同步左侧高亮；
//! - 点击触发的平滑滚动期间用「世代锁」抑制 spy，避免途经分区依次闪过高亮。
//!
//! 各分区拆分为独立子模块，状态完全独立、互不共享，全部挂载后各自存活。
//!
//! 仅 WASM 前端交互（照 mcp.rs / friends.rs 的 `#[cfg(target_arch = "wasm32")]`
//! 门控模式）。

mod backup_section;
mod cache_section;
mod image_section;
mod ratelimit_section;
mod runner_section;
mod security_section;
mod site_section;
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

    /// 分区锚点元素的 DOM id（scroll-spy 判定与 scrollIntoView 定位共用）。
    fn dom_id(&self) -> String {
        format!("settings-sec-{}", self.as_str())
    }
}

/// 计算滚动容器中当前命中的分区（仅 wasm；模式复用 changelog.rs 的判定线法：
/// 容器内部分区数量有限，每次滚动事件十几次 rect 读取成本可忽略，无需
/// IntersectionObserver 那套可见性集合管理）。
#[cfg(target_arch = "wasm32")]
fn compute_visible_section() -> Option<SettingsSection> {
    let document = web_sys::window()?.document()?;
    let container = document.get_element_by_id("settings-scroll")?;
    let container_top = container.get_bounding_client_rect().top();

    // 贴底 → 强制末项：末分区内容短时永远到不了判定线。
    let at_bottom = f64::from(container.scroll_top()) + f64::from(container.client_height())
        >= f64::from(container.scroll_height()) - 4.0;
    let all = SettingsSection::all();
    if at_bottom {
        return all.last().copied();
    }

    // 判定线 = 容器顶部往下 40px（须大于分区的 scroll-mt-2=8px 落点余量）。
    // 顶部已越过判定线的最后一个分区 = 当前阅读位置。
    const THRESHOLD_PX: f64 = 40.0;
    let mut current: Option<SettingsSection> = None;
    for section in all {
        let Some(el) = document.get_element_by_id(&section.dom_id()) else {
            break;
        };
        if el.get_bounding_client_rect().top() - container_top <= THRESHOLD_PX {
            current = Some(section);
        } else {
            break;
        }
    }
    // 首分区尚未越过判定线（页面顶附近）→ 回退首项。
    current.or_else(|| all.first().copied())
}

/// 平滑滚动到指定分区（仅 wasm）。平滑由滚动容器的 `scroll-smooth`
/// （scroll-behavior: smooth）提供，这里只需瞬时 scrollIntoView。
#[cfg(target_arch = "wasm32")]
fn scroll_to_section(section: SettingsSection) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = document.get_element_by_id(&section.dom_id()) else {
        return;
    };
    el.scroll_into_view();
}

/// 渲染单个分区组件。全部分区常驻挂载，各分区状态独立、互不共享。
fn render_section(section: SettingsSection, toast: Callback<(String, bool)>) -> Element {
    match section {
        SettingsSection::Security => rsx! {
            SecuritySection { toast }
        },
        SettingsSection::RateLimit => rsx! {
            RateLimitSection { toast }
        },
        SettingsSection::Site => rsx! {
            SiteSection { toast }
        },
        SettingsSection::Cache => rsx! {
            CacheSection { toast }
        },
        SettingsSection::Backup => rsx! {
            BackupSection { toast }
        },
        SettingsSection::Image => rsx! {
            ImageSection { toast }
        },
        SettingsSection::Runner => rsx! {
            RunnerSection { toast }
        },
        SettingsSection::Trash => rsx! {
            TrashSection { toast }
        },
        SettingsSection::Upload => rsx! {
            UploadSection { toast }
        },
        SettingsSection::System => rsx! {
            SystemSection {}
        },
    }
}

/// 管理后台站点配置页面（滚动联动分区布局）。
///
/// 左侧分类导航 + 右侧纵向堆叠的全部分区。点击导航平滑滚动到对应分区；
/// 手动滚动时 scroll-spy 同步左侧高亮（`active` 信号驱动）。
#[component]
pub fn SiteSettingsPage() -> Element {
    let mut active = use_signal(|| SettingsSection::Site);
    // scroll-spy 锁定：点击导航后的平滑滚动期间抑制 spy 改写 active，避免途经
    // 分区依次闪过高亮。u32 为单调世代号，防止上一次点击的超时回调误清本次的锁。
    // 仅 wasm 使用（server 目标下 onscroll/onclick 体内的 DOM 逻辑整体 cfg 掉）。
    #[cfg(target_arch = "wasm32")]
    let mut spy_lock: Signal<Option<(SettingsSection, u32)>> = use_signal(|| None);
    #[cfg(target_arch = "wasm32")]
    let mut lock_gen: Signal<u32> = use_signal(|| 0);
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
        div { class: "animate-page-enter w-full flex-1 min-h-0 flex flex-col px-6 py-8 sm:py-12",
            // 页头（固定，不随右侧内容滚动）
            div { class: "flex-shrink-0 flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70 mb-6",
                div {
                    h1 { class: "text-3xl sm:text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                        "站点配置"
                    }
                    p { class: "text-sm text-[var(--color-paper-secondary)] mt-1.5",
                        "管理站点公开配置、安全策略与后台行为参数"
                    }
                }
            }

            // 操作提示条（各分区共享；grid-rows 高度过渡 + 淡入淡出，3 秒自动消失）
            div { class: if toast_state().is_some() { "ygg-toast is-open" } else { "ygg-toast" },
                div { class: if display_err() { "ygg-toast-inner text-sm rounded-lg px-3 py-2 bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300" } else { "ygg-toast-inner text-sm rounded-lg px-3 py-2 bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300" },
                    "{display_msg()}"
                }
            }

            // 顶部横向分类导航栏（水平胶囊语言，消除双侧栏宽度不协调问题）
            div { class: "flex-shrink-0 flex items-center gap-2 overflow-x-auto pb-3 mb-6 border-b border-[var(--color-paper-border)]/60 scrollbar-none select-none",
                for (idx, &section) in SettingsSection::all().iter().enumerate() {
                    {
                        let is_active = active() == section;
                        let label = section.label();
                        let icon_path = section.icon_path();
                        let base = "inline-flex items-center gap-2 px-3.5 py-1.5 rounded-full text-xs font-medium transition-all whitespace-nowrap cursor-pointer shrink-0 border";
                        let color = if is_active {
                            "bg-[var(--color-paper-accent)] text-[var(--color-paper-theme)] shadow-2xs border-transparent font-semibold"
                        } else {
                            "text-[var(--color-paper-secondary)] bg-[var(--color-paper-entry)]/80 hover:bg-[var(--color-paper-theme)] hover:text-[var(--color-paper-primary)] border-[var(--color-paper-border)]/70"
                        };
                        rsx! {
                            button {
                                key: "{section.as_str()}",
                                class: "animate-row-enter {base} {color}",
                                style: "animation-delay: {idx * 25}ms",
                                onclick: move |_| {
                                    active.set(section);
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        let g = lock_gen().wrapping_add(1);
                                        lock_gen.set(g);
                                        spy_lock.set(Some((section, g)));
                                        scroll_to_section(section);
                                        spawn(async move {
                                            crate::utils::time::sleep_ms(1200).await;
                                            if matches!(spy_lock(), Some((_, gg)) if gg == g) {
                                                spy_lock.set(None);
                                            }
                                        });
                                    }
                                },
                                svg {
                                    class: "w-3.5 h-3.5 flex-shrink-0",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
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

            // 内容区：全部分区常驻挂载、纵向堆叠在同一个滚动容器里
            div {
                id: "settings-scroll",
                class: "flex-1 min-w-0 min-h-0 overflow-y-auto pb-8 rounded-2xl scroll-smooth",
                onscroll: move |_| {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let Some(current) = compute_visible_section() else {
                            return;
                        };
                        if let Some((target, _)) = spy_lock() {
                            if current == target {
                                spy_lock.set(None);
                            }
                            return;
                        }
                        if *active.peek() != current {
                            active.set(current);
                        }
                    }
                },
                div { class: "flex flex-col gap-8 max-w-5xl mx-auto w-full",
                    for (idx, &section) in SettingsSection::all().iter().enumerate() {
                        section {
                            key: "{section.as_str()}",
                            id: "{section.dom_id()}",
                            class: "scroll-mt-2 animate-section-enter",
                            style: "animation-delay: {idx * 40}ms",
                            {render_section(section, toast)}
                        }
                    }
                }
            }
        }
    }
}
