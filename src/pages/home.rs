//! 首页与分页首页：刊物式开场、最新文章和编号文章流。
//! 使用服务端分页数据；首页开场独立于文章区加载，翻页订阅路由变化。

use dioxus::prelude::*;

use crate::api::posts::{list_published_posts, PostListResponse};
use crate::components::empty_state::EmptyState;
use crate::components::post_card::PostCard;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::home_skeleton::HomePostsSkeleton;
use crate::components::ui::Pagination;
use crate::router::Route;

const POSTS_PER_PAGE: i32 = 10;

#[component]
pub fn Home() -> Element {
    rsx! { HomePage { page: 1 } }
}

#[component]
pub fn HomePage(page: i32) -> Element {
    let current_page = page.max(1);
    rsx! {
        div { class: "home-page",
            HomeIntro { current_page }
            section { id: "home-posts", class: "home-posts", tabindex: "-1", aria_label: "文章列表",
                SuspenseBoundary {
                    fallback: move |_| rsx! { DelayedSkeleton { HomePostsSkeleton { current_page } } },
                    // 翻页时卸载旧资源和重试任务，避免旧响应覆盖新页。
                    HomePosts { key: "page-{current_page}", current_page }
                }
            }
            footer { class: "home-colophon",
                span { class: "home-colophon-mark", aria_hidden: "true", "✳" }
                p { "文字落下的地方，便有了生长。" }
                Link { class: "home-text-link", to: Route::Friends {},
                    "去朋友的花园坐坐" span { class: "home-arrow", aria_hidden: "true", "↗" }
                }
            }
        }
    }
}

/// 首页与路由骨架屏共享静态开场，避免数据加载时首屏布局跳动。
#[component]
pub(crate) fn HomeIntro(current_page: i32) -> Element {
    if current_page > 1 {
        return rsx! {
            header { class: "home-page-heading home-enter",
                Link { class: "home-text-link", to: Route::Home {},
                    span { class: "home-arrow", aria_hidden: "true", "←" } "回到首页"
                }
                p { class: "home-eyebrow", "THE JOURNAL / 过往篇章" }
                h1 { "往前翻，" span { "还有故事。" } }
                p { class: "home-description", "第 {current_page} 页 · 沿着文字，走进更早的年轮。" }
            }
        };
    }

    rsx! {
        header { class: "home-hero",
            div { class: "home-hero-copy home-enter",
                p { class: "home-eyebrow", span { aria_hidden: "true" } "YGGDRASIL / 一隅数字花园" }
                h1 { "让想法生根，" span { "让文字成林。" } }
                p { class: "home-description",
                    "记录技术、生活，与偶然闪光的念头。"
                    br {}
                    "在这里，慢慢写，也慢慢生长。"
                }
                div { class: "home-hero-actions",
                    a { class: "home-reading-link", href: "#home-posts",
                        "开始阅读" span { class: "home-arrow", aria_hidden: "true", "↓" }
                    }
                    Link { class: "home-text-link", to: Route::About {},
                        "关于这里" span { class: "home-arrow", aria_hidden: "true", "↗" }
                    }
                }
            }
            Link { class: "home-garden home-enter", to: Route::About {}, aria_label: "认识这棵世界树",
                svg { class: "home-rings", view_box: "0 0 360 360", fill: "none", "aria-hidden": "true",
                    g { class: "home-ring-lines", stroke: "currentColor", stroke_width: "0.8",
                        for i in 0..9 {
                            ellipse {
                                key: "{i}", cx: "180", cy: "180",
                                rx: "{42 + i * 14}", ry: "{38 + i * 13}",
                                transform: "rotate({i * 19} 180 180)",
                            }
                        }
                    }
                    circle { cx: "180", cy: "180", r: "169", stroke: "currentColor", stroke_dasharray: "1 9", opacity: "0.3" }
                    path { d: "M180 6V28M180 332V354M6 180H28M332 180H354", stroke: "currentColor", opacity: "0.4" }
                    g { class: "home-sprout", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                        path { d: "M180 207V175M180 190C157 190 148 173 151 156C169 156 183 166 180 190Z" }
                        path { d: "M180 176C180 156 192 147 210 149C210 165 199 177 180 176ZM180 190L163 170M180 176L198 159M166 208H194" }
                    }
                    g { class: "home-ring-orbit", fill: "currentColor",
                        circle { cx: "180", cy: "37", r: "4" }
                        circle { cx: "73", cy: "272", r: "2.5", opacity: "0.6" }
                    }
                }
                span { class: "home-garden-caption", "每一篇，都是新的年轮" span { class: "home-arrow", aria_hidden: "true", "↗" } }
            }
        }
    }
}

