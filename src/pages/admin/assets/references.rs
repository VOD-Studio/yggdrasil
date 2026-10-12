//! References popover shared by all asset cards.
use crate::components::ui::{Popover, BADGE_BASE};
use crate::models::asset::{AssetDto, AssetRef, AssetRefPostStatus};
use crate::router::Route;
use dioxus::prelude::*;

// —— 引用明细浮层 ——

/// Material Symbols 图标 path（Google 原版存档见 public/icons/；
/// 组件内 fill=currentColor 随明暗主题）。
const ARTICLE_ICON_PATH: &str = "M277-279h275v-60H277v60Zm0-171h406v-60H277v60Zm0-171h406v-60H277v60Zm-97 501q-24 0-42-18t-18-42v-600q0-24 18-42t42-18h600q24 0 42 18t18 42v600q0 24-18 42t-42 18H180Zm0-60h600v-600H180v600Zm0-600v600-600Z";
const COMMENT_ICON_PATH: &str = "M880-80 720-240H140q-24 0-42-18t-18-42v-520q0-24 18-42t42-18h680q24 0 42 18t18 42v740ZM140-300h606l74 80v-600H140v520Zm0 0v-520 520Z";
const ACCOUNT_ICON_PATH: &str = "M222-255q63-44 125-67.5T480-346q71 0 133.5 23.5T739-255q44-54 62.5-109T820-480q0-145-97.5-242.5T480-820q-145 0-242.5 97.5T140-480q0 61 19 116t63 109Zm160.5-234.5Q343-529 343-587t39.5-97.5Q422-724 480-724t97.5 39.5Q617-645 617-587t-39.5 97.5Q538-450 480-450t-97.5-39.5ZM480-80q-82 0-155-31.5t-127.5-86Q143-252 111.5-325T80-480q0-83 31.5-155.5t86-127Q252-817 325-848.5T480-880q83 0 155.5 31.5t127 86q54.5 54.5 86 127T880-480q0 82-31.5 155t-86 127.5q-54.5 54.5-127 86T480-80Zm107.5-76Q640-172 691-212q-51-36-104-55t-107-19q-54 0-107 19t-104 55q51 40 103.5 56T480-140q55 0 107.5-16Zm-52-375.5Q557-553 557-587t-21.5-55.5Q514-664 480-664t-55.5 21.5Q403-621 403-587t21.5 55.5Q446-510 480-510t55.5-21.5ZM480-587Zm0 374Z";
const LINK_ICON_PATH: &str = "M450-280H280q-83 0-141.5-58.5T80-480q0-83 58.5-141.5T280-680h170v60H280q-58.33 0-99.17 40.76-40.83 40.77-40.83 99Q140-422 180.83-381q40.84 41 99.17 41h170v60ZM325-450v-60h310v60H325Zm185 170v-60h170q58.33 0 99.17-40.76 40.83-40.77 40.83-99Q820-538 779.17-579q-40.84-41-99.17-41H510v-60h170q83 0 141.5 58.5T880-480q0 83-58.5 141.5T680-280H510Z";
const OPEN_NEW_ICON_PATH: &str = "M180-120q-24 0-42-18t-18-42v-600q0-24 18-42t42-18h279v60H180v600h600v-279h60v279q0 24-18 42t-42 18H180Zm202-219-42-43 398-398H519v-60h321v321h-60v-218L382-339Z";

/// 浮层行内 Material Symbols 图标（标准 960 视口 + currentColor，明暗主题自适应）。
fn ref_icon(d: &'static str, class: &'static str) -> Element {
    rsx! {
        svg {
            class,
            xmlns: "http://www.w3.org/2000/svg",
            height: "24px",
            view_box: "0 -960 960 960",
            width: "24px",
            fill: "currentColor",
            path { d }
        }
    }
}

