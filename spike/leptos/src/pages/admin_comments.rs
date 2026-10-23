//! 评论管理（`/admin/comments`）。
//!
//! 标题行（总数与待审数）→ 待审 / 已通过 / 垃圾 / 回收站选项卡（带数量）→ 评论流：
//! 头像首字、作者与网址、「回复 X」、时间、正文与图片、所属文章与按状态给出的操作
//! （通过、回复、标为垃圾、不是垃圾、移到回收站、恢复）。回复在行内展开 Composer，
//! 以管理员身份直接发表。勾选后底部浮起批量操作条。默认打开待审，没有待审时打开
//! 已通过。样式见 styles/pages/admin-comments.css。
//!
//! 与 Dioxus 版的差异：数据经 Resource 在 SSR 读取并随页面水合；操作结果以页内
//! AlertBox 提示（原型不移植 Toast）；没有后台侧栏；未登录时显示提示与主应用登录链接。

use std::collections::HashSet;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::admin::{AllCommentsResponse, CommentCounts};
use crate::components::empty_state::EmptyState;
use crate::components::forms::AlertBox;
use crate::components::skeletons::admin_comments_skeleton::AdminCommentsListSkeleton;
use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, Checkbox, Composer, ComposerInput, ComposerSend,
    ComposerSize, IconButton, IconName, LoadError, SegmentedControl, SegmentedItem, TablePager,
};
use crate::error::AppError;
use crate::models::comment::{AdminComment, CommentStatus};
use crate::utils::time::to_local;

/// 每页评论数，与服务端列表接口一致。
const COMMENTS_PER_PAGE: i32 = 20;

/// 主应用登录页（原型不移植登录，会话 Cookie 不区分端口）。
const MAIN_APP_LOGIN: &str = "http://127.0.0.1:8080/login";

/// 列表加载结果：实际展示的选项卡与响应。
type Loaded = Result<(String, AllCommentsResponse), AppError>;

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

/// 读取列表。`requested` 为空表示还没决定：先读待审，没有待审时换到已通过。
async fn load_comments(requested: String, page: i32) -> Loaded {
    use crate::api::admin::get_all_comments;
    if !requested.is_empty() {
        let resp = get_all_comments(page, Some(requested.clone())).await?;
        return Ok((requested, resp));
    }
    let resp = get_all_comments(page, Some("pending".to_string())).await?;
    if resp.counts.pending == 0 {
        let resp = get_all_comments(page, Some("approved".to_string())).await?;
        return Ok(("approved".to_string(), resp));
    }
    Ok(("pending".to_string(), resp))
}

/// 后台页头（移植自 `components::admin_layout::AdminPageHead`）。
///
/// - `title`：页面标题
/// - `class`：附加类
/// - `actions`：右侧内容，统一包进 `.admin-page-actions`
/// - `children`：副标题内容；为空时 `p` 由 CSS `min-height: 1lh` 保留一行高度
#[component]
fn AdminPageHead(
    #[prop(into)] title: String,
    #[prop(optional)] class: Option<&'static str>,
    #[prop(optional)] actions: Option<Children>,
    children: Children,
) -> impl IntoView {
    let extra = class.unwrap_or_default();
    view! {
        <div class=format!("admin-page-head {extra}")>
            <div>
                <h1>{title}</h1>
                <p>{children()}</p>
            </div>
            {actions.map(|actions| view! { <div class="admin-page-actions">{actions()}</div> })}
        </div>
    }
}

/// 评论管理路由：数据在主包里创建，页面视图按路由拆分。
pub struct AdminCommentsRoute {
    /// 用户选择的选项卡；空字符串表示由服务端结果决定。
    tab: RwSignal<String>,
    /// 当前页码。
    page: RwSignal<i32>,
    /// 递增触发重新加载。
    reload: RwSignal<u32>,
    /// 评论列表。
    data: Resource<Loaded>,
}

#[lazy_route]
impl LazyRoute for AdminCommentsRoute {
    fn data() -> Self {
        let query = use_query_map();
        let initial = query
            .read_untracked()
            .get("status")
            .filter(|s| matches!(s.as_str(), "pending" | "approved" | "spam" | "trash"))
            .unwrap_or_default();
        let tab = RwSignal::new(initial);
        let page = RwSignal::new(1_i32);
        let reload = RwSignal::new(0_u32);
        let data = Resource::new(
            move || (tab.get(), page.get(), reload.get()),
            |(requested, p, _)| load_comments(requested, p),
        );
        Self {
            tab,
            page,
            reload,
            data,
        }
    }

