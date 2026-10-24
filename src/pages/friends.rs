//! 友链页：两列名片、空状态与加载失败，以及可一键复制的「交换友链」信息。

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::friends::list_friend_links;
use crate::api::settings::{get_site_card, SiteCard};
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::friends_skeleton::FriendsSkeleton;
use crate::components::ui::{Icon, IconName, LoadError};
use crate::models::friend_link::FriendLink;

/// 友链路由，对应 `/friends`（`SsrMode::InOrder`）。
///
/// 友链列表与站点名片都在 `data()` 里创建；列表失败只让列表区块显示重试卡片，
/// 名片失败则直接不显示「交换友链」，页面其余部分照常。
pub struct FriendsRoute {
    links: Resource<Result<Vec<FriendLink>, ServerFnError>>,
    card: Resource<Result<SiteCard, ServerFnError>>,
}

#[lazy_route]
impl LazyRoute for FriendsRoute {
    fn data() -> Self {
        Self {
            links: Resource::new(|| (), |_| list_friend_links()),
            card: Resource::new(|| (), |_| get_site_card()),
        }
    }

    fn view(this: Self) -> AnyView {
        let FriendsRoute { links, card } = this;
        view! {
            <Title text="友链 · Yggdrasil"/>
            <div class="friends">
                <Suspense fallback=|| {
                    view! {
                        <DelayedSkeleton>
                            <FriendsSkeleton/>
                        </DelayedSkeleton>
                    }
                }>
                    {move || Suspend::new(async move {
                        // 逐个等待，保持 SSR 唤醒顺序确定。
                        let links = links.await.ok();
                        let card = card.await.ok();
                        view! { <FriendsBody initial=links card=card/> }
                    })}
                </Suspense>
            </div>
        }
        .into_any()
    }
}

/// 页头标题与说明；骨架屏复用，数据到达前后不跳动。
///
/// `count` 缺省时不渲染人数行（骨架屏）；可传值、`Option` 或信号。
#[component]
pub fn FriendsHeader(#[prop(optional, into)] count: MaybeProp<usize>) -> impl IntoView {
    view! {
        <header class="friends-header page-enter">
            <div>
                <h1 class="friends-title">"友链"</h1>
                <p class="friends-intro">"一些认真记录、持续创造的朋友。"</p>
            </div>
            {move || {
                count
                    .get()
                    .map(|count| {
                        view! {
                            <p class="friends-count tabular-nums">
                                {format!("{count} 位朋友 · 按添加顺序")}
                            </p>
                        }
                    })
            }}
        </header>
    }
}

