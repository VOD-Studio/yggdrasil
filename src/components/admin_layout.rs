//! 后台管理布局组件
//!
//! 提供全新设计的柔和/软扁平化风格的管理员专属后台布局。
//! 采用圆角矩形、大空间距与友好的交互设计。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::api::auth::{get_current_user, logout};
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
use crate::components::ui::UserAvatar;
use crate::context::UserContext;
use crate::router::Route;
use crate::theme::ThemeToggle;

#[component]
pub fn AdminLayout() -> Element {
    let mut ctx: UserContext = use_context();
    let navigator = dioxus::router::navigator();
    let route = use_route::<Route>();

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

    let is_write_route =
        matches!(route, Route::Write {}) || matches!(route, Route::WriteEdit { .. });
    // 写作、设置和日志页面在主内容区内部自行滚动。
    let internal_scroll_route = is_write_route
        || matches!(route, Route::SiteSettingsPage {})
        || matches!(route, Route::Logs {});
    let side_user = (ctx.user)();
    let sidebar = rsx! {
        AdminSidebar {
            route: route.clone(),
            user_name: side_user.as_ref().map(|user| user.display_label().to_string()),
            avatar_url: side_user.as_ref().and_then(|user| user.avatar_url.clone()),
            on_logout: move |_| {
                spawn(async move {
                    if logout().await.is_ok_and(|response| response.success) {
                        crate::bridges::navigation::clear_admin_state();
                    }
                    ctx.user.set(None);
                    ctx.checked.set(false);
                    let _ = navigator.push(Route::Login {});
                });
            },
        }
    };
    let authenticated = matches!(((ctx.checked)(), (ctx.user)()), (true, Some(_)));
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
    rsx! {
        AdminShell {
            sidebar,
            content,
            internal_scroll: internal_scroll_route,
            checking: !authenticated,
        }
    }
}

/// 认证和路由留在 AdminLayout，图鉴只向此壳传入固定内容。
#[component]
pub(crate) fn AdminShell(
    sidebar: Element,
    content: Element,
    #[props(default)] internal_scroll: bool,
    #[props(default)] checking: bool,
    #[props(default)] preview: bool,
) -> Element {
    let root_class = if preview {
        "showcase-admin-shell flex bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] font-sans"
    } else {
        "min-h-dvh flex bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] font-sans"
    };
    let height_class = if preview { "h-full" } else { "h-screen" };
    let card_overflow = if internal_scroll || checking {
        "overflow-hidden"
    } else {
        "overflow-y-auto"
    };
    let main_class = if internal_scroll {
        "flex-1 w-full max-w-7xl mx-auto flex flex-col min-h-0"
    } else {
        "flex-1 w-full max-w-7xl mx-auto px-6 py-12"
    };
    rsx! {
        div { class: "{root_class}",
            {sidebar}
            div { class: "flex-1 flex flex-col min-w-0 {height_class} p-2 md:p-4",
                div {
                    "data-vt-scroll": (!preview).then_some("admin-main"),
                    class: "flex-1 bg-[var(--color-paper-theme)] rounded-[2rem] shadow-sm border border-[var(--color-paper-border)] {card_overflow} relative flex flex-col",
                    main { class: "{main_class}", {content} }
                }
            }
        }
    }
}