    fn view(this: Self) -> AnyView {
        view! { <AdminCommentsPage tab=this.tab page=this.page reload=this.reload data=this.data/> }
            .into_any()
    }
}

/// 页面共享状态（全部为 Copy 句柄）。
#[derive(Clone, Copy)]
struct PageState {
    tab: RwSignal<String>,
    page: RwSignal<i32>,
    reload: RwSignal<u32>,
    loading: RwSignal<bool>,
    selected: RwSignal<HashSet<i64>>,
    busy: RwSignal<bool>,
    replying: RwSignal<Option<i64>>,
    /// 操作提示：(是否成功, 文本)，替代 Toast。
    notice: RwSignal<Option<(bool, String)>>,
    /// 最近一次展示的选项卡（tab 为空时由服务端决定）。
    shown_tab: StoredValue<String>,
    /// 单条与批量共用：status 为 approved / spam / trash，或 restore。
    apply: Callback<(Vec<i64>, &'static str)>,
}

impl PageState {
    /// 清空选择并重新加载。
    fn refresh(self) {
        if self.tab.get_untracked().is_empty() {
            self.tab.set(self.shown_tab.get_value());
        }
        self.selected.update(|s| s.clear());
        self.loading.set(true);
        self.reload.update(|n| *n += 1);
    }
}

/// 评论管理页面。
#[component]
fn AdminCommentsPage(
    tab: RwSignal<String>,
    page: RwSignal<i32>,
    reload: RwSignal<u32>,
    data: Resource<Loaded>,
) -> impl IntoView {
    let loading = RwSignal::new(true);
    let selected = RwSignal::new(HashSet::<i64>::new());
    let busy = RwSignal::new(false);
    let replying = RwSignal::new(None::<i64>);
    let notice = RwSignal::new(None::<(bool, String)>);
    let shown_tab = StoredValue::new(String::new());

    let apply = Callback::new(move |(ids, action): (Vec<i64>, &'static str)| {
        if ids.is_empty() || busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            let result = if action == "restore" {
                crate::api::admin::restore_comments(ids).await
            } else {
                crate::api::admin::batch_update_comment_status(ids, action.to_string()).await
            };
            match result {
                Ok(res) if res.success => {
                    let n = res.updated_count;
                    notice.set(Some((
                        true,
                        match action {
                            "approved" => format!("已通过 {n} 条评论"),
                            "spam" => format!("已把 {n} 条标为垃圾"),
                            "trash" => format!("已把 {n} 条移到回收站"),
                            _ => res.message,
                        },
                    )));
                    if tab.get_untracked().is_empty() {
                        tab.set(shown_tab.get_value());
                    }
                    selected.update(|s| s.clear());
                    loading.set(true);
                    reload.update(|n| *n += 1);
                }
                Ok(res) => notice.set(Some((false, res.message))),
                Err(e) => notice.set(Some((false, format!("操作失败：{e}")))),
            }
            busy.set(false);
        });
    });

    let st = PageState {
        tab,
        page,
        reload,
        loading,
        selected,
        busy,
        replying,
        notice,
        shown_tab,
        apply,
    };

    view! {
        <Transition fallback=move || page_view(st, None)>
            {move || Suspend::new(async move {
                let result = data.await;
                loading.set(false);
                match result {
                    Ok(loaded) => {
                        shown_tab.set_value(loaded.0.clone());
                        page_view(st, Some(Ok(loaded)))
                    }
                    Err(e @ (AppError::Unauthorized | AppError::Forbidden)) => {
                        e.commit_status();
                        unauthorized_view(e).into_any()
                    }
                    Err(e) => page_view(st, Some(Err(e))),
                }
            })}
        </Transition>
    }
}