#[component]
fn HomePosts(current_page: i32) -> Element {
    let router = dioxus::router::router();
    let mut posts_res = use_server_future(move || {
        let page = match router.current::<Route>() {
            Route::HomePage { page } => page.max(1),
            _ => current_page,
        };
        list_published_posts(page, POSTS_PER_PAGE)
    })?;

    let posts_data = posts_res.read();
    match posts_data.as_ref() {
        Some(Ok(PostListResponse { posts, total })) => {
            let total = *total;
            rsx! {
                HomePostsHeading { current_page, total }
                if !posts.is_empty() {
                    div { key: "page-{current_page}", class: "home-post-list",
                        for (index, post) in posts.iter().enumerate() {
                            div {
                                key: "{post.id}",
                                class: if current_page == 1 && index == 0 { "home-entry home-entry-featured home-enter" } else { "home-entry home-enter" },
                                style: "--home-delay: {index.min(5) * 45}ms",
                                if current_page == 1 && index == 0 {
                                    span { class: "home-featured-label", "最新一篇" span { " / LATEST ENTRY" } }
                                } else {
                                    span { class: "home-entry-number", aria_hidden: "true",
                                        {format!("{:02}", (i64::from(current_page) - 1) * i64::from(POSTS_PER_PAGE) + index as i64 + 1)}
                                    }
                                }
                                PostCard { post: post.clone(), compact: true }
                                span { class: "home-entry-arrow home-arrow", aria_hidden: "true", "↗" }
                            }
                        }
                    }
                    Pagination {
                        variant: "frontend", current_page, total, per_page: POSTS_PER_PAGE,
                        prev_route: if current_page - 1 <= 1 { Route::Home {} } else { Route::HomePage { page: current_page - 1 } },
                        next_route: Route::HomePage { page: current_page + 1 },
                        unit: "篇",
                    }
                } else if total == 0 {
                    EmptyState { title: "还没有文章", description: "这片花园，正等待第一颗文字的种子。" }
                } else {
                    div { class: "home-status",
                        EmptyState { title: "已经翻到最后了", description: "这一页还没有文字，回到首页看看最近的记录吧。" }
                        Link { class: "home-reading-link", to: Route::Home {}, "回到首页" span { class: "home-arrow", aria_hidden: "true", "↗" } }
                    }
                }
            }
        }
        Some(Err(_)) => rsx! {
            HomePostsHeading { current_page }
            HomePostsError {
                current_page,
                on_recovered: move |response| posts_res.set(Some(Ok(response))),
            }
        },
        _ => rsx! { DelayedSkeleton { HomePostsSkeleton { current_page } } },
    }
}

#[component]
fn HomePostsHeading(current_page: i32, total: Option<i64>) -> Element {
    rsx! {
        div { class: "home-section-heading home-enter",
            div { class: "home-section-title",
                h2 { if current_page == 1 { "最近写下" } else { "过往篇章" } }
                if let Some(total) = total {
                    span { class: "home-post-count", "共 {total} 篇" }
                }
            }
            Link { class: "home-text-link", to: Route::Archives {},
                "全部归档" span { class: "home-arrow", aria_hidden: "true", "↗" }
            }
        }
    }
}

