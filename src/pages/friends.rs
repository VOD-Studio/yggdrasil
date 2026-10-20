//! 友链页：两列名片、空状态与加载失败，以及可一键复制的「交换友链」信息。

use dioxus::prelude::*;

use crate::api::friends::list_friend_links;
use crate::api::settings::{get_site_card, SiteCard};
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::friends_skeleton::FriendsSkeleton;
use crate::components::ui::{Icon, IconName, LoadError};
use crate::models::friend_link::FriendLink;

/// 友链页面组件，对应路由 `/friends`。
#[component]
pub fn Friends() -> Element {
    rsx! {
        document::Title { "友链 · Yggdrasil" }
        div { class: "friends",
            SuspenseBoundary {
                fallback: |_| rsx! { DelayedSkeleton { FriendsSkeleton {} } },
                FriendsBody {}
            }
        }
    }
}

/// 页头标题与说明；骨架屏复用，数据到达前后不跳动。
#[component]
pub fn FriendsHeader(#[props(default)] count: Option<usize>) -> Element {
    rsx! {
        header { class: "friends-header page-enter",
            div {
                h1 { class: "friends-title", "友链" }
                p { class: "friends-intro", "一些认真记录、持续创造的朋友。" }
            }
            if let Some(count) = count {
                p { class: "friends-count tabular-nums", "{count} 位朋友 · 按添加顺序" }
            }
        }
    }
}

#[component]
fn FriendsBody() -> Element {
    // SSR 只轮询正在挂起的任务，提前发出的第二个请求会丢失唤醒；逐个等待。
    let mut links = use_server_future(list_friend_links)?;
    let card = use_server_future(get_site_card)?;
    let mut retrying = use_signal(|| false);
    let mut retry_failed = use_signal(|| false);

    let listing = links.read().as_ref().map(|r| r.as_ref().ok().cloned());
    let card = card.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let count = listing
        .as_ref()
        .and_then(|l| l.as_ref())
        .filter(|l| !l.is_empty())
        .map(Vec::len);

    rsx! {
        FriendsHeader { count }
        match listing {
            Some(Some(links)) if !links.is_empty() => rsx! {
                ul { class: "friends-grid page-enter", style: "--enter-index: 1",
                    for link in links {
                        li { key: "{link.id}", FriendCard { link } }
                    }
                }
            },
            Some(Some(_)) => rsx! {
                div { class: "friends-empty page-enter",
                    span { class: "friends-empty-icon", aria_hidden: "true", Icon { name: IconName::Users, size: 20 } }
                    p { class: "friends-empty-title", "还没有友链" }
                    p { class: "friends-empty-description", "想交换链接的话，可以先看看下面这里的信息。" }
                }
            },
            Some(None) => rsx! {
                LoadError {
                    title: "友链暂时没能加载",
                    class: "friends-error",
                    retrying: retrying(),
                    retry_failed: retry_failed(),
                    on_retry: move |_| {
                        retrying.set(true);
                        retry_failed.set(false);
                        spawn(async move {
                            let result = list_friend_links().await;
                            retrying.set(false);
                            match result {
                                Ok(list) => links.set(Some(Ok(list))),
                                Err(_) => retry_failed.set(true),
                            }
                        });
                    },
                }
            },
            None => rsx! {},
        }
        if let Some(card) = card {
            SwapInfo { card }
        }
    }
}

/// 去掉协议与末尾斜杠的主机名，用作名片副标题。
fn display_host(url: &str) -> &str {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
}