/// 未登录或非管理员：提示并给出主应用登录链接。
fn unauthorized_view(e: AppError) -> impl IntoView {
    let (title, description) = if matches!(e, AppError::Forbidden) {
        (
            "权限不足",
            "当前账号不是管理员，请换用管理员账号登录主应用。",
        )
    } else {
        ("未登录", "请先在主应用登录管理员账号，再刷新本页。")
    };
    view! {
        <div class="admin-comments">
            <AdminPageHead title="评论">{title}</AdminPageHead>
            <div class="admin-panel">
                <EmptyState icon=IconName::Shield title=title description=description/>
                <a class=button_class(ButtonVariant::Primary, ButtonSize::Sm) href=MAIN_APP_LOGIN>
                    "前往登录"
                </a>
            </div>
        </div>
    }
}

/// 整页视图。`loaded` 为 None 表示首次加载中，Err 表示加载失败。
fn page_view(
    st: PageState,
    loaded: Option<Result<(String, AllCommentsResponse), AppError>>,
) -> AnyView {
    let PageState {
        tab,
        page,
        reload,
        loading,
        selected,
        busy,
        replying,
        notice,
        apply,
        ..
    } = st;

    let error = matches!(loaded, Some(Err(_)));
    let (active, resp) = match loaded {
        Some(Ok((active, resp))) => (active, Some(resp)),
        _ => (tab.get_untracked(), None),
    };
    let has_resp = resp.is_some();
    let counts = resp.as_ref().map(|r| r.counts).unwrap_or_default();
    let rows: Vec<AdminComment> = resp
        .as_ref()
        .map(|r| r.comments.clone())
        .unwrap_or_default();
    let total = resp.as_ref().map(|r| r.total).unwrap_or(0);
    let all_count = counts.pending + counts.approved + counts.spam;
    let page_ids: Vec<i64> = rows.iter().map(|c| c.id).collect();
    let active_count = count_of(&counts, &active);

    let status_items: Vec<SegmentedItem<String>> = [
        ("pending", "待审", counts.pending),
        ("approved", "已通过", counts.approved),
        ("spam", "垃圾", counts.spam),
        ("trash", "回收站", counts.trash),
    ]
    .into_iter()
    .map(|(value, label, n)| {
        let item = SegmentedItem::new(value.to_string(), label);
        if has_resp {
            item.count(n)
        } else {
            item
        }
    })
    .collect();

    // 本页全选框。
    let page_checkbox = (!rows.is_empty()).then(|| {
        let ids_for_check = page_ids.clone();
        let ids_for_change = page_ids.clone();
        let page_all = Signal::derive(move || {
            selected.with(|set| ids_for_check.iter().all(|id| set.contains(id)))
        });
        view! {
            <Checkbox
                checked=page_all
                label="选择本页全部评论".to_string()
                onchange=Callback::new(move |on: bool| {
                    selected.update(|set| {
                        for id in &ids_for_change {
                            if on {
                                set.insert(*id);
                            } else {
                                set.remove(id);
                            }
                        }
                    });
                })
            />
        }
    });

    let active_for_tabs = active.clone();
    let body = if error {
        view! {
            <LoadError
                title="评论加载失败"
                on_retry=Callback::new(move |_| {
                    loading.set(true);
                    reload.update(|n| *n += 1);
                })
            />
        }
        .into_any()
    } else if !has_resp {
        view! { <AdminCommentsListSkeleton/> }.into_any()
    } else if rows.is_empty() {
        // 切换选项卡时旧数据为空但新选项卡有条数：加载中先显示骨架而非空状态。
        let active = active.clone();
        view! {
            {move || {
                if loading.get() && active_count > 0 {
                    view! { <AdminCommentsListSkeleton/> }.into_any()
                } else {
                    view! { <div class="admin-panel">{empty_view(&active)}</div> }.into_any()
                }
            }}
        }
        .into_any()
    } else {
        let items = rows
            .into_iter()
            .map(|comment| {
                let id = comment.id;
                view! {
                    <CommentItem
                        selected=Signal::derive(move || selected.with(|s| s.contains(&id)))
                        replying=Signal::derive(move || replying.get() == Some(id))
                        busy=Signal::derive(move || busy.get())
                        on_select=Callback::new(move |(id, on): (i64, bool)| {
                            selected.update(|s| {
                                if on {
                                    s.insert(id);
                                } else {
                                    s.remove(&id);
                                }
                            });
                        })
                        on_action=Callback::new(move |(id, action): (i64, &'static str)| {
                            if action == "reply" {
                                replying.update(|r| {
                                    *r = if *r == Some(id) { None } else { Some(id) };
                                });
                            } else {
                                apply.run((vec![id], action));
                            }
                        })
                        on_replied=Callback::new(move |_| {
                            replying.set(None);
                            st.refresh();
                        })
                        comment=comment
                    />
                }
            })
            .collect_view();
        let list = view! {
            <div
                class="admin-panel admin-comment-list"
                aria-busy=move || loading.get().then_some("true")
            >
                {items}
                <TablePager
                    page=page
                    total=total
                    per_page=COMMENTS_PER_PAGE
                    unit="条"
                    on_change=Callback::new(move |p: i32| {
                        loading.set(true);
                        page.set(p);
                    })
                />
            </div>
        };
        list.into_any()
    };

    /// 各选项卡的空状态。
    fn empty_view(active: &str) -> AnyView {
        match active {
            "pending" => view! {
                <EmptyState icon=IconName::Check title="没有待审评论" description="新评论需要审核时会出现在这里。"/>
            }
            .into_any(),
            "spam" => view! {
                <EmptyState icon=IconName::Shield title="没有垃圾评论" description="被标为垃圾的评论会集中在这里。"/>
            }
            .into_any(),
            "trash" => view! {
                <EmptyState icon=IconName::Trash title="回收站是空的" description="移到回收站的评论可以在这里恢复。"/>
            }
            .into_any(),
            _ => view! {
                <EmptyState icon=IconName::MessageSquare title="还没有评论" description="读者的评论通过审核后会出现在这里。"/>
            }
            .into_any(),
        }
    }

    let active_for_bar = active.clone();
    let batch_bar = move || {
        if selected.with(|s| s.is_empty()) {
            return None;
        }
        let count = selected.with(|s| s.len());
        let ids = move || selected.get_untracked().into_iter().collect::<Vec<i64>>();
        let ghost = button_class(ButtonVariant::Ghost, ButtonSize::Sm);
        let actions = if active_for_bar == "trash" {
            view! {
                <button
                    class=ghost.clone()
                    type="button"
                    disabled=move || busy.get()
                    on:click=move |_| apply.run((ids(), "restore"))
                >
                    "恢复"
                </button>
            }
            .into_any()
        } else {
            let approve = (active_for_bar != "approved").then(|| {
                view! {
                    <button
                        class=ghost.clone()
                        type="button"
                        disabled=move || busy.get()
                        on:click=move |_| apply.run((ids(), "approved"))
                    >
                        "通过"
                    </button>
                }
            });
            let spam = (active_for_bar != "spam").then(|| {
                view! {
                    <button
                        class=ghost.clone()
                        type="button"
                        disabled=move || busy.get()
                        on:click=move |_| apply.run((ids(), "spam"))
                    >
                        "标为垃圾"
                    </button>
                }
            });
            view! {
                {approve}
                {spam}
                <button
                    class=format!("{ghost} batch-bar-danger")
                    type="button"
                    disabled=move || busy.get()
                    on:click=move |_| apply.run((ids(), "trash"))
                >
                    "移到回收站"
                </button>
            }
            .into_any()
        };
        Some(view! {
            <div class="batch-bar" role="toolbar" aria-label="批量操作">
                <span class="batch-bar-count">{format!("已选 {count} 条")}</span>
                {actions}
                <span class="batch-bar-sep" aria-hidden="true"></span>
                <IconButton
                    icon=IconName::X
                    label="取消选择"
                    size=ButtonSize::Sm
                    onclick=Callback::new(move |_| selected.update(|s| s.clear()))
                />
            </div>
        })
    };

    view! {
        <div class="admin-comments">
            <AdminPageHead title="评论">
                {has_resp.then(|| format!("{all_count} 条 · 待审 {}", counts.pending))}
            </AdminPageHead>
            {move || {
                notice
                    .get()
                    .map(|(ok, message)| {
                        view! {
                            <AlertBox message=message variant=if ok { "success" } else { "error" }/>
                        }
                    })
            }}
            <div class="admin-toolbar">
                <SegmentedControl
                    aria_label="评论状态"
                    items=status_items
                    value=Signal::derive(move || active_for_tabs.clone())
                    on_change=Callback::new(move |value: String| {
                        selected.update(|s| s.clear());
                        replying.set(None);
                        notice.set(None);
                        loading.set(true);
                        page.set(1);
                        tab.set(value);
                    })
                />
                {page_checkbox}
            </div>
            {body}
            {batch_bar}
        </div>
    }
    .into_any()
}

