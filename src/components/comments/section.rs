//! 评论区段组件
//!
//! 管理单篇文章的评论上下文（回复目标、刷新触发器、待审核评论），
//! 负责加载评论列表、轮询待审核评论状态并渲染表单与列表。

use dioxus::prelude::*;

use crate::api::comments::{check_pending_status, get_comments, CommentTreeResponse};
use crate::components::comments::form::CommentForm;
use crate::components::comments::list::CommentList;
use crate::components::skeletons::comment_skeleton::CommentListSkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::utils::comment_storage::{self, PendingComment};
use crate::utils::time::sleep_ms;

/// 待审核评论状态的轮询间隔（毫秒）。
///
/// 仅在本地存在待审核评论时才轮询；30s 在「审核通过后尽快反映」与「不触发 strict
/// 限流（默认 1 req/s, burst 5）」之间取平衡。
const PENDING_POLL_INTERVAL_MS: u32 = 30_000;

/// 评论上下文，供评论相关组件共享状态。
///
/// 字段：
/// - `active_reply`：当前正在回复的评论 ID
/// - `refresh_trigger`：刷新触发信号，切换时触发评论列表重新加载
/// - `pending_comments`：本地存储的待审核评论
/// - `current_user`：当前登录用户（`None` 为匿名访客）；登录后评论表单
///   显示身份行、免作者信息字段，评论直发免审核
#[derive(Clone, Copy)]
pub struct CommentContext {
    /// 当前正在回复的评论 ID。
    pub active_reply: Signal<Option<i64>>,
    /// 刷新触发信号，切换时触发评论列表重新加载。
    pub refresh_trigger: Signal<bool>,
    /// 本地存储的待审核评论。
    pub pending_comments: Signal<Vec<PendingComment>>,
    /// 当前登录用户；`None` 表示匿名（或尚未完成探测）。
    pub current_user: Signal<Option<crate::models::user::PublicUser>>,
}

