//! 评论管理（`/admin/comments`）。
//!
//! 标题行（总数与待审数）→ 待审 / 已通过 / 垃圾 / 回收站选项卡（带数量）→ 评论流：
//! 头像首字、作者与网址、「回复 X」、时间、正文与图片、所属文章与按状态给出的操作
//! （通过、回复、标为垃圾、不是垃圾、移到回收站、恢复）。回复在行内展开 Composer，
//! 以管理员身份直接发表。勾选后底部浮起批量操作条。默认打开待审，没有待审时打开
//! 已通过。数据仅在 WASM 前端读取。样式见 styles/pages/admin-comments.css。

use crate::components::admin_layout::AdminPageHead;
use std::collections::HashSet;

use dioxus::prelude::*;

use crate::api::comments::{AllCommentsResponse, CommentCounts};
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::admin_comments_skeleton::AdminCommentsListSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Checkbox, FilterTabs, IconButton, IconName,
    LoadError, TablePager,
};
use crate::models::comment::{AdminComment, CommentStatus};
use crate::utils::error::failure_message;
use crate::utils::time::to_local;

/// 每页评论数，与服务端列表接口一致。
const COMMENTS_PER_PAGE: i32 = 20;

/// 评论正文拆成文字与图片地址：Markdown 图片 `![alt](url)` 单独显示为缩略图。
fn split_images(md: &str) -> (String, Vec<String>) {
    let mut text = String::new();
    let mut images = Vec::new();
    let mut rest = md;
    while let Some(start) = rest.find("![") {
        let after = &rest[start..];
        let parsed = after.find("](").and_then(|mid| {
            let tail = &after[mid + 2..];
            tail.find(')')
                .map(|end| (mid + 2 + end + 1, tail[..end].trim()))
        });
        match parsed {
            Some((len, url)) if !url.is_empty() => {
                text.push_str(&rest[..start]);
                images.push(url.split_whitespace().next().unwrap_or(url).to_string());
                rest = &rest[start + len..];
            }
            _ => {
                text.push_str(&rest[..start + 2]);
                rest = &rest[start + 2..];
            }
        }
    }
    text.push_str(rest);
    (text.trim().to_string(), images)
}

/// 某个选项卡对应的条数。
fn count_of(counts: &CommentCounts, tab: &str) -> i64 {
    match tab {
        "pending" => counts.pending,
        "approved" => counts.approved,
        "spam" => counts.spam,
        "trash" => counts.trash,
        _ => 0,
    }
}

/// 评论管理入口，默认第 1 页。
#[component]
pub fn AdminComments() -> Element {
    rsx! {
        AdminCommentsPage { page: 1 }
    }
}

