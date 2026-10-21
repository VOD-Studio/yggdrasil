//! 后台布局：桌面应用式外壳。
//!
//! 左侧栏（surface-1）按「内容 · 站点 · 系统」分组，文章、素材、评论右侧显示数量，
//! 当前项为浮起的行与陶土小方点；侧栏可收成 56px 图标栏，拖拽右缘调宽，松手按
//! heavy 弹簧吸附。主画布 surface-0，顶栏是面包屑与「查看前台」。760px 以下侧栏
//! 收进左侧抽屉，顶栏为菜单、页面标题与搜索。样式见 styles/components/admin.css。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::api::admin_stats::AdminNavCounts;
use crate::api::auth::{get_current_user, logout};
use crate::components::admin_palette::AdminPalette;
use crate::components::search_palette::{use_search_palette, use_search_palette_provider};
use crate::components::skeletons::admin_comments_skeleton::AdminCommentsSkeleton;
use crate::components::skeletons::assets_skeleton::AssetsSkeleton;
use crate::components::skeletons::dashboard_skeleton::AdminDashboardSkeleton;
use crate::components::skeletons::friends_admin_skeleton::FriendsAdminSkeleton;
use crate::components::skeletons::logs_skeleton::LogsSkeleton;
use crate::components::skeletons::mcp_skeleton::McpSkeleton;
use crate::components::skeletons::post_preview_skeleton::PostPreviewSkeleton;
use crate::components::skeletons::posts_skeleton::PostsSkeleton;
use crate::components::skeletons::posts_trash_skeleton::PostsTrashSkeleton;
use crate::components::skeletons::profile_skeleton::ProfileSkeleton;
use crate::components::skeletons::runner_skeleton::RunnerSkeleton;
use crate::components::skeletons::settings_admin_skeleton::SettingsAdminSkeleton;
use crate::components::skeletons::system_skeleton::SystemSkeleton;
use crate::components::skeletons::write_skeleton::WriteSkeleton;
use crate::components::ui::{
    Breadcrumb, Crumb, Icon, IconButton, IconName, SheetClose, SideSheet, UserAvatar,
};
use crate::context::UserContext;
use crate::router::Route;
use crate::theme::ThemeToggle;

/// 侧栏默认宽度与可调范围（px）。
const SIDEBAR_DEFAULT: f64 = 232.0;
const SIDEBAR_MIN: f64 = 200.0;
const SIDEBAR_MAX: f64 = 320.0;
/// 图标栏宽度，与 admin.css 的 `--admin-rail` 一致。
const SIDEBAR_RAIL: f64 = 56.0;
/// 拖到这个宽度以下松手收成图标栏。
const SIDEBAR_COLLAPSE_AT: f64 = 152.0;
/// 方向键每次调整的宽度。
const SIDEBAR_KEY_STEP: f64 = 16.0;
/// 拖拽死区：移动不到 4px 算点按。
const DRAG_DEAD_ZONE: f64 = 4.0;

const SIDEBAR_WIDTH_KEY: &str = "yggdrasil-admin-sidebar-width";
const SIDEBAR_COLLAPSED_KEY: &str = "yggdrasil-admin-sidebar-collapsed";

/// 首屏恢复侧栏宽度与折叠状态，避免 WASM 接管前侧栏跳一下。外壳挂载后改由
/// 根元素的 `data-sidebar` 与 `--admin-sidebar-w` 决定。
const SIDEBAR_PRELOAD_SCRIPT: &str = r#"
(function() {
    try {
        var html = document.documentElement;
        var width = parseFloat(localStorage.getItem('yggdrasil-admin-sidebar-width'));
        if (width >= 200 && width <= 320) html.style.setProperty('--admin-sidebar-w', width + 'px');
        if (localStorage.getItem('yggdrasil-admin-sidebar-collapsed') === 'true') {
            html.setAttribute('data-admin-sidebar', 'collapsed');
        }
    } catch (e) {}
})();
"#;

/// 侧栏的宽度与折叠状态。`width` 是展开时的宽度，收起后保留，展开时恢复。
#[derive(Clone, Copy, Debug, PartialEq)]
struct SidebarState {
    width: f64,
    collapsed: bool,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            width: SIDEBAR_DEFAULT,
            collapsed: false,
        }
    }
}

impl SidebarState {
    /// 从存储的原始值恢复；越界或损坏的宽度回到默认值。
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn restore(width: Option<&str>, collapsed: Option<&str>) -> Self {
        let width = width
            .and_then(|w| w.parse::<f64>().ok())
            .filter(|w| (SIDEBAR_MIN..=SIDEBAR_MAX).contains(w))
            .unwrap_or(SIDEBAR_DEFAULT);
        Self {
            width,
            collapsed: collapsed == Some("true"),
        }
    }

