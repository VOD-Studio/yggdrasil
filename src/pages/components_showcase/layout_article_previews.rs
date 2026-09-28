//! 布局与文章组件的本地样例。所有展示复用正式组件，演示操作仅改本地状态。

use dioxus::prelude::*;

use crate::components::admin_layout::{AdminShell, AdminSidebar};
use crate::components::footer::FooterView;
use crate::components::frontend_layout::FrontendShell;
use crate::components::header::Header;
use crate::components::nav::build_nav_items;
use crate::components::post::post_content::PostContent;
use crate::components::post::post_footer::PostFooter;
use crate::components::post::post_toc::PostToc;
use crate::components::skeletons::home_skeleton::HomeSkeleton;
use crate::models::post::PostNav;
use crate::router::Route;

pub(super) fn preview(slug: &str, detail: bool) -> Option<Element> {
    match slug {
        "admin-layout" => Some(rsx! { AdminLayoutPreview { detail } }),
        "header" => Some(rsx! { HeaderPreview { detail } }),
        "footer" => Some(rsx! { FooterPreview { detail } }),
        "frontend-layout" => Some(rsx! { FrontendLayoutPreview { detail } }),
        "post-content" => Some(rsx! { PostContentPreview { detail } }),
        "post-footer" => Some(rsx! { PostFooterPreview { detail } }),
        "post-toc" => Some(rsx! { PostTocPreview { detail } }),
        _ => None,
    }
}

