//! 评论管理（`/admin/comments`）。
//!
//! 标题行（总数与待审数）→ 待审 / 已通过 / 垃圾 / 回收站选项卡（带数量）→ 评论流：
//! 头像首字、作者与网址、「回复 X」、时间、正文与图片、所属文章与按状态给出的操作
//! （通过、回复、标为垃圾、不是垃圾、移到回收站、恢复）。回复在行内展开 Composer，
//! 以管理员身份直接发表。勾选后底部浮起批量操作条。默认打开待审，没有待审时打开
//! 已通过。数据仅在 WASM 前端读取（Effect + 世代号）。样式见 styles/pages/admin-comments.css。

use std::collections::HashSet;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_params_map, use_query_map};
use leptos_router::{lazy_route, LazyRoute};

use crate::api::comments::{AllCommentsResponse, CommentCounts};
use crate::components::admin_layout::AdminPageHead;
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::admin_comments_skeleton::AdminCommentsListSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Checkbox, Composer, ComposerInput,
    ComposerSend, ComposerSize, IconButton, IconName, LoadError, SegmentedControl, SegmentedItem,
    TablePager,
};
use crate::models::comment::{AdminComment, CommentStatus};
use crate::utils::error::failure_message;
use crate::utils::time::{hm, to_local, ymd};

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

/// 评论管理路由：`comments` 与 `comments/:page`（缺省第 1 页）。
pub struct AdminCommentsRoute {
    page: Memo<i32>,
}

#[lazy_route]
impl LazyRoute for AdminCommentsRoute {
    fn data() -> Self {
        let params = use_params_map();
        let page = Memo::new(move |_| {
            params
                .read()
                .get("page")
                .and_then(|p| p.parse::<i32>().ok())
                .unwrap_or(1)
                .max(1)
        });
        Self { page }
    }

    fn view(this: Self) -> AnyView {
        let page = this.page;
        view! {
            {move || {
                let page = page.get();
                view! { <AdminCommentsPage page=page/> }
            }}
        }
        .into_any()
    }
}

/// 列表区域的显示形态；只在形态变化时重建 DOM。
#[derive(Clone, PartialEq)]
enum Body {
    Error,
    Skeleton,
    Empty(String),
    List,
}