/// 评论流的一条。操作按状态给出，回调 `(评论 id, 操作)`。
#[component]
fn CommentItem(
    comment: AdminComment,
    #[prop(into)] selected: Signal<bool>,
    #[prop(into)] replying: Signal<bool>,
    #[prop(into)] busy: Signal<bool>,
    on_select: Callback<(i64, bool)>,
    on_action: Callback<(i64, &'static str)>,
    on_replied: Callback<()>,
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
    let when = to_local(comment.created_at)
        .format("%Y-%m-%d %H:%M")
        .to_string();
    let actions: &'static [(&'static str, &'static str)] = match comment.status {
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
                        view! {
                            <a href=url.clone() target="_blank" rel="noopener">
                                <img src=format!("{url}?thumb=240x240") alt="评论图片" loading="lazy"/>
                            </a>
                        }
                    })
                    .collect_view()}
            </div>
        }
    });
    let action_buttons = actions
        .iter()
        .copied()
        .map(|(action, label)| {
            view! {
                <button
                    class=button_class(ButtonVariant::Ghost, ButtonSize::Xs)
                    data-danger=(action == "trash").then_some("true")
                    type="button"
                    disabled=move || busy.get()
                    aria-expanded=move || (action == "reply").then(|| replying.get().to_string())
                    on:click=move |_| on_action.run((id, action))
                >
                    {label}
                </button>
            }
        })
        .collect_view();
    let post_id = comment.post_id;
    let author = comment.author_name.clone();
    let checkbox_label = format!("选择 {} 的评论", comment.author_name);

    view! {
        <article
            class="admin-comment"
            data-selected=move || selected.get().then_some("true")
            data-reply=comment.parent_id.is_some().then_some("true")
        >
            <Checkbox
                checked=selected
                label=checkbox_label
                onchange=Callback::new(move |on: bool| on_select.run((id, on)))
            />
            {avatar}
            <div class="admin-comment-body">
                <div class="admin-comment-head">
                    <span class="admin-comment-author">{comment.author_name.clone()}</span>
                    {site.map(|site| view! { <span class="admin-comment-site">{site}</span> })}
                    {comment
                        .reply_to
                        .clone()
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
                    <div class="admin-comment-actions">{action_buttons}</div>
                </div>
                {move || {
                    replying
                        .get()
                        .then(|| {
                            view! {
                                <ReplyComposer
                                    post_id=post_id
                                    parent_id=id
                                    author=author.clone()
                                    on_done=Callback::new(move |_| on_replied.run(()))
                                />
                            }
                        })
                }}
            </div>
        </article>
    }
}