#[component]
fn AdminLayoutPreview(detail: bool) -> Element {
    let mut active = use_signal(|| "全部文章");
    let mut status = use_signal(|| "侧栏操作只更新这份本地演示。");

    let active_name = active();
    let (stat_1_label, stat_1_val, stat_2_label, stat_2_val) = match active_name {
        "仪表盘" => ("总访客", "1,280", "今日阅读", "342"),
        "写文章" => ("当前字数", "1,420", "自动保存", "2分钟前"),
        "笔记" | "笔记本" => ("公开笔记", "24", "笔记本数", "05"),
        "回收站" => ("待清理", "02", "保留期限", "30天"),
        "评论管理" => ("待审核", "01", "累计评论", "156"),
        "素材" => ("媒体文件", "48", "已占用", "12.8MB"),
        "友链" => ("正常链接", "18", "待确认", "02"),
        "个人信息" => ("账号角色", "管理员", "安全状态", "双重认证"),
        "设置" | "系统" => ("系统状态", "正常", "运行时间", "14天"),
        "日志" => ("今日日志", "84条", "错误告警", "0"),
        _ => ("草稿", "03", "已发布", "12"),
    };

    rsx! {
        div { class: "showcase-admin-window", "data-showcase-preview": "admin-layout",
            AdminShell {
                preview: true,
                sidebar: rsx! { AdminSidebar {
                    route: Route::Admin {},
                    user_name: Some("图鉴访客".to_string()),
                    avatar_url: None,
                    demo_active: Some(active().to_string()),
                    preview: true,
                    on_demo_navigate: move |label| {
                        active.set(label);
                        status.set("已切换本地示例栏目。");
                    },
                    on_logout: move |_| status.set("这里只演示退出按钮，不会退出当前账号。"),
                } },
                content: rsx! {
                    article { class: "showcase-admin-content flex flex-col h-full",
                        div { class: "flex items-center justify-between",
                            p { class: "text-xs font-mono text-paper-secondary tracking-wider", "ADMIN / LOCAL DEMO" }
                            if detail {
                                span { class: "text-xs px-2 py-0.5 rounded-full bg-[var(--color-paper-entry)] text-paper-secondary border border-[var(--color-paper-border)]", "本地演示" }
                            }
                        }
                        h2 { class: "text-xl font-semibold mt-1.5 flex items-center gap-2", "{active()}" }
                        p { class: "text-sm text-paper-secondary mt-1", "{status()}" }

                        div { class: "mt-4 grid grid-cols-2 gap-3",
                            div { class: "rounded-2xl bg-paper-entry p-3.5 border border-[var(--color-paper-border)]/50",
                                small { class: "text-paper-secondary text-xs", "{stat_1_label}" }
                                strong { class: "block text-lg mt-0.5 font-bold tracking-tight", "{stat_1_val}" }
                            }
                            div { class: "rounded-2xl bg-paper-entry p-3.5 border border-[var(--color-paper-border)]/50",
                                small { class: "text-paper-secondary text-xs", "{stat_2_label}" }
                                strong { class: "block text-lg mt-0.5 font-bold tracking-tight", "{stat_2_val}" }
                            }
                        }

                        if !detail {
                            div { class: "mt-2.5 rounded-lg bg-paper-entry/50 border border-[var(--color-paper-border)]/40 px-2.5 py-1.5 flex items-center justify-between text-xs",
                                div { class: "flex items-center gap-1.5 min-w-0",
                                    span { class: "w-1.5 h-1.5 rounded-full bg-emerald-500 flex-shrink-0" }
                                    span { class: "truncate font-medium text-[11px]", "响应式设计实践" }
                                }
                                span { class: "text-paper-secondary text-[10px] font-mono flex-shrink-0 ml-1", "已发布" }
                            }
                        }

                        if detail {
                            div { class: "mt-5 flex-1 min-h-0 flex flex-col",
                                p { class: "text-xs font-medium text-paper-secondary mb-2 tracking-wide", "示例内容列表" }
                                div { class: "rounded-xl border border-[var(--color-paper-border)] divide-y divide-[var(--color-paper-border)] bg-paper-entry/40 overflow-hidden text-xs",
                                    div { class: "px-3.5 py-2.5 flex items-center justify-between hover:bg-paper-entry/80 transition-colors",
                                        div { class: "flex items-center gap-2 min-w-0",
                                            span { class: "w-1.5 h-1.5 rounded-full bg-emerald-500 flex-shrink-0" }
                                            span { class: "truncate font-medium", "Dioxus 0.7 响应式设计实践" }
                                        }
                                        span { class: "text-paper-secondary flex-shrink-0 ml-2 font-mono text-[11px]", "09-22" }
                                    }
                                    div { class: "px-3.5 py-2.5 flex items-center justify-between hover:bg-paper-entry/80 transition-colors",
                                        div { class: "flex items-center gap-2 min-w-0",
                                            span { class: "w-1.5 h-1.5 rounded-full bg-amber-500 flex-shrink-0" }
                                            span { class: "truncate font-medium", "Rust 全栈博客架构与布局演进" }
                                        }
                                        span { class: "text-paper-secondary flex-shrink-0 ml-2 font-mono text-[11px]", "09-18" }
                                    }
                                    div { class: "px-3.5 py-2.5 flex items-center justify-between hover:bg-paper-entry/80 transition-colors",
                                        div { class: "flex items-center gap-2 min-w-0",
                                            span { class: "w-1.5 h-1.5 rounded-full bg-emerald-500 flex-shrink-0" }
                                            span { class: "truncate font-medium", "Tailwind CSS v4 样式系统接入记录" }
                                        }
                                        span { class: "text-paper-secondary flex-shrink-0 ml-2 font-mono text-[11px]", "09-15" }
                                    }
                                }
                                p { class: "text-xs text-paper-secondary mt-auto pt-3", "可展开内容管理或工具菜单，点击任意栏目查看联动状态。" }
                            }
                        }
                    }
                },
            }
        }
    }
}

