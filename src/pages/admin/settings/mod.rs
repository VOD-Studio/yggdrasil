//! 后台「设置」页面（`/admin/settings`）。
//!
//! 左侧分区目录 + 右侧纵向堆叠的全部分区，按「站点、安全、上传、图片缓存、图片处理、
//! 素材清理、回收站、自动备份、代码运行器、接口限流、启动配置」排列。全部分区常驻
//! 挂载在同一个滚动容器里：
//! - 点目录 → `scrollIntoView` 平滑滚动到分区，期间用「世代锁」抑制 scroll-spy，
//!   避免途经分区依次闪过高亮；
//! - 手动滚动 → scroll-spy（判定线法）同步目录高亮。
//!
//! 各分区各自读取与提交（见 `kit.rs`），改动由底部浮起的保存条统一保存：只提交有
//! 改动的分区，「放弃」把全部草稿还原。需要重启才生效的分区在目录与卡片上标出。
//! 样式见 styles/pages/admin-settings.css。

mod backup_section;
mod cache_section;
mod image_section;
mod kit;
mod purge_section;
mod ratelimit_section;
mod runner_section;
mod security_section;
mod site_section;
mod system_section;
mod trash_section;
mod upload_section;

use crate::components::admin_layout::AdminPageHead;
use leptos::prelude::*;
use leptos_router::{lazy_route, LazyRoute};

use backup_section::BackupSection;
use cache_section::CacheSection;
use image_section::ImageSection;
use kit::SettingsBus;
use purge_section::AssetPurgeSection;
use ratelimit_section::RateLimitSection;
use runner_section::RunnerSection;
use security_section::SecuritySection;
use site_section::SiteSection;
use system_section::SystemSection;
use trash_section::TrashSection;
use upload_section::UploadSection;

use crate::components::ui::{button_class, use_toast, ButtonSize, ButtonVariant};

/// 设置页面的分区枚举，顺序即页面与导航顺序。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsSection {
    /// 站点公开配置（页脚链接、首页标语、站点简介）。
    Site,
    /// 安全配置（CSRF / cookie / 代理 / 会话）。
    Security,
    /// 素材上传并发。
    Upload,
    /// 图片磁盘缓存。
    Cache,
    /// 图片处理（WebP 编码 / 尺寸上限），重启生效。
    Image,
    /// 无引用素材自动清理。
    AssetPurge,
    /// 回收站自动清理。
    Trash,
    /// 自动备份调度。
    Backup,
    /// 代码运行器（沙箱资源限制），重启生效。
    Runner,
    /// 接口限流，重启生效。
    RateLimit,
    /// 启动配置（只读）。
    System,
}

impl SettingsSection {
    fn as_str(&self) -> &'static str {
        match self {
            SettingsSection::Site => "site",
            SettingsSection::Security => "security",
            SettingsSection::Upload => "upload",
            SettingsSection::Cache => "cache",
            SettingsSection::Image => "image",
            SettingsSection::AssetPurge => "assetpurge",
            SettingsSection::Trash => "trash",
            SettingsSection::Backup => "backup",
            SettingsSection::Runner => "runner",
            SettingsSection::RateLimit => "ratelimit",
            SettingsSection::System => "system",
        }
    }

    /// 分区名称（导航与卡片标题）。
    fn label(&self) -> &'static str {
        match self {
            SettingsSection::Site => "站点",
            SettingsSection::Security => "安全",
            SettingsSection::Upload => "上传",
            SettingsSection::Cache => "图片缓存",
            SettingsSection::Image => "图片处理",
            SettingsSection::AssetPurge => "素材清理",
            SettingsSection::Trash => "回收站",
            SettingsSection::Backup => "自动备份",
            SettingsSection::Runner => "代码运行器",
            SettingsSection::RateLimit => "接口限流",
            SettingsSection::System => "启动配置",
        }
    }

    /// 修改后需要重启服务才生效。
    fn needs_restart(&self) -> bool {
        matches!(
            self,
            SettingsSection::Image
                | SettingsSection::Runner
                | SettingsSection::RateLimit
                | SettingsSection::System
        )
    }

    /// 所有分区，按导航顺序。
    fn all() -> [SettingsSection; 11] {
        [
            SettingsSection::Site,
            SettingsSection::Security,
            SettingsSection::Upload,
            SettingsSection::Cache,
            SettingsSection::Image,
            SettingsSection::AssetPurge,
            SettingsSection::Trash,
            SettingsSection::Backup,
            SettingsSection::Runner,
            SettingsSection::RateLimit,
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

    // 判定线 = 容器顶部往下 64px（须大于 .settings-card 的 scroll-margin-top：桌面 1rem、窄屏 3.5rem）。
    // 顶部已越过判定线的最后一个分区 = 当前阅读位置。
    const THRESHOLD_PX: f64 = 64.0;
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

/// 渲染单个分区组件。全部分区常驻挂载，各自读取与提交，经保存条统一保存。
fn render_section(section: SettingsSection) -> AnyView {
    match section {
        SettingsSection::Site => view! { <SiteSection/> }.into_any(),
        SettingsSection::Security => view! { <SecuritySection/> }.into_any(),
        SettingsSection::Upload => view! { <UploadSection/> }.into_any(),
        SettingsSection::Cache => view! { <CacheSection/> }.into_any(),
        SettingsSection::Image => view! { <ImageSection/> }.into_any(),
        SettingsSection::AssetPurge => view! { <AssetPurgeSection/> }.into_any(),
        SettingsSection::Trash => view! { <TrashSection/> }.into_any(),
        SettingsSection::Backup => view! { <BackupSection/> }.into_any(),
        SettingsSection::Runner => view! { <RunnerSection/> }.into_any(),
        SettingsSection::RateLimit => view! { <RateLimitSection/> }.into_any(),
        SettingsSection::System => view! { <SystemSection/> }.into_any(),
    }
}

/// `/admin/settings` 的懒加载路由。
pub struct SettingsRoute;

#[lazy_route]
impl LazyRoute for SettingsRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <SiteSettingsPage/> }.into_any()
    }
}

