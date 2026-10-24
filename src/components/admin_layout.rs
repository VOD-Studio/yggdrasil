//! 后台布局：桌面应用式外壳。
//!
//! 左侧栏（surface-1）按「内容 · 站点 · 系统」分组，文章、素材、评论右侧显示数量，
//! 当前项为浮起的行与陶土小方点；侧栏可收成 56px 图标栏，拖拽右缘调宽，松手按
//! heavy 弹簧吸附。主画布 surface-0，顶栏是面包屑与「查看前台」。767px 以下侧栏
//! 收进左侧抽屉，顶栏为菜单、页面标题与搜索。样式见 styles/components/admin.css。
//!
//! 外壳在懒加载的父路由 `/admin` 里只构建一次（Leptos 组件体只运行一次），切换后台页面时
//! 侧栏与顶栏保持同一批 DOM 节点；随路由变化的部分（激活栏目、面包屑、骨架屏）都从
//! `use_location()` 的路径派生。

use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::Outlet;
use leptos_router::hooks::use_location;

use crate::api::admin_stats::{get_admin_nav_counts, AdminNavCounts};
use crate::api::auth::{get_current_user, logout};
use crate::components::admin_palette::AdminPalette;
use crate::components::palette::{use_search_palette, use_search_palette_provider};
use crate::components::skeletons::admin_comments_skeleton::AdminCommentsSkeleton;
use crate::components::skeletons::assets_skeleton::AssetsSkeleton;
use crate::components::skeletons::dashboard_skeleton::AdminDashboardSkeleton;
use crate::components::skeletons::friends_admin_skeleton::FriendsAdminSkeleton;
use crate::components::skeletons::logs_skeleton::LogsSkeleton;
use crate::components::skeletons::mcp_skeleton::McpSkeleton;
use crate::components::skeletons::notes_admin_skeleton::{
    AdminNotebooksSkeleton, AdminNotesSkeleton,
};
use crate::components::skeletons::post_preview_skeleton::PostPreviewSkeleton;
use crate::components::skeletons::posts_skeleton::PostsSkeleton;
use crate::components::skeletons::posts_trash_skeleton::PostsTrashSkeleton;
use crate::components::skeletons::profile_skeleton::ProfileSkeleton;
use crate::components::skeletons::runner_skeleton::RunnerSkeleton;
use crate::components::skeletons::settings_admin_skeleton::SettingsAdminSkeleton;
use crate::components::skeletons::system_skeleton::SystemSkeleton;
use crate::components::skeletons::write_skeleton::WriteSkeleton;
use crate::components::ui::{
    Breadcrumb, ButtonSize, Crumb, Icon, IconButton, IconName, SheetClose, SideSheet, UserAvatar,
};
use crate::context::UserContext;
use crate::routes::Route;
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

/// 后台页面种类：由 `/admin` 下的路径字符串分类，替代原先对 `Route` 枚举的匹配。
///
/// 侧栏激活项、面包屑、页面标题与骨架屏都由它决定。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminPage {
    /// 仪表盘（`/admin`）。
    Dashboard,
    /// 文章管理。
    Posts,
    /// 文章回收站。
    PostsTrash,
    /// 写文章。
    Write,
    /// 编辑文章。
    WriteEdit,
    /// 文章预览。
    PostPreview,
    /// 笔记管理。
    AdminNotes,
    /// 新建笔记。
    NewNote,
    /// 编辑笔记。
    EditNote,
    /// 笔记本管理。
    AdminNotebooks,
    /// 素材管理。
    Assets,
    /// 友链管理。
    FriendsAdmin,
    /// 评论管理（含分页）。
    AdminComments,
    /// 系统状态。
    System,
    /// 代码运行器。
    Runner,
    /// MCP 接入。
    Mcp,
    /// 运行日志。
    Logs,
    /// 站点设置。
    Settings,
    /// 个人资料。
    Profile,
    /// 其余路径（未匹配的后台地址）。
    Other,
}