/// 评论管理。`page` 来自 `/admin/comments/:page`，之后翻页只改本页状态。
#[component]
fn AdminCommentsPage(page: i32) -> impl IntoView {
    let toast = use_toast();
    let query = use_query_map();
    // 空字符串表示还没决定：先读待审，没有待审时换到已通过。
    let tab = RwSignal::new(
        query
            .read_untracked()
            .get("status")
            .filter(|s| matches!(s.as_str(), "pending" | "approved" | "spam" | "trash"))
            .unwrap_or_default(),
    );
    let current_page = RwSignal::new(page.max(1));
    let data = RwSignal::new(None::<AllCommentsResponse>);
    let error = RwSignal::new(false);
    let loading = RwSignal::new(true);
    let reload = RwSignal::new(0_u32);
    let selected = RwSignal::new(HashSet::<i64>::new());
    let busy = RwSignal::new(false);
    let replying = RwSignal::new(None::<i64>);
    let generation = StoredValue::new(0_u64);

    Effect::new(move |_| {
        let requested = tab.get();
        let p = current_page.get();
        reload.track();
        let mut id = 0;
        generation.update_value(|g| {
            *g += 1;
            id = *g;
        });
        loading.set(true);
        spawn_local(async move {
            let status = if requested.is_empty() {
                "pending".to_string()
            } else {
                requested.clone()
            };
            let result = crate::api::comments::get_all_comments(p, Some(status)).await;
            if generation.try_get_value() != Some(id) {
                return;
            }
            match result {
                Ok(resp) => {
                    if requested.is_empty() {
                        if resp.counts.pending == 0 {
                            let _ = tab.try_set("approved".into());
                            return;
                        }
                        let _ = tab.try_set("pending".into());
                    }
                    let _ = data.try_set(Some(resp));
                    let _ = error.try_set(false);
                }
                Err(_) => {
                    let _ = error.try_set(true);
                }
            }
            let _ = loading.try_set(false);
        });
    });

    let refresh = move || {
        selected.update(|s| s.clear());
        reload.update(|n| *n += 1);
    };
    // 单条与批量共用：status 为 approved / spam / trash，或 restore。
    let apply = Callback::new(move |(ids, action): (Vec<i64>, &'static str)| {
        if ids.is_empty() || busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
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
            let _ = busy.try_set(false);
        });
    });

    let counts =
        Memo::new(move |_| data.with(|d| d.as_ref().map(|r| r.counts).unwrap_or_default()));
    let loaded = Memo::new(move |_| data.with(Option::is_some));
    let rows = Memo::new(move |_| {
        data.with(|d| d.as_ref().map(|r| r.comments.clone()).unwrap_or_default())
    });
    let total = Signal::derive(move || data.with(|d| d.as_ref().map(|r| r.total).unwrap_or(0)));
    let selection_empty = Memo::new(move |_| selected.with(|s| s.is_empty()));
    let page_all = Memo::new(move |_| {
        selected.with(|sel| {
            rows.with(|rows| !rows.is_empty() && rows.iter().all(|c| sel.contains(&c.id)))
        })
    });

    let status_items = Signal::derive(move || {
        let counts = counts.get();
        let loaded = loaded.get();
        [
            ("pending", "待审", counts.pending),
            ("approved", "已通过", counts.approved),
            ("spam", "垃圾", counts.spam),
            ("trash", "回收站", counts.trash),
        ]
        .into_iter()
        .map(|(value, label, n)| {
            let item = SegmentedItem::new(value.to_string(), label);
            if loaded {
                item.count(n)
            } else {
                item
            }
        })
        .collect::<Vec<_>>()
    });

    let body = Memo::new(move |_| {
        if error.get() {
            Body::Error
        } else if !loaded.get()
            || (loading.get()
                && rows.with(Vec::is_empty)
                && tab.with(|t| count_of(&counts.get(), t)) > 0)
        {
            Body::Skeleton
        } else if rows.with(Vec::is_empty) {
            Body::Empty(tab.get())
        } else {
            Body::List
        }
    });

    view! {
        <div class="admin-comments">
            <AdminPageHead title="评论">
                {move || {
                    loaded
                        .get()
                        .then(|| {
                            let c = counts.get();
                            format!("{} 条 · 待审 {}", c.pending + c.approved + c.spam, c.pending)
                        })
                }}
            </AdminPageHead>
            <div class="admin-toolbar">
                <SegmentedControl
                    aria_label="评论状态"
                    items=status_items
                    value=tab
                    on_change=move |value: String| {
                        selected.update(|s| s.clear());
                        replying.set(None);
                        current_page.set(1);
                        tab.set(value);
                    }
                />
                {move || {
                    (!rows.with(Vec::is_empty))
                        .then(|| {
                            view! {
                                <Checkbox
                                    checked=page_all
                                    label="选择本页全部评论".to_string()
                                    onchange=move |on: bool| {
                                        let ids: Vec<i64> = rows
                                            .with_untracked(|r| r.iter().map(|c| c.id).collect());
                                        selected
                                            .update(|set| {
                                                for id in ids {
                                                    if on {
                                                        set.insert(id);
                                                    } else {
                                                        set.remove(&id);
                                                    }
                                                }
                                            });
                                    }
                                />
                            }
                        })
                }}
            </div>

            {move || match body.get() {
                Body::Error => {
                    view! {
                        <LoadError
                            title="评论加载失败"
                            on_retry=move |()| reload.update(|n| *n += 1)
                        />
                    }
                        .into_any()
                }
                Body::Skeleton => view! { <AdminCommentsListSkeleton/> }.into_any(),
                Body::Empty(active) => {
                    view! {
                        <div class="admin-panel">
                            {match active.as_str() {
                                "pending" => {
                                    view! {
                                        <EmptyState
                                            icon=IconName::Check
                                            title="没有待审评论"
                                            description="新评论需要审核时会出现在这里。"
                                        />
                                    }
                                        .into_any()
                                }
                                "spam" => {
                                    view! {
                                        <EmptyState
                                            icon=IconName::Shield
                                            title="没有垃圾评论"
                                            description="被标为垃圾的评论会集中在这里。"
                                        />
                                    }
                                        .into_any()
                                }
                                "trash" => {
                                    view! {
                                        <EmptyState
                                            icon=IconName::Trash
                                            title="回收站是空的"
                                            description="移到回收站的评论可以在这里恢复。"
                                        />
                                    }
                                        .into_any()
                                }
                                _ => {
                                    view! {
                                        <EmptyState
                                            icon=IconName::MessageSquare
                                            title="还没有评论"
                                            description="读者的评论通过审核后会出现在这里。"
                                        />
                                    }
                                        .into_any()
                                }
                            }}
                        </div>
                    }
                        .into_any()
                }
                Body::List => {
                    view! {
                        <div
                            class="admin-panel admin-comment-list"
                            aria-busy=move || loading.get().then_some("true")
                        >
                            <For
                                each=move || rows.get()
                                key=|c| format!("{}-{}", c.id, c.status.as_str())
                                children=move |comment| {
                                    let id = comment.id;
                                    view! {
                                        <CommentItem
                                            comment=comment
                                            selected=Signal::derive(move || {
                                                selected.with(|s| s.contains(&id))
                                            })
                                            replying=Signal::derive(move || {
                                                replying.get() == Some(id)
                                            })
                                            busy=busy
                                            on_select=move |(id, on): (i64, bool)| {
                                                selected
                                                    .update(|s| {
                                                        if on {
                                                            s.insert(id);
                                                        } else {
                                                            s.remove(&id);
                                                        }
                                                    });
                                            }
                                            on_action=move |(id, action): (i64, &'static str)| {
                                                if action == "reply" {
                                                    replying
                                                        .update(|r| {
                                                            *r = if *r == Some(id) { None } else { Some(id) };
                                                        });
                                                } else {
                                                    apply.run((vec![id], action));
                                                }
                                            }
                                            on_replied=move |()| {
                                                replying.set(None);
                                                refresh();
                                            }
                                        />
                                    }
                                }
                            />
                            <TablePager
                                page=current_page
                                total=total
                                per_page=COMMENTS_PER_PAGE
                                unit="条"
                                on_change=move |p| current_page.set(p)
                            />
                        </div>
                    }
                        .into_any()
                }
            }}

            <Show when=move || !selection_empty.get()>
                <div class="batch-bar" role="toolbar" aria-label="批量操作">
                    <span class="batch-bar-count">
                        {move || format!("已选 {} 条", selected.with(|s| s.len()))}
                    </span>
                    {move || {
                        let active = tab.get();
                        let take = move || selected.get_untracked().into_iter().collect::<Vec<_>>();
                        if active == "trash" {
                            view! {
                                <button
                                    class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                    type="button"
                                    disabled=move || busy.get()
                                    on:click=move |_| apply.run((take(), "restore"))
                                >
                                    "恢复"
                                </button>
                            }
                                .into_any()
                        } else {
                            view! {
                                {(active != "approved")
                                    .then(|| {
                                        view! {
                                            <button
                                                class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                                type="button"
                                                disabled=move || busy.get()
                                                on:click=move |_| apply.run((take(), "approved"))
                                            >
                                                "通过"
                                            </button>
                                        }
                                    })}
                                {(active != "spam")
                                    .then(|| {
                                        view! {
                                            <button
                                                class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                                type="button"
                                                disabled=move || busy.get()
                                                on:click=move |_| apply.run((take(), "spam"))
                                            >
                                                "标为垃圾"
                                            </button>
                                        }
                                    })}
                                <button
                                    class=format!(
                                        "{} batch-bar-danger",
                                        button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                                    )
                                    type="button"
                                    disabled=move || busy.get()
                                    on:click=move |_| apply.run((take(), "trash"))
                                >
                                    "移到回收站"
                                </button>
                            }
                                .into_any()
                        }
                    }}
                    <span class="batch-bar-sep" aria-hidden="true"></span>
                    <IconButton
                        icon=IconName::X
                        label="取消选择"
                        size=ButtonSize::Sm
                        onclick=move |_| selected.update(|s| s.clear())
                    />
                </div>
            </Show>
        </div>
    }
}

