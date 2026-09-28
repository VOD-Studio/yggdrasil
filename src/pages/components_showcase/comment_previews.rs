//! 评论组件图鉴：固定样例与局部上下文，展示节点仍由正式评论组件渲染。

use dioxus::prelude::*;

use crate::components::comments::card::CommentCardShell;
use crate::components::comments::form::CommentForm;
use crate::components::comments::list::CommentList;
use crate::components::comments::pending_item::PendingCommentItem;
use crate::components::comments::section::{CommentContext, CommentSectionContent, CommentSource};
use crate::models::comment::PublicComment;
use crate::utils::comment_storage::PendingComment;

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

pub(super) fn preview(slug: &str, detail: bool) -> Option<Element> {
    match slug {
        "comment-card-shell" => Some(rsx! {
            CommentCardShellPreview { detail }
        }),
        "pending-comment-item"
        | "comment-item"
        | "comment-list"
        | "comment-form"
        | "comment-section" => Some(rsx! {
            CommentPreview { key: "{slug}", slug: slug.to_string(), detail }
        }),
        _ => None,
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
        "comment-item" => approved_sample().into_iter().take(2).collect(),
        "comment-section" if !detail => approved_sample().into_iter().take(1).collect(),
        "comment-form" | "pending-comment-item" => Vec::new(),
        _ => approved_sample(),
    }
}

fn initial_pending(slug: &str, detail: bool) -> Vec<PendingComment> {
    match slug {
        "comment-list" => vec![pending_sample()],
        "comment-section" if detail => vec![pending_sample()],
        _ => Vec::new(),
    }
}

fn scope(slug: &str) -> &'static str {
    match slug {
        "pending-comment-item" => "showcase-pending-comment-item",
        "comment-item" => "showcase-comment-item",
        "comment-list" => "showcase-comment-list",
        "comment-form" => "showcase-comment-form",
        _ => "showcase-comment-section",
    }
}