impl AdminPage {
    /// 按路径（`use_location().pathname`，不含查询串）分类；末尾斜杠忽略。
    pub fn from_path(path: &str) -> Self {
        let path = path.trim_end_matches('/');
        let Some(rest) = path.strip_prefix("/admin") else {
            return AdminPage::Other;
        };
        let mut segments = rest.split('/').skip(1);
        let first = segments.next();
        let second = segments.next();
        let third = segments.next();
        match (first, second, third, segments.next()) {
            (None, ..) if rest.is_empty() => AdminPage::Dashboard,
            (Some("write"), None, ..) => AdminPage::Write,
            (Some("write"), Some(_), None, _) => AdminPage::WriteEdit,
            (Some("preview"), Some(_), None, _) => AdminPage::PostPreview,
            (Some("posts"), None, ..) => AdminPage::Posts,
            (Some("posts"), Some("trash"), None, _) => AdminPage::PostsTrash,
            (Some("notes"), None, ..) => AdminPage::AdminNotes,
            (Some("notes"), Some("new"), None, _) => AdminPage::NewNote,
            (Some("notes"), Some("edit"), Some(_), None) => AdminPage::EditNote,
            (Some("notebooks"), None, ..) => AdminPage::AdminNotebooks,
            (Some("assets"), None, ..) => AdminPage::Assets,
            (Some("friends"), None, ..) => AdminPage::FriendsAdmin,
            (Some("comments"), None, ..) => AdminPage::AdminComments,
            (Some("comments"), Some(_), None, _) => AdminPage::AdminComments,
            (Some("system"), None, ..) => AdminPage::System,
            (Some("runner"), None, ..) => AdminPage::Runner,
            (Some("mcp"), None, ..) => AdminPage::Mcp,
            (Some("logs"), None, ..) => AdminPage::Logs,
            (Some("settings"), None, ..) => AdminPage::Settings,
            (Some("profile"), None, ..) => AdminPage::Profile,
            _ => AdminPage::Other,
        }
    }

    /// 写文章与笔记编辑器自带页头。
    fn is_write(self) -> bool {
        matches!(
            self,
            AdminPage::Write | AdminPage::WriteEdit | AdminPage::NewNote | AdminPage::EditNote
        )
    }