    /// 松手吸附：拖到阈值以下收起；阈值与最小宽度之间回弹到最小宽度。
    fn settle(self, dragged: f64) -> Self {
        if dragged < SIDEBAR_COLLAPSE_AT {
            Self {
                collapsed: true,
                ..self
            }
        } else {
            Self {
                width: dragged.clamp(SIDEBAR_MIN, SIDEBAR_MAX),
                collapsed: false,
            }
        }
    }

    /// 方向键调宽：收起时向右展开；最小宽度再向左则收起。
    fn nudge(self, delta: f64) -> Self {
        match (self.collapsed, delta > 0.0) {
            (true, true) => Self {
                collapsed: false,
                ..self
            },
            (true, false) => self,
            (false, false) if self.width <= SIDEBAR_MIN => Self {
                collapsed: true,
                ..self
            },
            (false, _) => Self {
                width: (self.width + delta).clamp(SIDEBAR_MIN, SIDEBAR_MAX),
                collapsed: false,
            },
        }
    }

    /// 侧栏当前显示宽度。
    fn shown(self) -> f64 {
        if self.collapsed {
            SIDEBAR_RAIL
        } else {
            self.width
        }
    }
}

/// 拖拽中的显示宽度：跟手，下限是图标栏宽度。
fn dragged_width(start_width: f64, dx: f64) -> f64 {
    (start_width + dx).clamp(SIDEBAR_RAIL, SIDEBAR_MAX)
}

#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
fn save_sidebar(state: SidebarState) {
    #[cfg(target_arch = "wasm32")]
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(SIDEBAR_WIDTH_KEY, &state.width.round().to_string());
        let _ = storage.set_item(SIDEBAR_COLLAPSED_KEY, &state.collapsed.to_string());
    }
}

fn load_sidebar() -> SidebarState {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
        let read = |key: &str| {
            storage
                .as_ref()
                .and_then(|s| s.get_item(key).ok().flatten())
        };
        SidebarState::restore(
            read(SIDEBAR_WIDTH_KEY).as_deref(),
            read(SIDEBAR_COLLAPSED_KEY).as_deref(),
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (SIDEBAR_WIDTH_KEY, SIDEBAR_COLLAPSED_KEY);
        SidebarState::default()
    }
}

/// 侧栏中的一个栏目。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdminSection {
    Dashboard,
    Posts,
    Notes,
    Assets,
    Comments,
    Friends,
    Mcp,
    Settings,
    System,
    Logs,
    Runner,
}

/// 侧栏分组：分组名与其中的栏目。
const NAV_GROUPS: [(Option<&str>, &[AdminSection]); 4] = [
    (None, &[AdminSection::Dashboard]),
    (
        Some("内容"),
        &[
            AdminSection::Posts,
            AdminSection::Notes,
            AdminSection::Assets,
            AdminSection::Comments,
        ],
    ),
    (
        Some("站点"),
        &[
            AdminSection::Friends,
            AdminSection::Mcp,
            AdminSection::Settings,
        ],
    ),
    (
        Some("系统"),
        &[
            AdminSection::System,
            AdminSection::Logs,
            AdminSection::Runner,
        ],
    ),
];

