//! 写文章与编辑文章（`/admin/write`、`/admin/write/:id`）。
//!
//! 无干扰编辑：页头（返回、面包屑、保存状态、预览、属性、保存按钮）替代后台顶栏；
//! 衬线标题、元信息行与 Tiptap 正文居中，文章属性在右侧抽屉。保存后留在本页，
//! 新文章首次保存后换到编辑路由。未保存的修改在同一标签页留有本地副本，离开前确认。
//! 新文章正文为空时给出起步模板。编辑器通过 [`crate::bridges::tiptap`] 在 WASM 前端
//! 初始化。样式见 styles/pages/admin-write.css。

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use leptos_router::{lazy_route, LazyRoute};

mod cover;
mod editor;
mod metadata;
mod save;
mod state;

use super::note_drafts::use_draft_protection;
use crate::api::posts::get_post_by_id;
use crate::bridges::library::LibraryLoadError;
use crate::components::assets::{AssetPickerModal, AssetSelection};
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::skeletons::write_skeleton::WriteSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconButton, IconName, LoadError,
    Menu, MenuItem,
};
use crate::models::post::Post;
use crate::pages::not_found::NotFound;
use crate::routes::Route;
use crate::utils::error::failure_message;
use editor::use_write_editor;
use metadata::MetadataPanel;
use state::{PostDraft, SaveIndicator, SaveState};

/// 起步模板：（名称，说明，正文）。「空白」没有正文，只收起入口。
const STARTERS: [(&str, &str, &str); 3] = [
    (
        "技术长文",
        "带目录与可运行代码",
        "## 背景\n\n要解决什么问题，为什么值得写。\n\n## 设计\n\n关键取舍与整体结构。\n\n## 实现\n\n```python runnable\nprint(\"hello\")\n```\n\n## 小结\n\n结论与后续。\n",
    ),
    (
        "读书笔记",
        "引用块与脚注",
        "> 摘录的原文。\n\n## 读后\n\n写下想法[^1]。\n\n[^1]: 出处：书名，页码。\n",
    ),
    ("空白", "从零开始", ""),
];

/// 新建文章（`/admin/write`）。
pub struct WriteRoute;

#[lazy_route]
impl LazyRoute for WriteRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <WriteEditor post_id=None initial=PostDraft::default() published_at=None/> }
            .into_any()
    }
}

/// 编辑文章（`/admin/write/:id`）。
pub struct WriteEditRoute;

#[lazy_route]
impl LazyRoute for WriteEditRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <WriteEdit/> }.into_any()
    }
}

/// 编辑文章：先取文章，再以服务端内容挂载编辑器。
///
/// 私人草稿只在浏览器鉴权后读取，不进入 SSR；取数用 Effect + 镜像信号，
/// 世代号保证换文章或重试时只认最新一次结果。
#[component]
fn WriteEdit() -> impl IntoView {
    let params = use_params_map();
    let id = Memo::new(move |_| {
        params
            .read()
            .get("id")
            .and_then(|id| id.parse::<i32>().ok())
    });
    let reload = RwSignal::new(0_u32);
    // 带上文章 id：换文章时旧结果不能顶替新参数，直到新请求落定。
    let loaded = RwSignal::new(None::<(i32, Result<Option<Post>, ()>)>);
    let generation = StoredValue::new(0_u64);
    Effect::new(move |_| {
        reload.track();
        let Some(id) = id.get() else {
            return;
        };
        let Some(current) = generation.try_update_value(|g| {
            *g += 1;
            *g
        }) else {
            return;
        };
        loaded.set(None);
        leptos::task::spawn_local(async move {
            let result = get_post_by_id(id).await.map(|res| res.post).map_err(|_| ());
            if generation.try_get_value() == Some(current) {
                let _ = loaded.try_set(Some((id, result)));
            }
        });
    });

    move || {
        let Some(id) = id.get() else {
            return view! { <NotFound/> }.into_any();
        };
        let result = loaded
            .get()
            .and_then(|(loaded_id, result)| (loaded_id == id).then_some(result));
        match result {
            Some(Ok(Some(post))) => view! {
                <WriteEditor
                    post_id=Some(id)
                    initial=PostDraft::from(post.clone())
                    published_at=post.published_at
                />
            }
            .into_any(),
            Some(Ok(None)) => view! {
                <div class="write-missing">
                    <EmptyState
                        icon=IconName::FileX
                        title="找不到这篇文章"
                        description="它可能已被移到回收站或彻底删除。"
                        action=EmptyStateAction::new(
                            "返回文章",
                            move |_| crate::navigation::push(&Route::Posts.to_string()),
                        )
                    />
                </div>
            }
            .into_any(),
            Some(Err(())) => view! {
                <div class="write-missing">
                    <LoadError
                        title="文章加载失败"
                        on_retry=move |_| reload.update(|n| *n += 1)
                    />
                </div>
            }
            .into_any(),
            None => view! { <WriteSkeleton/> }.into_any(),
        }
    }
}