/// 行内回复：以当前管理员身份发表，直接通过。
#[component]
fn ReplyComposer(
    post_id: i32,
    parent_id: i64,
    #[prop(into)] author: String,
    on_done: Callback<()>,
) -> impl IntoView {
    let text = RwSignal::new(String::new());
    let sending = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let send = Callback::new(move |_: ()| {
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
                    error.set(None);
                    on_done.run(());
                }
                Ok(res) => error.set(Some(res.message)),
                Err(e) => error.set(Some(format!("回复失败：{e}"))),
            }
            sending.set(false);
        });
    });
    view! {
        <Composer size=ComposerSize::Sm class="admin-comment-reply">
            <ComposerInput
                multiline=true
                autofocus=true
                aria_label=format!("回复 {author}")
                placeholder=format!("回复 {author}…（⌘↵ 发送）")
                value=text
                oninput=Callback::new(move |value: String| text.set(value))
                on_submit=Callback::new(move |_: ()| send.run(()))
            />
            <ComposerSend
                label="发送回复"
                icon=IconName::Send
                disabled=Signal::derive(move || sending.get() || text.with(|t| t.trim().is_empty()))
                onclick=Callback::new(move |_| send.run(()))
            />
        </Composer>
        {move || error.get().map(|message| view! { <AlertBox message=message variant="error"/> })}
    }
}
