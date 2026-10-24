//! 评论组件图鉴：固定样例与局部上下文，展示节点仍由正式评论组件渲染。

use leptos::prelude::*;

use crate::components::comments::card::CommentCardShell;
use crate::components::comments::form::CommentForm;
use crate::components::comments::item::CommentItem;
use crate::components::comments::list::CommentList;
use crate::components::comments::pending_item::PendingCommentItem;
use crate::components::comments::section::{
    CommentContext, CommentSectionContent, CommentSubmission, CommentSubmitFuture,
};
use crate::models::comment::PublicComment;
use crate::utils::comment_storage::PendingComment;

const MODE_ACTIVE_CLASS: &str = "px-3 py-1.5 rounded-full border border-clay-text text-xs font-medium text-clay-text bg-clay-text/10 transition-colors cursor-pointer";
const MODE_IDLE_CLASS: &str = "px-3 py-1.5 rounded-full border border-ink-3 text-xs text-fg-2 hover:text-fg hover:border-ink-3/80 transition-colors cursor-pointer";
const ACTION_CLASS: &str =
    "px-3 py-1.5 rounded-full border border-ink-3 text-xs text-fg-2 hover:text-fg";
const PANEL_CLASS: &str = "rounded-2xl border border-ink-3/70 bg-surface-2/30 p-4 sm:p-5 shadow-xs";

#[cfg(target_arch = "wasm32")]
struct VisibleObserver {
    observer: web_sys::IntersectionObserver,
    _callback:
        wasm_bindgen::closure::Closure<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for VisibleObserver {
    fn drop(&mut self) {
        self.observer.disconnect();
    }
}

pub(super) fn preview(slug: &str, detail: bool) -> Option<AnyView> {
    match slug {
        "comment-card-shell" => Some(view! { <CommentCardShellPreview detail=detail/> }.into_any()),
        "comment-item" => Some(view! { <CommentItemPreview detail=detail/> }.into_any()),
        "comment-list" => Some(view! { <CommentListPreview detail=detail/> }.into_any()),
        "pending-comment-item" | "comment-form" | "comment-section" => {
            Some(view! { <CommentPreview slug=slug.to_string() detail=detail/> }.into_any())
        }
        _ => None,
    }
}

/// 固定样例评论的公共字段；其余字段由调用处用结构体更新语法覆盖。
fn base_comment(
    id: i64,
    parent_id: Option<i64>,
    depth: i32,
    author: &str,
    html: &str,
) -> PublicComment {
    PublicComment {
        id,
        parent_id,
        depth,
        author_name: author.to_string(),
        author_url: None,
        avatar_url: String::new(),
        is_author: false,
        content_html: Some(html.to_string()),
        created_at: String::new(),
        created_at_iso: String::new(),
    }
}

fn approved_sample() -> Vec<PublicComment> {
    [
        (
            101,
            None,
            0,
            "青禾",
            "春天的第一片叶子，总会记得来时的风。",
            "昨天 17:30",
        ),
        (102, Some(101), 1, "阿木", "我也喜欢这句话。", "昨天 18:02"),
        (
            103,
            Some(102),
            2,
            "青禾",
            "谢谢，愿你也有好天气。",
            "今天 09:15",
        ),
    ]
    .into_iter()
    .map(
        |(id, parent_id, depth, author_name, content, created_at)| PublicComment {
            id,
            parent_id,
            depth,
            author_name: author_name.to_string(),
            author_url: None,
            avatar_url: String::new(),
            is_author: id != 102,
            content_html: Some(format!("<p>{content}</p>")),
            created_at: created_at.to_string(),
            created_at_iso: format!("2026-09-22T{:02}:00:00Z", 10 + depth),
        },
    )
    .collect()
}

fn pending_sample() -> PendingComment {
    PendingComment {
        id: -10,
        parent_id: Some(102),
        depth: 2,
        author_name: "叶子访客".to_string(),
        author_url: None,
        avatar_url: String::new(),
        content_md: "这条样例评论正在等待审核。".to_string(),
        created_at: "2026-01-01T08:00:00Z".to_string(),
        stored_at: "2026-01-01T08:00:00Z".to_string(),
    }
}

fn initial_approved(slug: &str, detail: bool) -> Vec<PublicComment> {
    match slug {
        "comment-section" if !detail => approved_sample().into_iter().take(1).collect(),
        "comment-form" | "pending-comment-item" => Vec::new(),
        _ => approved_sample(),
    }
}

fn initial_pending(slug: &str, detail: bool) -> Vec<PendingComment> {
    match slug {
        "comment-section" if detail => vec![pending_sample()],
        _ => Vec::new(),
    }
}

fn scope(slug: &str) -> &'static str {
    match slug {
        "pending-comment-item" => "showcase-pending-comment-item",
        "comment-form" => "showcase-comment-form",
        _ => "showcase-comment-section",
    }
}