/// 友链主体。`initial` 为 `None` 表示列表加载失败；重试结果写进本地信号，
/// 不重取外层 `Resource`（否则整段 DOM 会退回 `Suspense` 的骨架屏）。
#[component]
fn FriendsBody(initial: Option<Vec<FriendLink>>, card: Option<SiteCard>) -> impl IntoView {
    let listing = RwSignal::new(initial);
    let retrying = RwSignal::new(false);
    let retry_failed = RwSignal::new(false);

    let count = move || {
        listing.with(|listing| {
            listing
                .as_ref()
                .filter(|links| !links.is_empty())
                .map(Vec::len)
        })
    };

    let on_retry = Callback::new(move |()| {
        retrying.set(true);
        retry_failed.set(false);
        leptos::task::spawn_local(async move {
            let result = list_friend_links().await;
            let _ = retrying.try_set(false);
            match result {
                Ok(list) => {
                    let _ = listing.try_set(Some(list));
                }
                Err(_) => {
                    let _ = retry_failed.try_set(true);
                }
            }
        });
    });

    view! {
        <FriendsHeader count=Signal::derive(count)/>
        {move || match listing.get() {
            Some(links) if !links.is_empty() => {
                view! {
                    <ul class="friends-grid page-enter" style="--enter-index: 1">
                        {links
                            .into_iter()
                            .map(|link| view! { <li><FriendCard link=link/></li> })
                            .collect_view()}
                    </ul>
                }
                    .into_any()
            }
            Some(_) => {
                view! {
                    <div class="friends-empty page-enter">
                        <span class="friends-empty-icon" aria-hidden="true">
                            <Icon name=IconName::Users size=20/>
                        </span>
                        <p class="friends-empty-title">"还没有友链"</p>
                        <p class="friends-empty-description">
                            "想交换链接的话，可以先看看下面这里的信息。"
                        </p>
                    </div>
                }
                    .into_any()
            }
            None => {
                view! {
                    <LoadError
                        title="友链暂时没能加载"
                        class="friends-error"
                        retrying=retrying
                        retry_failed=retry_failed
                        on_retry=on_retry
                    />
                }
                    .into_any()
            }
        }}
        {card.map(|card| view! { <SwapInfo card=card/> })}
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
fn FriendCard(link: FriendLink) -> impl IntoView {
    let img_loaded = RwSignal::new(false);
    let id = link.id;
    let initial = link
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "?".to_string());

    // SSR 图片可能在 hydration 前已完成加载，补查缓存状态；失败时始终显示首字。
    Effect::new(move |_| {
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

    let avatar = link
        .avatar_url
        .as_ref()
        .filter(|url| !url.trim().is_empty())
        .map(|avatar_url| {
            view! {
                <img
                    id=format!("friend-avatar-{id}")
                    class=move || {
                        if img_loaded.get() {
                            "friend-avatar-image is-loaded"
                        } else {
                            "friend-avatar-image"
                        }
                    }
                    src=avatar_url.clone()
                    alt=""
                    width="44"
                    height="44"
                    loading="lazy"
                    decoding="async"
                    on:load=move |_| img_loaded.set(true)
                    on:error=move |_| img_loaded.set(false)
                />
            }
        });
    let description = (!link.description.trim().is_empty())
        .then(|| view! { <span class="friend-description">{link.description.clone()}</span> });

    view! {
        <a
            class="friend-card"
            href=link.url.clone()
            target="_blank"
            rel="noopener noreferrer"
            aria-label=format!("访问 {}（在新标签页打开）", link.name)
        >
            <span class="friend-avatar" aria-hidden="true">
                <span>{initial}</span>
                {avatar}
            </span>
            <span class="friend-copy">
                <span class="friend-name-row">
                    <span class="friend-name">{link.name.clone()}</span>
                    <span class="friend-go" aria-hidden="true">
                        <Icon name=IconName::ArrowUpRight size=15/>
                    </span>
                </span>
                <span class="friend-host">{display_host(&link.url).to_string()}</span>
                {description}
            </span>
        </a>
    }
}

/// 「交换友链」：本站名称、地址、简介与订阅，点击复制；复制失败时提示手动选择原文。
#[component]
fn SwapInfo(card: SiteCard) -> impl IntoView {
    let mut rows = vec![("名称", card.name.clone(), false)];
    if !card.url.is_empty() {
        rows.push(("地址", card.url.clone(), true));
    }
    rows.push(("简介", card.description.clone(), false));
    if !card.url.is_empty() {
        rows.push(("订阅", card.feed_url(), true));
    }
    view! {
        <section
            class="friends-swap page-enter"
            style="--enter-index: 2"
            aria-labelledby="friends-swap-title"
        >
            <h2 id="friends-swap-title" class="friends-swap-title">
                "交换友链"
            </h2>
            <p class="friends-swap-intro">"欢迎持续写作的个人站点。这是本站的信息，点击即可复制。"</p>
            <dl class="friends-swap-grid">
                {rows
                    .into_iter()
                    .map(|(label, value, mono)| view! { <SwapRow label=label value=value mono=mono/> })
                    .collect_view()}
            </dl>
        </section>
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
fn SwapRow(label: &'static str, value: String, mono: bool) -> impl IntoView {
    let state = RwSignal::new(CopyState::Idle);
    let on_click = {
        let value = value.clone();
        move |_| {
            #[cfg(target_arch = "wasm32")]
            {
                let value = value.clone();
                leptos::task::spawn_local(async move {
                    let next = if crate::utils::js::copy_text(&value).await {
                        CopyState::Copied
                    } else {
                        CopyState::Failed
                    };
                    let _ = state.try_set(next);
                    crate::utils::time::sleep_ms(1600).await;
                    if state.try_get() == Some(CopyState::Copied) {
                        let _ = state.try_set(CopyState::Idle);
                    }
                });
            }
            #[cfg(not(target_arch = "wasm32"))]
            let _ = &value;
        }
    };
    view! {
        <div class="friends-swap-row">
            <dt>{label}</dt>
            // 复制失败时整段可一次选中，方便手动复制。
            <dd
                class=if mono { "is-mono" } else { "" }
                data-selectable=move || (state.get() == CopyState::Failed).then_some("true")
            >
                {value}
            </dd>
            <button
                type="button"
                class="friends-swap-copy"
                data-state=move || match state.get() {
                    CopyState::Idle => "idle",
                    CopyState::Copied => "copied",
                    CopyState::Failed => "failed",
                }
                aria-label=format!("复制{label}")
                on:click=on_click
            >
                {move || match state.get() {
                    CopyState::Idle => "复制",
                    CopyState::Copied => "已复制",
                    CopyState::Failed => "手动复制",
                }}
            </button>
            <span class="sr-only" role="status" aria-live="polite">
                {move || match state.get() {
                    CopyState::Copied => format!("已复制{label}"),
                    CopyState::Failed => format!("复制失败，请手动选择{label}复制"),
                    CopyState::Idle => String::new(),
                }}
            </span>
        </div>
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