/// 编辑器页面本体，新建与编辑共用。
#[component]
fn WriteEditor(
    post_id: Option<i32>,
    initial: PostDraft,
    published_at: Option<DateTime<Utc>>,
) -> impl IntoView {
    let state = use_write_editor(post_id, initial, published_at);
    let toast = use_toast();
    let form = state.form;
    let slot = post_id.map_or_else(|| "post-new".to_string(), |id| format!("post-{id}"));
    let (recovery, cache_failed) =
        use_draft_protection(slot, form, move || state.dirty() || state.busy());
    let drawer_open = RwSignal::new(true);
    // 只有点「属性」打开时才播滑入动画；首屏、换页与编辑器挂载时直接出现。
    let drawer_animate = RwSignal::new(false);
    // 首屏与 SSR 一致，水合后窄屏默认收起属性抽屉。
    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let wide = web_sys::window()
                .and_then(|w| w.inner_width().ok())
                .and_then(|w| w.as_f64())
                .is_some_and(|w| w >= 1024.0);
            drawer_open.set(wide);
        }
    });
    let starter_dismissed = RwSignal::new(false);

    let ready = move || state.phase.with(|phase| phase.is_ready());
    let saving = move || state.save.with(SaveState::is_saving);
    let recovering = move || recovery.with(Option::is_some);
    let published = Memo::new(move |_| state.saved.with(PostDraft::is_published));
    let indicator = Memo::new(move |_| {
        state.save.with(|save| {
            SaveIndicator::of(
                save,
                state.dirty(),
                post_id.is_none(),
                state.saved_at.get(),
                recovering(),
            )
        })
    });
    let complete = Memo::new(move |_| {
        form.with(|d| !d.title.trim().is_empty() && !d.content.trim().is_empty())
    });
    let can_save = move || ready() && !recovering() && !saving() && complete.get();
    let save_hint = move || (!complete.get()).then_some("填写标题和正文后才能保存");
    let show_starters = move || {
        post_id.is_none()
            && ready()
            && !recovering()
            && !starter_dismissed.get()
            && form.with(|d| d.content.trim().is_empty())
    };
    let crumb = move || {
        form.with(|d| {
            if d.title.trim().is_empty() {
                if post_id.is_some() {
                    "未命名"
                } else {
                    "新建"
                }
                .to_string()
            } else {
                d.title.clone()
            }
        })
    };
    let tag_count = move || form.with(|d| d.parsed_tags().len());
    let published_on = move || {
        state
            .published_at
            .get()
            .filter(|_| published.get())
            .map(|at| {
                crate::utils::time::to_local(at)
                    .format("%Y.%m.%d")
                    .to_string()
            })
    };
    let saved_slug = move || state.saved.with(|s| s.slug.clone());

    let submit = move |status: &'static str| save::submit(state, toast, status);
    let retry = move |_| {
        let current = state.save.get_untracked();
        if let SaveState::Failed { status, .. } = current {
            save::submit(state, toast, status);
        }
    };
    let move_to_trash = move |_| {
        let Some(id) = post_id else {
            return;
        };
        leptos::task::spawn_local(async move {
            match crate::api::posts::delete_post(id).await {
                Ok(res) if res.success => {
                    toast.success("已移到回收站");
                    crate::navigation::push(&Route::Posts.to_string());
                }
                Ok(res) => toast.error(res.message),
                Err(e) => toast.error(failure_message("移到回收站失败", &e)),
            }
        });
    };

    let insert_picked_images = move |picks: Vec<AssetSelection>| {
        #[cfg(target_arch = "wasm32")]
        state.editor.with_value(|handle| {
            if let Some(handle) = handle {
                if let Ok(json) = serde_json::to_string(&picks) {
                    handle.instance().insert_images_from_library(&json);
                }
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        let _ = picks;
    };

    view! {
        <div
            class="write-page"
            on:keydown=move |e: leptos::ev::KeyboardEvent| {
                if (e.meta_key() || e.ctrl_key()) && e.key() == "s" {
                    e.prevent_default();
                    if can_save() {
                        submit(if published.get_untracked() { "published" } else { "draft" });
                    }
                }
            }
        >
            <header class="write-head">
                <div class="write-head-left">
                    <a
                        class=format!("{} btn-icon", button_class(ButtonVariant::Ghost, ButtonSize::Md))
                        href=Route::Posts.to_string()
                        aria-label="返回文章列表"
                    >
                        <Icon name=IconName::ArrowLeft size=16/>
                    </a>
                    <nav class="write-crumbs" aria-label="面包屑">
                        <a href=Route::Posts.to_string()>"文章"</a>
                        <span aria-hidden="true">"/"</span>
                        <span aria-current="page">{crumb}</span>
                    </nav>
                </div>
                <div class="write-head-actions">
                    {move || {
                        let indicator = indicator.get();
                        if indicator == SaveIndicator::Failed {
                            view! {
                                <button
                                    class="write-status"
                                    type="button"
                                    data-tone=indicator.tone()
                                    title=move || {
                                        state
                                            .save
                                            .with(|save| match save {
                                                SaveState::Failed { message, .. } => Some(message.clone()),
                                                _ => None,
                                            })
                                    }
                                    on:click=retry
                                >
                                    <span class="write-status-dot" aria-hidden="true"></span>
                                    <span role="status">{indicator.label()}</span>
                                </button>
                            }
                                .into_any()
                        } else {
                            view! {
                                <span
                                    class="write-status"
                                    data-tone=indicator.tone()
                                    data-pulse=(indicator == SaveIndicator::Saving).then_some("true")
                                >
                                    <span class="write-status-dot" aria-hidden="true"></span>
                                    <span role="status" aria-live="polite">
                                        {indicator.label()}
                                    </span>
                                </span>
                            }
                                .into_any()
                        }
                    }}
                    {post_id
                        .is_some()
                        .then(|| {
                            view! {
                                <a
                                    class=format!(
                                        "{} write-hide-narrow",
                                        button_class(ButtonVariant::Ghost, ButtonSize::Md),
                                    )
                                    href=move || Route::PostPreview { slug: saved_slug() }.to_string()
                                >
                                    "预览"
                                </a>
                            }
                        })}
                    <button
                        class=button_class(ButtonVariant::Ghost, ButtonSize::Md)
                        type="button"
                        aria-pressed=move || drawer_open.get().to_string()
                        aria-controls="write-drawer"
                        // 窄屏只剩图标，名称由 aria-label 补出。
                        aria-label="属性"
                        on:click=move |_| {
                            drawer_animate.set(true);
                            drawer_open.update(|open| *open = !*open);
                        }
                    >
                        <Icon name=IconName::PanelRight size=15/>
                        <span class="write-hide-narrow">"属性"</span>
                    </button>
                    {move || {
                        if published.get() {
                            view! {
                                <button
                                    class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                                    type="button"
                                    disabled=move || !can_save()
                                    title=save_hint
                                    on:click=move |_| submit("published")
                                >
                                    "更新"
                                </button>
                                <Menu
                                    label="更多操作"
                                    items=vec![MenuItem::new("unpublish", "转为草稿").icon(IconName::EyeOff)]
                                    on_select=move |_| {
                                        if can_save() {
                                            submit("draft");
                                        }
                                    }
                                />
                            }
                                .into_any()
                        } else {
                            view! {
                                <button
                                    class=format!(
                                        "{} write-hide-narrow",
                                        button_class(ButtonVariant::Ghost, ButtonSize::Md),
                                    )
                                    type="button"
                                    disabled=move || !can_save()
                                    title=save_hint
                                    on:click=move |_| submit("draft")
                                >
                                    "存草稿"
                                </button>
                                <button
                                    class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                                    type="button"
                                    disabled=move || !can_save()
                                    title=save_hint
                                    on:click=move |_| submit("published")
                                >
                                    "发布"
                                </button>
                            }
                                .into_any()
                        }
                    }}
                </div>
            </header>

            {move || {
                recovery
                    .get()
                    .map(|local| {
                        let restore = local.clone();
                        view! {
                            <div class="write-banner" role="alert">
                                <Icon name=IconName::RotateCcwClock size=18/>
                                <div>
                                    <p class="write-banner-title">"这个标签页留有未保存的内容"</p>
                                    <p>
                                        "本地副本与服务端版本不同。恢复会替换编辑器里的内容，保存后才生效；选择之前编辑器暂停输入。"
                                    </p>
                                    <div class="write-banner-actions">
                                        <button
                                            class=button_class(ButtonVariant::Primary, ButtonSize::Sm)
                                            type="button"
                                            disabled=move || !ready()
                                            on:click=move |_| {
                                                form.set(restore.clone());
                                                state.set_content(&restore.content);
                                                recovery.set(None);
                                            }
                                        >
                                            "恢复本地副本"
                                        </button>
                                        <button
                                            class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                            type="button"
                                            on:click=move |_| recovery.set(None)
                                        >
                                            "使用服务端版本"
                                        </button>
                                    </div>
                                </div>
                            </div>
                        }
                    })
            }}

            <div class="write-body">
                <main class="write-main" data-vt-scroll="admin-write-content">
                    <div class="write-doc">
                        <LibraryLoadError library=state.library/>
                        {move || {
                            state
                                .phase
                                .with(|phase| phase.error().map(str::to_owned))
                                .map(|message| {
                                    view! {
                                        <p class="write-notice" role="alert">
                                            <span>{message}</span>
                                            <button
                                                class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                                type="button"
                                                on:click=move |_| state.retry()
                                            >
                                                "重试"
                                            </button>
                                        </p>
                                    }
                                })
                        }}
                        {move || {
                            cache_failed
                                .get()
                                .then(|| {
                                    view! {
                                        <p class="write-notice" role="alert">
                                            "浏览器无法保存本地恢复副本，请先保存再离开。"
                                        </p>
                                    }
                                })
                        }}
                        // 同 id 的错误消息会原地更新，key 带上消息让它随之重绘。
                        <For
                            each=move || state.upload_errors.get()
                            key=|err| (err.id.clone(), err.message.clone())
                            let:err
                        >
                            <p class="write-notice" role="alert">
                                <span>
                                    {format!("图片上传失败：{} — {}", err.file_name, err.message)}
                                </span>
                                <IconButton
                                    icon=IconName::X
                                    label="关闭提示"
                                    size=ButtonSize::Sm
                                    onclick={
                                        let id = err.id.clone();
                                        move |_| {
                                            #[cfg(target_arch = "wasm32")]
                                            state.editor.with_value(|handle| {
                                                if let Some(handle) = handle {
                                                    handle.instance().remove_upload_by_upload_id(&id);
                                                }
                                            });
                                            state.upload_errors.update(|errors| errors.retain(|e| e.id != id));
                                        }
                                    }
                                />
                            </p>
                        </For>

                        <label class="sr-only" for="write-title">
                            "标题"
                        </label>
                        // 单段文本框随内容换行；回车不换行，直接进入正文。
                        <textarea
                            id="write-title"
                            class="write-title"
                            rows="1"
                            placeholder="文章标题"
                            readonly=recovering
                            prop:value=move || form.with(|d| d.title.clone())
                            on:input=move |e| {
                                let title = event_target_value(&e).replace(['\n', '\r'], "");
                                form.update(|d| d.title = title);
                            }
                            on:keydown=move |e: leptos::ev::KeyboardEvent| {
                                if e.key() == "Enter" && !e.is_composing() {
                                    e.prevent_default();
                                    focus_editor();
                                }
                            }
                        >
                            {form.with_untracked(|d| d.title.clone())}
                        </textarea>
                        <p class="write-meta">
                            {move || {
                                let slug = saved_slug();
                                if slug.is_empty() {
                                    view! { <span>"保存后生成链接"</span> }.into_any()
                                } else {
                                    view! { <span class="write-meta-slug">{format!("/post/{slug}")}</span> }
                                        .into_any()
                                }
                            }}
                            {move || {
                                let count = tag_count();
                                (count > 0).then(|| view! { <span>{format!("{count} 个标签")}</span> })
                            }}
                            {move || match published_on() {
                                Some(date) => view! { <span>{format!("已发布于 {date}")}</span> }.into_any(),
                                None => view! { <span>"草稿 · 未发布"</span> }.into_any(),
                            }}
                        </p>
                        <div class="write-rule" aria-hidden="true"></div>

                        // inert 只在等待选择恢复时出现：属性存在即生效，不能写成 false。
                        <div
                            class="write-editor-wrap"
                            inert=move || recovering().then_some("true")
                            data-empty=move || show_starters().then_some("true")
                        >
                            <div id="tiptap-editor" class="write-editor"></div>
                        </div>

                        {move || {
                            show_starters()
                                .then(|| {
                                    view! {
                                        <div class="write-starters" role="group" aria-label="起步模板">
                                            {STARTERS
                                                .iter()
                                                .map(|&(name, hint, body)| {
                                                    view! {
                                                        <button
                                                            class="write-starter"
                                                            type="button"
                                                            on:click=move |_| {
                                                                starter_dismissed.set(true);
                                                                if !body.is_empty() {
                                                                    state.set_content(body);
                                                                }
                                                                focus_editor();
                                                            }
                                                        >
                                                            <span class="write-starter-name">{name}</span>
                                                            <span class="write-starter-hint">{hint}</span>
                                                        </button>
                                                    }
                                                })
                                                .collect_view()}
                                        </div>
                                        <p class="write-starter-note">
                                            "标题和正文都填写后才能保存；摘要、标签和封面可以稍后在「属性」里补。"
                                        </p>
                                    }
                                })
                        }}
                    </div>
                </main>
                {move || {
                    drawer_open
                        .get()
                        .then(|| {
                            view! {
                                <div
                                    id="write-drawer"
                                    class="write-drawer-slot"
                                    data-enter=move || drawer_animate.get().then_some("true")
                                >
                                    <MetadataPanel state open=drawer_open on_trash=move_to_trash/>
                                </div>
                            }
                        })
                }}
            </div>

            {move || {
                (state.phase.with(|phase| phase.is_loading())
                    && state.library.read_state().is_none_or(|r| r.is_ok()))
                    .then(|| view! { <div class="write-loading"><WriteSkeleton/></div> })
            }}

            // 正文插图：素材库多选。
            <AssetPickerModal
                visible=state.body_picker_visible
                cover_uploading=state.body_picker_uploading
                title="选择图片"
                multi=true
                on_select=insert_picked_images
            />
        </div>
    }
}

/// 把焦点放进正文编辑区。
fn focus_editor() {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| {
                d.query_selector("#tiptap-editor [contenteditable]")
                    .ok()
                    .flatten()
            })
            .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = el.focus();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::STARTERS;

    #[test]
    fn starters_offer_long_form_reading_notes_and_blank() {
        let names: Vec<_> = STARTERS.iter().map(|s| s.0).collect();
        assert_eq!(names, ["技术长文", "读书笔记", "空白"]);
        assert!(STARTERS[0].2.contains("runnable"));
        assert!(STARTERS[1].2.contains("[^1]:"));
        assert!(STARTERS[2].2.is_empty());
    }
}
