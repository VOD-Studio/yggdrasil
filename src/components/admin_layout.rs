//! 后台管理布局组件
//!
//! 提供全新设计的柔和/软扁平化风格的管理员专属后台布局。
//! 采用圆角矩形、大空间距与友好的交互设计。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::api::auth::{get_current_user, logout};
use crate::components::admin_skeleton::AdminDashboardSkeleton;
use crate::components::skeletons::admin_comments_skeleton::AdminCommentsSkeleton;
use crate::components::skeletons::assets_skeleton::AssetsSkeleton;

use crate::components::skeletons::friends_admin_skeleton::FriendsAdminSkeleton;
use crate::components::skeletons::mcp_skeleton::McpSkeleton;
use crate::components::skeletons::post_preview_skeleton::PostPreviewSkeleton;
use crate::components::skeletons::posts_skeleton::PostsSkeleton;
use crate::components::skeletons::posts_trash_skeleton::PostsTrashSkeleton;
use crate::components::skeletons::runner_skeleton::RunnerSkeleton;
use crate::components::skeletons::settings_admin_skeleton::SettingsAdminSkeleton;
use crate::components::skeletons::system_skeleton::SystemSkeleton;
use crate::components::write_skeleton::WriteSkeleton;
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

    let nav_items_top = vec![(Route::Admin {}, "仪表盘"), (Route::Write {}, "写文章")];
    let nav_items_bottom = vec![(Route::Assets {}, "素材"), (Route::FriendsAdmin {}, "友链")];

    let is_write_route =
        matches!(route, Route::Write {}) || matches!(route, Route::WriteEdit { .. });

    // 所有 admin 页面共用同一 shell:外层圆角卡片(滚动容器) + 内部 main 负责居中限宽。
    // write 路由例外:卡片不滚动(overflow-hidden),main 作为 flex 容器不带头尾 padding,
    // 由 write 页面自身组织 [内容区 flex-1 overflow-y-auto] + [底栏 flex-shrink-0] 的分区布局,
    // 这样底栏永远贴卡片底部不随内容滚动,也不会出现 sticky + 负 margin 的跳动。
    let card_overflow = if is_write_route {
        "overflow-hidden"
    } else {
        "overflow-y-auto"
    };
    let main_class = if is_write_route {
        "flex-1 w-full max-w-7xl mx-auto flex flex-col min-h-0"
    } else {
        "flex-1 w-full max-w-7xl mx-auto px-6 py-12"
    };

    let root_class =
        "min-h-dvh flex bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] font-sans";

    let nav_content = rsx! {
        aside { class: "w-64 flex-shrink-0 hidden md:flex flex-col h-screen sticky top-0 p-4 bg-[var(--color-paper-entry)]",
            // Logo
            div { class: "mb-10 px-4",
                Link {
                    class: "font-extrabold text-2xl tracking-tight text-[var(--color-paper-primary)] hover:text-[var(--color-paper-accent)] transition-colors",
                    to: Route::Home {},
                    "Yggdrasil."
                }
            }
            // Nav Items
            nav { class: "flex-1 flex flex-col gap-2",
                for (dest, label) in nav_items_top {
                    {
                        let is_active = route == dest || (label == "写文章" && is_write_route);
                        let base_class = "flex items-center px-4 py-3 rounded-2xl text-sm font-medium transition-all";
                        let text_class = if is_active {
                            "bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] shadow-sm border border-[var(--color-paper-border)]"
                        } else {
                            "text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-theme)]/50 hover:text-[var(--color-paper-primary)] border border-transparent"
                        };
                        rsx! {
                            Link { key: "{label}", class: "{base_class} {text_class}", to: dest, "{label}" }
                        }
                    }
                }
                // 「内容管理」子菜单：全部文章 / 回收站 / 评论管理（issue #17）。
                ContentNavGroup {}
                for (dest, label) in nav_items_bottom {
                    {
                        let is_active = route == dest || (label == "写文章" && is_write_route);
                        let base_class = "flex items-center px-4 py-3 rounded-2xl text-sm font-medium transition-all";
                        let text_class = if is_active {
                            "bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] shadow-sm border border-[var(--color-paper-border)]"
                        } else {
                            "text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-theme)]/50 hover:text-[var(--color-paper-primary)] border border-transparent"
                        };
                        rsx! {
                            Link { key: "{label}", class: "{base_class} {text_class}", to: dest, "{label}" }
                        }
                    }
                }
                ToolsNavGroup {}
            }
            // Bottom Tools
            div { class: "mt-auto pt-6 border-t border-[var(--color-paper-border)] flex items-center justify-between px-4",
                ThemeToggle {}
                button {
                    class: "text-sm font-medium px-4 py-2 rounded-2xl bg-[var(--color-paper-theme)] border border-[var(--color-paper-border)] shadow-sm hover:shadow-md transition-all text-[var(--color-paper-secondary)] hover:text-red-500 cursor-pointer",
                    onclick: move |_| {
                        spawn(async move {
                            let _ = logout().await;
                            ctx.user.set(None);
                            ctx.checked.set(false);
                            let _ = navigator.push(Route::Login {});
                        });
                    },
                    "退出"
                }
            }
        }
    };

    let skeleton_route = route.clone();
    match ((ctx.checked)(), (ctx.user)()) {
        (true, Some(_)) => {
            rsx! {
                div { class: "{root_class}",
                    {nav_content}
                    div { class: "flex-1 flex flex-col min-w-0 h-screen p-2 md:p-4",
                        div { class: "flex-1 bg-[var(--color-paper-theme)] rounded-[2rem] shadow-sm border border-[var(--color-paper-border)] {card_overflow} relative flex flex-col",
                            main { class: "{main_class}",
                                // 与前台 frontend_layout.rs 同理：admin 内的 use_server_future(...)?
                                // （如 preview.rs）pending 时会向上抛 RenderError::Suspended；没有
                                // SuspenseBoundary 时挂起 scope 渲染为空占位节点，主内容区在 server fn
                                // 往返期间整片空白。fallback 复用登录校验期的同款路由骨架屏（同样的
                                // flex wrapper + animate-pulse），骨架屏→骨架屏→内容全程无缝、无空白帧。
                                SuspenseBoundary {
                                    fallback: move |_| rsx! {
                                        div { class: "flex-1 min-h-0 flex flex-col animate-pulse", {admin_route_skeleton(&skeleton_route)} }
                                    },
                                    Outlet::<Route> {}
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {
            rsx! {
                div { class: "{root_class}",
                    {nav_content}
                    div { class: "flex-1 flex flex-col min-w-0 h-screen p-2 md:p-4",
                        div { class: "flex-1 bg-[var(--color-paper-theme)] rounded-[2rem] shadow-sm border border-[var(--color-paper-border)] overflow-hidden relative flex flex-col",
                            main { class: "{main_class}",
                                // flex-1 撑满 main(使 write 骨架屏能引用到确定高度),
                                // 非 write 页面的 py-12 padding 由 main_class 自带,这里不重复加。
                                div { class: "flex-1 min-h-0 flex flex-col animate-pulse",
                                    {admin_route_skeleton(&route)}
                                }
                            }
                        }
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
        Route::SiteSettingsPage {} => rsx! {
            SettingsAdminSkeleton {}
        },
        Route::PostPreview { .. } => rsx! {
            PostPreviewSkeleton {}
        },
        _ => rsx! {
            AdminDashboardSkeleton {}
        },
    }
}

/// 「内容管理」子菜单组：全部文章 / 回收站 / 评论管理。
///
/// 父项整行点击仅切换展开/收起（不跳转），chevron 旋转 + grid-template-rows
/// 0fr↔1fr 过渡动画（复用 posts_trash.rs AutoPurgeSettings 的既有模式）。
/// 当前路由落在组内时自动展开，保证激活子项始终可见；用户手动收起后，
/// 仅当再次从组外导航进入组内路由时才重新展开。
#[component]
fn ContentNavGroup() -> Element {
    let route = use_route::<Route>();
    // 判断路由是否属于本组（回收站/评论分页路由一并归入）。
    fn in_group(route: &Route) -> bool {
        matches!(
            route,
            Route::Posts {}
                | Route::PostsTrash {}
                | Route::AdminComments {}
                | Route::AdminCommentsPage { .. }
        )
    }
    let group_active = in_group(&route);
    let mut expanded = use_signal(|| group_active);

    // 路由从组外进入组内时自动展开。闭包内读 router().current 建立
    // ReactiveContext 订阅（仓库约定 #5），路由变化时本 effect 重跑。
    use_effect(move || {
        let current = router().current::<Route>();
        if in_group(&current) {
            expanded.set(true);
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

    rsx! {
        div { class: "flex flex-col gap-1",
            button {
                class: "flex items-center justify-between w-full px-4 py-3 rounded-2xl text-sm font-medium transition-all cursor-pointer {parent_text_class}",
                onclick: move |_| expanded.set(!expanded()),
                span { "内容管理" }
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
                    div { class: "ml-4 pl-3 border-l border-[var(--color-paper-border)] flex flex-col gap-1",
                        for (dest, label, active) in [
                            (Route::Posts {}, "全部文章", matches!(route, Route::Posts {})),
                            (Route::PostsTrash {}, "回收站", matches!(route, Route::PostsTrash {})),
                            (
                                Route::AdminComments {},
                                "评论管理",
                                matches!(route, Route::AdminComments {} | Route::AdminCommentsPage { .. }),
                            ),
                        ]
                        {
                            Link {
                                key: "{label}",
                                class: if active { "flex items-center px-3 py-2 rounded-xl text-sm font-medium transition-all bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] shadow-sm border border-[var(--color-paper-border)]" } else { "flex items-center px-3 py-2 rounded-xl text-sm font-medium transition-all text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-theme)]/50 hover:text-[var(--color-paper-primary)] border border-transparent" },
                                to: dest,
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 「工具」子菜单组：试运行 / MCP / 系统（issue #26）。
///
/// 父项整行点击仅切换展开/收起（不跳转），chevron 旋转 + grid-template-rows
/// 0fr↔1fr 过渡动画（复用 posts_trash.rs AutoPurgeSettings 的既有模式）。
/// 当前路由落在组内时自动展开，保证激活子项始终可见；用户手动收起后，
/// 仅当再次从组外导航进入组内路由时才重新展开。
#[component]
fn ToolsNavGroup() -> Element {
    let route = use_route::<Route>();
    // 判断路由是否属于本组。
    fn in_group(route: &Route) -> bool {
        matches!(
            route,
            Route::Runner {} | Route::Mcp {} | Route::System {} | Route::SiteSettingsPage {}
        )
    }
    let group_active = in_group(&route);
    let mut expanded = use_signal(|| group_active);

    // 路由从组外进入组内时自动展开。闭包内读 router().current 建立
    // ReactiveContext 订阅（仓库约定 #5），路由变化时本 effect 重跑。
    use_effect(move || {
        let current = router().current::<Route>();
        if in_group(&current) {
            expanded.set(true);
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

    rsx! {
        div { class: "flex flex-col gap-1",
            button {
                class: "flex items-center justify-between w-full px-4 py-3 rounded-2xl text-sm font-medium transition-all cursor-pointer {parent_text_class}",
                onclick: move |_| expanded.set(!expanded()),
                span { "工具" }
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
                    div { class: "ml-4 pl-3 border-l border-[var(--color-paper-border)] flex flex-col gap-1",
                        for (dest, label, active) in [
                            (
                                Route::SiteSettingsPage {},
                                "设置",
                                matches!(route, Route::SiteSettingsPage {}),
                            ),
                            (Route::Runner {}, "试运行", matches!(route, Route::Runner {})),
                            (Route::Mcp {}, "MCP", matches!(route, Route::Mcp {})),
                            (Route::System {}, "系统", matches!(route, Route::System {})),
                        ]
                        {
                            Link {
                                key: "{label}",
                                class: if active { "flex items-center px-3 py-2 rounded-xl text-sm font-medium transition-all bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] shadow-sm border border-[var(--color-paper-border)]" } else { "flex items-center px-3 py-2 rounded-xl text-sm font-medium transition-all text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-theme)]/50 hover:text-[var(--color-paper-primary)] border border-transparent" },
                                to: dest,
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}
