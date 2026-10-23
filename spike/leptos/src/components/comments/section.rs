//! 评论区段组件
//!
//! 管理单篇文章的评论上下文（回复目标、刷新触发器、待审核评论），
//! 负责加载评论列表、轮询待审核评论状态并渲染表单与列表。

use std::future::Future;
use std::pin::Pin;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::comments::{
    check_pending_status, create_comment, get_comments, CommentTreeResponse,
};
use crate::components::comments::form::CommentForm;
use crate::components::comments::list::CommentList;
use crate::components::post::enhance_assets::LightboxAssets;
use crate::components::skeletons::comment_skeleton::CommentListSkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::error::AppError;
use crate::models::comment::PublicComment;
use crate::models::user::PublicUser;
use crate::utils::comment_storage::{self, AuthorInfo, PendingComment};
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
    pub active_reply: RwSignal<Option<i64>>,
    /// 刷新触发信号，切换时触发评论列表重新加载。
    pub refresh_trigger: RwSignal<bool>,
    /// 本地存储的待审核评论。
    pub pending_comments: RwSignal<Vec<PendingComment>>,
    /// 当前登录用户；`None` 表示匿名（或尚未完成探测）。
    pub current_user: RwSignal<Option<PublicUser>>,
    /// 提交评论；生产走服务端并写本地待审核，图鉴只写内存。返回给读者的提示：Ok 成功、Err 失败。
    pub submit: Callback<CommentSubmission, CommentSubmitFuture>,
    /// 是否允许评论插图上传；关闭时隐藏上传按钮，编辑器不接管粘贴/拖放上传。
    pub image_upload: bool,
    /// DOM ID 前缀，区分同页多个评论区的表单。
    pub id_scope: &'static str,
}

/// 表单交给 `CommentContext::submit` 的评论内容。
pub struct CommentSubmission {
    pub post_id: i32,
    pub parent_id: Option<i64>,
    /// 匿名访客；登录用户的身份由服务端从会话推导。
    pub anonymous: bool,
    pub author_name: String,
    pub author_email: String,
    pub author_url: String,
    pub content_md: String,
    pub honeypot: String,
}

/// 评论提交结果：Ok 为成功提示，Err 为失败提示。
pub type CommentSubmitFuture = Pin<Box<dyn Future<Output = Result<String, String>>>>;

/// 提交到服务端；匿名评论成功后保存作者信息并写入本地待审核列表。
fn submit_to_server(
    sub: CommentSubmission,
    pending_comments: RwSignal<Vec<PendingComment>>,
) -> CommentSubmitFuture {
    Box::pin(async move {
        let author_url = (!sub.author_url.trim().is_empty()).then(|| sub.author_url.clone());
        let resp = create_comment(
            sub.post_id,
            sub.parent_id,
            sub.author_name.clone(),
            sub.author_email.clone(),
            author_url.clone(),
            sub.content_md.clone(),
            sub.honeypot,
        )
        .await
        .map_err(|_| "提交失败，请稍后重试".to_string())?;
        if !resp.success {
            return Err(resp.message);
        }
        // 登录评论直发 approved：不写本地待审核存储，列表刷新后即按正式状态展示。
        if sub.anonymous {
            comment_storage::save_author(&AuthorInfo {
                name: sub.author_name.clone(),
                email: sub.author_email,
                url: sub.author_url,
            });
            if let Some(id) = resp.comment_id {
                let now = chrono::Utc::now().to_rfc3339();
                let pending = PendingComment {
                    id,
                    parent_id: sub.parent_id,
                    depth: resp.depth.unwrap_or(0),
                    author_name: sub.author_name,
                    author_url,
                    avatar_url: resp.avatar_url.unwrap_or_default(),
                    content_md: sub.content_md,
                    created_at: now.clone(),
                    stored_at: now,
                };
                comment_storage::save_pending_comment(sub.post_id, pending.clone());
                pending_comments.update(|list| list.push(pending));
            }
        }
        Ok(resp.message)
    })
}

/// 轮询待审核评论状态：只要本地还有待审核评论，就定期查询其审核状态。
///
/// 一旦某条评论变为非 pending（通常已通过），就从本地移除并刷新已审核列表，
/// 使其以正式状态进入评论树。无待审核评论时退出并清除 `polling` 标记，
/// 不给访客留常驻定时器；组件卸载（信号已释放）时同样退出。
async fn poll_pending(
    post_id: i32,
    pending_comments: RwSignal<Vec<PendingComment>>,
    refresh_trigger: RwSignal<bool>,
    polling: RwSignal<bool>,
) {
    loop {
        let Some(ids) = pending_comments
            .try_with_untracked(|list| list.iter().map(|c| c.id).collect::<Vec<i64>>())
        else {
            // 组件已卸载。
            return;
        };
        if ids.is_empty() {
            // 无待审核评论：停止轮询。pending_comments 再变化时由 Effect 重新启动。
            let _ = polling.try_set(false);
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
                // 评论状态已变化（多为已通过）：刷新已审核列表。
                let _ = refresh_trigger.try_update(|v| *v = !*v);
                let _ =
                    pending_comments.try_update(|list| list.retain(|c| !to_remove.contains(&c.id)));
            }
        }
        // Err（如限流）静默忽略，统一在下方 sleep 后下一轮重试。

        sleep_ms(PENDING_POLL_INTERVAL_MS).await;
    }
}