/// 评论管理。`page` 来自 `/admin/comments/page/:page`，之后翻页只改本页状态。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn AdminCommentsPage(page: i32) -> Element {
    let toast = use_toast();
    // 空字符串表示还没决定：先读待审，没有待审时换到已通过。
    let mut tab = use_signal(|| {
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .and_then(|w| w.location().search().ok())
                .and_then(|s| {
                    s.trim_start_matches('?')
                        .split('&')
                        .find_map(|pair| pair.strip_prefix("status=").map(str::to_string))
                })
                .filter(|s| matches!(s.as_str(), "pending" | "approved" | "spam" | "trash"))
                .unwrap_or_default()
        }
        #[cfg(not(target_arch = "wasm32"))]
        String::new()
    });
    let mut current_page = use_signal(|| page.max(1));
    let mut data = use_signal(|| None::<AllCommentsResponse>);
    let mut error = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut reload = use_signal(|| 0_u32);
    let mut selected = use_signal(HashSet::<i64>::new);
    let mut busy = use_signal(|| false);
    let mut replying = use_signal(|| None::<i64>);

    use_effect(move || {
        let requested = tab();
        let p = current_page();
        let _ = reload();
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            loading.set(true);
            let status = if requested.is_empty() {
                "pending".to_string()
            } else {
                requested.clone()
            };
            match crate::api::comments::get_all_comments(p, Some(status)).await {
                Ok(resp) => {
                    if requested.is_empty() {
                        if resp.counts.pending == 0 {
                            tab.set("approved".into());
                            return;
                        }
                        tab.set("pending".into());
                    }
                    data.set(Some(resp));
                    error.set(false);
                }
                Err(_) => error.set(true),
            }
            loading.set(false);
        });
    });

    let mut refresh = move || {
        selected.write().clear();
        reload += 1;
    };
    // 单条与批量共用：status 为 approved / spam / trash，或 restore。
    let mut apply = move |ids: Vec<i64>, action: &'static str| {
        if ids.is_empty() || busy() {
            return;
        }
        busy.set(true);
        spawn(async move {
            let result = if action == "restore" {
                crate::api::comments::restore_comments(ids.clone()).await
            } else {
                crate::api::comments::batch_update_comment_status(ids.clone(), action.into()).await
            };
            match result {
                Ok(res) if res.success => {
                    let n = res.updated_count;
                    toast.success(match action {
                        "approved" => format!("已通过 {n} 条评论"),
                        "spam" => format!("已把 {n} 条标为垃圾"),
                        "trash" => format!("已把 {n} 条移到回收站"),
                        _ => res.message,
                    });
                    refresh();
                }
                Ok(res) => toast.error(res.message),
                Err(e) => toast.error(failure_message("操作失败", &e)),
            }
            busy.set(false);
        });
    };

    let resp = data();
    let counts = resp.as_ref().map(|r| r.counts).unwrap_or_default();
    let active = tab();
    let rows = resp
        .as_ref()
        .map(|r| r.comments.clone())
        .unwrap_or_default();
    let total = resp.as_ref().map(|r| r.total).unwrap_or(0);
    let selection = selected();
    let all_count = counts.pending + counts.approved + counts.spam;
    let page_ids: Vec<i64> = rows.iter().map(|c| c.id).collect();
    let page_all = !page_ids.is_empty() && page_ids.iter().all(|id| selection.contains(id));

    rsx! {
        div { class: "admin-comments",
            AdminPageHead { title: "评论",
                if resp.is_some() {
                    "{all_count} 条 · 待审 {counts.pending}"
                }
            }
            div { class: "admin-toolbar",
                FilterTabs {
                    class: "",
                    aria_label: "评论状态",
                    items: vec![("pending", "待审"), ("approved", "已通过"), ("spam", "垃圾"), ("trash", "回收站")],
                    counts: if resp.is_some() { vec![counts.pending, counts.approved, counts.spam, counts.trash] } else { Vec::new() },
                    active_value: active.clone(),
                    on_change: move |value: String| {
                        selected.write().clear();
                        replying.set(None);
                        current_page.set(1);
                        tab.set(value);
                    },
                }
                if !rows.is_empty() {
                    Checkbox {
                        checked: page_all,
                        label: "选择本页全部评论".to_string(),
                        onchange: move |on: bool| {
                            let mut set = selected.write();
                            for id in &page_ids {
                                if on {
                                    set.insert(*id);
                                } else {
                                    set.remove(id);
                                }
                            }
                        },
                    }
                }
            }

            if error() {
                LoadError { title: "评论加载失败", on_retry: move |_| reload += 1 }
            } else if resp.is_none() || (loading() && rows.is_empty() && count_of(&counts, &active) > 0) {
                AdminCommentsListSkeleton {}
            } else if rows.is_empty() {
                div { class: "admin-panel",
                    match active.as_str() {
                        "pending" => rsx! {
                            EmptyState { icon: IconName::Check, title: "没有待审评论", description: "新评论需要审核时会出现在这里。" }
                        },
                        "spam" => rsx! {
                            EmptyState { icon: IconName::Shield, title: "没有垃圾评论", description: "被标为垃圾的评论会集中在这里。" }
                        },
                        "trash" => rsx! {
                            EmptyState { icon: IconName::Trash, title: "回收站是空的", description: "移到回收站的评论可以在这里恢复。" }
                        },
                        _ => rsx! {
                            EmptyState { icon: IconName::MessageSquare, title: "还没有评论", description: "读者的评论通过审核后会出现在这里。" }
                        },
                    }
                }
            } else {
                div { class: "admin-panel admin-comment-list", "aria-busy": loading().then_some("true"),
                    for comment in rows.into_iter() {
                        CommentItem {
                            key: "{comment.id}",
                            selected: selection.contains(&comment.id),
                            replying: replying() == Some(comment.id),
                            busy: busy(),
                            on_select: move |(id, on): (i64, bool)| {
                                if on {
                                    selected.write().insert(id);
                                } else {
                                    selected.write().remove(&id);
                                }
                            },
                            on_action: move |(id, action): (i64, &'static str)| {
                                if action == "reply" {
                                    replying.set(if replying() == Some(id) { None } else { Some(id) });
                                } else {
                                    apply(vec![id], action);
                                }
                            },
                            on_replied: move |_| {
                                replying.set(None);
                                refresh();
                            },
                            comment,
                        }
                    }
                    TablePager {
                        page: current_page(),
                        total,
                        per_page: COMMENTS_PER_PAGE,
                        unit: "条",
                        on_change: move |p| current_page.set(p),
                    }
                }
            }

            if !selection.is_empty() {
                div { class: "batch-bar", role: "toolbar", aria_label: "批量操作",
                    span { class: "batch-bar-count", "已选 {selection.len()} 条" }
                    if active == "trash" {
                        button {
                            class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                            r#type: "button",
                            disabled: busy(),
                            onclick: move |_| apply(selected().into_iter().collect(), "restore"),
                            "恢复"
                        }
                    } else {
                        if active != "approved" {
                            button {
                                class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                                r#type: "button",
                                disabled: busy(),
                                onclick: move |_| apply(selected().into_iter().collect(), "approved"),
                                "通过"
                            }
                        }
                        if active != "spam" {
                            button {
                                class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                                r#type: "button",
                                disabled: busy(),
                                onclick: move |_| apply(selected().into_iter().collect(), "spam"),
                                "标为垃圾"
                            }
                        }
                        button {
                            class: "{button_class(ButtonVariant::Ghost, ButtonSize::Sm)} batch-bar-danger",
                            r#type: "button",
                            disabled: busy(),
                            onclick: move |_| apply(selected().into_iter().collect(), "trash"),
                            "移到回收站"
                        }
                    }
                    span { class: "batch-bar-sep", aria_hidden: "true" }
                    IconButton {
                        icon: IconName::X,
                        label: "取消选择",
                        size: ButtonSize::Sm,
                        onclick: move |_| selected.write().clear(),
                    }
                }
            }
        }
    }
}

