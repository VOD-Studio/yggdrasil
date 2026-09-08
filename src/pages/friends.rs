//! 友链页：纸面名片、交错入场与完整的加载 / 空 / 错误状态。

use dioxus::prelude::*;

use crate::api::friends::list_friend_links;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::friends_skeleton::FriendsSkeleton;
use crate::models::friend_link::FriendLink;
use crate::router::Route;

/// 友链页面组件，对应路由 `/friends`。
#[component]
pub fn Friends() -> Element {
    rsx! {
        div { class: "friends-page",
            header { class: "friends-intro",
                div { class: "friends-intro-copy",
                    p { class: "friends-eyebrow", span {} "FRIENDS & NEIGHBORS" }
                    h1 { "友链" span { class: "friends-title-dot", "。" } }
                    p { class: "friends-intro-lead", "循着链接，去看看另一片风景。" }
                    p { class: "friends-intro-note", "一些认真记录、持续创造的朋友。" }
                }
                div { class: "friends-orbit", aria_hidden: "true",
                    div { class: "friends-orbit-ring friends-orbit-ring-one" }
                    div { class: "friends-orbit-ring friends-orbit-ring-two" }
                    span { class: "friends-orbit-node friends-orbit-node-one", "✳" }
                    span { class: "friends-orbit-node friends-orbit-node-two", "↗" }
                    span { class: "friends-orbit-dot friends-orbit-dot-one" }
                    span { class: "friends-orbit-dot friends-orbit-dot-two" }
                    span { class: "friends-orbit-caption", "小小链接，大大世界" }
                }
            }
            SuspenseBoundary {
                fallback: |_| rsx! { DelayedSkeleton { FriendsSkeleton { with_header: false } } },
                FriendsContent {}
            }
            footer { class: "friends-colophon",
                span { aria_hidden: "true", "✳" }
                p { "链接相邻，灵感相通。" }
            }
        }
    }
}

/// 保留服务端顺序，局部重试不会让标题与整页一起消失。
#[component]
fn FriendsContent() -> Element {
    let mut links_res = use_server_future(list_friend_links)?;
    let links_data = links_res.read();

    match &*links_data {
        Some(Ok(links)) if !links.is_empty() => rsx! {
            section { aria_labelledby: "friends-list-title",
                div { class: "friends-section-heading",
                    h2 { id: "friends-list-title", "我的友邻" span { class: "friends-count", "{links.len():02}" } }
                    span { class: "friends-section-hint", "每一扇门，都通往一个有趣的世界" }
                }
                ul { class: "friends-grid",
                    for (index, link) in links.iter().enumerate() {
                        li {
                            key: "{link.id}",
                            class: "friends-card-enter",
                            style: "--friend-delay: {index.min(7) * 65 + 100}ms",
                            FriendCard { link: link.clone(), number: index + 1 }
                        }
                    }
                }
            }
        },
        Some(Ok(_)) => rsx! {
            section { class: "friends-state", role: "status",
                span { class: "friends-state-symbol", aria_hidden: "true", "✳" }
                h2 { "留一扇门，等一位新朋友" }
                p { "新的相遇正在路上，先去读读这里的故事吧。" }
                Link { class: "friends-action", to: Route::Home {}, "去读文章" span { aria_hidden: "true", "↗" } }
            }
        },
        Some(Err(_)) => rsx! {
            section { class: "friends-state", role: "alert",
                span { class: "friends-state-symbol", aria_hidden: "true", "↻" }
                h2 { "暂时没能遇见朋友们" }
                p { "友链加载失败，请稍后再试。" }
                button {
                    class: "friends-action",
                    r#type: "button",
                    onclick: move |_| links_res.restart(),
                    "重新加载" span { aria_hidden: "true", "↻" }
                }
            }
        },
        None => rsx! { DelayedSkeleton { FriendsSkeleton { with_header: false } } },
    }
}

/// 整张名片是原生链接，鼠标、触屏和键盘共享同一交互入口。
#[component]
fn FriendCard(link: FriendLink, number: usize) -> Element {
    let mut img_loaded = use_signal(|| false);
    let id = link.id;
    let initial = link
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "?".to_string());
    let address = link
        .url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');

    // SSR 图片可能在 hydration 前已完成加载，补查缓存状态；失败时始终显示首字。
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            if let Some(img) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id(&format!("friend-avatar-{id}")))
                .and_then(|element| element.dyn_into::<web_sys::HtmlImageElement>().ok())
            {
                if img.complete() {
                    img_loaded.set(img.natural_width() > 0);
                }
            }
        }
    });

    rsx! {
        a {
            class: "friend-card",
            href: "{link.url}",
            target: "_blank",
            rel: "noopener noreferrer",
            aria_label: "访问 {link.name}（在新标签页打开）",
            div { class: "friend-card-top",
                div { class: "friend-avatar", aria_hidden: "true",
                    span { "{initial}" }
                    if let Some(avatar_url) = link.avatar_url.as_ref().filter(|url| !url.trim().is_empty()) {
                        img {
                            id: "friend-avatar-{id}",
                            class: if img_loaded() { "friend-avatar-image is-loaded" } else { "friend-avatar-image" },
                            src: "{avatar_url}",
                            alt: "",
                            width: "52",
                            height: "52",
                            loading: "lazy",
                            decoding: "async",
                            onload: move |_| img_loaded.set(true),
                            onerror: move |_| img_loaded.set(false),
                        }
                    }
                }
                span { class: "friend-number", aria_hidden: "true", "{number:02}" }
            }
            div { class: "friend-card-copy",
                h3 { "{link.name}" }
                p { class: "friend-description",
                    if link.description.trim().is_empty() {
                        "故事就在门的另一边，去看看吧。"
                    } else {
                        "{link.description}"
                    }
                }
            }
            div { class: "friend-card-bottom",
                span { class: "friend-address", "{address}" }
                span { class: "friend-visit",
                    span { class: "friend-visit-label", "去串个门" }
                    span { class: "friend-arrow", aria_hidden: "true", span { "↗" } }
                }
            }
        }
    }
}