/// 管理后台设置页面：左侧分区目录，右侧纵向堆叠全部分区，底部浮起保存条。
///
/// 点目录平滑滚动到分区，手动滚动时 scroll-spy 同步目录高亮。各分区的改动由保存条
/// 统一提交，只提交有改动的分区；需要重启的分区在目录与卡片上标出。
#[component]
pub fn SiteSettingsPage() -> impl IntoView {
    let bus = SettingsBus::new();
    provide_context(bus);
    let toast = use_toast();
    let active = RwSignal::new(SettingsSection::Site);
    // scroll-spy 锁定：点击目录后的平滑滚动期间不让 spy 改写高亮，避免途经分区依次闪过。
    // u32 为单调世代号，防止上一次点击的超时回调误清本次的锁。
    #[cfg(target_arch = "wasm32")]
    let spy_lock: RwSignal<Option<(SettingsSection, u32)>> = RwSignal::new(None);
    #[cfg(target_arch = "wasm32")]
    let lock_gen: RwSignal<u32> = RwSignal::new(0);
    // 一次保存涉及的分区：全部提交结束后据此报告结果。
    let submitted = RwSignal::new(None::<Vec<SettingsSection>>);

    Effect::new(move |_| {
        let saving = bus.saving.get();
        if !saving.is_empty() {
            return;
        }
        let Some(done) = submitted.get() else {
            return;
        };
        submitted.set(None);
        if bus.failed.get_untracked().is_empty() {
            let restart = done.iter().any(SettingsSection::needs_restart);
            toast.success(if restart {
                "设置已保存，带「需重启」的部分在重启服务后生效"
            } else {
                "设置已保存"
            });
        }
    });

    let nav_links = SettingsSection::all()
        .into_iter()
        .map(|section| {
            view! {
                <a
                    href=format!("#{}", section.dom_id())
                    aria-current=move || (active.get() == section).then_some("true")
                    data-dirty=move || {
                        bus.dirty.with(|list| list.contains(&section)).then_some("true")
                    }
                    on:click=move |e| {
                        e.prevent_default();
                        active.set(section);
                        #[cfg(target_arch = "wasm32")]
                        {
                            let g = lock_gen.get_untracked().wrapping_add(1);
                            lock_gen.set(g);
                            spy_lock.set(Some((section, g)));
                            scroll_to_section(section);
                            leptos::task::spawn_local(async move {
                                crate::utils::time::sleep_ms(1200).await;
                                if matches!(spy_lock.get_untracked(), Some((_, gg)) if gg == g) {
                                    spy_lock.set(None);
                                }
                            });
                        }
                    }
                >
                    <span>{section.label()}</span>
                    {section
                        .needs_restart()
                        .then(|| view! { <span class="settings-restart-dot" title="修改后需重启"></span> })}
                </a>
            }
        })
        .collect_view();

    let slots = SettingsSection::all()
        .into_iter()
        .map(|section| view! { <div class="settings-slot">{render_section(section)}</div> })
        .collect_view();

    let savebar = move || {
        let dirty = bus.dirty.get();
        let saving = !bus.saving.with(|list| list.is_empty());
        let failed = bus.failed.get();
        (!dirty.is_empty() || saving).then(|| {
            let text = if saving {
                "正在保存…".to_string()
            } else if !failed.is_empty() {
                format!("{} 个分区没有保存成功", failed.len())
            } else {
                format!("有未保存的更改 · {} 个分区", dirty.len())
            };
            let retry = !failed.is_empty();
            view! {
                <div class="settings-savebar" role="status">
                    <span class="settings-savebar-text">
                        <span class="settings-savebar-dot" aria-hidden="true"></span>
                        {text}
                    </span>
                    <button
                        class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                        type="button"
                        disabled=saving
                        on:click=move |_| bus.reset_all()
                    >
                        "放弃"
                    </button>
                    <button
                        class=button_class(ButtonVariant::Primary, ButtonSize::Sm)
                        type="button"
                        disabled=saving
                        on:click=move |_| {
                            submitted.set(Some(bus.dirty.get_untracked()));
                            bus.save_all();
                        }
                    >
                        {if retry { "重试" } else { "保存设置" }}
                    </button>
                </div>
            }
        })
    };

    view! {
        <div
            id="settings-scroll"
            class="settings-page"
            data-vt-scroll="admin-settings"
            on:scroll=move |_| {
                #[cfg(target_arch = "wasm32")]
                {
                    let Some(current) = compute_visible_section() else {
                        return;
                    };
                    if let Some((target, _)) = spy_lock.get_untracked() {
                        if current == target {
                            spy_lock.set(None);
                        }
                        return;
                    }
                    if active.get_untracked() != current {
                        active.set(current);
                    }
                }
            }
        >
            <div class="settings-layout">
                // data-app-anchor：让全局 initAnchorClick 放行，下方 a 的 click 才能执行。
                <nav class="settings-nav" aria-label="设置分区" data-app-anchor="true">
                    {nav_links}
                </nav>
                <div class="settings-main">
                    <AdminPageHead title="设置">
                        "大部分设置保存后立即生效；带 "
                        <span class="settings-restart-dot" aria-label="需重启标记"></span>
                        " 的分区需要重启服务。"
                    </AdminPageHead> {slots} {savebar}
                </div>
            </div>
        </div>
    }
}