/// 评论区段组件。
///
/// Props：
/// - `post_id`：所属文章 ID
///
/// 负责：
/// - 提供 `CommentContext` 上下文
/// - 加载本地待审核评论并定期轮询其审核状态
/// - 加载已审核评论列表并合并展示（与原版 `use_resource` 一致：仅客户端加载，
///   SSR 输出骨架屏）
/// - 空评论时展示提示文案
#[component]
pub fn CommentSection(post_id: i32) -> impl IntoView {
    let pending_comments = RwSignal::new(Vec::<PendingComment>::new());
    let ctx = CommentContext {
        active_reply: RwSignal::new(None),
        refresh_trigger: RwSignal::new(false),
        pending_comments,
        current_user: RwSignal::new(None),
        submit: Callback::new(move |sub: CommentSubmission| {
            submit_to_server(sub, pending_comments)
        }),
        image_upload: true,
        id_scope: "post",
    };
    provide_context(ctx);

    // 挂载后从本地存储加载待审核评论以防 SSR Hydration Mismatch
    Effect::new(move |_| {
        let pending = comment_storage::load_pending_comments(post_id);
        comment_storage::prune_all_expired();
        ctx.pending_comments.set(pending);
    });

    // 探测登录态：登录用户的评论表单切换为身份行变体（免作者信息字段）。
    // 刻意使用评论区自己的信号而非全局用户上下文，避免污染后台守卫语义。
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(resp) = crate::api::auth::get_current_user().await {
                if let Some(u) = resp.user {
                    let _ = ctx.current_user.try_set(Some(u));
                }
            }
        });
    });

    // 待审核评论轮询：pending_comments 由空变非空（提交 / 载入）时启动单个轮询任务；
    // `polling` 标记保证同一时刻只有一个轮询循环在跑。
    let polling = RwSignal::new(false);
    Effect::new(move |_| {
        let has_pending = ctx.pending_comments.with(|list| !list.is_empty());
        if has_pending && !polling.get_untracked() {
            polling.set(true);
            spawn_local(poll_pending(
                post_id,
                ctx.pending_comments,
                ctx.refresh_trigger,
                polling,
            ));
        }
    });

    // 评论数据，refresh_trigger 变化时重新加载（仅客户端，同原版 use_resource）。
    // 不用 LocalResource：Resource 读取会登记到外层 <Suspense>，每次刷新都让 Suspense
    // 退回 fallback、把整段评论区（含正在输入的编辑器）移出再插回 DOM。这里由 Effect
    // 直接写镜像信号；刷新期间保留旧数据，与原版 use_resource 重启时的表现一致。
    let comments_resource: RwSignal<Option<Result<CommentTreeResponse, AppError>>> =
        RwSignal::new(None);
    let fetch_generation = StoredValue::new(0u64);
    Effect::new(move |_| {
        ctx.refresh_trigger.track();
        let Some(current) = fetch_generation.try_update_value(|g| {
            *g += 1;
            *g
        }) else {
            return;
        };
        spawn_local(async move {
            let value = get_comments(post_id).await;
            if fetch_generation.try_get_value() == Some(current) {
                let _ = comments_resource.try_set(Some(value));
            }
        });
    });

    // 本地去重兜底：已审核评论列表加载后，凡 id 已出现在已审核集合中的 pending
    // 占位项立即移除。这是独立于上方轮询的确定性清理——不依赖 check_pending_status
    // 远程调用成功，只要 get_comments 返回了已通过的评论，对应占位项就会被清除。
    Effect::new(move |_| {
        let approved_ids: Option<std::collections::HashSet<i64>> =
            comments_resource.with(|data| match data {
                Some(Ok(resp)) => Some(resp.comments.iter().map(|c| c.id).collect()),
                _ => None,
            });
        let Some(approved_ids) = approved_ids else {
            return;
        };
        let to_remove: Vec<i64> = ctx.pending_comments.with(|list| {
            list.iter()
                .filter(|p| approved_ids.contains(&p.id))
                .map(|p| p.id)
                .collect()
        });
        if !to_remove.is_empty() {
            comment_storage::remove_pending_ids(post_id, &to_remove);
            ctx.pending_comments
                .update(|list| list.retain(|p| !to_remove.contains(&p.id)));
        }
    });

    // 灯箱绑定：评论图片（审核后渲染的 <img>）点击放大，与正文图一致。
    // 评论列表随加载/刷新重建节点后重跑此 effect 重新绑定，TS 端 data-lb-bound
    // 守卫保证重复绑定幂等。下一帧执行，确保列表 DOM 已提交。
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        let loaded = comments_resource.with(|data| matches!(data, Some(Ok(_))));
        if !loaded {
            return;
        }
        request_animation_frame(|| {
            let Some(window) = web_sys::window() else {
                return;
            };
            let sel: wasm_bindgen::JsValue = ".comment-list".into();
            // 合并而非覆盖 __lightboxSelectors：正文已把 .post-content / .entry-cover
            // 写入同一全局；覆盖会让 lightbox.js 晚加载时的自启动丢掉正文图绑定。
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
            let selectors_val: wasm_bindgen::JsValue = arr.into();
            let _ = js_sys::Reflect::set(&window, &"__lightboxSelectors".into(), &selectors_val);
            // 显式绑定评论区（重复调用由 TS 端守卫幂等）；脚本未加载时入队/no-op。
            let call_arg = js_sys::Array::of1(&sel);
            crate::utils::js::invoke_optional_global(&window, "__initLightbox", &[call_arg.into()]);
        });
    });

    let comments = Memo::new(move |_| {
        comments_resource.with(|data| match data {
            Some(Ok(CommentTreeResponse { comments, .. })) => Some(comments.clone()),
            _ => None,
        })
    });
    let approved_count = Memo::new(move |_| {
        comments_resource.with(|data| match data {
            Some(Ok(CommentTreeResponse { count, .. })) => Some(*count),
            _ => None,
        })
    });
    let failed = Memo::new(move |_| comments_resource.with(|data| matches!(data, Some(Err(_)))));

    view! {
        // 评论区图片灯箱按需注入（脚本未就绪时迟到调用入队兜底）。
        <LightboxAssets/>
        <CommentSectionContent
            post_id=post_id
            comments=comments
            approved_count=approved_count
            failed=failed
        />
    }
}

