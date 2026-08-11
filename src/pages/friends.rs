//! 友链页面模块。
//!
//! 对应路由 `/friends`，展示通过 `list_friend_links` server function 获取的活跃友链
//! 卡片网格。数据获取与三态渲染结构照 `archives.rs`；卡片视觉语言融合「极简名片」
//! 设计：无实线边框 + 大圆角（`rounded-card` 32px）+ hover 上浮 + 名称转全站强调色。
//! 头像磁贴内置兜底：无头像或图片加载失败时显示名称首字符。

use dioxus::prelude::*;

use crate::api::friends::list_friend_links;
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::friends_skeleton::FriendsSkeleton;
use crate::models::friend_link::FriendLink;

/// 友链页面组件，对应路由 `/friends`。
///
/// 渲染页面标题，并委托给 `FriendsContent` 展示友链卡片网格。
#[component]
pub fn Friends() -> Element {
    rsx! {
        div { class: "animate-page-enter",
            header { class: "page-header mb-6",
                h1 { class: "text-4xl font-bold text-paper-primary tracking-tight",
                    "友链"
                }
                p { class: "text-paper-secondary mt-2", "交换过链接的伙伴们" }
            }
            FriendsContent {}
        }
    }
}

/// 友链页面内容组件。
///
/// 通过 `use_server_future` 获取全部活跃友链；加载中显示骨架屏，失败显示错误提示。
#[component]
fn FriendsContent() -> Element {
    let links_res = use_server_future(list_friend_links)?;

    let links_data = links_res.read();
    match &*links_data {
        Some(Ok(links)) => {
            if links.is_empty() {
                rsx! {
                    EmptyState {
                        title: "还没有友链",
                        description: "在后台「友链」中添加第一位伙伴吧。",
                    }
                }
            } else {
                rsx! {
                    div { class: "mt-2 text-base text-paper-secondary",
                        "共 "
                        span { class: "font-medium text-paper-primary", "{links.len()}" }
                        " 位伙伴"
                    }
                    div { class: "grid grid-cols-1 sm:grid-cols-2 gap-6 mt-6",
                        for link in links.iter() {
                            FriendCard { key: "{link.id}", link: link.clone() }
                        }
                    }
                }
            }
        }
        Some(Err(_)) => {
            rsx! {
                div { class: "text-center text-red-500 dark:text-red-400 py-20", "加载失败" }
            }
        }
        None => {
            rsx! {
                DelayedSkeleton { FriendsSkeleton {} }
            }
        }
    }
}

/// 单张友链名片卡。
///
/// 头像区为 56px 圆角磁贴：有 `avatar_url` 且未加载失败时渲染 `<img>` 覆盖磁贴，
/// 否则显示名称首字符兜底。整卡被 `<a target="_blank">` 覆盖链接到对方站点。
#[component]
fn FriendCard(link: FriendLink) -> Element {
    // 图片加载失败时置位，之后不再渲染 <img>（保持首字符磁贴兜底）。
    let mut img_failed = use_signal(|| false);
    let initial: String = link
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".to_string());

    rsx! {
        div { class: "group relative bg-paper-entry rounded-card p-8 shadow-sm border border-transparent hover:border-paper-border hover:-translate-y-0.5 hover:shadow-md transition-all duration-300",
            div { class: "flex items-start gap-5",
                div { class: "relative h-14 w-14 shrink-0 rounded-2xl bg-paper-code-bg flex items-center justify-center overflow-hidden",
                    span { class: "text-xl font-semibold text-paper-primary select-none",
                        "{initial}"
                    }
                    if let Some(avatar_url) = &link.avatar_url {
                        if !img_failed() {
                            img {
                                class: "absolute inset-0 w-full h-full object-cover rounded-2xl",
                                src: "{avatar_url}",
                                alt: "{link.name} 的头像",
                                onerror: move |_| img_failed.set(true),
                            }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    h3 { class: "text-lg font-semibold text-paper-primary group-hover:text-paper-accent transition-colors truncate",
                        "{link.name}"
                    }
                    if !link.description.is_empty() {
                        p { class: "text-sm text-paper-secondary mt-1.5 leading-relaxed line-clamp-2",
                            "{link.description}"
                        }
                    }
                }
            }
            div { class: "mt-6 flex items-center gap-1",
                span { class: "text-sm text-paper-tertiary group-hover:text-paper-secondary transition-colors",
                    "访问站点"
                }
                span { class: "text-sm text-paper-tertiary group-hover:text-paper-secondary group-hover:translate-x-0.5 group-hover:-translate-y-0.5 transition-transform inline-block",
                    "↗"
                }
            }
            a {
                class: "absolute inset-0 z-10",
                href: "{link.url}",
                target: "_blank",
                rel: "noopener noreferrer",
                aria_label: "访问 {link.name}",
            }
        }
    }
}