impl AdminSection {
    pub(crate) fn label(self) -> &'static str {
        match self {
            AdminSection::Dashboard => "仪表盘",
            AdminSection::Posts => "文章",
            AdminSection::Notes => "笔记",
            AdminSection::Assets => "素材",
            AdminSection::Comments => "评论",
            AdminSection::Friends => "友链",
            AdminSection::Mcp => "MCP 接入",
            AdminSection::Settings => "设置",
            AdminSection::System => "系统状态",
            AdminSection::Logs => "日志",
            AdminSection::Runner => "代码运行器",
        }
    }

    pub(crate) fn icon(self) -> IconName {
        match self {
            AdminSection::Dashboard => IconName::LayoutDashboard,
            AdminSection::Posts => IconName::FileText,
            AdminSection::Notes => IconName::NotebookPen,
            AdminSection::Assets => IconName::Image,
            AdminSection::Comments => IconName::MessageSquare,
            AdminSection::Friends => IconName::Link,
            AdminSection::Mcp => IconName::Plug,
            AdminSection::Settings => IconName::SlidersHorizontal,
            AdminSection::System => IconName::Activity,
            AdminSection::Logs => IconName::List,
            AdminSection::Runner => IconName::Terminal,
        }
    }

    pub(crate) fn route(self) -> Route {
        match self {
            AdminSection::Dashboard => Route::Admin {},
            AdminSection::Posts => Route::Posts {},
            AdminSection::Notes => Route::AdminNotes {},
            AdminSection::Assets => Route::Assets {},
            AdminSection::Comments => Route::AdminComments {},
            AdminSection::Friends => Route::FriendsAdmin {},
            AdminSection::Mcp => Route::Mcp {},
            AdminSection::Settings => Route::SiteSettingsPage {},
            AdminSection::System => Route::System {},
            AdminSection::Logs => Route::Logs {},
            AdminSection::Runner => Route::Runner {},
        }
    }

    /// 路由所属栏目：写作、回收站与预览归「文章」，笔记本与笔记编辑归「笔记」。
    /// 个人资料不在侧栏栏目里，由底部用户行标示。
    pub(crate) fn of(route: &Route) -> Option<Self> {
        Some(match route {
            Route::Admin {} => AdminSection::Dashboard,
            Route::Posts {}
            | Route::PostsTrash {}
            | Route::Write {}
            | Route::WriteEdit { .. }
            | Route::PostPreview { .. } => AdminSection::Posts,
            Route::AdminNotes {}
            | Route::AdminNotebooks {}
            | Route::NewNote {}
            | Route::EditNote { .. } => AdminSection::Notes,
            Route::Assets {} => AdminSection::Assets,
            Route::AdminComments {} | Route::AdminCommentsPage { .. } => AdminSection::Comments,
            Route::FriendsAdmin {} => AdminSection::Friends,
            Route::Mcp {} => AdminSection::Mcp,
            Route::SiteSettingsPage {} => AdminSection::Settings,
            Route::System {} => AdminSection::System,
            Route::Logs {} => AdminSection::Logs,
            Route::Runner {} => AdminSection::Runner,
            _ => return None,
        })
    }

    fn group(self) -> &'static str {
        NAV_GROUPS
            .iter()
            .find(|(_, items)| items.contains(&self))
            .and_then(|(group, _)| *group)
            .unwrap_or("")
    }
}

/// 顶栏面包屑与手机顶栏标题。子页面以所属栏目为上一级（可点击），栏目页以分组名
/// 为上一级。
pub(crate) fn admin_crumbs(route: &Route) -> (Vec<Crumb<Route>>, &'static str) {
    let sub = |section: AdminSection, title: &'static str| {
        (
            vec![
                Crumb::link(section.label(), section.route()),
                Crumb::current(title),
            ],
            title,
        )
    };
    match route {
        Route::Admin {} => (vec![Crumb::current("概览")], "仪表盘"),
        Route::PostsTrash {} => sub(AdminSection::Posts, "回收站"),
        Route::Write {} => sub(AdminSection::Posts, "写文章"),
        Route::WriteEdit { .. } => sub(AdminSection::Posts, "编辑文章"),
        Route::PostPreview { .. } => sub(AdminSection::Posts, "预览"),
        Route::AdminNotebooks {} => sub(AdminSection::Notes, "笔记本"),
        Route::NewNote {} => sub(AdminSection::Notes, "新建笔记"),
        Route::EditNote { .. } => sub(AdminSection::Notes, "编辑笔记"),
        Route::Profile {} => (
            vec![Crumb::text("账户"), Crumb::current("个人资料")],
            "个人资料",
        ),
        other => match AdminSection::of(other) {
            Some(section) => (
                vec![
                    Crumb::text(section.group()),
                    Crumb::current(section.label()),
                ],
                section.label(),
            ),
            None => (vec![Crumb::current("后台")], "后台"),
        },
    }
}

/// 侧栏数量文字：评论有待审时显示待审数（陶土强调），否则显示总数。
fn nav_count(section: AdminSection, counts: Option<AdminNavCounts>) -> Option<(String, bool)> {
    let counts = counts?;
    match section {
        AdminSection::Posts => Some((counts.posts.to_string(), false)),
        AdminSection::Assets => Some((counts.assets.to_string(), false)),
        AdminSection::Comments if counts.pending_comments > 0 => {
            Some((format!("{} 待审", counts.pending_comments), true))
        }
        AdminSection::Comments => Some((counts.comments.to_string(), false)),
        _ => None,
    }
}