/// 列表区的展示状态（只在状态切换时重建对应分支）。
#[derive(Clone, Copy, PartialEq)]
enum ListState {
    /// 加载中：骨架屏。
    Loading,
    /// 加载失败：错误提示。
    Failed,
    /// 已加载但无任何评论。
    Empty,
    /// 已加载且有评论（含本地待审核）。
    List,
}

/// 正式文章和图鉴共同使用的评论区展示；数据获取与持久化由上层负责。
#[component]
pub(crate) fn CommentSectionContent(
    post_id: i32,
    #[prop(into)] comments: Signal<Option<Vec<PublicComment>>>,
    #[prop(into)] approved_count: Signal<Option<i64>>,
    #[prop(into)] failed: Signal<bool>,
) -> impl IntoView {
    let ctx = expect_context::<CommentContext>();
    let pending = ctx.pending_comments;
    let total_count = move || {
        approved_count
            .get()
            .map(|count| count + pending.with(|list| list.len()) as i64)
    };

    let list_state =
        Memo::new(
            move |_| match comments.with(|list| list.as_ref().map(|list| list.is_empty())) {
                Some(true) if pending.with(|list| list.is_empty()) => ListState::Empty,
                Some(_) => ListState::List,
                None if failed.get() => ListState::Failed,
                None => ListState::Loading,
            },
        );
    let approved = Signal::derive(move || comments.get().unwrap_or_default());

    view! {
        <section class="comment-section" aria-labelledby="comments-title">
            <div class="comment-section-head">
                <h2 id="comments-title">
                    "评论"
                    {move || {
                        total_count()
                            .map(|count| {
                                view! { <span class="comment-count tabular-nums">{count}</span> }
                            })
                    }}
                </h2>
                <span class="comment-order">"按时间排序"</span>
            </div>

            // 真实的评论输入表单始终立即可见且可交互，避免 CLS
            <CommentForm post_id=post_id parent_id=None parent_indent=None/>

            // 根据数据状态渲染列表区、错误提示或骨架屏
            {move || match list_state.get() {
                ListState::Empty => {
                    view! { <p class="comment-empty">"还没有评论，来写下第一条吧。"</p> }.into_any()
                }
                ListState::List => {
                    view! { <CommentList comments=approved pending=pending post_id=post_id/> }
                        .into_any()
                }
                ListState::Failed => {
                    view! {
                        <p class="comment-empty is-error" role="alert">
                            "评论暂时没能加载，刷新页面再试一次。"
                        </p>
                    }
                        .into_any()
                }
                ListState::Loading => {
                    view! {
                        <DelayedSkeleton>
                            <CommentListSkeleton/>
                        </DelayedSkeleton>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}