/// 评论流的一条。操作按状态给出，回调 `(评论 id, 操作)`。
#[component]
fn CommentItem(
    comment: AdminComment,
    selected: bool,
    replying: bool,
    busy: bool,
    on_select: EventHandler<(i64, bool)>,
    on_action: EventHandler<(i64, &'static str)>,
    on_replied: EventHandler<()>,
) -> Element {
    let id = comment.id;
    let (text, images) = split_images(&comment.content_md);
    let initial = comment
        .author_name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    let site = comment
        .author_url
        .as_deref()
        .filter(|u| !u.trim().is_empty())
        .map(|u| {
            u.trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/')
                .to_string()
        });
    let when = to_local(comment.created_at)
        .format("%Y-%m-%d %H:%M")
        .to_string();
    let actions: &[(&'static str, &'static str)] = match comment.status {
        CommentStatus::Pending => &[
            ("approved", "通过"),
            ("spam", "标为垃圾"),
            ("trash", "移到回收站"),
        ],
        CommentStatus::Approved => &[
            ("reply", "回复"),
            ("spam", "标为垃圾"),
            ("trash", "移到回收站"),
        ],
        CommentStatus::Spam => &[("approved", "不是垃圾"), ("trash", "移到回收站")],
        CommentStatus::Trash => &[("restore", "恢复为待审")],
    };
    rsx! {
        article {
            class: "admin-comment",
            "data-selected": selected.then_some("true"),
            "data-reply": comment.parent_id.is_some().then_some("true"),
            Checkbox {
                checked: selected,
                label: format!("选择 {} 的评论", comment.author_name),
                onchange: move |on| on_select.call((id, on)),
            }
            if comment.avatar_url.is_empty() {
                span { class: "admin-comment-avatar", aria_hidden: "true", "{initial}" }
            } else {
                img { class: "admin-comment-avatar", src: "{comment.avatar_url}", alt: "", loading: "lazy" }
            }
            div { class: "admin-comment-body",
                div { class: "admin-comment-head",
                    span { class: "admin-comment-author", "{comment.author_name}" }
                    if let Some(site) = site {
                        span { class: "admin-comment-site", "{site}" }
                    }
                    if let Some(to) = &comment.reply_to {
                        span { class: "admin-comment-to", "回复 {to}" }
                    }
                    time { "{when}" }
                }
                if !text.is_empty() {
                    p { class: "admin-comment-text", "{text}" }
                }
                if !images.is_empty() {
                    div { class: "admin-comment-images",
                        for url in images {
                            a { key: "{url}", href: "{url}", target: "_blank", rel: "noopener",
                                img { src: "{url}?thumb=240x240", alt: "评论图片", loading: "lazy" }
                            }
                        }
                    }
                }
                div { class: "admin-comment-foot",
                    a {
                        class: "admin-comment-post",
                        href: "/post/{comment.post_slug}",
                        target: "_blank",
                        rel: "noopener",
                        "于《{comment.post_title}》"
                    }
                    div { class: "admin-comment-actions",
                        for (action, label) in actions.iter().copied() {
                            button {
                                key: "{action}",
                                class: "{button_class(ButtonVariant::Ghost, ButtonSize::Xs)}",
                                "data-danger": (action == "trash").then_some("true"),
                                r#type: "button",
                                disabled: busy,
                                aria_expanded: (action == "reply").then(|| replying.to_string()),
                                onclick: move |_| on_action.call((id, action)),
                                "{label}"
                            }
                        }
                    }
                }
                if replying {
                    ReplyComposer {
                        post_id: comment.post_id,
                        parent_id: id,
                        author: comment.author_name.clone(),
                        on_done: move |_| on_replied.call(()),
                    }
                }
            }
        }
    }
}

/// 行内回复：以当前管理员身份发表，直接通过。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn ReplyComposer(
    post_id: i32,
    parent_id: i64,
    author: String,
    on_done: EventHandler<()>,
) -> Element {
    let toast = use_toast();
    let mut text = use_signal(String::new);
    let mut sending = use_signal(|| false);
    let mut send = move || {
        let content = text.peek().trim().to_string();
        if content.is_empty() || sending() {
            return;
        }
        sending.set(true);
        spawn(async move {
            match crate::api::comments::create_comment(
                post_id,
                Some(parent_id),
                String::new(),
                String::new(),
                None,
                content,
                String::new(),
            )
            .await
            {
                Ok(res) if res.success => {
                    toast.success("回复已发表");
                    on_done.call(());
                }
                Ok(res) => toast.error(res.message),
                Err(e) => toast.error(failure_message("回复失败", &e)),
            }
            sending.set(false);
        });
    };
    rsx! {
        div { class: "composer admin-comment-reply",
            textarea {
                class: "composer-input",
                aria_label: "回复 {author}",
                placeholder: "回复 {author}…（⌘↵ 发送）",
                rows: "2",
                autofocus: true,
                value: "{text}",
                oninput: move |e| text.set(e.value()),
                onkeydown: move |e| {
                    let m = e.modifiers();
                    if (m.meta() || m.ctrl()) && e.key() == Key::Enter {
                        e.prevent_default();
                        send();
                    }
                },
            }
            button {
                class: "composer-send",
                r#type: "button",
                aria_label: "发送回复",
                disabled: sending() || text().trim().is_empty(),
                onclick: move |_| send(),
                crate::components::ui::Icon { name: IconName::Send, size: 16 }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_images_pulls_markdown_images_out_of_text() {
        let (text, images) = split_images("看图 ![截图](/uploads/a.webp) 就懂了");
        assert_eq!(text, "看图  就懂了");
        assert_eq!(images, ["/uploads/a.webp"]);
        let (text, images) = split_images("![](/uploads/b.webp \"标题\")");
        assert_eq!(text, "");
        assert_eq!(images, ["/uploads/b.webp"]);
        let (text, images) = split_images("不是图片 ![没有地址]");
        assert_eq!(text, "不是图片 ![没有地址]");
        assert!(images.is_empty());
    }

    #[test]
    fn count_of_matches_tabs() {
        let counts = CommentCounts {
            pending: 1,
            approved: 2,
            spam: 3,
            trash: 4,
        };
        assert_eq!(count_of(&counts, "spam"), 3);
        assert_eq!(count_of(&counts, "trash"), 4);
        assert_eq!(count_of(&counts, ""), 0);
    }
}