#[component]
pub fn AdminLayout() -> Element {
    let mut ctx: UserContext = use_context();
    let navigator = dioxus::router::navigator();
    let route = use_route::<Route>();
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
    let mut counts = use_signal(|| None::<AdminNavCounts>);

    use_effect(move || {
        if !(ctx.checked)() {
            (ctx.checked).set(true);
            spawn(async move {
                match get_current_user().await {
                    Ok(response) => {
                        if let Some(user) = response.user {
                            ctx.user.set(Some(std::sync::Arc::new(user)));
                        } else {
                            let _ = navigator.push(Route::Login {});
                        }
                    }
                    Err(_) => {
                        let _ = navigator.push(Route::Login {});
                    }
                }
            });
        }
    });

    let authenticated = matches!(((ctx.checked)(), (ctx.user)()), (true, Some(_)));
    // 登录后与每次换页刷新侧栏数量：增删文章、素材或审核评论后回到列表即可看到新值。
    use_effect(move || {
        let _current = router().current::<Route>();
        let signed_in = matches!(((ctx.checked)(), (ctx.user)()), (true, Some(_)));
        #[cfg(target_arch = "wasm32")]
        if signed_in {
            spawn(async move {
                if let Ok(next) = crate::api::admin_stats::get_admin_nav_counts().await {
                    counts.set(Some(next));
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = signed_in;
    });

    let is_write_route =
        matches!(route, Route::Write {}) || matches!(route, Route::WriteEdit { .. });
    // 写作、设置和日志页面在主内容区内部自行滚动。
    let internal_scroll_route = is_write_route
        || matches!(route, Route::SiteSettingsPage {})
        || matches!(route, Route::Logs {});
    let side_user = (ctx.user)();
    let on_logout = use_callback(move |()| {
        spawn(async move {
            if logout().await.is_ok_and(|response| response.success) {
                crate::bridges::navigation::clear_admin_state();
            }
            ctx.user.set(None);
            ctx.checked.set(false);
            let _ = navigator.push(Route::Login {});
        });
    });
    use_search_palette_provider();
    let user_name = side_user
        .as_ref()
        .map(|user| user.display_label().to_string());
    let avatar_url = side_user.as_ref().and_then(|user| user.avatar_url.clone());
    let sidebar = |drawer: bool| {
        rsx! {
            AdminSidebar {
                route: route.clone(),
                user_name: user_name.clone(),
                avatar_url: avatar_url.clone(),
                counts: counts(),
                on_logout: move |_| on_logout.call(()),
                drawer,
            }
        }
    };
    let skeleton_route = route.clone();
    let content = if authenticated {
        rsx! {
            // 路由挂起时沿用同一布局内的页面骨架。
            SuspenseBoundary {
                fallback: move |_| rsx! {
                    div { class: "flex-1 min-h-0 flex flex-col animate-pulse", {admin_route_skeleton(&skeleton_route)} }
                },
                Outlet::<Route> {}
            }
        }
    } else {
        rsx! {
            div { class: "flex-1 min-h-0 flex flex-col animate-pulse",
                {admin_route_skeleton(&route)}
            }
        }
    };
    let (crumbs, title) = admin_crumbs(&route);
    rsx! {
        AdminShell {
            sidebar: sidebar(false),
            drawer: sidebar(true),
            crumbs,
            title,
            content,
            internal_scroll: internal_scroll_route,
            focus_write: is_write_route,
            checking: !authenticated,
        }
        if authenticated {
            AdminPalette { on_logout: move |_| on_logout.call(()) }
        }
    }
}

/// 后台外壳：侧栏、顶栏与主画布。认证和路由留在 AdminLayout，图鉴只向此壳传入
/// 固定内容；`preview` 时不读写本地存储，也不参与页面转场与首屏恢复。
#[component]
pub(crate) fn AdminShell(
    sidebar: Element,
    drawer: Element,
    crumbs: Vec<Crumb<Route>>,
    title: &'static str,
    content: Element,
    #[props(default)] internal_scroll: bool,
    #[props(default)] focus_write: bool,
    #[props(default)] checking: bool,
    #[props(default)] preview: bool,
    #[props(default)] on_demo_navigate: Option<EventHandler<&'static str>>,
) -> Element {
    // 挂载前为 None：首屏由预载脚本写在 <html> 上的状态决定，服务端与水合输出一致。
    let mut state = use_signal(|| None::<SidebarState>);
    let mut drag = use_signal(|| None::<(f64, f64, f64)>);
    let mut drawer_open = use_signal(|| false);

    use_effect(move || {
        if !preview {
            state.set(Some(load_sidebar()));
        } else {
            state.set(Some(SidebarState::default()));
        }
    });
    let mut commit = move |next: SidebarState| {
        state.set(Some(next));
        if !preview {
            save_sidebar(next);
        }
    };
    use_context_provider(|| SidebarControl {
        state,
        toggle: Callback::new(move |()| {
            let current = state.peek().unwrap_or_default();
            let next = SidebarState {
                collapsed: !current.collapsed,
                ..current
            };
            state.set(Some(next));
            if !preview {
                save_sidebar(next);
            }
        }),
    });

    let current = state();
    let live = drag().map(|(start_x, start_width, x)| dragged_width(start_width, x - start_x));
    let shown = live.or(current.map(SidebarState::shown));
    let data_sidebar = current.map(|s| if s.collapsed { "collapsed" } else { "expanded" });
    let root_style = match (current, shown) {
        (Some(s), Some(w)) => format!(
            "--admin-sidebar-w: {}px; --admin-sidebar-shown: {w}px;",
            s.width
        ),
        _ => String::new(),
    };
    let valuenow = shown.unwrap_or(SIDEBAR_DEFAULT).round();
    let scroll_class = if internal_scroll || checking {
        "admin-scroll admin-scroll-fixed"
    } else {
        "admin-scroll"
    };
    let main_class = if internal_scroll {
        "admin-main admin-main-fill"
    } else {
        "admin-main"
    };
    let menu_id = if preview {
        "showcase-admin-drawer"
    } else {
        "admin-nav-drawer"
    };
    let menu_trigger = format!("{menu_id}-trigger");
    // 图鉴里的外壳在前台布局中，那里的面板句柄属于前台搜索，不接。
    let palette = use_search_palette().filter(|_| on_demo_navigate.is_none());

    rsx! {
        div {
            class: "admin-shell",
            "data-preview": preview.then_some("true"),
            // 写作页自带页头，隐藏后台顶栏。
            "data-focus": focus_write.then_some("write"),
            "data-sidebar": data_sidebar,
            "data-resizing": drag().is_some().then_some("true"),
            style: "{root_style}",
            if !preview {
                script { dangerous_inner_html: "{SIDEBAR_PRELOAD_SCRIPT}" }
            }
            div { class: "admin-side-slot",
                {sidebar}
                div {
                    id: if preview { "showcase-admin-resize" } else { "admin-resize" },
                    class: "admin-resize",
                    role: "separator",
                    tabindex: "0",
                    aria_orientation: "vertical",
                    aria_label: "调整侧栏宽度",
                    aria_valuemin: "{SIDEBAR_RAIL}",
                    aria_valuemax: "{SIDEBAR_MAX}",
                    aria_valuenow: "{valuenow}",
                    title: "拖动调整宽度，双击恢复默认",
                    onpointerdown: move |e| {
                        if e.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) {
                            return;
                        }
                        e.prevent_default();
                        let x = e.client_coordinates().x;
                        let start = state.peek().unwrap_or_default().shown();
                        drag.set(Some((x, start, x)));
                        #[cfg(target_arch = "wasm32")]
                        {
                            let id = if preview { "showcase-admin-resize" } else { "admin-resize" };
                            if let Some(el) = web_sys::window()
                                .and_then(|w| w.document())
                                .and_then(|d| d.get_element_by_id(id))
                            {
                                let _ = el.set_pointer_capture(e.pointer_id());
                            }
                        }
                    },
                    onpointermove: move |e| {
                        if let Some((start_x, start_width, _)) = drag() {
                            drag.set(Some((start_x, start_width, e.client_coordinates().x)));
                        }
                    },
                    onpointerup: move |_| {
                        let Some((start_x, start_width, x)) = drag() else {
                            return;
                        };
                        drag.set(None);
                        if (x - start_x).abs() < DRAG_DEAD_ZONE {
                            return;
                        }
                        let base = state.peek().unwrap_or_default();
                        commit(base.settle(dragged_width(start_width, x - start_x)));
                    },
                    onpointercancel: move |_| drag.set(None),
                    ondoubleclick: move |_| commit(SidebarState::default()),
                    onkeydown: move |e| {
                        let base = state.peek().unwrap_or_default();
                        let next = match e.key() {
                            Key::ArrowLeft => base.nudge(-SIDEBAR_KEY_STEP),
                            Key::ArrowRight => base.nudge(SIDEBAR_KEY_STEP),
                            Key::Enter => SidebarState { collapsed: !base.collapsed, ..base },
                            _ => return,
                        };
                        e.prevent_default();
                        commit(next);
                    },
                }
            }
            div { class: "admin-canvas",
                div {
                    "data-vt-scroll": (!preview).then_some("admin-main"),
                    class: "{scroll_class}",
                    header { class: "admin-topbar",
                        button {
                            id: "{menu_trigger}",
                            class: "admin-topbar-menu touch-icon",
                            r#type: "button",
                            aria_label: "打开后台导航",
                            aria_haspopup: "dialog",
                            aria_expanded: "{drawer_open()}",
                            aria_controls: menu_id,
                            onclick: move |_| drawer_open.set(true),
                            Icon { name: IconName::Menu, size: 19 }
                        }
                        div { class: "admin-topbar-crumbs",
                            if on_demo_navigate.is_some() {
                                nav { class: "breadcrumb", aria_label: "面包屑",
                                    for (i, crumb) in crumbs.iter().enumerate() {
                                        if i > 0 {
                                            span { aria_hidden: "true", "/" }
                                        }
                                        span {
                                            aria_current: (i + 1 == crumbs.len()).then_some("page"),
                                            "{crumb.label}"
                                        }
                                    }
                                }
                            } else {
                                Breadcrumb::<Route> { items: crumbs.clone() }
                            }
                        }
                        span { class: "admin-topbar-title", "{title}" }
                        div { class: "admin-topbar-actions",
                            if let Some(palette) = palette {
                                button {
                                    class: "touch-icon",
                                    r#type: "button",
                                    aria_label: "搜索或执行命令",
                                    aria_haspopup: "dialog",
                                    onclick: move |_| palette.open(),
                                    Icon { name: IconName::Search, size: 19 }
                                }
                            }
                            if on_demo_navigate.is_none() {
                                // admin→前台跨 layout，保留整页加载以避开 Dioxus 0.7.10 的卸载问题。
                                a { class: "btn btn-sm btn-ghost admin-topbar-site", href: "/",
                                    "查看前台"
                                    Icon { name: IconName::ArrowUpRight, size: 14 }
                                }
                            }
                        }
                    }
                    main { class: "{main_class}", {content} }
                }
            }
        }
        SideSheet {
            open: drawer_open,
            id: menu_id,
            trigger_id: menu_trigger.clone(),
            label: "后台导航",
            {drawer}
        }
    }
}

/// 侧栏折叠状态与切换，由 [`AdminShell`] 提供给侧栏头部的收起按钮。
#[derive(Clone, Copy)]
struct SidebarControl {
    state: Signal<Option<SidebarState>>,
    toggle: Callback<()>,
}

/// 正式后台与图鉴共享侧栏结构；图鉴导航通过回调更新局部选中项。
///
/// Props：
/// - `counts`：文章、素材、评论的数量；未加载时不显示
/// - `drawer`：放在手机抽屉里（始终展开、点选后关闭抽屉、没有收起按钮）
/// - `demo_active` / `on_demo_navigate`：图鉴用局部选中项与导航回调
#[component]
pub(crate) fn AdminSidebar(
    route: Route,
    user_name: Option<String>,
    avatar_url: Option<String>,
    on_logout: EventHandler<()>,
    #[props(default)] counts: Option<AdminNavCounts>,
    #[props(default)] drawer: bool,
    #[props(default)] demo_active: Option<String>,
    #[props(default)] on_demo_navigate: Option<EventHandler<&'static str>>,
) -> Element {
    let control = try_use_context::<SidebarControl>();
    let collapsed = !drawer
        && control
            .and_then(|c| (c.state)())
            .is_some_and(|s| s.collapsed);
    let sheet_close = try_use_context::<SheetClose>().filter(|_| drawer);
    let active_section = match &demo_active {
        Some(label) => NAV_GROUPS
            .iter()
            .flat_map(|(_, items)| items.iter().copied())
            .find(|section| section.label() == label),
        None => AdminSection::of(&route),
    };
    let profile_active = demo_active
        .as_deref()
        .map_or(matches!(route, Route::Profile {}), |label| {
            label == "个人资料"
        });
    let after_pick = move || {
        if let Some(close) = sheet_close {
            close.close();
        }
    };
    let preview = on_demo_navigate.is_some();
    let palette = use_search_palette().filter(|_| !preview);
    // 图标栏里按钮贴左缘，提示朝右展开以免被裁。
    let toggle_align = if collapsed { "start" } else { "end" };

    rsx! {
        aside {
            "data-vt-shell": (!drawer && !preview).then_some("admin-sidebar"),
            class: if drawer { "admin-side admin-side-drawer" } else { "admin-side" },
            aria_label: "后台导航",
            div { class: "admin-side-head",
                if let Some(on_navigate) = on_demo_navigate {
                    button {
                        class: "wordmark",
                        r#type: "button",
                        onclick: move |_| on_navigate.call("仪表盘"),
                        span { class: "wordmark-mark", aria_hidden: "true" }
                        span { class: "admin-side-text", "Yggdrasil" }
                    }
                } else {
                    // admin→前台跨 layout，保留整页加载以避开 Dioxus 0.7.10 的卸载问题。
                    a { class: "wordmark", href: "/", aria_label: "Yggdrasil 首页",
                        span { class: "wordmark-mark", aria_hidden: "true" }
                        span { class: "admin-side-text", "Yggdrasil" }
                    }
                }
                span { class: "admin-side-badge admin-side-text", "后台" }
                if drawer {
                    IconButton {
                        icon: IconName::X,
                        label: "关闭导航",
                        tooltip_align: "end",
                        onclick: move |_| after_pick(),
                    }
                } else if let Some(control) = control {
                    IconButton {
                        icon: IconName::PanelLeft,
                        label: if collapsed { "展开侧栏" } else { "收起侧栏" },
                        pressed: Some(collapsed),
                        class: "admin-side-toggle",
                        tooltip_align: toggle_align,
                        tooltip_placement: "bottom",
                        onclick: move |_| control.toggle.call(()),
                    }
                }
            }
            if let Some(palette) = palette {
                button {
                    class: "admin-search",
                    r#type: "button",
                    aria_haspopup: "dialog",
                    "data-tip": "搜索或执行命令 ⌘K",
                    onclick: move |_| {
                        after_pick();
                        palette.open();
                    },
                    Icon { name: IconName::Search, size: 15 }
                    span { class: "admin-search-text", "搜索或执行命令" }
                    kbd { class: "admin-search-kbd", "⌘K" }
                }
            }
            nav { class: "admin-nav", aria_label: "后台栏目",
                for (group , items) in NAV_GROUPS {
                    div { key: "{group.unwrap_or(\"\")}", class: "admin-nav-group",
                        if let Some(group) = group {
                            p { class: "admin-nav-label", "{group}" }
                        }
                        for section in items.iter().copied() {
                            {
                                let active = active_section == Some(section);
                                let count = nav_count(section, counts);
                                let body = rsx! {
                                    span { class: "admin-nav-dot", aria_hidden: "true" }
                                    Icon { name: section.icon(), size: 17, class: "admin-nav-icon" }
                                    span { class: "admin-nav-text", "{section.label()}" }
                                    if let Some((count, alert)) = count {
                                        span {
                                            class: if alert { "admin-nav-count admin-nav-count-alert" } else { "admin-nav-count" },
                                            "{count}"
                                        }
                                    }
                                };
                                if let Some(on_navigate) = on_demo_navigate {
                                    rsx! {
                                        button {
                                            key: "{section.label()}",
                                            class: "admin-nav-item",
                                            r#type: "button",
                                            "data-tip": section.label(),
                                            aria_current: active.then_some("page"),
                                            onclick: move |_| {
                                                on_navigate.call(section.label());
                                                after_pick();
                                            },
                                            {body}
                                        }
                                    }
                                } else {
                                    rsx! {
                                        Link {
                                            key: "{section.label()}",
                                            class: "admin-nav-item",
                                            to: section.route(),
                                            "data-tip": section.label(),
                                            aria_current: active.then_some("page"),
                                            onclick: move |_| after_pick(),
                                            {body}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "admin-side-foot",
                div { class: "admin-side-theme", ThemeToggle { labels: drawer } }
                div { class: "admin-user",
                    if let Some(name) = user_name {
                        {
                            let body = rsx! {
                                UserAvatar {
                                    name: name.clone(),
                                    avatar_url: avatar_url.clone(),
                                    class: "w-7 h-7 rounded-full text-xs flex-shrink-0",
                                }
                                span { class: "admin-user-text",
                                    span { class: "admin-user-name", "{name}" }
                                    span { class: "admin-user-role", "个人资料" }
                                }
                            };
                            if let Some(on_navigate) = on_demo_navigate {
                                rsx! {
                                    button {
                                        class: "admin-user-link",
                                        r#type: "button",
                                        "data-tip": "个人资料",
                                        aria_current: profile_active.then_some("page"),
                                        onclick: move |_| {
                                            on_navigate.call("个人资料");
                                            after_pick();
                                        },
                                        {body}
                                    }
                                }
                            } else {
                                rsx! {
                                    Link {
                                        class: "admin-user-link",
                                        to: Route::Profile {},
                                        "data-tip": "个人资料",
                                        aria_current: profile_active.then_some("page"),
                                        onclick: move |_| after_pick(),
                                        {body}
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "admin-user-link", aria_hidden: "true",
                            span { class: "w-7 h-7 rounded-full bg-ink-2 animate-pulse flex-shrink-0" }
                            span { class: "admin-user-text",
                                span { class: "h-3 w-16 rounded bg-ink-2 animate-pulse" }
                            }
                        }
                    }
                    IconButton {
                        icon: IconName::LogOut,
                        label: "退出登录",
                        size: crate::components::ui::ButtonSize::Sm,
                        class: "admin-user-logout",
                        tooltip_align: "end",
                        onclick: move |_| on_logout.call(()),
                    }
                }
            }
        }
    }
}

/// 根据当前后台路由，渲染对应的专属骨架屏。
fn admin_route_skeleton(route: &Route) -> Element {
    match route {
        Route::Admin {} => rsx! {
            AdminDashboardSkeleton {}
        },
        Route::Write {} | Route::WriteEdit { .. } => rsx! {
            WriteSkeleton {}
        },
        Route::Posts {} => rsx! {
            PostsSkeleton {}
        },
        Route::AdminNotes {}
        | Route::AdminNotebooks {}
        | Route::NewNote {}
        | Route::EditNote { .. } => rsx! {
            crate::pages::notes::NotesSkeleton {}
        },
        Route::PostsTrash {} => rsx! {
            PostsTrashSkeleton {}
        },
        Route::Assets {} => rsx! {
            AssetsSkeleton {}
        },
        Route::FriendsAdmin {} => rsx! {
            FriendsAdminSkeleton {}
        },
        Route::AdminComments {} | Route::AdminCommentsPage { .. } => {
            rsx! {
                AdminCommentsSkeleton {}
            }
        }
        Route::System {} => rsx! {
            SystemSkeleton {}
        },
        Route::Runner {} => rsx! {
            RunnerSkeleton {}
        },
        Route::Mcp {} => rsx! {
            McpSkeleton {}
        },
        Route::Logs {} => rsx! {
            LogsSkeleton {}
        },
        Route::SiteSettingsPage {} => rsx! {
            SettingsAdminSkeleton {}
        },
        Route::Profile {} => rsx! {
            ProfileSkeleton {}
        },
        Route::PostPreview { .. } => rsx! {
            PostPreviewSkeleton {}
        },
        _ => rsx! {
            AdminDashboardSkeleton {}
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_restore_rejects_out_of_range_width() {
        let state = SidebarState::restore(Some("280"), Some("true"));
        assert_eq!(state.width, 280.0);
        assert!(state.collapsed);
        assert_eq!(
            SidebarState::restore(Some("40"), None).width,
            SIDEBAR_DEFAULT
        );
        assert_eq!(
            SidebarState::restore(Some("abc"), None).width,
            SIDEBAR_DEFAULT
        );
        assert!(!SidebarState::restore(None, Some("1")).collapsed);
    }

    #[test]
    fn sidebar_settle_snaps_to_rail_or_minimum() {
        let base = SidebarState::default();
        assert!(base.settle(120.0).collapsed);
        // 收起时保留展开宽度，再展开回到原宽度。
        assert_eq!(base.settle(120.0).width, SIDEBAR_DEFAULT);
        let snapped = base.settle(170.0);
        assert!(!snapped.collapsed);
        assert_eq!(snapped.width, SIDEBAR_MIN);
        assert_eq!(base.settle(400.0).width, SIDEBAR_MAX);
        assert_eq!(base.settle(260.0).width, 260.0);
    }

    #[test]
    fn sidebar_nudge_collapses_below_minimum_and_expands_from_rail() {
        let narrow = SidebarState {
            width: SIDEBAR_MIN,
            collapsed: false,
        };
        assert!(narrow.nudge(-SIDEBAR_KEY_STEP).collapsed);
        let rail = SidebarState {
            width: 240.0,
            collapsed: true,
        };
        assert_eq!(rail.nudge(-SIDEBAR_KEY_STEP), rail);
        let expanded = rail.nudge(SIDEBAR_KEY_STEP);
        assert!(!expanded.collapsed);
        assert_eq!(expanded.width, 240.0);
        assert_eq!(expanded.nudge(SIDEBAR_KEY_STEP).width, 256.0);
    }

    #[test]
    fn dragged_width_follows_pointer_within_rail_and_maximum() {
        assert_eq!(dragged_width(232.0, -300.0), SIDEBAR_RAIL);
        assert_eq!(dragged_width(232.0, 40.0), 272.0);
        assert_eq!(dragged_width(232.0, 400.0), SIDEBAR_MAX);
    }

    #[test]
    fn subpages_belong_to_their_section() {
        assert_eq!(
            AdminSection::of(&Route::PostsTrash {}),
            Some(AdminSection::Posts)
        );
        assert_eq!(
            AdminSection::of(&Route::EditNote { id: 1 }),
            Some(AdminSection::Notes)
        );
        assert_eq!(AdminSection::of(&Route::Profile {}), None);
        let (crumbs, title) = admin_crumbs(&Route::AdminNotebooks {});
        assert_eq!(title, "笔记本");
        assert_eq!(crumbs[0].to, Some(Route::AdminNotes {}));
        let (crumbs, title) = admin_crumbs(&Route::Logs {});
        assert_eq!(title, "日志");
        assert_eq!(crumbs[0].label, "系统");
        assert_eq!(crumbs[0].to, None);
    }

    #[test]
    fn comment_count_prefers_pending() {
        let mut counts = AdminNavCounts {
            posts: 41,
            assets: 37,
            comments: 8,
            pending_comments: 0,
        };
        assert_eq!(
            nav_count(AdminSection::Comments, Some(counts)),
            Some(("8".to_string(), false))
        );
        counts.pending_comments = 2;
        assert_eq!(
            nav_count(AdminSection::Comments, Some(counts)),
            Some(("2 待审".to_string(), true))
        );
        assert_eq!(nav_count(AdminSection::Notes, Some(counts)), None);
        assert_eq!(nav_count(AdminSection::Posts, None), None);
    }
}