/// 图鉴提交仅生成内存中的待审核项，复用正式列表的合并和渲染路径。
fn local_pending_comment(
    approved: &[PublicComment],
    existing: &[PendingComment],
    parent_id: Option<i64>,
    author_name: String,
    author_url: String,
    content_md: String,
) -> PendingComment {
    let parent_depth = parent_id.and_then(|id| {
        approved
            .iter()
            .find(|comment| comment.id == id)
            .map(|comment| comment.depth)
            .or_else(|| {
                existing
                    .iter()
                    .find(|comment| comment.id == id)
                    .map(|comment| comment.depth)
            })
    });
    let next_id = existing
        .iter()
        .map(|comment| comment.id)
        .min()
        .unwrap_or(0)
        .min(0)
        - 1;
    let now = chrono::Utc::now().to_rfc3339();
    PendingComment {
        id: next_id,
        parent_id,
        depth: parent_depth.map_or(0, |depth| depth + 1),
        author_name,
        author_url: (!author_url.trim().is_empty()).then_some(author_url),
        avatar_url: String::new(),
        content_md,
        created_at: now.clone(),
        stored_at: now,
    }
}

/// 图鉴提交：只写入当前预览的内存列表，不请求、不持久化。
fn local_submit(
    approved: RwSignal<Vec<PublicComment>>,
    pending: RwSignal<Vec<PendingComment>>,
) -> Callback<CommentSubmission, CommentSubmitFuture> {
    Callback::new(move |s: CommentSubmission| -> CommentSubmitFuture {
        let comment = approved.with_untracked(|approved| {
            pending.with_untracked(|existing| {
                local_pending_comment(
                    approved,
                    existing,
                    s.parent_id,
                    s.author_name,
                    s.author_url,
                    s.content_md,
                )
            })
        });
        let _ = pending.try_update(|list| list.push(comment));
        Box::pin(std::future::ready(Ok(
            "已加入本地演示列表，内容不会保存".to_string()
        )))
    })
}

/// 图鉴用的本地评论上下文：不触达评论接口、不持久化、不允许上传图片。
fn provide_local_context(
    active_reply: RwSignal<Option<i64>>,
    approved: RwSignal<Vec<PublicComment>>,
    pending: RwSignal<Vec<PendingComment>>,
    id_scope: &'static str,
) {
    provide_context(CommentContext {
        active_reply,
        refresh_trigger: RwSignal::new(false),
        pending_comments: pending,
        current_user: RwSignal::new(None),
        submit: local_submit(approved, pending),
        image_upload: false,
        id_scope,
    });
}