/// 整张名片是原生链接，在新标签页打开；头像加载失败时显示站名首字。
#[component]
fn FriendCard(link: FriendLink) -> Element {
    let mut img_loaded = use_signal(|| false);
    let id = link.id;
    let initial = link
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "?".to_string());

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
            span { class: "friend-avatar", aria_hidden: "true",
                span { "{initial}" }
                if let Some(avatar_url) = link.avatar_url.as_ref().filter(|url| !url.trim().is_empty()) {
                    img {
                        id: "friend-avatar-{id}",
                        class: if img_loaded() { "friend-avatar-image is-loaded" } else { "friend-avatar-image" },
                        src: "{avatar_url}",
                        alt: "",
                        width: "44",
                        height: "44",
                        loading: "lazy",
                        decoding: "async",
                        onload: move |_| img_loaded.set(true),
                        onerror: move |_| img_loaded.set(false),
                    }
                }
            }
            span { class: "friend-copy",
                span { class: "friend-name-row",
                    span { class: "friend-name", "{link.name}" }
                    span { class: "friend-go", aria_hidden: "true", Icon { name: IconName::ArrowUpRight, size: 15 } }
                }
                span { class: "friend-host", {display_host(&link.url)} }
                if !link.description.trim().is_empty() {
                    span { class: "friend-description", "{link.description}" }
                }
            }
        }
    }
}

/// 「交换友链」：本站名称、地址、简介与订阅，点击复制；复制失败时提示手动选择原文。
#[component]
fn SwapInfo(card: SiteCard) -> Element {
    let mut rows = vec![("名称", card.name.clone(), false)];
    if !card.url.is_empty() {
        rows.push(("地址", card.url.clone(), true));
    }
    rows.push(("简介", card.description.clone(), false));
    if !card.url.is_empty() {
        rows.push(("订阅", card.feed_url(), true));
    }
    rsx! {
        section { class: "friends-swap page-enter", style: "--enter-index: 2", aria_labelledby: "friends-swap-title",
            h2 { id: "friends-swap-title", class: "friends-swap-title", "交换友链" }
            p { class: "friends-swap-intro", "欢迎持续写作的个人站点。这是本站的信息，点击即可复制。" }
            dl { class: "friends-swap-grid",
                for (label, value, mono) in rows {
                    SwapRow { key: "{label}", label, value, mono }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
enum CopyState {
    Idle,
    Copied,
    Failed,
}

#[component]
fn SwapRow(label: &'static str, value: String, mono: bool) -> Element {
    #[allow(unused_mut)]
    let mut state = use_signal(|| CopyState::Idle);
    let button_label = match state() {
        CopyState::Idle => "复制",
        CopyState::Copied => "已复制",
        CopyState::Failed => "手动复制",
    };
    rsx! {
        div { class: "friends-swap-row",
            dt { "{label}" }
            // 复制失败时整段可一次选中，方便手动复制。
            dd {
                class: if mono { "is-mono" } else { "" },
                "data-selectable": (state() == CopyState::Failed).then_some("true"),
                "{value}"
            }
            button {
                r#type: "button",
                class: "friends-swap-copy",
                "data-state": match state() { CopyState::Idle => "idle", CopyState::Copied => "copied", CopyState::Failed => "failed" },
                aria_label: "复制{label}",
                onclick: move |_| {
                    let value = value.clone();
                    async move {
                        #[cfg(target_arch = "wasm32")]
                        {
                            if crate::utils::js::copy_text(&value).await {
                                state.set(CopyState::Copied);
                            } else {
                                state.set(CopyState::Failed);
                            }
                            crate::utils::time::sleep_ms(1600).await;
                            if state() == CopyState::Copied {
                                state.set(CopyState::Idle);
                            }
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        let _ = value;
                    }
                },
                "{button_label}"
            }
            span { class: "sr-only", role: "status", aria_live: "polite",
                match state() {
                    CopyState::Copied => "已复制{label}",
                    CopyState::Failed => "复制失败，请手动选择{label}复制",
                    CopyState::Idle => "",
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::display_host;

    #[test]
    fn host_drops_scheme_and_trailing_slash() {
        assert_eq!(display_host("https://example.com/"), "example.com");
        assert_eq!(
            display_host("http://blog.example.dev/notes"),
            "blog.example.dev/notes"
        );
    }
}