/// 正式后台与图鉴共享侧栏结构；图鉴导航通过回调更新局部选中项。
#[component]
pub(crate) fn AdminSidebar(
    route: Route,
    user_name: Option<String>,
    avatar_url: Option<String>,
    on_logout: EventHandler<()>,
    #[props(default)] demo_active: Option<String>,
    #[props(default)] on_demo_navigate: Option<EventHandler<&'static str>>,
    #[props(default)] preview: bool,
) -> Element {
    let nav_items_top = vec![(Route::Admin {}, "仪表盘"), (Route::Write {}, "写文章")];
    let nav_items_bottom = vec![(Route::Assets {}, "素材"), (Route::FriendsAdmin {}, "友链")];
    let is_write_route =
        matches!(route, Route::Write {}) || matches!(route, Route::WriteEdit { .. });
    let is_profile_route = demo_active
        .as_deref()
        .map_or(matches!(route, Route::Profile {}), |active| {
            active == "个人信息"
        });
    let chip_state_class = if is_profile_route {
        "bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] shadow-sm border border-[var(--color-paper-border)]"
    } else {
        "text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-theme)]/50 hover:text-[var(--color-paper-primary)] border border-transparent"
    };
    let aside_class = if preview {
        "w-48 flex-shrink-0 flex flex-col h-full sticky top-0 p-3 bg-[var(--color-paper-entry)]"
    } else {
        "w-48 flex-shrink-0 hidden md:flex flex-col h-screen sticky top-0 p-3 bg-[var(--color-paper-entry)]"
    };
    rsx! {
        aside { "data-vt-shell": (!preview).then_some("admin-sidebar"), class: "{aside_class}",
            div { class: "mb-8 px-3",
                if let Some(on_navigate) = on_demo_navigate {
                    button {
                        class: "font-extrabold text-2xl tracking-tight text-[var(--color-paper-primary)] hover:text-[var(--color-paper-accent)] transition-colors",
                        r#type: "button",
                        onclick: move |_| on_navigate.call("仪表盘"),
                        "Yggdrasil."
                    }
                } else {
                    // admin→前台跨 layout，保留整页加载以避开 Dioxus 0.7.10 的卸载问题。
                    Link {
                        class: "font-extrabold text-2xl tracking-tight text-[var(--color-paper-primary)] hover:text-[var(--color-paper-accent)] transition-colors",
                        to: NavigationTarget::<Route>::External("/".to_string()),
                        "Yggdrasil."
                    }
                }
            }
            nav { class: "flex-1 flex flex-col gap-2",
                for (dest, label) in nav_items_top {
                    {nav_item(&route, dest, label, is_write_route, demo_active.as_deref(), on_demo_navigate)}
                }
                ContentNavGroup { demo_active: demo_active.clone(), on_demo_navigate }
                for (dest, label) in nav_items_bottom {
                    {nav_item(&route, dest, label, is_write_route, demo_active.as_deref(), on_demo_navigate)}
                }
                ToolsNavGroup { demo_active: demo_active.clone(), on_demo_navigate }
            }
            div { class: "mt-auto pt-4 border-t border-[var(--color-paper-border)] flex flex-col gap-1",
                if let Some(name) = user_name {
                    if let Some(on_navigate) = on_demo_navigate {
                        button {
                            class: "flex items-center gap-2.5 px-3 py-2 rounded-2xl text-sm font-medium transition-all {chip_state_class}",
                            r#type: "button",
                            onclick: move |_| on_navigate.call("个人信息"),
                            UserAvatar {
                                name: name.clone(),
                                avatar_url: avatar_url.clone(),
                                class: "w-7 h-7 rounded-full text-xs flex-shrink-0 border border-[var(--color-paper-border)]",
                            }
                            span { class: "truncate", "{name}" }
                        }
                    } else {
                        Link {
                            class: "flex items-center gap-2.5 px-3 py-2 rounded-2xl text-sm font-medium transition-all {chip_state_class}",
                            to: Route::Profile {},
                            UserAvatar {
                                name: name.clone(),
                                avatar_url: avatar_url.clone(),
                                class: "w-7 h-7 rounded-full text-xs flex-shrink-0 border border-[var(--color-paper-border)]",
                            }
                            span { class: "truncate", "{name}" }
                        }
                    }
                } else {
                    div { class: "flex items-center gap-2.5 px-3 py-2",
                        div { class: "w-7 h-7 rounded-full bg-[var(--color-paper-theme)] animate-pulse flex-shrink-0" }
                        div { class: "h-3.5 w-16 rounded bg-[var(--color-paper-theme)] animate-pulse" }
                    }
                }
                div { class: "flex items-center justify-between px-3",
                    ThemeToggle {}
                    button {
                        class: "text-sm font-medium px-3 py-1.5 rounded-2xl bg-[var(--color-paper-theme)] border border-[var(--color-paper-border)] shadow-sm hover:shadow-md transition-all text-[var(--color-paper-secondary)] hover:text-red-500 cursor-pointer",
                        r#type: "button",
                        onclick: move |_| on_logout.call(()),
                        "退出"
                    }
                }
            }
        }
    }
}