/// 评论区段组件。
///
/// Props：
/// - `post_id`：所属文章 ID
///
/// 负责：
/// - 提供 `CommentContext` 上下文
/// - 加载本地待审核评论并定期轮询其审核状态
/// - 加载已审核评论列表并合并展示
/// - 空评论时展示提示文案
#[component]
pub fn CommentSection(post_id: i32) -> Element {
    let mut ctx = use_context_provider(|| CommentContext {
        active_reply: Signal::new(None),
        refresh_trigger: Signal::new(false),
        pending_comments: Signal::new(Vec::new()),
        current_user: Signal::new(None),
    });

    // 挂载后从本地存储异步加载待审核评论以防 SSR Hydration Mismatch
    use_effect(move || {
        let pending = comment_storage::load_pending_comments(post_id);
        comment_storage::prune_all_expired();
        ctx.pending_comments.set(pending);
    });

    // 探测登录态：登录用户的评论表单切换为身份行变体（免作者信息字段）。
    // 刻意使用评论区自己的信号而非全局 UserContext——AdminLayout 的守卫以
    // 「checked=true 且 user=None」表示已确认未登录并跳转登录页，在前台探测
    // 会污染该语义（匿名访客此后进入 /admin 将永远卡在骨架屏）。
    use_effect(move || {
        spawn(async move {
            if let Ok(resp) = crate::api::auth::get_current_user().await {
                if let Some(u) = resp.user {
                    ctx.current_user.set(Some(u));
                }
            }
        });
    });

    // 轮询待审核评论状态：只要本地还有待审核评论，就定期查询其审核状态。
    //
    // 必须用 use_resource 而非 use_future：use_future 不跟踪响应式依赖——闭包仅运行
    // 一次，async 结束后即便依赖信号变化也不会重启（Dioxus 0.7.10 use_future 源码
    // 证实其无 ReactiveContext）。上一版修复（8268546）误以为在同步段读取
    // pending_comments 能让 use_future 自动重启，实际并不能：页面刷新时 use_effect
    // 异步载入 localStorage 的 pending，而此时已 return 退出的 future 永不重启，
    // 轮询彻底失效，「审核中」徽章永久残留（issue #9 回归）。use_resource 内置
    // ReactiveContext，pending_comments 变化（提交 / 载入 / 本轮移除）时自动取消旧
    // 任务并重启；无待审核评论时 return 退出，不给访客留常驻定时器。一旦某条评论
    // 变为非 pending（通常已通过），就从本地移除并刷新已审核列表，使其以正式状态
    // 进入评论树。
    let _pending_poll = use_resource(move || {
        let mut pending_comments = ctx.pending_comments;
        let mut refresh_trigger = ctx.refresh_trigger;
        async move {
            loop {
                let ids: Vec<i64> = pending_comments.read().iter().map(|c| c.id).collect();
                if ids.is_empty() {
                    // 无待审核评论：停止轮询。pending_comments 再变化时 use_resource 自动重启。
                    return;
                }

                if let Ok(statuses) = check_pending_status(ids).await {
                    let to_remove: Vec<i64> = statuses
                        .into_iter()
                        .filter(|s| s.status != "pending")
                        .map(|s| s.id)
                        .collect();
                    if !to_remove.is_empty() {
                        comment_storage::remove_pending_ids(post_id, &to_remove);
                        // 评论状态已变化（多为已通过）：刷新已审核列表。peek 不订阅信号，
                        // 避免给本 resource 引入额外依赖；先取值再 set，规避借用冲突。
                        let next = !*refresh_trigger.peek();
                        refresh_trigger.set(next);
                        pending_comments
                            .write()
                            .retain(|c| !to_remove.contains(&c.id));
                    }
                }
                // Err（如限流）静默忽略，统一在下方 sleep 后下一轮重试。

                sleep_ms(PENDING_POLL_INTERVAL_MS).await;
            }
        }
    });

    // 评论数据资源，refresh_trigger 变化时自动重新加载
    let comments_resource = use_resource(move || {
        let _ = (ctx.refresh_trigger)();
        async move { get_comments(post_id).await }
    });

    // 本地去重兜底：已审核评论列表加载后，凡 id 已出现在已审核集合中的 pending
    // 占位项立即移除。这是独立于上方轮询的确定性清理——不依赖 check_pending_status
    // 远程调用成功（限流 / 网络失败时轮询无法移除占位项），只要 get_comments 返回了
    // 已通过的评论，对应占位项就会被清除，根治「审核中」徽章残留（issue #9）。
    use_effect(move || {
        let data = comments_resource.read();
        if let Some(Ok(CommentTreeResponse { comments, .. })) = &*data {
            let approved_ids: std::collections::HashSet<i64> =
                comments.iter().map(|c| c.id).collect();
            let to_remove: Vec<i64> = ctx
                .pending_comments
                .read()
                .iter()
                .filter(|p| approved_ids.contains(&p.id))
                .map(|p| p.id)
                .collect();
            if !to_remove.is_empty() {
                comment_storage::remove_pending_ids(post_id, &to_remove);
                ctx.pending_comments
                    .write()
                    .retain(|p| !to_remove.contains(&p.id));
            }
        }
    });

    // 灯箱绑定：评论图片（审核后渲染的 <img>）点击放大，与正文图一致。
    // lightbox.js 由 Dioxus.toml 全局注入；评论列表随加载/刷新重建节点后重跑此
    // effect 重新绑定，TS 端 data-lb-bound 守卫保证重复绑定幂等（同 assets.rs 模式）。
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        // 订阅 comments_resource：数据落地（DOM 提交后）重绑。
        let data = comments_resource.read();
        if !matches!(&*data, Some(Ok(_))) {
            return;
        }
        let window = web_sys::window()
            .expect("CommentSection use_effect 仅在 WASM 浏览器上下文执行：无 window");
        let sel: wasm_bindgen::JsValue = ".comment-list".into();
        // 合并而非覆盖 __lightboxSelectors：PostContent 已把 .post-content /
        // .entry-cover 写入同一全局；覆盖会让 lightbox.js 晚加载时的 IIFE 自启动
        // 丢掉正文图绑定。缺数组成员时按新数组处理。
        let existing =
            js_sys::Reflect::get(&window, &"__lightboxSelectors".into()).unwrap_or_default();
        let arr = if existing.is_array() {
            js_sys::Array::from(&existing)
        } else {
            js_sys::Array::new()
        };
        if !arr.includes(&sel, 0) {
            arr.push(&sel);
        }
        let selectors_val = js_sys::Object::from(arr).into();
        let _ = js_sys::Reflect::set(&window, &"__lightboxSelectors".into(), &selectors_val);
        // 显式绑定评论区（重复调用由 TS 端守卫幂等）；脚本未加载时 no-op，
        // 由自启动读取上方合并后的配置兜底。
        let call_arg = js_sys::Array::of1(&sel);
        crate::utils::js::invoke_optional_global(&window, "__initLightbox", &[call_arg.into()]);
    });

    let data = comments_resource.read();

    // 动态计算总评论数（已审核 + 本地待审核）
    let total_count = if let Some(Ok(CommentTreeResponse { count, .. })) = &*data {
        let approved_count = *count;
        let pending_count = ctx.pending_comments.read().len() as i64;
        Some(approved_count + pending_count)
    } else {
        None
    };

    rsx! {
        div { class: "space-y-6",
            // 标题栏：精致图标 + 评论区 + 数量徽章
            div { class: "flex items-center justify-between",
                div { class: "flex items-center gap-2.5",
                    svg {
                        class: "w-5 h-5 text-[var(--color-paper-accent)]",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z",
                        }
                    }
                    h2 { class: "text-lg font-bold text-paper-primary tracking-tight", "评论区" }
                    if let Some(count) = total_count {
                        span { class: "px-2 py-0.5 rounded-full text-xs font-semibold bg-[var(--color-paper-accent)]/15 text-[var(--color-paper-accent)]",
                            "{count}"
                        }
                    }
                }
            }

            // 真实的评论输入表单始终立即可见且可交互，避免 CLS
            CommentForm { post_id, parent_id: None, parent_indent: None }

            // 根据数据状态渲染列表区、错误提示或骨架屏
            match &*data {
                Some(Ok(CommentTreeResponse { comments, .. })) => {
                    let approved_count = comments.len();
                    let pending_count = ctx.pending_comments.read().len();
                    let has_any = approved_count > 0 || pending_count > 0;
                    if !has_any {
                        rsx! {
                            div { class: "text-center py-10 px-4 rounded-2xl bg-[var(--color-paper-entry)]/40 border border-dashed border-[var(--color-paper-border)]/60 my-4",
                                p { class: "text-sm text-paper-secondary font-medium", "暂无评论" }
                                p { class: "text-xs text-paper-tertiary mt-1", "成为第一个分享想法的人吧！" }
                            }
                        }
                    } else {
                        rsx! {
                            CommentList {
                                comments: comments.clone(),
                                pending: ctx.pending_comments.read().clone(),
                                post_id,
                            }
                        }
                    }
                }
                Some(Err(_)) => rsx! {
                    div { class: "text-center text-red-500 dark:text-red-400 py-8 text-sm", "评论加载失败，请刷新重试" }
                },
                None => rsx! {
                    DelayedSkeleton { CommentListSkeleton {} }
                },
            }
        }
    }
}