fn sample_route(label: &str) -> Route {
    match label {
        "笔记" => Route::Notes {},
        "归档" => Route::Archives {},
        "友链" => Route::Friends {},
        "关于" => Route::About {},
        "搜索" => Route::Search {},
        _ => Route::Home {},
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HeaderViewportMode {
    Desktop,
    Mobile,
}

#[component]
fn HeaderPreview(detail: bool) -> Element {
    let mut active = use_signal(|| "笔记");
    let mut viewport_mode = use_signal(|| HeaderViewportMode::Desktop);
    let mut max_width_choice = use_signal(|| "max-w-6xl");
    let menu_id = if detail {
        "mobile-nav-menu-showcase-header-detail"
    } else {
        "mobile-nav-menu-showcase-header-card"
    };

    let search_button = rsx! {
        button {
            class: if active() == "搜索" {
                "relative p-2 rounded-full text-paper-accent transition-colors duration-200"
            } else {
                "relative p-2 rounded-full text-paper-secondary hover:text-paper-accent transition-colors duration-200"
            },
            r#type: "button",
            aria_label: "演示搜索操作",
            title: "搜索",
            onclick: move |_| active.set("搜索"),
            svg {
                xmlns: "http://www.w3.org/2000/svg",
                class: "w-4 h-4 md:w-5 md:h-5",
                view_box: "0 -960 960 960",
                fill: "currentColor",
                path { d: "M784-120 532-372q-30 24-69 38t-83 14q-109 0-184.5-75.5T120-580q0-109 75.5-184.5T380-840q109 0 184.5 75.5T640-580q0 44-14 83t-38 69l252 252-56 56ZM380-400q75 0 127.5-52.5T560-580q0-75-52.5-127.5T380-760q-75 0-127.5 52.5T200-580q0 75 52.5 127.5T380-400Z" }
            }
            if active() == "搜索" {
                span { class: "nav-indicator hidden md:block", "aria-hidden": "true" }
            }
        }
    };

    if !detail {
        return rsx! {
            div { class: "showcase-layout-window", "data-showcase-preview": "header",
                Header {
                    nav_items: build_nav_items(sample_route(active())),
                    right_content: rsx! { {search_button} },
                    max_width: "max-w-6xl",
                    menu_id,
                    page_shell: false,
                    on_demo_navigate: move |label| active.set(label),
                }
                div { class: "showcase-header-card-body",
                    div { class: "showcase-header-card-hero",
                        div {
                            p { class: "text-[11px] font-mono text-paper-secondary", "ACTIVE / 当前栏目" }
                            p { class: "text-sm font-semibold text-paper-primary mt-0.5", "{active()}" }
                        }
                        span { class: "text-[10px] px-2 py-0.5 rounded-full bg-paper-accent-soft text-paper-accent font-medium", "交互演示" }
                    }
                    div { class: "showcase-header-card-tags",
                        span { "自适应居中" }
                        span { "平滑下划指示" }
                        span { "窄屏自动折叠" }
                    }
                }
            }
        };
    }

    rsx! {
        div { class: "w-full flex flex-col gap-3",
            div { class: "flex flex-wrap items-center justify-between gap-3 p-3 rounded-xl bg-paper-entry/50 border border-[var(--sc-line)]",
                div { class: "flex items-center gap-2",
                    span { class: "text-xs font-mono text-paper-secondary tracking-wider", "VIEWPORT" }
                    div { class: "showcase-picker-mode-switch",
                        button {
                            r#type: "button",
                            aria_pressed: "{viewport_mode() == HeaderViewportMode::Desktop}",
                            onclick: move |_| viewport_mode.set(HeaderViewportMode::Desktop),
                            "🖥️ 桌面端"
                        }
                        button {
                            r#type: "button",
                            aria_pressed: "{viewport_mode() == HeaderViewportMode::Mobile}",
                            onclick: move |_| viewport_mode.set(HeaderViewportMode::Mobile),
                            "📱 移动端 (380px)"
                        }
                    }
                }
                if viewport_mode() == HeaderViewportMode::Desktop {
                    div { class: "flex items-center gap-2",
                        span { class: "text-xs font-mono text-paper-secondary tracking-wider", "WIDTH" }
                        div { class: "showcase-picker-mode-switch",
                            button {
                                r#type: "button",
                                aria_pressed: "{max_width_choice() == \"max-w-3xl\"}",
                                onclick: move |_| max_width_choice.set("max-w-3xl"),
                                "3xl (768px)"
                            }
                            button {
                                r#type: "button",
                                aria_pressed: "{max_width_choice() == \"max-w-4xl\"}",
                                onclick: move |_| max_width_choice.set("max-w-4xl"),
                                "4xl (896px)"
                            }
                            button {
                                r#type: "button",
                                aria_pressed: "{max_width_choice() == \"max-w-6xl\"}",
                                onclick: move |_| max_width_choice.set("max-w-6xl"),
                                "6xl (1152px)"
                            }
                        }
                    }
                }
            }

            div {
                class: if viewport_mode() == HeaderViewportMode::Mobile { "showcase-layout-phone-wrap" } else { "w-full" },
                div {
                    class: if viewport_mode() == HeaderViewportMode::Mobile {
                        "showcase-mobile-phone-frame"
                    } else {
                        "showcase-layout-window showcase-detail-window-scroll"
                    },
                    "data-showcase-preview": "header",

                    if viewport_mode() == HeaderViewportMode::Mobile {
                        div { class: "showcase-phone-notch",
                            span { "09:41" }
                            div { class: "showcase-phone-notch-pill" }
                            span { "5G 100%" }
                        }
                    }

                    Header {
                        nav_items: build_nav_items(sample_route(active())),
                        right_content: rsx! { {search_button} },
                        max_width: if viewport_mode() == HeaderViewportMode::Mobile { "w-full" } else { max_width_choice() },
                        menu_id,
                        page_shell: false,
                        on_demo_navigate: move |label| active.set(label),
                    }

                    div {
                        class: if viewport_mode() == HeaderViewportMode::Mobile {
                            "showcase-phone-scroll p-4 space-y-4"
                        } else {
                            "flex-1 p-6 md:p-8 space-y-6 max-w-4xl mx-auto w-full"
                        },
                        div { class: "rounded-2xl border border-[var(--sc-line)] bg-paper-entry/40 p-5",
                            div { class: "flex items-center justify-between",
                                span { class: "text-xs font-mono text-paper-secondary uppercase tracking-wider", "当前演示栏目" }
                                span { class: "text-xs px-2.5 py-0.5 rounded-full bg-paper-accent-soft text-paper-accent font-semibold", "{active()}" }
                            }
                            h3 { class: "text-lg font-semibold mt-2 text-paper-primary",
                                match active() {
                                    "首页" => "首页 · 最新动态与精选文章",
                                    "笔记" => "笔记 · 碎片记录与灵感闪光",
                                    "归档" => "归档 · 按年份组织的历史文章",
                                    "友链" => "友链 · 互联互通的网络邻居",
                                    "关于" => "关于 · 站点介绍与设计故事",
                                    "搜索" => "搜索 · 全局快速文本检索",
                                    _ => "栏目演示",
                                }
                            }
                            p { class: "text-xs text-paper-secondary mt-1.5 leading-relaxed",
                                if viewport_mode() == HeaderViewportMode::Mobile {
                                    "点击右上角汉堡按钮，可展开移动端全屏抽屉菜单；点击任意栏目自动收起并切换当前状态。"
                                } else {
                                    "点击上方导航栏项目可切换激活项；向下滚动本区域可体验 Header 粘性吸顶 (sticky) 与磨砂毛玻璃 (backdrop-blur) 效果。"
                                }
                            }
                        }

                        div { class: "rounded-2xl border border-[var(--sc-line)]/60 bg-paper-entry/20 p-5 space-y-3",
                            p { class: "text-xs font-mono text-paper-secondary tracking-wider", "FEATURE HIGHLIGHTS / 特性说明" }
                            ul { class: "text-xs text-paper-secondary space-y-2 list-disc list-inside",
                                li { "自适应居中对齐：通过 max_width 参数确保顶部导航项左右边缘与下方文章正文精准对齐。" }
                                li { "指示器平滑过渡：激活项下方带独立的 nav-indicator 下划线，栏目切换时平滑滑动。" }
                                li { "移动端交互：在移动端或窄屏视口下自动转换为旋转图标汉堡按钮与平滑展开面板。" }
                            }
                        }

                        div { class: "py-12 text-center text-xs font-mono text-paper-tertiary",
                            "— 往下滚动查看 Header 磨砂吸顶效果 —"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FooterPreview(detail: bool) -> Element {
    let scroll_id = if detail {
        "showcase-footer-scroll-detail"
    } else {
        "showcase-footer-scroll-card"
    };
    rsx! {
        div { class: "showcase-layout-window", "data-showcase-preview": "footer",
            div { id: scroll_id, class: "showcase-footer-scroll",
                p { "页脚会陪你到页面尽头。" }
                if detail {
                    p { "向下滚动这一小块内容，再点击页脚右侧按钮，就会回到演示顶部。" }
                    div { style: "height: 180px" }
                    p { "这里是演示内容的末尾。" }
                }
            }
            FooterView {
                github_url: Some("https://github.com".to_string()),
                top_visible: true,
                page_shell: false,
                inline_top: true,
                on_top: move |_| scroll_demo_to_top(scroll_id),
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrontendLayoutMode {
    Desktop,
    Mobile,
    Skeleton,
}

#[component]
fn FrontendLayoutPreview(detail: bool) -> Element {
    let mut active = use_signal(|| "首页");
    let mut layout_mode = use_signal(|| FrontendLayoutMode::Desktop);
    let mut max_width_choice = use_signal(|| "max-w-4xl");
    let menu_id = if detail {
        "mobile-nav-menu-showcase-layout-detail"
    } else {
        "mobile-nav-menu-showcase-layout-card"
    };
    let scroll_id = if detail {
        "showcase-frontend-scroll-detail"
    } else {
        "showcase-frontend-scroll-card"
    };

    let search_button = rsx! {
        button {
            class: if active() == "搜索" {
                "p-2 rounded-full text-paper-accent transition-colors duration-200"
            } else {
                "p-2 rounded-full text-paper-secondary hover:text-paper-accent transition-colors duration-200"
            },
            r#type: "button",
            aria_label: "演示搜索操作",
            onclick: move |_| active.set("搜索"),
            svg {
                xmlns: "http://www.w3.org/2000/svg",
                class: "w-4 h-4 md:w-5 md:h-5",
                view_box: "0 -960 960 960",
                fill: "currentColor",
                path { d: "M784-120 532-372q-30 24-69 38t-83 14q-109 0-184.5-75.5T120-580q0-109 75.5-184.5T380-840q109 0 184.5 75.5T640-580q0 44-14 83t-38 69l252 252-56 56ZM380-400q75 0 127.5-52.5T560-580q0-75-52.5-127.5T380-760q-75 0-127.5 52.5T200-580q0 75 52.5 127.5T380-400Z" }
            }
        }
    };

    if !detail {
        return rsx! {
            div { id: scroll_id, class: "showcase-layout-window", "data-showcase-preview": "frontend-layout",
                FrontendShell {
                    min_height: "h-full min-h-0",
                    max_width: "max-w-4xl",
                    header: rsx! {
                        Header {
                            nav_items: build_nav_items(sample_route(active())),
                            right_content: rsx! { {search_button} },
                            max_width: "max-w-4xl",
                            menu_id,
                            page_shell: false,
                            on_demo_navigate: move |label| active.set(label),
                        }
                    },
                    main_content: rsx! {
                        article { class: "flex flex-col justify-center h-full",
                            h2 { class: "font-semibold text-paper-primary", "一页内容，安心阅读" }
                            p { class: "text-paper-secondary mt-1", "前台布局将导航、正文和页脚排成清晰的阅读路线。" }
                        }
                    },
                    footer: rsx! {
                        FooterView {
                            github_url: Some("https://github.com".to_string()),
                            top_visible: false,
                            page_shell: false,
                            inline_top: true,
                            on_top: move |_| scroll_demo_to_top(scroll_id),
                        }
                    },
                }
            }
        };
    }

    rsx! {
        div { class: "w-full flex flex-col gap-3",
            div { class: "flex flex-wrap items-center justify-between gap-3 p-3 rounded-xl bg-paper-entry/50 border border-[var(--sc-line)]",
                div { class: "flex items-center gap-2",
                    span { class: "text-xs font-mono text-paper-secondary tracking-wider", "LAYOUT" }
                    div { class: "showcase-picker-mode-switch",
                        button {
                            r#type: "button",
                            aria_pressed: "{layout_mode() == FrontendLayoutMode::Desktop}",
                            onclick: move |_| layout_mode.set(FrontendLayoutMode::Desktop),
                            "🖥️ 完整页面"
                        }
                        button {
                            r#type: "button",
                            aria_pressed: "{layout_mode() == FrontendLayoutMode::Mobile}",
                            onclick: move |_| layout_mode.set(FrontendLayoutMode::Mobile),
                            "📱 移动端布局"
                        }
                        button {
                            r#type: "button",
                            aria_pressed: "{layout_mode() == FrontendLayoutMode::Skeleton}",
                            onclick: move |_| layout_mode.set(FrontendLayoutMode::Skeleton),
                            "🦴 骨架屏加载"
                        }
                    }
                }
                if layout_mode() == FrontendLayoutMode::Desktop {
                    div { class: "flex items-center gap-2",
                        span { class: "text-xs font-mono text-paper-secondary tracking-wider", "WIDTH" }
                        div { class: "showcase-picker-mode-switch",
                            button {
                                r#type: "button",
                                aria_pressed: "{max_width_choice() == \"max-w-4xl\"}",
                                onclick: move |_| max_width_choice.set("max-w-4xl"),
                                "4xl (896px 常规)"
                            }
                            button {
                                r#type: "button",
                                aria_pressed: "{max_width_choice() == \"max-w-6xl\"}",
                                onclick: move |_| max_width_choice.set("max-w-6xl"),
                                "6xl (1152px 宽屏)"
                            }
                        }
                    }
                }
            }

            div {
                class: if layout_mode() == FrontendLayoutMode::Mobile { "showcase-layout-phone-wrap" } else { "w-full" },
                div {
                    id: scroll_id,
                    class: if layout_mode() == FrontendLayoutMode::Mobile {
                        "showcase-mobile-phone-frame"
                    } else {
                        "showcase-layout-window showcase-detail-window-scroll"
                    },
                    "data-showcase-preview": "frontend-layout",

                    if layout_mode() == FrontendLayoutMode::Mobile {
                        div { class: "showcase-phone-notch",
                            span { "09:41" }
                            div { class: "showcase-phone-notch-pill" }
                            span { "5G 100%" }
                        }
                    }

                    FrontendShell {
                        min_height: "min-h-[480px]",
                        max_width: if layout_mode() == FrontendLayoutMode::Mobile { "w-full" } else { max_width_choice() },
                        header: rsx! {
                            Header {
                                nav_items: build_nav_items(sample_route(active())),
                                right_content: rsx! { {search_button} },
                                max_width: if layout_mode() == FrontendLayoutMode::Mobile { "w-full" } else { max_width_choice() },
                                menu_id,
                                page_shell: false,
                                on_demo_navigate: move |label| active.set(label),
                            }
                        },
                        main_content: rsx! {
                            if layout_mode() == FrontendLayoutMode::Skeleton {
                                div { class: "space-y-4 py-4",
                                    div { class: "p-3 rounded-xl bg-paper-accent-soft text-paper-accent text-xs font-mono flex items-center justify-between",
                                        span { "SUSPENSE BOUNDARY SKELETON PLACEHOLDER" }
                                        span { "路由加载中" }
                                    }
                                    HomeSkeleton { current_page: 1 }
                                }
                            } else {
                                article { class: "space-y-6",
                                    div { class: "border-b border-[var(--sc-line)] pb-5",
                                        span { class: "text-xs font-mono text-paper-secondary", "FRONTEND LAYOUT SPECIMEN" }
                                        h2 { class: "text-2xl font-bold tracking-tight text-paper-primary mt-1",
                                            match active() {
                                                "首页" => "一页内容，安心阅读",
                                                "笔记" => "公开笔记 · 随笔与灵感",
                                                "归档" => "岁月归档 · 时间线检索",
                                                "友链" => "友情链接 · 志趣相投的伙伴",
                                                "关于" => "关于本站 · 架构设计与理念",
                                                "搜索" => "站内检索 · 快速定位内容",
                                                _ => "前台内容展示",
                                            }
                                        }
                                        p { class: "mt-2 text-sm text-paper-secondary leading-relaxed",
                                            "前台布局通过统一的 FrontendShell 组合顶部 Header、弹性内容主区 main 与底部 Footer。当前选中栏目：{active()}。"
                                        }
                                    }

                                    div { class: "space-y-4",
                                        div { class: "p-5 rounded-2xl border border-[var(--sc-line)] bg-paper-entry/40 hover:bg-paper-entry/70 transition-colors",
                                            div { class: "flex items-center gap-2 text-xs text-paper-secondary font-mono",
                                                span { class: "px-2 py-0.5 rounded-full bg-paper-accent-soft text-paper-accent font-medium", "精选文章" }
                                                span { "•" }
                                                span { "2026-09-28" }
                                                span { "•" }
                                                span { "5 分钟阅读" }
                                            }
                                            h3 { class: "text-lg font-semibold text-paper-primary mt-2", "Dioxus 0.7 与全栈 Rust 博客布局演进" }
                                            p { class: "text-xs text-paper-secondary mt-1.5 leading-relaxed",
                                                "探讨全栈 Rust 前端在 SSR、水合、骨架屏与页面过渡动画上的工程实践，兼顾极致的加载性能与阅读体验。"
                                            }
                                        }

                                        div { class: "p-5 rounded-2xl border border-[var(--sc-line)] bg-paper-entry/40 hover:bg-paper-entry/70 transition-colors",
                                            div { class: "flex items-center gap-2 text-xs text-paper-secondary font-mono",
                                                span { class: "px-2 py-0.5 rounded-full bg-paper-entry text-paper-secondary border border-[var(--sc-line)]", "设计系统" }
                                                span { "•" }
                                                span { "2026-09-15" }
                                            }
                                            h3 { class: "text-lg font-semibold text-paper-primary mt-2", "统一宽度与响应式对齐的排版艺术" }
                                            p { class: "text-xs text-paper-secondary mt-1.5 leading-relaxed",
                                                "导航栏 max-w 与正文容器严格对齐，保证视口放大缩小时视觉焦点稳定不飘移。"
                                            }
                                        }
                                    }

                                    div { class: "pt-8 pb-4 text-center text-xs font-mono text-paper-tertiary",
                                        "— 滚动至底部可点击页脚回到顶部 —"
                                    }
                                }
                            }
                        },
                        footer: rsx! {
                            FooterView {
                                github_url: Some("https://github.com".to_string()),
                                top_visible: true,
                                page_shell: false,
                                inline_top: true,
                                on_top: move |_| scroll_demo_to_top(scroll_id),
                            }
                        },
                    }
                }
            }
        }
    }
}

fn scroll_demo_to_top(id: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id))
    {
        element.set_scroll_top(0);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = id;
}

fn article_html(prefix: &str, alternate: bool) -> String {
    let opening = if alternate {
        "雨停之后，树叶留下细小的光。"
    } else {
        "一片叶子，也记得来时的风。"
    };
    format!(
        r#"<h2 id="{prefix}-opening">从一片叶子开始</h2>
<p>{opening} 这段固定文章展示正文排版和图片，不连接任何服务。</p>
<h3 id="{prefix}-details">沿途所见</h3>
<ul><li>清晨的光</li><li>缓慢生长的枝条</li></ul>
<blockquote><p>让想法，像树一样生长。</p></blockquote>
<pre><code class="language-rust">let leaf = "Yggdrasil";
println!("{{leaf}}");</code></pre>
<h2 id="{prefix}-image">留下一张照片</h2>
<p><img src="/images/empty-state/dog-camera.webp" alt="相机里的两只小狗" loading="lazy"></p>"#
    )
}

fn toc_html(prefix: &str) -> String {
    format!(
        r##"<ul><li><a href="#{prefix}-opening">从一片叶子开始</a><ul><li><a href="#{prefix}-details">沿途所见</a></li></ul></li><li><a href="#{prefix}-image">留下一张照片</a></li></ul>"##
    )
}

#[component]
fn PostContentPreview(detail: bool) -> Element {
    let mut alternate = use_signal(|| false);
    let (scope_id, heading_prefix) = if detail {
        (
            "showcase-post-content-detail",
            "showcase-post-content-detail",
        )
    } else {
        ("showcase-post-content-card", "showcase-post-content-card")
    };
    rsx! {
        div { class: "showcase-article-window", "data-showcase-preview": "post-content",
            if detail {
                button {
                    class: "mb-4 text-sm text-paper-accent",
                    r#type: "button",
                    onclick: move |_| alternate.toggle(),
                    "切换文章样例"
                }
            }
            for variant in std::iter::once(alternate()) {
                PostContent {
                    key: "sample-{variant}",
                    content_html: article_html(heading_prefix, variant),
                    scope_id: Some(scope_id),
                }
            }
        }
    }
}

#[component]
fn PostFooterPreview(detail: bool) -> Element {
    let _ = detail;
    let mut post = super::sample_post();
    post.tags = vec!["设计".to_string(), "阅读".to_string(), "Rust".to_string()];
    post.prev_post = Some(PostNav {
        title: "沿着风的方向".to_string(),
        slug: "sample-previous".to_string(),
    });
    post.next_post = Some(PostNav {
        title: "下一圈年轮".to_string(),
        slug: "sample-next".to_string(),
    });
    rsx! {
        div { class: "showcase-article-window", "data-showcase-preview": "post-footer",
            PostFooter { post, sample: true }
            p { class: "mt-3 text-xs text-paper-secondary", "样例标签和相邻文章不跳转。" }
        }
    }
}

#[component]
fn PostTocPreview(detail: bool) -> Element {
    let (scroll_id, nav_id, content_id) = if detail {
        (
            "showcase-post-toc-scroll-detail",
            "showcase-post-toc-nav-detail",
            "showcase-post-toc-content-detail",
        )
    } else {
        (
            "showcase-post-toc-scroll-card",
            "showcase-post-toc-nav-card",
            "showcase-post-toc-content-card",
        )
    };
    rsx! {
        div {
            id: scroll_id,
            class: "showcase-toc-window showcase-article-window",
            "data-showcase-preview": "post-toc",
            "data-local-anchor-scroll": "true",
            PostToc {
                toc_html: toc_html(content_id),
                title: "文章目录",
                nav_id: Some(nav_id),
                content_id: Some(content_id),
                scroll_id: Some(scroll_id),
            }
            PostContent { content_html: article_html(content_id, false), scope_id: Some(content_id) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_and_toc_share_scoped_headings() {
        let html = article_html("sample", false);
        let toc = toc_html("sample");
        for id in ["sample-opening", "sample-details", "sample-image"] {
            assert!(html.contains(&format!("id=\"{id}\"")));
            assert!(toc.contains(&format!("href=\"#{id}\"")));
        }
        assert!(!html.contains("data-runnable"));
    }

    #[test]
    fn alternate_article_changes_real_content() {
        assert_ne!(article_html("sample", false), article_html("sample", true));
    }
}