fn nav_item(
    route: &Route,
    dest: Route,
    label: &'static str,
    is_write_route: bool,
    demo_active: Option<&str>,
    on_demo_navigate: Option<EventHandler<&'static str>>,
) -> Element {
    let is_active = demo_active.map_or(
        *route == dest || (label == "写文章" && is_write_route),
        |active| active == label,
    );
    let base_class = "flex items-center px-3 py-2.5 rounded-2xl text-sm font-medium transition-all";
    let text_class = if is_active {
        "bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] shadow-sm border border-[var(--color-paper-border)]"
    } else {
        "text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-theme)]/50 hover:text-[var(--color-paper-primary)] border border-transparent"
    };
    rsx! {
        if let Some(on_navigate) = on_demo_navigate {
            button { key: "{label}", class: "{base_class} {text_class}", r#type: "button", onclick: move |_| on_navigate.call(label), "{label}" }
        } else {
            Link { key: "{label}", class: "{base_class} {text_class}", to: dest, "{label}" }
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

/// 侧栏子菜单组的分类标识：内容管理组与工具组结构完全一致（父项整行点击
/// 切换展开/收起，chevron 旋转 + grid-template-rows 0fr↔1fr 过渡动画，复用
/// posts_trash.rs AutoPurgeSettings 的既有模式），仅路由集合与标签不同，故
/// 抽成单一 NavGroup 组件、以枚举区分（而非函数指针 props：函数指针参与
/// derive(PartialEq) 比较会触发 rustc 的
/// `unpredictable_function_pointer_comparisons` lint）。
#[derive(Clone, Copy, PartialEq)]
enum NavGroupKind {
    /// 「内容管理」：全部文章 / 回收站 / 评论管理。
    Content,
    /// 「工具」：设置 / 试运行 / MCP / 日志 / 系统（issue #26）。
    Tools,
}

impl NavGroupKind {
    fn label(self) -> &'static str {
        match self {
            NavGroupKind::Content => "内容管理",
            NavGroupKind::Tools => "工具",
        }
    }

    /// 判断路由是否属于本组（回收站/评论分页路由一并归入内容管理组）。
    fn contains(self, route: &Route) -> bool {
        match self {
            NavGroupKind::Content => matches!(
                route,
                Route::Posts {}
                    | Route::AdminNotes {}
                    | Route::AdminNotebooks {}
                    | Route::NewNote {}
                    | Route::EditNote { .. }
                    | Route::PostsTrash {}
                    | Route::AdminComments {}
                    | Route::AdminCommentsPage { .. }
            ),
            NavGroupKind::Tools => matches!(
                route,
                Route::Runner {}
                    | Route::Mcp {}
                    | Route::System {}
                    | Route::SiteSettingsPage {}
                    | Route::Logs {}
            ),
        }
    }
}

/// 侧栏可折叠子菜单组的共享实现。`kind` 选择路由归属判定与标签，`items` 提供
/// 子项列表（路由 / 标签 / 是否激活，由调用方基于当前路由预先算好）。
///
/// 当前路由落在组内时自动展开，保证激活子项始终可见；用户手动收起后，
/// 仅当再次从组外导航进入组内路由时才重新展开——而非组内路由间导航
/// （如评论分页）也强制重新展开。
#[component]
fn NavGroup(
    kind: NavGroupKind,
    items: Vec<(Route, &'static str, bool)>,
    #[props(default)] on_demo_navigate: Option<EventHandler<&'static str>>,
) -> Element {
    let route = use_route::<Route>();
    let group_active = if on_demo_navigate.is_some() {
        items.iter().any(|(_, _, active)| *active)
    } else {
        kind.contains(&route)
    };
    let mut expanded = use_signal(|| group_active);
    // 记录上一次 effect 运行时路由是否在组内，用于判断"从组外→组内"的跳变
    // （而非"当前在组内"这一恒真条件），避免组内路由间导航（如评论分页）
    // 反复触发强制展开——这才是本组件文档承诺的行为。
    let mut was_in_group = use_signal(|| group_active);

    // 闭包内读 router().current 建立 ReactiveContext 订阅（仓库约定 #5），
    // 路由变化时本 effect 重跑。
    use_effect(move || {
        if on_demo_navigate.is_some() {
            return;
        }
        let current = router().current::<Route>();
        let now_in_group = kind.contains(&current);
        let was = was_in_group();

        if now_in_group && !was {
            expanded.set(true);
        }
        if was != now_in_group {
            was_in_group.set(now_in_group);
        }
    });

    let chevron_rotate = if expanded() { "rotate-180" } else { "" };
    // 父项样式：与顶层导航项同盒模型；组内路由激活时仅提为 primary 文字色，
    // 不给自己加 pill（pill 高亮由激活子项承担，避免双层高亮竞争）。
    let parent_text_class = if group_active {
        "text-[var(--color-paper-primary)] border border-transparent"
    } else {
        "text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-theme)]/50 hover:text-[var(--color-paper-primary)] border border-transparent"
    };
    let label = kind.label();

    rsx! {
        div { class: "flex flex-col gap-1",
            button {
                class: "flex items-center justify-between w-full px-3 py-2.5 rounded-2xl text-sm font-medium transition-all cursor-pointer {parent_text_class}",
                r#type: "button",
                aria_expanded: expanded(),
                onclick: move |_| expanded.set(!expanded()),
                span { "{label}" }
                svg {
                    class: "w-4 h-4 transition-transform duration-200 flex-shrink-0 {chevron_rotate}",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M19 9l-7 7-7-7",
                    }
                }
            }
            // 展开动画容器：与 AutoPurgeSettings 完全同款（grid 0fr↔1fr + 内层 overflow-hidden）。
            div {
                class: "grid transition-all duration-300 ease-in-out",
                style: if expanded() { "grid-template-rows: 1fr; opacity: 1; pointer-events: auto;" } else { "grid-template-rows: 0fr; opacity: 0; pointer-events: none;" },
                div { class: "overflow-hidden min-h-0",
                    // 左侧竖线引导 + 缩进表示层级。
                    div { class: "ml-3 pl-2.5 border-l border-[var(--color-paper-border)] flex flex-col gap-1",
                        for (dest, item_label, active) in items {
                            if let Some(on_navigate) = on_demo_navigate {
                                button {
                                    key: "{item_label}",
                                    class: if active { "flex items-center px-2.5 py-1.5 rounded-xl text-sm font-medium transition-all bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] shadow-sm border border-[var(--color-paper-border)]" } else { "flex items-center px-2.5 py-1.5 rounded-xl text-sm font-medium transition-all text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-theme)]/50 hover:text-[var(--color-paper-primary)] border border-transparent" },
                                    r#type: "button",
                                    onclick: move |_| on_navigate.call(item_label),
                                    "{item_label}"
                                }
                            } else {
                                Link {
                                    key: "{item_label}",
                                    class: if active { "flex items-center px-2.5 py-1.5 rounded-xl text-sm font-medium transition-all bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] shadow-sm border border-[var(--color-paper-border)]" } else { "flex items-center px-2.5 py-1.5 rounded-xl text-sm font-medium transition-all text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-theme)]/50 hover:text-[var(--color-paper-primary)] border border-transparent" },
                                    to: dest,
                                    "{item_label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 「内容管理」子菜单组：全部文章 / 回收站 / 评论管理（issue #17）。NavGroup
/// 的薄封装，只提供本组的子项列表。
#[component]
fn ContentNavGroup(
    #[props(default)] demo_active: Option<String>,
    #[props(default)] on_demo_navigate: Option<EventHandler<&'static str>>,
) -> Element {
    let route = use_route::<Route>();
    let active = |label: &str, route_active: bool| {
        demo_active
            .as_ref()
            .map_or(route_active, |name| name == label)
    };
    let items = vec![
        (
            Route::AdminNotes {},
            "笔记",
            active(
                "笔记",
                matches!(
                    route,
                    Route::AdminNotes {} | Route::NewNote {} | Route::EditNote { .. }
                ),
            ),
        ),
        (
            Route::AdminNotebooks {},
            "笔记本",
            active("笔记本", matches!(route, Route::AdminNotebooks {})),
        ),
        (
            Route::Posts {},
            "全部文章",
            active("全部文章", matches!(route, Route::Posts {})),
        ),
        (
            Route::PostsTrash {},
            "回收站",
            active("回收站", matches!(route, Route::PostsTrash {})),
        ),
        (
            Route::AdminComments {},
            "评论管理",
            active(
                "评论管理",
                matches!(
                    route,
                    Route::AdminComments {} | Route::AdminCommentsPage { .. }
                ),
            ),
        ),
    ];
    rsx! {
        NavGroup { kind: NavGroupKind::Content, items, on_demo_navigate }
    }
}

/// 「工具」子菜单组：设置 / 试运行 / MCP / 日志 / 系统（issue #26）。NavGroup
/// 的薄封装，只提供本组的子项列表。
#[component]
fn ToolsNavGroup(
    #[props(default)] demo_active: Option<String>,
    #[props(default)] on_demo_navigate: Option<EventHandler<&'static str>>,
) -> Element {
    let route = use_route::<Route>();
    let active = |label: &str, route_active: bool| {
        demo_active
            .as_ref()
            .map_or(route_active, |name| name == label)
    };
    let items = vec![
        (
            Route::SiteSettingsPage {},
            "设置",
            active("设置", matches!(route, Route::SiteSettingsPage {})),
        ),
        (
            Route::Runner {},
            "试运行",
            active("试运行", matches!(route, Route::Runner {})),
        ),
        (
            Route::Mcp {},
            "MCP",
            active("MCP", matches!(route, Route::Mcp {})),
        ),
        (
            Route::Logs {},
            "日志",
            active("日志", matches!(route, Route::Logs {})),
        ),
        (
            Route::System {},
            "系统",
            active("系统", matches!(route, Route::System {})),
        ),
    ];
    rsx! {
        NavGroup { kind: NavGroupKind::Tools, items, on_demo_navigate }
    }
}