/// 评论流的一条。操作按状态给出，回调 `(评论 id, 操作)`。
#[component]
fn CommentItem(
    comment: AdminComment,
    #[prop(into)] selected: Signal<bool>,
    #[prop(into)] replying: Signal<bool>,
    #[prop(into)] busy: Signal<bool>,
    #[prop(into)] on_select: Callback<(i64, bool)>,
    #[prop(into)] on_action: Callback<(i64, &'static str)>,
    #[prop(into)] on_replied: Callback<()>,
) -> impl IntoView {
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
    let when = {
        let t = to_local(comment.created_at);
        format!("{} {}", ymd(&t, '-'), hm(&t))
    };
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
    let is_reply = comment.parent_id.is_some();
    let author_label = format!("选择 {} 的评论", comment.author_name);
    let avatar = if comment.avatar_url.is_empty() {
        view! { <span class="admin-comment-avatar" aria-hidden="true">{initial}</span> }.into_any()
    } else {
        view! { <img class="admin-comment-avatar" src=comment.avatar_url.clone() alt="" loading="lazy"/> }
            .into_any()
    };
    let images_view = (!images.is_empty()).then(|| {
        view! {
            <div class="admin-comment-images">
                {images
                    .into_iter()
                    .map(|url| {
                        let thumb = format!("{url}?thumb=240x240");
                        view! {
                            <a href=url target="_blank" rel="noopener">
                                <img src=thumb alt="评论图片" loading="lazy"/>
                            </a>
                        }
                    })
                    .collect_view()}
            </div>
        }
    });
    let post_id = comment.post_id;
    let author = comment.author_name.clone();
    view! {
        <article
            class="admin-comment"
            data-selected=move || selected.get().then_some("true")
            data-reply=is_reply.then_some("true")
        >
            <Checkbox
                checked=selected
                label=author_label
                onchange=move |on| on_select.run((id, on))
            />
            {avatar}
            <div class="admin-comment-body">
                <div class="admin-comment-head">
                    <span class="admin-comment-author">{comment.author_name.clone()}</span>
                    {site.map(|site| view! { <span class="admin-comment-site">{site}</span> })}
                    {comment
                        .reply_to
                        .as_ref()
                        .map(|to| view! { <span class="admin-comment-to">{format!("回复 {to}")}</span> })}
                    <time>{when}</time>
                </div>
                {(!text.is_empty()).then(|| view! { <p class="admin-comment-text">{text}</p> })}
                {images_view}
                <div class="admin-comment-foot">
                    <a
                        class="admin-comment-post"
                        href=format!("/post/{}", comment.post_slug)
                        target="_blank"
                        rel="noopener"
                    >
                        {format!("于《{}》", comment.post_title)}
                    </a>
                    <div class="admin-comment-actions">
                        {actions
                            .iter()
                            .copied()
                            .map(|(action, label)| {
                                view! {
                                    <button
                                        class=button_class(ButtonVariant::Ghost, ButtonSize::Xs)
                                        data-danger=(action == "trash").then_some("true")
                                        type="button"
                                        disabled=move || busy.get()
                                        aria-expanded=move || {
                                            (action == "reply").then(|| replying.get().to_string())
                                        }
                                        on:click=move |_| on_action.run((id, action))
                                    >
                                        {label}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                </div>
                <Show when=move || replying.get()>
                    <ReplyComposer
                        post_id=post_id
                        parent_id=id
                        author=author.clone()
                        on_done=move |()| on_replied.run(())
                    />
                </Show>
            </div>
        </article>
    }
}

/// 行内回复：以当前管理员身份发表，直接通过。
#[component]
fn ReplyComposer(
    post_id: i32,
    parent_id: i64,
    author: String,
    #[prop(into)] on_done: Callback<()>,
) -> impl IntoView {
    let toast = use_toast();
    let text = RwSignal::new(String::new());
    let sending = RwSignal::new(false);
    let send = move || {
        let content = text.get_untracked().trim().to_string();
        if content.is_empty() || sending.get_untracked() {
            return;
        }
        sending.set(true);
        spawn_local(async move {
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
                    on_done.run(());
                }
                Ok(res) => toast.error(res.message),
                Err(e) => toast.error(failure_message("回复失败", &e)),
            }
            let _ = sending.try_set(false);
        });
    };
    let aria = format!("回复 {author}");
    let placeholder = format!("回复 {author}…（⌘↵ 发送）");
    view! {
        <Composer size=ComposerSize::Sm class="admin-comment-reply">
            <ComposerInput
                multiline=true
                autofocus=true
                aria_label=aria
                placeholder=placeholder
                value=text
                oninput=move |value: String| text.set(value)
                on_submit=move |()| send()
            />
            <ComposerSend
                label="发送回复"
                icon=IconName::Send
                disabled=Signal::derive(move || {
                    sending.get() || text.with(|t| t.trim().is_empty())
                })
                onclick=move |_| send()
            />
        </Composer>
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