/// 重试在错误面板的作用域内运行；成功后回填资源，不触发 SSR future 的悬挂骨架。
#[component]
fn HomePostsError(current_page: i32, on_recovered: EventHandler<PostListResponse>) -> Element {
    let router = dioxus::router::router();
    let mut retrying = use_signal(|| false);
    let mut retry_failed = use_signal(|| false);

    rsx! {
        div { class: "home-error", role: "group", aria_labelledby: "home-error-title",
            div { class: "home-error-copy",
                p { class: "home-error-eyebrow",
                    span { class: "home-error-pause", aria_hidden: "true", "Ⅱ" }
                    "稍作停留" span { class: "home-error-eyebrow-en", " / A LITTLE PAUSE" }
                }
                h3 { id: "home-error-title", "文章暂时无法加载" }
                p { class: "home-error-description", role: "status", aria_live: "polite", aria_atomic: "true",
                    if retry_failed() {
                        "这次仍未加载成功，稍后再试一次吧。"
                    } else {
                        "加载遇到了一点阻碍，稍后再试一次吧。"
                    }
                    if retrying() {
                        span { class: "sr-only", "正在重新加载文章，请稍候。" }
                    }
                }
                button {
                    class: "home-error-retry",
                    r#type: "button",
                    aria_disabled: retrying(),
                    aria_busy: retrying(),
                    onclick: move |_| {
                        if retrying() {
                            return;
                        }
                        retrying.set(true);
                        retry_failed.set(false);
                        let request_route = router.current::<Route>();
                        spawn(async move {
                            let result = list_published_posts(current_page, POSTS_PER_PAGE).await;
                            // Suspense 切换期间旧面板可能尚未卸载，也不能回填另一条路由。
                            if router.current::<Route>() != request_route {
                                return;
                            }
                            retrying.set(false);
                            match result {
                                Ok(response) => on_recovered.call(response),
                                Err(_) => retry_failed.set(true),
                            }
                        });
                    },
                    svg {
                        class: "home-error-retry-icon", view_box: "0 0 24 24", fill: "none",
                        stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round",
                        "aria-hidden": "true", "focusable": "false",
                        path { d: "M20 7V3M20 7H16M20 7A8 8 0 1 0 20 16" }
                    }
                    if retrying() { "正在重新加载…" } else { "重新加载" }
                }
            }
            svg {
                class: "home-error-art", view_box: "0 0 240 240", fill: "none",
                "aria-hidden": "true", "focusable": "false",
                g { class: "home-error-rings", stroke: "currentColor", stroke_width: "0.8", stroke_linecap: "round",
                    circle { cx: "120", cy: "112", r: "94", stroke_dasharray: "370 52 72 97", transform: "rotate(-62 120 112)" }
                    circle { cx: "120", cy: "112", r: "78", stroke_dasharray: "264 42 65 119", transform: "rotate(-28 120 112)" }
                    circle { cx: "120", cy: "112", r: "62", stroke_dasharray: "198 36 59 97", transform: "rotate(-78 120 112)" }
                    circle { cx: "120", cy: "112", r: "108", stroke_dasharray: "1 11", opacity: "0.55" }
                }
                g { stroke: "currentColor", stroke_linecap: "round", stroke_linejoin: "round",
                    path { class: "home-error-book-fill", stroke_width: "1.5", d: "M120 161C101 148 79 146 58 150L58 184C82 180 102 185 120 194C138 185 158 180 182 184V150C161 146 139 148 120 161Z" }
                    path { stroke_width: "1.5", d: "M120 162V194M53 156L50 190C76 186 100 192 120 201C140 192 164 186 190 190L187 156" }
                    path { opacity: "0.35", d: "M70 161C83 160 96 163 108 168M70 170C83 169 96 172 108 177M132 168C144 163 157 160 170 161M132 177C144 172 157 169 170 170" }
                    path { stroke_width: "1.8", d: "M120 161V105" }
                    path { class: "home-error-leaf", stroke_width: "1.5", d: "M120 131C96 132 81 115 83 93C104 93 121 106 120 131ZM120 113C118 89 131 73 154 72C157 94 143 113 120 113Z" }
                    path { stroke_width: "1.2", d: "M120 132L95 106M120 113L142 86" }
                    path { opacity: "0.5", d: "M52 72V82M47 77H57M182 102V110M178 106H186M162 38V44M159 41H165" }
                }
                g { fill: "currentColor",
                    circle { cx: "70", cy: "36", r: "2.5", opacity: "0.65" }
                    circle { cx: "209", cy: "140", r: "2", opacity: "0.5" }
                    circle { cx: "31", cy: "123", r: "1.5", opacity: "0.4" }
                }
                path { d: "M104 219H136", stroke: "currentColor", stroke_linecap: "round", opacity: "0.35" }
            }
        }
    }
}