/// 引用明细浮层的一行：图标 + 文案 + 状态徽标 + 跳转。
///
/// 跳转规则：
/// - 已发布文章 → 原生 `<a target="_blank">` 新标签整页打开前台文章页
///   （规避 issue #36：admin→前台跨布局 Link + 目标页 suspend 触发 Dioxus 0.7
///   arena 双重回收 bug；新标签也不打断后台上下文）。
/// - 草稿/回收站文章 → 同 AdminLayout 的 WriteEdit 客户端导航（同布局链安全）；
///   页面随导航卸载，浮层随之销毁，无需显式关闭。
/// - 用户头像无管理页可链，纯展示；友链 → /admin/friends（同布局客户端导航）。
fn render_ref_row(r: &AssetRef, close: Callback<()>) -> Element {
    const ICON_CLASS: &str = "w-4 h-4 shrink-0 text-[var(--color-paper-tertiary)]";
    const ROW_CLASS: &str = "flex items-center gap-2.5 -mx-1.5 px-1.5 py-1.5 rounded-xl hover:bg-[var(--color-paper-theme)] transition-colors group/row";
    const TEXT_CLASS: &str = "flex-1 min-w-0 truncate text-sm text-[var(--color-paper-primary)]";
    const OPEN_CLASS: &str = "w-3.5 h-3.5 shrink-0 text-[var(--color-paper-tertiary)] opacity-0 group-hover/row:opacity-100 transition-opacity";
    match r {
        AssetRef::Note { note_id, title } => rsx! {
            Link {class:ROW_CLASS,to:Route::EditNote {id:*note_id},
                {ref_icon(ARTICLE_ICON_PATH, ICON_CLASS)}
                span {class:TEXT_CLASS,"{title}"}
                span {class:"text-xs text-[var(--color-paper-secondary)]","含历史版本"}
            }
        },
        AssetRef::Post {
            post_id,
            title,
            slug,
            status,
        } => {
            let content = rsx! {
                {ref_icon(ARTICLE_ICON_PATH, ICON_CLASS)}
                span { class: TEXT_CLASS, "{title}" }
                if let Some((badge_label, badge_class)) = status.badge() {
                    span { class: "{BADGE_BASE} shrink-0 {badge_class}", "{badge_label}" }
                }
            };
            if *status == AssetRefPostStatus::Published {
                rsx! {
                    a {
                        class: ROW_CLASS,
                        href: "/post/{slug}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        title: "新标签打开文章页",
                        onclick: move |_| close(()),
                        {content}
                        {ref_icon(OPEN_NEW_ICON_PATH, OPEN_CLASS)}
                    }
                }
            } else {
                rsx! {
                    Link {
                        class: ROW_CLASS,
                        to: Route::WriteEdit { id: *post_id },
                        {content}
                    }
                }
            }
        }
        AssetRef::Comment {
            author_name,
            post_id,
            post_title,
            post_slug,
            post_status,
            ..
        } => {
            let content = rsx! {
                {ref_icon(COMMENT_ICON_PATH, ICON_CLASS)}
                span { class: TEXT_CLASS,
                    span { class: "text-[var(--color-paper-secondary)]", "{author_name} 在 " }
                    "《{post_title}》"
                }
                if let Some((badge_label, badge_class)) = post_status.badge() {
                    span { class: "{BADGE_BASE} shrink-0 {badge_class}", "{badge_label}" }
                }
            };
            if *post_status == AssetRefPostStatus::Published {
                rsx! {
                    a {
                        class: ROW_CLASS,
                        href: "/post/{post_slug}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        title: "新标签打开所属文章",
                        onclick: move |_| close(()),
                        {content}
                        {ref_icon(OPEN_NEW_ICON_PATH, OPEN_CLASS)}
                    }
                }
            } else {
                rsx! {
                    Link {
                        class: ROW_CLASS,
                        to: Route::WriteEdit { id: *post_id },
                        {content}
                    }
                }
            }
        }
        AssetRef::UserAvatar { label, .. } => rsx! {
            div { class: "flex items-center gap-2.5 -mx-1.5 px-1.5 py-1.5 rounded-xl",
                {ref_icon(ACCOUNT_ICON_PATH, ICON_CLASS)}
                span { class: TEXT_CLASS,
                    span { class: "text-[var(--color-paper-secondary)]", "用户头像 · " }
                    "{label}"
                }
            }
        },
        AssetRef::FriendAvatar { name, .. } => rsx! {
            Link {
                class: ROW_CLASS,
                to: Route::FriendsAdmin {},
                {ref_icon(LINK_ICON_PATH, ICON_CLASS)}
                span { class: TEXT_CLASS,
                    span { class: "text-[var(--color-paper-secondary)]", "友链头像 · " }
                    "{name}"
                }
            }
        },
    }
}

#[component]
pub(super) fn AssetReferences(
    asset: AssetDto,
    anchor_x: i32,
    anchor_y: i32,
    pop_align: &'static str,
    on_close: Callback<()>,
) -> Element {
    let posts: Vec<&AssetRef> = asset
        .refs
        .iter()
        .filter(|r| matches!(r, AssetRef::Post { .. }))
        .collect();
    let comments: Vec<&AssetRef> = asset
        .refs
        .iter()
        .filter(|r| matches!(r, AssetRef::Comment { .. }))
        .collect();
    let notes: Vec<&AssetRef> = asset
        .refs
        .iter()
        .filter(|r| matches!(r, AssetRef::Note { .. }))
        .collect();
    let avatars: Vec<&AssetRef> = asset
        .refs
        .iter()
        .filter(|r| {
            matches!(
                r,
                AssetRef::UserAvatar { .. } | AssetRef::FriendAvatar { .. }
            )
        })
        .collect();
    let total = asset.ref_count;
    rsx! {
        Popover {
            open: true,
            anchor_x,
            anchor_y,
            placement: "bottom",
            align: pop_align,
            on_close: move |_| on_close(()),
            div { class: "w-80 max-w-[calc(100vw-2rem)] max-h-[60vh] overflow-y-auto",
                p { class: "text-sm font-medium text-[var(--color-paper-primary)]",
                    "引用此素材 · {total} 处"
                }
                if asset.refs.is_empty() {
                    // ref_count 与明细正常必然一致；不一致（极端竞态）时兜底。
                    p { class: "mt-2 text-xs text-[var(--color-paper-tertiary)]",
                        "未找到引用明细，请刷新后重试"
                    }
                }
                if !posts.is_empty() {
                    p { class: "mt-3 mb-1 text-[10px] font-mono tracking-widest text-[var(--color-paper-tertiary)]",
                        "文章 · {posts.len()}"
                    }
                    for r in posts {
                        {render_ref_row(r, on_close)}
                    }
                }
                if !comments.is_empty() {
                    // 评论与笔记分别展示，笔记历史引用也受保护。
                    p { class: "mt-3 mb-1 text-[10px] font-mono tracking-widest text-[var(--color-paper-tertiary)]",
                        "评论 · {comments.len()}"
                    }
                    for r in comments {
                        {render_ref_row(r, on_close)}
                    }
                }
                if !notes.is_empty() {
                    p {class:"mt-3 mb-1 text-[10px] font-mono tracking-widest text-[var(--color-paper-tertiary)]","笔记 · {notes.len()}"}
                    for r in notes {{render_ref_row(r,on_close)}}
                }
                if !avatars.is_empty() {
                    p { class: "mt-3 mb-1 text-[10px] font-mono tracking-widest text-[var(--color-paper-tertiary)]",
                        "头像 · {avatars.len()}"
                    }
                    for r in avatars {
                        {render_ref_row(r, on_close)}
                    }
                }
            }
        }
    }
}