/// 形态切换按钮组；点击先写入 `mode`，再回调 `on_pick` 处理附带状态。
fn mode_buttons<M>(
    mode: RwSignal<M>,
    items: Vec<(M, &'static str)>,
    on_pick: Callback<M>,
) -> impl IntoView
where
    M: Copy + PartialEq + Send + Sync + 'static,
{
    items
        .into_iter()
        .map(|(mode_val, label)| {
            view! {
                <button
                    type="button"
                    class=move || {
                        if mode.get() == mode_val { MODE_ACTIVE_CLASS } else { MODE_IDLE_CLASS }
                    }
                    aria-pressed=move || (mode.get() == mode_val).to_string()
                    on:click=move |_| {
                        mode.set(mode_val);
                        on_pick.run(mode_val);
                    }
                >
                    {label}
                </button>
            }
        })
        .collect_view()
}

#[component]
fn CommentPreview(slug: String, detail: bool) -> impl IntoView {
    let defer_editor = !detail && matches!(slug.as_str(), "comment-form" | "comment-section");
    let visible = RwSignal::new(!defer_editor);
    let approved = RwSignal::new(initial_approved(&slug, detail));
    let pending = RwSignal::new(initial_pending(&slug, detail));
    provide_local_context(RwSignal::new(None), approved, pending, scope(&slug));

    let placeholder = NodeRef::<leptos::html::Div>::new();
    // 编辑器延迟到卡片进入视口再挂载；观察器随组件所有者释放时断开。
    #[cfg(target_arch = "wasm32")]
    {
        let observer = StoredValue::new_local(None::<VisibleObserver>);
        Effect::new(move |_| {
            use wasm_bindgen::JsCast;
            let Some(element) = placeholder.get() else {
                return;
            };
            if observer.with_value(|current| current.is_some()) {
                return;
            }
            let callback = wasm_bindgen::closure::Closure::<
                dyn FnMut(js_sys::Array, web_sys::IntersectionObserver),
            >::new(
                move |entries: js_sys::Array, watcher: web_sys::IntersectionObserver| {
                    if entries.iter().any(|entry| {
                        entry
                            .dyn_into::<web_sys::IntersectionObserverEntry>()
                            .is_ok_and(|entry| entry.is_intersecting())
                    }) {
                        watcher.disconnect();
                        let _ = visible.try_set(true);
                    }
                },
            );
            match web_sys::IntersectionObserver::new(callback.as_ref().unchecked_ref()) {
                Ok(watcher) => {
                    watcher.observe(&element);
                    observer.set_value(Some(VisibleObserver {
                        observer: watcher,
                        _callback: callback,
                    }));
                }
                Err(_) => visible.set(true),
            }
        });
    }

    move || {
        if !visible.get() {
            return view! {
                <div
                    node_ref=placeholder
                    class="min-h-[190px] w-full rounded-xl border border-ink-3 bg-surface-2/40 p-4"
                >
                    <div class="flex items-center gap-2 text-xs text-fg-3">
                        <span class="size-2 rounded-full bg-clay-text/50"></span>
                        "评论编辑器将在卡片出现时加载"
                    </div>
                    <div class="mt-4 h-9 rounded-lg bg-surface-0/70"></div>
                    <div class="mt-3 h-20 rounded-lg bg-surface-0/70"></div>
                </div>
            }
            .into_any();
        }

        let body = match slug.as_str() {
            "pending-comment-item" => {
                let mut comment = pending_sample();
                comment.parent_id = None;
                comment.depth = 0;
                view! { <PendingCommentItem comment=comment post_id=0/> }.into_any()
            }
            "comment-form" => view! {
                <CommentForm post_id=0 parent_id=None parent_indent=None/>
                {move || {
                    (!pending.with(|list| list.is_empty()))
                        .then(|| {
                            view! {
                                <CommentList
                                    comments=Signal::stored(Vec::<PublicComment>::new())
                                    pending=pending
                                    post_id=0
                                />
                            }
                        })
                }}
            }
            .into_any(),
            "comment-section" => view! {
                <CommentSectionContent
                    post_id=0
                    comments=Signal::derive(move || Some(approved.get()))
                    approved_count=Signal::derive(move || {
                        Some(approved.with(|list| list.len()) as i64)
                    })
                    failed=Signal::stored(false)
                />
            }
            .into_any(),
            _ => ().into_any(),
        };
        let actions = (detail && slug == "comment-section").then(|| {
            view! {
                <div class="flex flex-wrap gap-2 mb-4">
                    <button
                        type="button"
                        class=ACTION_CLASS
                        on:click=move |_| {
                            approved.set(Vec::new());
                            pending.set(Vec::new());
                        }
                    >
                        "空列表"
                    </button>
                    <button
                        type="button"
                        class=ACTION_CLASS
                        on:click=move |_| {
                            approved.set(approved_sample());
                            pending.set(Vec::new());
                        }
                    >
                        "仅已审核"
                    </button>
                    <button
                        type="button"
                        class=ACTION_CLASS
                        on:click=move |_| {
                            approved.set(approved_sample());
                            pending.set(vec![pending_sample()]);
                        }
                    >
                        "评论与待审核"
                    </button>
                </div>
            }
        });
        let note = detail
            .then(|| {
                view! { <p class="mb-4 text-xs text-fg-3">"本地演示，评论不会保存，也不会上传图片。"</p> }
            });
        view! {
            <div class="w-full min-w-0">
                {note}
                {actions}
                {body}
            </div>
        }
        .into_any()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommentItemMode {
    Standard,
    ReplyDraft,
    AuthorTypes,
    DeepNesting,
}

/// 标准回复链（顶层 + 一条回复），`reply_html` 为回复正文。
fn reply_chain(reply_html: &str) -> AnyView {
    let parent = PublicComment {
        is_author: true,
        created_at: "昨天 17:30".to_string(),
        created_at_iso: "2026-09-22T10:00:00Z".to_string(),
        ..base_comment(
            101,
            None,
            0,
            "青禾",
            "<p>春天的第一片叶子，总会记得来时的风。</p>",
        )
    };
    let reply = PublicComment {
        created_at: "昨天 18:02".to_string(),
        created_at_iso: "2026-09-22T11:00:00Z".to_string(),
        ..base_comment(102, Some(101), 1, "阿木", reply_html)
    };
    view! {
        <CommentItem comment=parent post_id=0/>
        <CommentItem comment=reply post_id=0/>
    }
    .into_any()
}

fn item_mode_rows(mode: CommentItemMode) -> AnyView {
    match mode {
        CommentItemMode::Standard => reply_chain("<p>我也喜欢这句话，愿新的一年枝繁叶茂。</p>"),
        CommentItemMode::ReplyDraft => {
            reply_chain("<p>我也喜欢这句话，点击下方回复体验草稿保留功能。</p>")
        }
        CommentItemMode::AuthorTypes => {
            let author_comment = PublicComment {
                is_author: true,
                created_at: "昨天 17:30".to_string(),
                created_at_iso: "2026-09-22T10:00:00Z".to_string(),
                ..base_comment(
                    301,
                    None,
                    0,
                    "青禾",
                    "<p>博主身份演示：带有专属绿色高亮的<strong>作者</strong>徽章。</p>",
                )
            };
            let visitor_with_url = PublicComment {
                author_url: Some("https://example.com".to_string()),
                created_at: "昨天 18:20".to_string(),
                created_at_iso: "2026-09-22T11:00:00Z".to_string(),
                ..base_comment(
                    302,
                    None,
                    0,
                    "远方的小树",
                    "<p>附带外链访客：作者昵称展示为外部主页链接（在新标签页安全打开，带 <code>rel=\"nofollow noopener\"</code> 属性）。</p>",
                )
            };
            let plain_visitor = PublicComment {
                created_at: "昨天 19:00".to_string(),
                created_at_iso: "2026-09-22T12:00:00Z".to_string(),
                ..base_comment(
                    303,
                    None,
                    0,
                    "林间漫步",
                    "<p>普通访客：未提供个人主页链接，昵称渲染为普通加粗文本。</p>",
                )
            };
            view! {
                <CommentItem comment=author_comment post_id=0/>
                <CommentItem comment=visitor_with_url post_id=0/>
                <CommentItem comment=plain_visitor post_id=0/>
            }
            .into_any()
        }
        CommentItemMode::DeepNesting => {
            let c0 = PublicComment {
                is_author: true,
                created_at: "10 分钟前".to_string(),
                created_at_iso: "2026-09-22T10:00:00Z".to_string(),
                ..base_comment(
                    401,
                    None,
                    0,
                    "青禾",
                    "<p>顶层评论（depth = 0）：无左侧边框引导线与 margin 缩进。</p>",
                )
            };
            let c1 = PublicComment {
                created_at: "8 分钟前".to_string(),
                created_at_iso: "2026-09-22T10:02:00Z".to_string(),
                ..base_comment(
                    402,
                    Some(401),
                    1,
                    "阿木",
                    "<p>一级嵌套（depth = 1）：启用左侧引导线 <code>border-l-2</code>，建立父子回复视觉联系。</p>",
                )
            };
            let c2 = PublicComment {
                created_at: "6 分钟前".to_string(),
                created_at_iso: "2026-09-22T10:04:00Z".to_string(),
                ..base_comment(
                    403,
                    Some(402),
                    2,
                    "山风",
                    "<p>二级嵌套（depth = 2）：在引导线基础上叠加 16px 阶梯缩进，凸显回复层级。</p>",
                )
            };
            let c3 = PublicComment {
                created_at: "4 分钟前".to_string(),
                created_at_iso: "2026-09-22T10:06:00Z".to_string(),
                ..base_comment(
                    404,
                    Some(403),
                    3,
                    "海棠",
                    "<p>三级嵌套（depth = 3）：叠加 32px 阶梯缩进，树形关系更深入。</p>",
                )
            };
            let c20 = PublicComment {
                created_at: "刚刚".to_string(),
                created_at_iso: "2026-09-22T10:10:00Z".to_string(),
                ..base_comment(
                    420,
                    Some(404),
                    20,
                    "边界防护测试",
                    "<p>极限深度（depth = 20）：到达最大嵌套深度防护限制，下方回复按钮自动隐藏，防止无限嵌套破坏页面排版。</p>",
                )
            };
            view! {
                <CommentItem comment=c0 post_id=0/>
                <CommentItem comment=c1 post_id=0/>
                <CommentItem comment=c2 post_id=0/>
                <CommentItem comment=c3 post_id=0/>
                <CommentItem comment=c20 post_id=0/>
            }
            .into_any()
        }
    }
}

#[component]
fn CommentItemPreview(detail: bool) -> impl IntoView {
    let mode = RwSignal::new(CommentItemMode::Standard);
    let active_reply = RwSignal::new(None::<i64>);
    let approved_store = RwSignal::new(Vec::<PublicComment>::new());
    let pending_store = RwSignal::new(Vec::<PendingComment>::new());
    provide_local_context(
        active_reply,
        approved_store,
        pending_store,
        "showcase-comment-item",
    );

    if !detail {
        let parent = PublicComment {
            is_author: true,
            created_at: "昨天 17:30".to_string(),
            created_at_iso: "2026-09-22T10:00:00Z".to_string(),
            ..base_comment(
                101,
                None,
                0,
                "青禾",
                "<p>春天的第一片叶子，总会记得来时的风。</p>",
            )
        };
        let reply = PublicComment {
            created_at: "昨天 18:02".to_string(),
            created_at_iso: "2026-09-22T11:00:00Z".to_string(),
            ..base_comment(102, Some(101), 1, "阿木", "<p>我也喜欢这句话。</p>")
        };
        return view! {
            <div class="comment-list w-full min-w-0 divide-y divide-ink-3/40 px-1">
                <CommentItem comment=parent post_id=0/>
                <CommentItem comment=reply post_id=0/>
            </div>
        }
        .into_any();
    }

    let buttons = mode_buttons(
        mode,
        vec![
            (CommentItemMode::Standard, "标准回复链"),
            (CommentItemMode::ReplyDraft, "展开回复与草稿"),
            (CommentItemMode::AuthorTypes, "身份与外链"),
            (CommentItemMode::DeepNesting, "深层缩进与边界 (depth=20)"),
        ],
        Callback::new(move |picked: CommentItemMode| {
            if picked == CommentItemMode::ReplyDraft {
                active_reply.set(Some(101));
            } else {
                active_reply.set(None);
            }
        }),
    );

    view! {
        <div class="w-full min-w-0">
            <p class="mb-3 text-xs text-fg-3">
                "本地演示，可切换不同形态体验 CommentItem 的回复表单联动、草稿保留与身份样式。"
            </p>
            <div class="flex flex-wrap items-center gap-2 mb-3">{buttons}</div>
            <p class="text-xs text-fg-2 mb-4 leading-relaxed">
                {move || match mode.get() {
                    CommentItemMode::Standard => "标准嵌套链：展示博主顶层评论与嵌套子回复，点击“回复”可展开内嵌表单并输入草稿。",
                    CommentItemMode::ReplyDraft => "回复展开与草稿保留：展开内嵌 CommentForm 表单，表单自动通过负 margin 抵消嵌套缩进；取消回复再重新打开时保留草稿。",
                    CommentItemMode::AuthorTypes => "身份与外链差异：对比博主专属“作者”徽章、访客个人主页外链（在新标签打开）、以及普通访客纯文本昵称。",
                    CommentItemMode::DeepNesting => "深层缩进与边界防护：展示 depth=1 至 depth=3 的递进缩进与引导线；当深度达到 depth >= 20 时，自动隐藏回复按钮防止无限嵌套破坏版面。",
                }}
            </p>
            <div class=PANEL_CLASS>
                <div class="comment-list space-y-0 divide-y divide-ink-3/40">
                    {move || item_mode_rows(mode.get())}
                </div>
            </div>
        </div>
    }
    .into_any()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommentListMode {
    Merged,
    ApprovedOnly,
    Empty,
    ComplexTree,
}

fn list_mode_data(mode: CommentListMode) -> (Vec<PublicComment>, Vec<PendingComment>) {
    match mode {
        CommentListMode::Merged => (approved_sample(), vec![pending_sample()]),
        CommentListMode::ApprovedOnly => (approved_sample(), Vec::new()),
        CommentListMode::Empty => (Vec::new(), Vec::new()),
        CommentListMode::ComplexTree => {
            let tree_approved = vec![
                PublicComment {
                    is_author: true,
                    created_at: "昨天 10:00".to_string(),
                    created_at_iso: "2026-09-22T10:00:00Z".to_string(),
                    ..base_comment(201, None, 0, "青禾", "<p>新功能组件图鉴上线，欢迎大家体验！</p>")
                },
                PublicComment {
                    created_at: "昨天 11:30".to_string(),
                    created_at_iso: "2026-09-22T11:30:00Z".to_string(),
                    ..base_comment(
                        202,
                        Some(201),
                        1,
                        "阿木",
                        "<p>交互非常流畅，尤其细节动画很精致。</p>",
                    )
                },
                PublicComment {
                    is_author: true,
                    created_at: "昨天 12:00".to_string(),
                    created_at_iso: "2026-09-22T12:00:00Z".to_string(),
                    ..base_comment(
                        203,
                        Some(202),
                        2,
                        "青禾",
                        "<p>感谢支持，我们会继续完善体验。</p>",
                    )
                },
                PublicComment {
                    created_at: "昨天 14:00".to_string(),
                    created_at_iso: "2026-09-22T14:00:00Z".to_string(),
                    ..base_comment(
                        204,
                        None,
                        0,
                        "林间微风",
                        "<p>请问后台支持自定义主题色配置吗？</p>",
                    )
                },
                PublicComment {
                    created_at: "昨天 15:00".to_string(),
                    created_at_iso: "2026-09-22T15:00:00Z".to_string(),
                    // 孤儿评论：指向不存在的父评论
                    ..base_comment(
                        205,
                        Some(99999),
                        1,
                        "探索者",
                        "<p>这是指向已失效父评论的孤儿评论（parent_id = 99999）。CommentList 自动将其降为顶层展示，确保评论不会静默丢失。</p>",
                    )
                },
            ];
            let tree_pending = vec![PendingComment {
                id: -20,
                parent_id: Some(204),
                depth: 1,
                author_name: "叶子访客".to_string(),
                author_url: None,
                avatar_url: String::new(),
                content_md: "支持的，可以在系统设置中调整调色板。".to_string(),
                created_at: "2026-09-22T14:30:00Z".to_string(),
                stored_at: "2026-09-22T14:30:00Z".to_string(),
            }];
            (tree_approved, tree_pending)
        }
    }
}

#[component]
fn CommentListPreview(detail: bool) -> impl IntoView {
    let mode = RwSignal::new(CommentListMode::Merged);
    let active_reply = RwSignal::new(None::<i64>);
    let approved_store = RwSignal::new(Vec::<PublicComment>::new());
    let pending_store = RwSignal::new(Vec::<PendingComment>::new());
    provide_local_context(
        active_reply,
        approved_store,
        pending_store,
        "showcase-comment-list",
    );

    if !detail {
        let comments = vec![PublicComment {
            is_author: true,
            created_at: "昨天 17:30".to_string(),
            created_at_iso: "2026-09-22T10:00:00Z".to_string(),
            ..base_comment(
                101,
                None,
                0,
                "青禾",
                "<p>春天的第一片叶子，总会记得来时的风。</p>",
            )
        }];
        let pending = vec![PendingComment {
            id: -10,
            parent_id: Some(101),
            depth: 1,
            author_name: "叶子访客".to_string(),
            author_url: None,
            avatar_url: String::new(),
            content_md: "这条样例评论正在等待审核。".to_string(),
            created_at: "2026-09-22T10:30:00Z".to_string(),
            stored_at: "2026-09-22T10:30:00Z".to_string(),
        }];
        return view! {
            <div class="w-full min-w-0 px-1">
                <CommentList comments=Signal::stored(comments) pending=Signal::stored(pending) post_id=0/>
            </div>
        }
        .into_any();
    }

    let buttons = mode_buttons(
        mode,
        vec![
            (CommentListMode::Merged, "评论与待审核"),
            (CommentListMode::ApprovedOnly, "仅已审核"),
            (CommentListMode::Empty, "空列表"),
            (CommentListMode::ComplexTree, "多分支与孤儿容错"),
        ],
        Callback::new(|_| {}),
    );

    view! {
        <div class="w-full min-w-0">
            <p class="mb-3 text-xs text-fg-3">
                "本地演示，可切换不同形态体验 CommentList 的树形合并、排序与孤儿容错。"
            </p>
            <div class="flex flex-wrap items-center gap-2 mb-3">{buttons}</div>
            <p class="text-xs text-fg-2 mb-4 leading-relaxed">
                {move || match mode.get() {
                    CommentListMode::Merged => "评论与待审核：服务端已审核评论与本地待审核评论统一合并为树形结构；待审核评论展示为带呼吸指示灯的半透明卡片，按时间升序插入对应层级。",
                    CommentListMode::ApprovedOnly => "仅已审核：纯已审核评论树，展现多级回复的深度缩进、左侧引导线与完整回复交互。",
                    CommentListMode::Empty => "空列表：无任何评论时的缺省占位展示，引导访客成为首位留言者。",
                    CommentListMode::ComplexTree => "多分支与孤儿容错：展示多个并行顶层主题、多级嵌套回复，以及父评论缺失时的孤儿评论容错恢复（自动升级为顶层展示）。",
                }}
            </p>
            <div class=PANEL_CLASS>
                {move || {
                    let (comments, pending) = list_mode_data(mode.get());
                    if comments.is_empty() && pending.is_empty() {
                        view! {
                            <div class="text-center py-8 px-4 rounded-2xl border border-dashed border-ink-3 text-fg-2">
                                "暂无评论"
                            </div>
                        }
                            .into_any()
                    } else {
                        view! {
                            <CommentList
                                comments=Signal::stored(comments)
                                pending=Signal::stored(pending)
                                post_id=0
                            />
                        }
                            .into_any()
                    }
                }}
            </div>
        </div>
    }
    .into_any()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CardShellMode {
    All,
    TopLevel,
    NestedReply,
    DeepIndent,
    Pending,
}

#[component]
fn ShellReplyButton(
    comment_id: i32,
    active_id: RwSignal<Option<i32>>,
    author_name: String,
) -> impl IntoView {
    let is_active = move || active_id.get() == Some(comment_id);
    view! {
        <div class="mt-2">
            <button
                type="button"
                class=move || {
                    format!(
                        "inline-flex items-center gap-1 text-xs font-medium px-2 py-1 rounded-md transition-colors cursor-pointer {}",
                        if is_active() {
                            "text-clay-text bg-clay-text/10"
                        } else {
                            "text-fg-3 hover:text-clay-text hover:bg-surface-2"
                        },
                    )
                }
                aria-label=format!("回复 {author_name}")
                on:click=move |_| {
                    if active_id.get_untracked() == Some(comment_id) {
                        active_id.set(None);
                    } else {
                        active_id.set(Some(comment_id));
                    }
                }
            >
                <svg
                    class="w-3.5 h-3.5"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    viewBox="0 0 24 24"
                >
                    <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        d="M3 10h10a5 5 0 015 5v2m-15-7l4-4m-4 4l4 4"
                    ></path>
                </svg>
                {move || if is_active() { "取消回复" } else { "回复" }}
            </button>
            {move || {
                is_active()
                    .then(|| {
                        view! {
                            <div class="mt-2.5 flex items-center gap-2 p-2.5 rounded-xl border border-ink-3 bg-surface-2/60 text-xs text-fg-2 animate-in fade-in duration-200">
                                <span class="size-1.5 rounded-full bg-clay-text shrink-0"></span>
                                <span>"children 插槽生效中：可挂载回复表单或快捷操作栏。"</span>
                            </div>
                        }
                    })
            }}
        </div>
    }
}

/// 卡片外壳的一条固定样例。
#[derive(Clone, Copy)]
struct ShellSample {
    id: i32,
    depth: i32,
    muted: bool,
    author: &'static str,
    is_author: bool,
    time: &'static str,
    is_pending: bool,
    html: &'static str,
}

impl ShellSample {
    const fn new(
        id: i32,
        depth: i32,
        author: &'static str,
        is_author: bool,
        time: &'static str,
        html: &'static str,
    ) -> Self {
        Self {
            id,
            depth,
            muted: false,
            author,
            is_author,
            time,
            is_pending: false,
            html,
        }
    }

    const fn pending(mut self) -> Self {
        self.muted = true;
        self.is_pending = true;
        self
    }
}

fn render_shell_comment(replying_to: RwSignal<Option<i32>>, sample: ShellSample) -> AnyView {
    let author = sample.author;
    let author_element = view! { <span class="font-medium text-fg">{author}</span> }.into_any();
    let author_badge = if sample.is_author {
        view! {
            <span class="px-1.5 py-0.5 rounded text-[10px] font-medium bg-clay-text/10 text-clay-text">
                "作者"
            </span>
        }
        .into_any()
    } else {
        ().into_any()
    };
    let timestamp = view! { <span class="text-fg-3">{sample.time}</span> }.into_any();
    let status_badge = if sample.is_pending {
        view! {
            <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20">
                <span class="w-1.5 h-1.5 rounded-full bg-amber-500 animate-pulse"></span>
                "审核中"
            </span>
        }
        .into_any()
    } else {
        ().into_any()
    };
    if sample.is_pending {
        view! {
            <CommentCardShell
                depth=sample.depth
                muted=sample.muted
                avatar_url=String::new()
                author_name=author.to_string()
                author_element=author_element
                author_badge=author_badge
                timestamp=timestamp
                status_badge=status_badge
                content_html=sample.html.to_string()
            />
        }
        .into_any()
    } else {
        view! {
            <CommentCardShell
                depth=sample.depth
                muted=sample.muted
                avatar_url=String::new()
                author_name=author.to_string()
                author_element=author_element
                author_badge=author_badge
                timestamp=timestamp
                status_badge=status_badge
                content_html=sample.html.to_string()
                content_extra_class="md-content"
            >
                <ShellReplyButton
                    comment_id=sample.id
                    active_id=replying_to
                    author_name=author.to_string()
                />
            </CommentCardShell>
        }
        .into_any()
    }
}

fn render_shell_samples(replying_to: RwSignal<Option<i32>>, samples: &[ShellSample]) -> AnyView {
    samples
        .iter()
        .map(|sample| render_shell_comment(replying_to, *sample))
        .collect_view()
        .into_any()
}

fn shell_mode_rows(mode: CardShellMode, replying_to: RwSignal<Option<i32>>) -> AnyView {
    let top = ShellSample::new(
        1,
        0,
        "青禾",
        true,
        "昨天 17:30",
        "<p>春天的第一片叶子，总会记得来时的风。</p>",
    );
    let nested = ShellSample::new(
        2,
        1,
        "阿木",
        false,
        "昨天 18:02",
        "<p>我也喜欢这句话，愿新的一年枝繁叶茂。</p>",
    );
    let deep = ShellSample::new(
        3,
        2,
        "青禾",
        true,
        "今天 09:15",
        "<p>谢谢，愿你也有好天气。</p>",
    );
    match mode {
        CardShellMode::All => render_shell_samples(
            replying_to,
            &[
                top,
                nested,
                deep,
                ShellSample::new(
                    4,
                    2,
                    "叶子访客",
                    false,
                    "刚刚",
                    "<p>这条样例回复正在等待审核，卡片整体呈半透明淡化展示。</p>",
                )
                .pending(),
            ],
        ),
        CardShellMode::TopLevel => render_shell_samples(replying_to, &[top]),
        CardShellMode::NestedReply => render_shell_samples(replying_to, &[nested]),
        CardShellMode::DeepIndent => render_shell_samples(replying_to, &[deep]),
        CardShellMode::Pending => render_shell_samples(
            replying_to,
            &[ShellSample::new(
                4,
                0,
                "叶子访客",
                false,
                "刚刚",
                "<p>这条样例评论正在等待审核，整卡呈现 75% 不透明度与审核中徽章。</p>",
            )
            .pending()],
        ),
    }
}

#[component]
fn CommentCardShellPreview(detail: bool) -> impl IntoView {
    let mode = RwSignal::new(CardShellMode::All);
    let replying_to = RwSignal::new(None::<i32>);

    if !detail {
        let samples = [
            ShellSample::new(
                1,
                0,
                "青禾",
                true,
                "昨天 17:30",
                "<p>春天的第一片叶子，总会记得来时的风。</p>",
            ),
            ShellSample::new(
                2,
                1,
                "阿木",
                false,
                "昨天 18:02",
                "<p>我也喜欢这句话，愿枝繁叶茂。</p>",
            ),
        ];
        return view! {
            <div class="w-full min-w-0 divide-y divide-ink-3/40 px-1">
                {render_shell_samples(replying_to, &samples)}
            </div>
        }
        .into_any();
    }

    let buttons = mode_buttons(
        mode,
        vec![
            (CardShellMode::All, "全部形态"),
            (CardShellMode::TopLevel, "顶层评论 (depth=0)"),
            (CardShellMode::NestedReply, "嵌套回复 (depth=1)"),
            (CardShellMode::DeepIndent, "深层嵌套 (depth=2)"),
            (CardShellMode::Pending, "待审核 (muted)"),
        ],
        Callback::new(|_| {}),
    );

    view! {
        <div class="w-full min-w-0">
            <p class="mb-3 text-xs text-fg-3">
                "本地演示，切换下方的形态标签可预览卡片骨架的不同状态与缩进效果。"
            </p>
            <div class="flex flex-wrap items-center gap-2 mb-3">{buttons}</div>
            <p class="text-xs text-fg-2 mb-4 leading-relaxed">
                {move || match mode.get() {
                    CardShellMode::All => "完整树形展示：包含顶层评论、一级嵌套引导线、二级阶梯缩进与待审核半透明状态。",
                    CardShellMode::TopLevel => "顶层评论（depth = 0）：无左侧边框与缩进，包含头像、作者徽章、绝对时间与回复操作。",
                    CardShellMode::NestedReply => "一级嵌套（depth = 1）：启用左侧引导线 border-l-2，区分父子回复关系。",
                    CardShellMode::DeepIndent => "深层嵌套（depth = 2）：在左侧引导线基础上叠加 margin-left 阶梯缩进。",
                    CardShellMode::Pending => "待审核状态（muted = true）：整卡 opacity-75 淡化，配合'审核中'徽章与呼吸指示灯。",
                }}
            </p>
            <div class="rounded-2xl border border-ink-3/70 bg-surface-2/30 p-4 sm:p-5 divide-y divide-ink-3/40 shadow-xs">
                {move || shell_mode_rows(mode.get(), replying_to)}
            </div>
        </div>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_previews_dispatch_properly() {
        Owner::new().with(|| {
            assert!(preview("comment-card-shell", false).is_some());
            assert!(preview("comment-card-shell", true).is_some());
            assert!(preview("comment-item", false).is_some());
            assert!(preview("comment-item", true).is_some());
            assert!(preview("comment-list", false).is_some());
            assert!(preview("comment-list", true).is_some());
            assert!(preview("pending-comment-item", false).is_some());
            assert!(preview("comment-form", false).is_some());
            assert!(preview("comment-section", false).is_some());
            assert!(preview("unknown-slug", false).is_none());
        });
    }

    #[test]
    fn deferred_editor_renders_placeholder_on_server() {
        let html = Owner::new().with(|| {
            preview("comment-form", false)
                .expect("评论预览必须实现")
                .to_html()
        });
        assert!(html.contains("评论编辑器将在卡片出现时加载"), "{html}");
    }

    #[test]
    fn local_reply_uses_parent_depth_and_unique_negative_id() {
        let parent = PendingComment {
            id: -10,
            parent_id: None,
            depth: 1,
            author_name: "样例".to_string(),
            author_url: None,
            avatar_url: String::new(),
            content_md: "原评论".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            stored_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let reply = local_pending_comment(
            &[],
            &[parent],
            Some(-10),
            "访客".to_string(),
            String::new(),
            "回复内容".to_string(),
        );
        assert_eq!(
            (reply.id, reply.parent_id, reply.depth),
            (-11, Some(-10), 2)
        );
        assert_eq!(reply.author_url, None);
        assert_eq!(reply.content_md, "回复内容");

        let approved = PublicComment {
            is_author: true,
            created_at: "昨天".to_string(),
            created_at_iso: "2026-01-01T00:00:00Z".to_string(),
            ..base_comment(101, None, 0, "作者", "<p>原评论</p>")
        };
        let reply = local_pending_comment(
            &[approved],
            &[],
            Some(101),
            "访客".to_string(),
            String::new(),
            "回复".to_string(),
        );
        assert_eq!((reply.id, reply.parent_id, reply.depth), (-1, Some(101), 1));
    }
}