#[component]
fn CommentPreview(slug: String, detail: bool) -> Element {
    let defer_editor = !detail && matches!(slug.as_str(), "comment-form" | "comment-section");
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
    let mut visible = use_signal(|| !defer_editor);
    #[cfg(target_arch = "wasm32")]
    let mut observer: Signal<Option<VisibleObserver>> = use_signal(|| None);
    let ctx = use_context_provider(|| {
        let approved = Signal::new(initial_approved(&slug, detail));
        CommentContext {
            active_reply: Signal::new(None),
            refresh_trigger: Signal::new(false),
            pending_comments: Signal::new(initial_pending(&slug, detail)),
            current_user: Signal::new(None),
            source: CommentSource::Local { approved },
            id_scope: scope(&slug),
        }
    });
    if !visible() {
        return rsx! {
            div {
                class: "min-h-[190px] w-full rounded-xl border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)]/40 p-4",
                onmounted: move |_event| {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use wasm_bindgen::JsCast;
                        if observer.peek().is_some() { return; }
                        let mounted = _event.data();
                        let Some(element) = mounted.downcast::<web_sys::Element>() else {
                            visible.set(true);
                            return;
                        };
                        let callback = wasm_bindgen::closure::Closure::<
                            dyn FnMut(js_sys::Array, web_sys::IntersectionObserver),
                        >::new(move |entries: js_sys::Array, watcher: web_sys::IntersectionObserver| {
                            if entries.iter().any(|entry| {
                                entry
                                    .dyn_into::<web_sys::IntersectionObserverEntry>()
                                    .is_ok_and(|entry| entry.is_intersecting())
                            }) {
                                watcher.disconnect();
                                visible.set(true);
                            }
                        });
                        match web_sys::IntersectionObserver::new(callback.as_ref().unchecked_ref()) {
                            Ok(watcher) => {
                                watcher.observe(&element);
                                observer.set(Some(VisibleObserver { observer: watcher, _callback: callback }));
                            }
                            Err(_) => visible.set(true),
                        }
                    }
                },
                div { class: "flex items-center gap-2 text-xs text-paper-tertiary", span { class: "size-2 rounded-full bg-[var(--color-paper-accent)]/50" } "评论编辑器将在卡片出现时加载" }
                div { class: "mt-4 h-9 rounded-lg bg-[var(--color-paper-theme)]/70" }
                div { class: "mt-3 h-20 rounded-lg bg-[var(--color-paper-theme)]/70" }
            }
        };
    }
    let CommentSource::Local { mut approved } = ctx.source else {
        unreachable!("图鉴上下文只使用本地数据源")
    };
    let mut pending = ctx.pending_comments;
    let sample = approved_sample();
    let visible = match slug.as_str() {
        "pending-comment-item" => {
            let mut comment = pending_sample();
            comment.parent_id = None;
            comment.depth = 0;
            rsx! { PendingCommentItem { comment, post_id: 0 } }
        }
        "comment-item" => rsx! {
            CommentList { comments: approved(), pending: pending(), post_id: 0 }
        },
        "comment-list" => rsx! {
            if approved().is_empty() && pending().is_empty() {
                div { class: "text-center py-8 px-4 rounded-2xl border border-dashed border-[var(--color-paper-border)] text-paper-secondary", "暂无评论" }
            } else {
                CommentList { comments: approved(), pending: pending(), post_id: 0 }
            }
        },
        "comment-form" => rsx! {
            CommentForm { post_id: 0, parent_id: None, parent_indent: None }
            if !pending().is_empty() {
                CommentList { comments: Vec::new(), pending: pending(), post_id: 0 }
            }
        },
        "comment-section" => rsx! {
            CommentSectionContent {
                post_id: 0,
                comments: Some(approved()),
                approved_count: Some(approved().len() as i64),
            }
        },
        _ => unreachable!("预览入口已按 slug 过滤"),
    };

    rsx! {
        div { class: "w-full min-w-0",
            if detail {
                p { class: "mb-4 text-xs text-paper-tertiary", "本地演示，评论不会保存，也不会上传图片。" }
                if matches!(slug.as_str(), "comment-list" | "comment-section") {
                    div { class: "flex flex-wrap gap-2 mb-4",
                        button {
                            r#type: "button",
                            class: "px-3 py-1.5 rounded-full border border-[var(--color-paper-border)] text-xs text-paper-secondary hover:text-paper-primary",
                            onclick: move |_| { approved.set(Vec::new()); pending.set(Vec::new()); },
                            "空列表"
                        }
                        button {
                            r#type: "button",
                            class: "px-3 py-1.5 rounded-full border border-[var(--color-paper-border)] text-xs text-paper-secondary hover:text-paper-primary",
                            onclick: move |_| { approved.set(approved_sample()); pending.set(Vec::new()); },
                            "仅已审核"
                        }
                        button {
                            r#type: "button",
                            class: "px-3 py-1.5 rounded-full border border-[var(--color-paper-border)] text-xs text-paper-secondary hover:text-paper-primary",
                            onclick: move |_| { approved.set(sample.clone()); pending.set(vec![pending_sample()]); },
                            "评论与待审核"
                        }
                    }
                }
            }
            {visible}
        }
    }
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
    mut active_id: Signal<Option<i32>>,
    author_name: String,
) -> Element {
    let is_active = active_id() == Some(comment_id);
    rsx! {
        div { class: "mt-2",
            button {
                r#type: "button",
                class: "inline-flex items-center gap-1 text-xs font-medium px-2 py-1 rounded-md transition-all cursor-pointer",
                class: if is_active {
                    "text-[var(--color-paper-accent)] bg-[var(--color-paper-accent)]/10"
                } else {
                    "text-paper-tertiary hover:text-paper-accent hover:bg-[var(--color-paper-entry)]"
                },
                aria_label: "回复 {author_name}",
                onclick: move |_| {
                    if is_active {
                        active_id.set(None);
                    } else {
                        active_id.set(Some(comment_id));
                    }
                },
                svg {
                    class: "w-3.5 h-3.5",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M3 10h10a5 5 0 015 5v2m-15-7l4-4m-4 4l4 4",
                    }
                }
                if is_active { "取消回复" } else { "回复" }
            }
            if is_active {
                div { class: "mt-2.5 flex items-center gap-2 p-2.5 rounded-xl border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)]/60 text-xs text-paper-secondary animate-in fade-in duration-200",
                    span { class: "size-1.5 rounded-full bg-[var(--color-paper-accent)] shrink-0" }
                    span { "children 插槽生效中：可挂载回复表单或快捷操作栏。" }
                }
            }
        }
    }
}