    /// 写作、设置和日志页面在主内容区内部自行滚动。
    fn scrolls_internally(self) -> bool {
        self.is_write() || matches!(self, AdminPage::Settings | AdminPage::Logs)
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
            AdminSection::Dashboard => Route::Admin,
            AdminSection::Posts => Route::Posts,
            AdminSection::Notes => Route::AdminNotes,
            AdminSection::Assets => Route::Assets,
            AdminSection::Comments => Route::AdminComments,
            AdminSection::Friends => Route::FriendsAdmin,
            AdminSection::Mcp => Route::Mcp,
            AdminSection::Settings => Route::SiteSettingsPage,
            AdminSection::System => Route::System,
            AdminSection::Logs => Route::Logs,
            AdminSection::Runner => Route::Runner,
        }
    }

    /// 页面所属栏目：写作、回收站与预览归「文章」，笔记本与笔记编辑归「笔记」。
    /// 个人资料不在侧栏栏目里，由底部用户行标示。
    pub(crate) fn of(page: AdminPage) -> Option<Self> {
        Some(match page {
            AdminPage::Dashboard => AdminSection::Dashboard,
            AdminPage::Posts
            | AdminPage::PostsTrash
            | AdminPage::Write
            | AdminPage::WriteEdit
            | AdminPage::PostPreview => AdminSection::Posts,
            AdminPage::AdminNotes
            | AdminPage::AdminNotebooks
            | AdminPage::NewNote
            | AdminPage::EditNote => AdminSection::Notes,
            AdminPage::Assets => AdminSection::Assets,
            AdminPage::AdminComments => AdminSection::Comments,
            AdminPage::FriendsAdmin => AdminSection::Friends,
            AdminPage::Mcp => AdminSection::Mcp,
            AdminPage::Settings => AdminSection::Settings,
            AdminPage::System => AdminSection::System,
            AdminPage::Logs => AdminSection::Logs,
            AdminPage::Runner => AdminSection::Runner,
            AdminPage::Profile | AdminPage::Other => return None,
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
pub(crate) fn admin_crumbs(page: AdminPage) -> (Vec<Crumb>, &'static str) {
    let sub = |section: AdminSection, title: &'static str| {
        (
            vec![
                Crumb::link(section.label(), section.route()),
                Crumb::current(title),
            ],
            title,
        )
    };
    match page {
        AdminPage::Dashboard => (vec![Crumb::current("概览")], "仪表盘"),
        AdminPage::PostsTrash => sub(AdminSection::Posts, "回收站"),
        AdminPage::Write => sub(AdminSection::Posts, "写文章"),
        AdminPage::WriteEdit => sub(AdminSection::Posts, "编辑文章"),
        AdminPage::PostPreview => sub(AdminSection::Posts, "预览"),
        AdminPage::AdminNotebooks => sub(AdminSection::Notes, "笔记本"),
        AdminPage::NewNote => sub(AdminSection::Notes, "新建笔记"),
        AdminPage::EditNote => sub(AdminSection::Notes, "编辑笔记"),
        AdminPage::Profile => (
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

/// 后台布局：校验登录态，提供侧栏/操作上下文，渲染外壳与嵌套路由出口。
///
/// 登录态校验期与路由挂起期沿用同一布局内的页面骨架屏；校验未通过时
/// `navigation::replace("/login")`。作为 `/admin` 懒加载父路由的视图使用，
/// 子路由内容由 `<Outlet/>` 渲染。
#[component]
pub fn AdminLayout() -> impl IntoView {
    let ctx = expect_context::<UserContext>();
    let pathname = use_location().pathname;
    let page = Memo::new(move |_| AdminPage::from_path(&pathname.read()));
    let counts = RwSignal::new(None::<AdminNavCounts>);

    // 进入后台时校验一次会话（水合后才会执行）；未登录或请求失败回登录页。
    Effect::new(move |_| {
        if ctx.checked.get_untracked() {
            return;
        }
        ctx.checked.set(true);
        spawn_local(async move {
            let signed_in = match get_current_user().await {
                Ok(response) => match response.user {
                    Some(user) => {
                        let _ = ctx.user.try_set(Some(Arc::new(user)));
                        true
                    }
                    None => false,
                },
                Err(_) => false,
            };
            if !signed_in {
                crate::navigation::replace("/login");
            }
        });
    });

    let signed_in = Memo::new(move |_| ctx.checked.get() && ctx.user.read().is_some());

    // 登录后与每次换页刷新侧栏数量：增删文章、素材或审核评论后回到列表即可看到新值。
    // 世代号丢弃过期响应。
    let count_generation = StoredValue::new(0_u64);
    Effect::new(move |_| {
        let _ = pathname.get();
        if !signed_in.get() {
            return;
        }
        let mut id = 0;
        count_generation.update_value(|g| {
            *g += 1;
            id = *g;
        });
        spawn_local(async move {
            if let Ok(next) = get_admin_nav_counts().await {
                if count_generation.try_get_value() == Some(id) {
                    let _ = counts.try_set(Some(next));
                }
            }
        });
    });

    let is_write = Memo::new(move |_| page.get().is_write());
    let internal_scroll = Memo::new(move |_| page.get().scrolls_internally());

    let on_logout = Callback::new(move |()| {
        spawn_local(async move {
            if logout().await.is_ok_and(|response| response.success) {
                crate::bridges::navigation::clear_admin_state();
            }
            let _ = ctx.user.try_set(None);
            let _ = ctx.checked.try_set(false);
            crate::navigation::push("/login");
        });
    });
    provide_context(AdminActions { logout: on_logout });
    use_search_palette_provider();

    let user_name = Memo::new(move |_| {
        ctx.user
            .read()
            .as_ref()
            .map(|user| user.display_label().to_string())
    });
    let avatar_url = Memo::new(move |_| {
        ctx.user
            .read()
            .as_ref()
            .and_then(|user| user.avatar_url.clone())
    });

    // 路由挂起或登录态校验期的页面骨架。
    let skeleton = move || {
        view! {
            <div class="flex-1 min-h-0 flex flex-col animate-pulse">
                {admin_route_skeleton(page.get())}
            </div>
        }
    };
    let content = move || {
        move || {
            if signed_in.get() {
                view! {
                    // 路由挂起时沿用同一布局内的页面骨架。
                    <Suspense fallback=skeleton>
                        <Outlet/>
                    </Suspense>
                }
                .into_any()
            } else {
                skeleton().into_any()
            }
        }
    };

    let crumbs = Signal::derive(move || admin_crumbs(page.get()).0);
    let title = Signal::derive(move || admin_crumbs(page.get()).1);

    view! {
        <AdminShell
            sidebar=move || {
                view! {
                    <AdminSidebar
                        page=page
                        user_name=user_name
                        avatar_url=avatar_url
                        counts=counts
                        on_logout=on_logout
                    />
                }
            }
            drawer=move || {
                view! {
                    <AdminSidebar
                        page=page
                        user_name=user_name
                        avatar_url=avatar_url
                        counts=counts
                        on_logout=on_logout
                        drawer=true
                    />
                }
            }
            crumbs=crumbs
            title=title
            content=content
            internal_scroll=internal_scroll
            focus_write=is_write
            checking=Signal::derive(move || !signed_in.get())
        />
        <Show when=move || signed_in.get()>
            <AdminPalette on_logout=on_logout/>
        </Show>
    }
}

/// 后台页面共用的操作（如个人资料页的「退出登录」），由 AdminLayout 提供。
#[derive(Clone, Copy)]
pub struct AdminActions {
    /// 退出当前设备并回到登录页。
    pub logout: Callback<()>,
}

// 页面迁移期间尚无调用方：模块级 allow 压住 props 结构体的 dead_code，页面合入后连同
// 模块包装与下面的 re-export allow 一并删除。
#[allow(dead_code)]
mod page_head {
    use leptos::prelude::*;

    /// 后台页面标题行：左侧 h1 与副标题，右侧可选操作区。样式见 admin.css `.admin-page-head`。
    ///
    /// - `title`：页面标题（手机宽度隐藏，顶栏已显示）
    /// - `class`：附加类，如日志页的 `logs-head`
    /// - `actions`：右侧内容（传闭包），统一包进 `.admin-page-actions`
    /// - `children`：副标题内容；为空时 `p` 由 CSS `min-height: 1lh` 保留一行高度
    #[component]
    pub(crate) fn AdminPageHead(
        #[prop(into)] title: Signal<String>,
        #[prop(optional)] class: Option<&'static str>,
        #[prop(optional, into)] actions: Option<ViewFn>,
        #[prop(optional)] children: Option<Children>,
    ) -> impl IntoView {
        let extra = class.unwrap_or_default();
        view! {
            <div class=format!("admin-page-head {extra}")>
                <div>
                    <h1>{move || title.get()}</h1>
                    <p>{children.map(|children| children())}</p>
                </div>
                {actions.map(|actions| view! { <div class="admin-page-actions">{actions.run()}</div> })}
            </div>
        }
    }
}

#[allow(unused_imports)]
pub(crate) use page_head::AdminPageHead;

/// 后台外壳：侧栏、顶栏与主画布。认证和路由留在 [`AdminLayout`]。
///
/// Props：
/// - `sidebar` / `drawer`：桌面侧栏与手机抽屉里的导航（抽屉每次打开重新创建）
/// - `crumbs` / `title`：顶栏面包屑与手机顶栏标题
/// - `content`：主画布内容（传返回响应式视图的闭包以随登录态切换）
/// - `internal_scroll`：页面在主内容区内部自行滚动
/// - `focus_write`：写作页自带页头，隐藏后台顶栏
/// - `checking`：登录态校验中，主画布不滚动
#[component]
pub(crate) fn AdminShell(
    #[prop(into)] sidebar: ViewFn,
    #[prop(into)] drawer: ViewFn,
    #[prop(into)] crumbs: Signal<Vec<Crumb>>,
    #[prop(into)] title: Signal<&'static str>,
    #[prop(into)] content: ViewFn,
    #[prop(optional, into)] internal_scroll: Signal<bool>,
    #[prop(optional, into)] focus_write: Signal<bool>,
    #[prop(optional, into)] checking: Signal<bool>,
) -> impl IntoView {
    // 挂载前为 None：首屏由预载脚本写在 <html> 上的状态决定，服务端与水合输出一致。
    let state = RwSignal::new(None::<SidebarState>);
    let drag = RwSignal::new(None::<(f64, f64, f64)>);
    let drawer_open = RwSignal::new(false);
    let resize_ref = NodeRef::<leptos::html::Div>::new();

    Effect::new(move |_| {
        state.set(Some(load_sidebar()));
    });
    let commit = move |next: SidebarState| {
        state.set(Some(next));
        save_sidebar(next);
    };
    provide_context(SidebarControl {
        state,
        toggle: Callback::new(move |()| {
            let current = state.get_untracked().unwrap_or_default();
            commit(SidebarState {
                collapsed: !current.collapsed,
                ..current
            });
        }),
    });

    let shown = move || {
        drag.get()
            .map(|(start_x, start_width, x)| dragged_width(start_width, x - start_x))
            .or(state.get().map(SidebarState::shown))
    };
    let data_sidebar = move || {
        state
            .get()
            .map(|s| if s.collapsed { "collapsed" } else { "expanded" })
    };
    let root_style = move || match (state.get(), shown()) {
        (Some(s), Some(w)) => format!(
            "--admin-sidebar-w: {}px; --admin-sidebar-shown: {w}px;",
            s.width
        ),
        _ => String::new(),
    };
    let valuenow = move || shown().unwrap_or(SIDEBAR_DEFAULT).round().to_string();
    let scroll_class = move || {
        if internal_scroll.get() || checking.get() {
            "admin-scroll admin-scroll-fixed"
        } else {
            "admin-scroll"
        }
    };
    let main_class = move || {
        if internal_scroll.get() {
            "admin-main admin-main-fill"
        } else {
            "admin-main"
        }
    };
    let menu_id = "admin-nav-drawer";
    let menu_trigger = format!("{menu_id}-trigger");
    let palette = use_search_palette();

    view! {
        <div
            class="admin-shell"
            // 写作页自带页头，隐藏后台顶栏。
            data-focus=move || focus_write.get().then_some("write")
            data-sidebar=data_sidebar
            data-resizing=move || drag.get().is_some().then_some("true")
            style=root_style
        >
            <script inner_html=SIDEBAR_PRELOAD_SCRIPT></script>
            <div class="admin-side-slot">
                {sidebar.run()}
                <div
                    id="admin-resize"
                    node_ref=resize_ref
                    class="admin-resize"
                    role="separator"
                    tabindex="0"
                    aria-orientation="vertical"
                    aria-label="调整侧栏宽度"
                    aria-valuemin=SIDEBAR_RAIL.to_string()
                    aria-valuemax=SIDEBAR_MAX.to_string()
                    aria-valuenow=valuenow
                    title="拖动调整宽度，双击恢复默认"
                    on:pointerdown=move |e| {
                        if e.button() != 0 {
                            return;
                        }
                        e.prevent_default();
                        let x = f64::from(e.client_x());
                        let start = state.get_untracked().unwrap_or_default().shown();
                        drag.set(Some((x, start, x)));
                        #[cfg(target_arch = "wasm32")]
                        if let Some(el) = resize_ref.get_untracked() {
                            let _ = el.set_pointer_capture(e.pointer_id());
                        }
                    }
                    on:pointermove=move |e| {
                        if let Some((start_x, start_width, _)) = drag.get_untracked() {
                            drag.set(Some((start_x, start_width, f64::from(e.client_x()))));
                        }
                    }
                    on:pointerup=move |_| {
                        let Some((start_x, start_width, x)) = drag.get_untracked() else {
                            return;
                        };
                        drag.set(None);
                        if (x - start_x).abs() < DRAG_DEAD_ZONE {
                            return;
                        }
                        let base = state.get_untracked().unwrap_or_default();
                        commit(base.settle(dragged_width(start_width, x - start_x)));
                    }
                    on:pointercancel=move |_| drag.set(None)
                    on:dblclick=move |_| commit(SidebarState::default())
                    on:keydown=move |e| {
                        let base = state.get_untracked().unwrap_or_default();
                        let next = match e.key().as_str() {
                            "ArrowLeft" => base.nudge(-SIDEBAR_KEY_STEP),
                            "ArrowRight" => base.nudge(SIDEBAR_KEY_STEP),
                            "Enter" => SidebarState {
                                collapsed: !base.collapsed,
                                ..base
                            },
                            _ => return,
                        };
                        e.prevent_default();
                        commit(next);
                    }
                ></div>
            </div>
            <div class="admin-canvas">
                <div data-vt-scroll="admin-main" class=scroll_class>
                    <header class="admin-topbar">
                        <button
                            id=menu_trigger.clone()
                            class="admin-topbar-menu touch-icon"
                            type="button"
                            aria-label="打开后台导航"
                            aria-haspopup="dialog"
                            aria-expanded=move || drawer_open.get().to_string()
                            aria-controls=menu_id
                            on:click=move |_| drawer_open.set(true)
                        >
                            <Icon name=IconName::Menu size=19/>
                        </button>
                        <div class="admin-topbar-crumbs">
                            <Breadcrumb items=crumbs/>
                        </div>
                        <span class="admin-topbar-title">{move || title.get()}</span>
                        <div class="admin-topbar-actions">
                            {palette
                                .map(|palette| {
                                    view! {
                                        <button
                                            class="touch-icon"
                                            type="button"
                                            aria-label="搜索或执行命令"
                                            aria-haspopup="dialog"
                                            on:click=move |_| palette.open()
                                        >
                                            <Icon name=IconName::Search size=19/>
                                        </button>
                                    }
                                })}
                            <a class="btn btn-sm btn-ghost admin-topbar-site" href="/">
                                "查看前台"
                                <Icon name=IconName::ArrowUpRight size=14/>
                            </a>
                        </div>
                    </header>
                    <main class=main_class>{content.run()}</main>
                </div>
            </div>
        </div>
        <SideSheet open=drawer_open id=menu_id trigger_id=menu_trigger label="后台导航">
            {drawer.run()}
        </SideSheet>
    }
}

/// 侧栏折叠状态与切换，由 [`AdminShell`] 提供给侧栏头部的收起按钮。
#[derive(Clone, Copy)]
struct SidebarControl {
    state: RwSignal<Option<SidebarState>>,
    toggle: Callback<()>,
}

/// 后台侧栏：栏目导航、搜索入口、主题切换与用户区。
///
/// Props：
/// - `page`：当前后台页面（决定激活栏目）
/// - `user_name` / `avatar_url`：底部用户行；未登录或未校验时显示占位
/// - `counts`：文章、素材、评论的数量；未加载时不显示
/// - `drawer`：放在手机抽屉里（始终展开、点选后关闭抽屉、没有收起按钮）
#[component]
pub(crate) fn AdminSidebar(
    #[prop(into)] page: Signal<AdminPage>,
    #[prop(into)] user_name: Signal<Option<String>>,
    #[prop(into)] avatar_url: Signal<Option<String>>,
    #[prop(into)] on_logout: Callback<()>,
    #[prop(into)] counts: Signal<Option<AdminNavCounts>>,
    #[prop(optional)] drawer: bool,
) -> impl IntoView {
    let control = use_context::<SidebarControl>();
    let collapsed = Signal::derive(move || {
        !drawer
            && control
                .and_then(|c| c.state.get())
                .is_some_and(|s| s.collapsed)
    });
    let sheet_close = use_context::<SheetClose>().filter(|_| drawer);
    let active_section = Memo::new(move |_| AdminSection::of(page.get()));
    let profile_active = Memo::new(move |_| page.get() == AdminPage::Profile);
    let after_pick = move || {
        if let Some(close) = sheet_close {
            close.close();
        }
    };
    let palette = use_search_palette();
    // 图标栏里按钮贴左缘，提示朝右展开以免被裁。
    let toggle_align =
        Signal::derive(move || if collapsed.get() { "start" } else { "end" }.to_string());

    let head_button = if drawer {
        view! {
            <IconButton
                icon=IconName::X
                label="关闭导航"
                tooltip_align="end"
                onclick=move |_| after_pick()
            />
        }
        .into_any()
    } else if let Some(control) = control {
        view! {
            <IconButton
                icon=IconName::PanelLeft
                label=Signal::derive(move || {
                    if collapsed.get() { "展开侧栏" } else { "收起侧栏" }.to_string()
                })
                pressed=Signal::derive(move || Some(collapsed.get()))
                class="admin-side-toggle"
                tooltip_align=toggle_align
                tooltip_placement="bottom"
                onclick=move |_| control.toggle.run(())
            />
        }
        .into_any()
    } else {
        ().into_any()
    };

    let search_button = palette.map(|palette| {
        view! {
            <button
                class="admin-search"
                type="button"
                aria-haspopup="dialog"
                data-tip="搜索或执行命令 ⌘K"
                on:click=move |_| {
                    after_pick();
                    palette.open();
                }
            >
                <Icon name=IconName::Search size=15/>
                <span class="admin-search-text">"搜索或执行命令"</span>
                <kbd class="admin-search-kbd">"⌘K"</kbd>
            </button>
        }
    });

    let groups = NAV_GROUPS
        .into_iter()
        .map(|(group, items)| {
            let label = group.map(|group| view! { <p class="admin-nav-label">{group}</p> });
            let items = items
                .iter()
                .copied()
                .map(|section| {
                    view! {
                        <a
                            class="admin-nav-item"
                            href=section.route().to_string()
                            data-tip=section.label()
                            aria-current=move || {
                                (active_section.get() == Some(section)).then_some("page")
                            }
                            on:click=move |_| after_pick()
                        >
                            <span class="admin-nav-dot" aria-hidden="true"></span>
                            <Icon name=section.icon() size=17 class="admin-nav-icon"/>
                            <span class="admin-nav-text">{section.label()}</span>
                            {move || {
                                nav_count(section, counts.get())
                                    .map(|(count, alert)| {
                                        view! {
                                            <span class=if alert {
                                                "admin-nav-count admin-nav-count-alert"
                                            } else {
                                                "admin-nav-count"
                                            }>{count}</span>
                                        }
                                    })
                            }}
                        </a>
                    }
                })
                .collect_view();
            view! {
                <div class="admin-nav-group">
                    {label} {items}
                </div>
            }
        })
        .collect_view();

    view! {
        <aside
            data-vt-shell=(!drawer).then_some("admin-sidebar")
            class=if drawer { "admin-side admin-side-drawer" } else { "admin-side" }
            aria-label="后台导航"
        >
            <div class="admin-side-head">
                <a class="wordmark" href="/" aria-label="Yggdrasil 首页">
                    <span class="wordmark-mark" aria-hidden="true"></span>
                    <span class="admin-side-text">"Yggdrasil"</span>
                </a>
                <span class="admin-side-badge admin-side-text">"后台"</span>
                {head_button}
            </div>
            {search_button}
            <nav class="admin-nav" aria-label="后台栏目">
                {groups}
            </nav>
            <div class="admin-side-foot">
                <div class="admin-side-theme">
                    <ThemeToggle/>
                </div>
                <div class="admin-user">
                    {move || match user_name.get() {
                        Some(name) => {
                            view! {
                                <a
                                    class="admin-user-link"
                                    href=Route::Profile.to_string()
                                    data-tip="个人资料"
                                    aria-current=move || profile_active.get().then_some("page")
                                    on:click=move |_| after_pick()
                                >
                                    <UserAvatar
                                        name=name.clone()
                                        avatar_url=avatar_url
                                        class="w-7 h-7 rounded-full text-xs flex-shrink-0"
                                    />
                                    <span class="admin-user-text">
                                        <span class="admin-user-name">{name}</span>
                                        <span class="admin-user-role">"个人资料"</span>
                                    </span>
                                </a>
                            }
                                .into_any()
                        }
                        None => {
                            view! {
                                <div class="admin-user-link" aria-hidden="true">
                                    <span class="w-7 h-7 rounded-full bg-ink-2 animate-pulse flex-shrink-0"></span>
                                    <span class="admin-user-text">
                                        <span class="h-3 w-16 rounded bg-ink-2 animate-pulse"></span>
                                    </span>
                                </div>
                            }
                                .into_any()
                        }
                    }}
                    <IconButton
                        icon=IconName::LogOut
                        label="退出登录"
                        size=ButtonSize::Sm
                        class="admin-user-logout"
                        tooltip_align="end"
                        onclick=move |_| on_logout.run(())
                    />
                </div>
            </div>
        </aside>
    }
}

/// 根据当前后台页面，渲染对应的专属骨架屏。
fn admin_route_skeleton(page: AdminPage) -> AnyView {
    match page {
        AdminPage::Dashboard | AdminPage::Other => view! { <AdminDashboardSkeleton/> }.into_any(),
        AdminPage::Write | AdminPage::WriteEdit | AdminPage::NewNote | AdminPage::EditNote => {
            view! { <WriteSkeleton/> }.into_any()
        }
        AdminPage::Posts => view! { <PostsSkeleton/> }.into_any(),
        AdminPage::AdminNotes => view! { <AdminNotesSkeleton/> }.into_any(),
        AdminPage::AdminNotebooks => view! { <AdminNotebooksSkeleton/> }.into_any(),
        AdminPage::PostsTrash => view! { <PostsTrashSkeleton/> }.into_any(),
        AdminPage::Assets => view! { <AssetsSkeleton/> }.into_any(),
        AdminPage::FriendsAdmin => view! { <FriendsAdminSkeleton/> }.into_any(),
        AdminPage::AdminComments => view! { <AdminCommentsSkeleton/> }.into_any(),
        AdminPage::System => view! { <SystemSkeleton/> }.into_any(),
        AdminPage::Runner => view! { <RunnerSkeleton/> }.into_any(),
        AdminPage::Mcp => view! { <McpSkeleton/> }.into_any(),
        AdminPage::Logs => view! { <LogsSkeleton/> }.into_any(),
        AdminPage::Settings => view! { <SettingsAdminSkeleton/> }.into_any(),
        AdminPage::Profile => view! { <ProfileSkeleton/> }.into_any(),
        AdminPage::PostPreview => view! { <PostPreviewSkeleton/> }.into_any(),
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
    fn page_from_path_matches_every_admin_route() {
        let cases = [
            (Route::Admin, AdminPage::Dashboard),
            (Route::Write, AdminPage::Write),
            (Route::WriteEdit { id: 7 }, AdminPage::WriteEdit),
            (
                Route::PostPreview {
                    slug: "a b".to_string(),
                },
                AdminPage::PostPreview,
            ),
            (Route::Posts, AdminPage::Posts),
            (Route::PostsTrash, AdminPage::PostsTrash),
            (Route::AdminNotes, AdminPage::AdminNotes),
            (Route::NewNote, AdminPage::NewNote),
            (Route::EditNote { id: 3 }, AdminPage::EditNote),
            (Route::AdminNotebooks, AdminPage::AdminNotebooks),
            (Route::Assets, AdminPage::Assets),
            (Route::FriendsAdmin, AdminPage::FriendsAdmin),
            (Route::AdminComments, AdminPage::AdminComments),
            (
                Route::AdminCommentsPage { page: 2 },
                AdminPage::AdminComments,
            ),
            (Route::System, AdminPage::System),
            (Route::Runner, AdminPage::Runner),
            (Route::Mcp, AdminPage::Mcp),
            (Route::Logs, AdminPage::Logs),
            (Route::SiteSettingsPage, AdminPage::Settings),
            (Route::Profile, AdminPage::Profile),
        ];
        for (route, expected) in cases {
            assert_eq!(
                AdminPage::from_path(&route.to_string()),
                expected,
                "{route}"
            );
        }
        // 末尾斜杠与未知路径。
        assert_eq!(AdminPage::from_path("/admin/"), AdminPage::Dashboard);
        assert_eq!(AdminPage::from_path("/admin/posts/"), AdminPage::Posts);
        assert_eq!(AdminPage::from_path("/admin/nope"), AdminPage::Other);
        assert_eq!(AdminPage::from_path("/admin/posts/1/2"), AdminPage::Other);
        assert_eq!(AdminPage::from_path("/administrator"), AdminPage::Other);
        assert_eq!(AdminPage::from_path("/login"), AdminPage::Other);
    }

    #[test]
    fn scroll_and_write_flags_follow_page() {
        assert!(AdminPage::Write.is_write());
        assert!(AdminPage::EditNote.is_write());
        assert!(!AdminPage::Posts.is_write());
        assert!(AdminPage::Settings.scrolls_internally());
        assert!(AdminPage::Logs.scrolls_internally());
        assert!(AdminPage::NewNote.scrolls_internally());
        assert!(!AdminPage::Dashboard.scrolls_internally());
    }

    #[test]
    fn subpages_belong_to_their_section() {
        assert_eq!(
            AdminSection::of(AdminPage::PostsTrash),
            Some(AdminSection::Posts)
        );
        assert_eq!(
            AdminSection::of(AdminPage::EditNote),
            Some(AdminSection::Notes)
        );
        assert_eq!(AdminSection::of(AdminPage::Profile), None);
        let (crumbs, title) = admin_crumbs(AdminPage::AdminNotebooks);
        assert_eq!(title, "笔记本");
        assert_eq!(crumbs[0].to, Some(Route::AdminNotes.to_string()));
        let (crumbs, title) = admin_crumbs(AdminPage::Logs);
        assert_eq!(title, "日志");
        assert_eq!(crumbs[0].label, "系统");
        assert_eq!(crumbs[0].to, None);
        let (crumbs, title) = admin_crumbs(AdminPage::Other);
        assert_eq!(title, "后台");
        assert_eq!(crumbs.len(), 1);
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

    #[test]
    fn page_head_renders_title_subtitle_and_actions() {
        let html = Owner::new().with(|| {
            view! {
                <AdminPageHead
                    title="日志"
                    class="logs-head"
                    actions=|| view! { <button>"刷新"</button> }
                >
                    "运行日志"
                </AdminPageHead>
            }
            .to_html()
        });
        assert!(
            html.contains("class=\"admin-page-head logs-head\""),
            "{html}"
        );
        assert!(html.contains("日志"), "{html}");
        assert!(html.contains("运行日志"), "{html}");
        assert!(html.contains("class=\"admin-page-actions\""), "{html}");
        let bare = Owner::new().with(|| view! { <AdminPageHead title="x"/> }.to_html());
        assert!(bare.contains("class=\"admin-page-head\""), "{bare}");
        assert!(!bare.contains("admin-page-actions"), "{bare}");
    }
}