#[component]
fn CommentCardShellPreview(detail: bool) -> Element {
    let mut mode = use_signal(|| CardShellMode::All);
    let replying_to = use_signal(|| None::<i32>);

    let render_comment = move |id: i32,
                               depth: i32,
                               muted: bool,
                               author: &'static str,
                               is_author: bool,
                               time: &'static str,
                               is_pending: bool,
                               html: &'static str| {
        let author_element = rsx! {
            span { class: "font-medium text-paper-primary", "{author}" }
        };
        let author_badge = if is_author {
            rsx! {
                span {
                    class: "px-1.5 py-0.5 rounded text-[10px] font-medium bg-[var(--color-paper-accent)]/10 text-[var(--color-paper-accent)]",
                    "作者"
                }
            }
        } else {
            rsx! {}
        };
        let timestamp = rsx! {
            span { class: "text-paper-tertiary", "{time}" }
        };
        let status_badge = if is_pending {
            rsx! {
                span {
                    class: "inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20",
                    span { class: "w-1.5 h-1.5 rounded-full bg-amber-500 animate-pulse" }
                    "审核中"
                }
            }
        } else {
            rsx! {}
        };
        if is_pending {
            rsx! {
                CommentCardShell {
                    depth,
                    muted,
                    avatar_url: String::new(),
                    author_name: author.to_string(),
                    author_element,
                    author_badge,
                    timestamp,
                    status_badge,
                    content_html: html.to_string(),
                }
            }
        } else {
            rsx! {
                CommentCardShell {
                    depth,
                    muted,
                    avatar_url: String::new(),
                    author_name: author.to_string(),
                    author_element,
                    author_badge,
                    timestamp,
                    status_badge,
                    content_html: html.to_string(),
                    content_extra_class: "md-content",

                    ShellReplyButton {
                        comment_id: id,
                        active_id: replying_to,
                        author_name: author.to_string(),
                    }
                }
            }
        }
    };

    let items = match mode() {
        CardShellMode::All => rsx! {
            {render_comment(1, 0, false, "青禾", true, "昨天 17:30", false, "<p>春天的第一片叶子，总会记得来时的风。</p>")}
            {render_comment(2, 1, false, "阿木", false, "昨天 18:02", false, "<p>我也喜欢这句话，愿新的一年枝繁叶茂。</p>")}
            {render_comment(3, 2, false, "青禾", true, "今天 09:15", false, "<p>谢谢，愿你也有好天气。</p>")}
            {render_comment(4, 2, true, "叶子访客", false, "刚刚", true, "<p>这条样例回复正在等待审核，卡片整体呈半透明淡化展示。</p>")}
        },
        CardShellMode::TopLevel => rsx! {
            {render_comment(1, 0, false, "青禾", true, "昨天 17:30", false, "<p>春天的第一片叶子，总会记得来时的风。</p>")}
        },
        CardShellMode::NestedReply => rsx! {
            {render_comment(2, 1, false, "阿木", false, "昨天 18:02", false, "<p>我也喜欢这句话，愿新的一年枝繁叶茂。</p>")}
        },
        CardShellMode::DeepIndent => rsx! {
            {render_comment(3, 2, false, "青禾", true, "今天 09:15", false, "<p>谢谢，愿你也有好天气。</p>")}
        },
        CardShellMode::Pending => rsx! {
            {render_comment(4, 0, true, "叶子访客", false, "刚刚", true, "<p>这条样例评论正在等待审核，整卡呈现 75% 不透明度与审核中徽章。</p>")}
        },
    };

    if !detail {
        return rsx! {
            div { class: "w-full min-w-0 divide-y divide-[var(--color-paper-border)]/40 px-1",
                {render_comment(1, 0, false, "青禾", true, "昨天 17:30", false, "<p>春天的第一片叶子，总会记得来时的风。</p>")}
                {render_comment(2, 1, false, "阿木", false, "昨天 18:02", false, "<p>我也喜欢这句话，愿枝繁叶茂。</p>")}
            }
        };
    }

    rsx! {
        div { class: "w-full min-w-0",
            p { class: "mb-3 text-xs text-paper-tertiary", "本地演示，切换下方的形态标签可预览卡片骨架的不同状态与缩进效果。" }
            div { class: "flex flex-wrap items-center gap-2 mb-3",
                for (mode_val, label) in [
                    (CardShellMode::All, "全部形态"),
                    (CardShellMode::TopLevel, "顶层评论 (depth=0)"),
                    (CardShellMode::NestedReply, "嵌套回复 (depth=1)"),
                    (CardShellMode::DeepIndent, "深层嵌套 (depth=2)"),
                    (CardShellMode::Pending, "待审核 (muted)"),
                ] {
                    button {
                        key: "{label}",
                        r#type: "button",
                        class: if mode() == mode_val {
                            "px-3 py-1.5 rounded-full border border-[var(--color-paper-accent)] text-xs font-medium text-[var(--color-paper-accent)] bg-[var(--color-paper-accent)]/10 transition-colors cursor-pointer"
                        } else {
                            "px-3 py-1.5 rounded-full border border-[var(--color-paper-border)] text-xs text-paper-secondary hover:text-paper-primary hover:border-[var(--color-paper-border)]/80 transition-colors cursor-pointer"
                        },
                        aria_pressed: "{mode() == mode_val}",
                        onclick: move |_| {
                            mode.set(mode_val);
                        },
                        "{label}"
                    }
                }
            }
            p { class: "text-xs text-paper-secondary mb-4 leading-relaxed",
                match mode() {
                    CardShellMode::All => "完整树形展示：包含顶层评论、一级嵌套引导线、二级阶梯缩进与待审核半透明状态。",
                    CardShellMode::TopLevel => "顶层评论（depth = 0）：无左侧边框与缩进，包含头像、作者徽章、绝对时间与回复操作。",
                    CardShellMode::NestedReply => "一级嵌套（depth = 1）：启用左侧引导线 border-l-2，区分父子回复关系。",
                    CardShellMode::DeepIndent => "深层嵌套（depth = 2）：在左侧引导线基础上叠加 margin-left 阶梯缩进。",
                    CardShellMode::Pending => "待审核状态（muted = true）：整卡 opacity-75 淡化，配合'审核中'徽章与呼吸指示灯。",
                }
            }
            div { class: "rounded-2xl border border-[var(--color-paper-border)]/70 bg-[var(--color-paper-entry)]/30 p-4 sm:p-5 divide-y divide-[var(--color-paper-border)]/40 shadow-xs",
                {items}
            }
        }
    }
}
