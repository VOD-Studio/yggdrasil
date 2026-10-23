//! 写文章与编辑文章（`/admin/write`、`/admin/write/:id`）。
//!
//! 无干扰编辑：页头（返回、面包屑、保存状态、预览、属性、保存按钮）替代后台顶栏；
//! 衬线标题、元信息行与 Tiptap 正文居中，文章属性在右侧抽屉。保存后留在本页，
//! 新文章首次保存后换到编辑路由。未保存的修改在同一标签页留有本地副本，离开前确认。
//! 新文章正文为空时给出起步模板。编辑器通过 [`crate::bridges::tiptap`] 在 WASM 前端
//! 初始化。样式见 styles/pages/admin-write.css。

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use dioxus::router::components::Link;

mod cover;
mod editor;
mod metadata;
mod save;
mod state;

use super::note_drafts::use_draft_protection;
use super::notes::client_request;
use crate::api::posts::get_post_by_id;
use crate::bridges::library::LibraryLoadError;
use crate::components::assets::{AssetPickerModal, AssetSelection};
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::skeletons::write_skeleton::WriteSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconButton, IconName, LoadError,
    Menu, MenuItem,
};
use crate::router::Route;
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

/// 新建文章。
#[component]
pub fn Write() -> Element {
    rsx! {
        WriteEditor { post_id: None, initial: PostDraft::default(), published_at: None }
    }
}

/// 编辑文章：先取文章，再以服务端内容挂载编辑器。
#[component]
pub fn WriteEdit(id: i32) -> Element {
    let navigator = use_navigator();
    let mut reload = use_signal(|| 0_u32);
    let post = use_resource(move || {
        let _ = reload();
        async move { client_request(get_post_by_id(id)).await.map(|res| res.post) }
    });
    let loaded = post.read();
    match loaded.as_ref() {
        Some(Ok(Some(post))) => rsx! {
            WriteEditor {
                key: "{id}",
                post_id: Some(id),
                initial: PostDraft::from(post.clone()),
                published_at: post.published_at,
            }
        },
        Some(Ok(None)) => rsx! {
            div { class: "write-missing",
                EmptyState {
                    icon: IconName::FileX,
                    title: "找不到这篇文章",
                    description: "它可能已被移到回收站或彻底删除。",
                    action: EmptyStateAction {
                        label: "返回文章".to_string(),
                        onclick: Callback::new(move |_| {
                            navigator.push(Route::Posts {});
                        }),
                    },
                }
            }
        },
        Some(Err(_)) => rsx! {
            div { class: "write-missing",
                LoadError { title: "文章加载失败", on_retry: move |_| reload += 1 }
            }
        },
        None => rsx! {
            WriteSkeleton {}
        },
    }
}

/// 编辑器页面本体，新建与编辑共用。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn WriteEditor(
    post_id: Option<i32>,
    initial: PostDraft,
    published_at: Option<DateTime<Utc>>,
) -> Element {
    let state = use_write_editor(post_id, initial, published_at);
    let toast = use_toast();
    let navigator = use_navigator();
    let mut form = state.form;
    let slot = post_id.map_or_else(|| "post-new".to_string(), |id| format!("post-{id}"));
    let (mut recovery, cache_failed) =
        use_draft_protection(slot, form, move || state.dirty() || state.busy());
    let mut drawer_open = use_signal(|| true);
    // 只有点「属性」打开时才播滑入动画；首屏、换页与编辑器挂载时直接出现。
    let mut drawer_animate = use_signal(|| false);
    // 首屏与 SSR 一致，水合后窄屏默认收起属性抽屉。
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let wide = web_sys::window()
            .and_then(|w| w.inner_width().ok())
            .and_then(|w| w.as_f64())
            .is_some_and(|w| w >= 1024.0);
        drawer_open.set(wide);
    });
    let mut starter_dismissed = use_signal(|| false);

    let phase = state.phase.read().clone();
    let ready = phase.is_ready();
    let save_state = state.save.read().clone();
    let saving = save_state.is_saving();
    let recovering = recovery().is_some();
    let saved = state.saved.read().clone();
    let published = saved.is_published();
    let draft = form.read().clone();
    let indicator = SaveIndicator::of(
        &save_state,
        state.dirty(),
        post_id.is_none(),
        (state.saved_at)(),
        recovering,
    );
    let complete = !draft.title.trim().is_empty() && !draft.content.trim().is_empty();
    let can_save = ready && !recovering && !saving && complete;
    let save_hint = (!complete).then_some("填写标题和正文后才能保存");
    let show_starters = post_id.is_none()
        && ready
        && !recovering
        && !starter_dismissed()
        && draft.content.trim().is_empty();
    let crumb = if draft.title.trim().is_empty() {
        if post_id.is_some() {
            "未命名"
        } else {
            "新建"
        }
        .to_string()
    } else {
        draft.title.clone()
    };
    let tag_count = draft.parsed_tags().len();
    let published_on = (state.published_at)().filter(|_| published).map(|at| {
        crate::utils::time::to_local(at)
            .format("%Y.%m.%d")
            .to_string()
    });

    let submit = move |status: &'static str| save::submit(state, toast, status);
    let retry = move |_| {
        // 先取出再匹配：if-let 条件里的 peek() 借用会活到整个分支，submit 写入时冲突。
        let current = state.save.peek().clone();
        if let SaveState::Failed { status, .. } = current {
            save::submit(state, toast, status);
        }
    };
    let move_to_trash = move |_| {
        let Some(id) = post_id else {
            return;
        };
        spawn(async move {
            match crate::api::posts::delete_post(id).await {
                Ok(res) if res.success => {
                    toast.success("已移到回收站");
                    navigator.push(Route::Posts {});
                }
                Ok(res) => toast.error(res.message),
                Err(e) => toast.error(failure_message("移到回收站失败", &e)),
            }
        });
    };

    rsx! {
        div {
            class: "write-page",
            onkeydown: move |e| {
                let modifiers = e.modifiers();
                if (modifiers.meta() || modifiers.ctrl())
                    && e.key() == Key::Character("s".into())
                {
                    e.prevent_default();
                    if can_save {
                        submit(if published { "published" } else { "draft" });
                    }
                }
            },
            header { class: "write-head",
                div { class: "write-head-left",
                    Link {
                        class: "{button_class(ButtonVariant::Ghost, ButtonSize::Md)} btn-icon",
                        to: Route::Posts {},
                        aria_label: "返回文章列表",
                        Icon { name: IconName::ArrowLeft, size: 16 }
                    }
                    nav { class: "write-crumbs", aria_label: "面包屑",
                        Link { to: Route::Posts {}, "文章" }
                        span { aria_hidden: "true", "/" }
                        span { aria_current: "page", "{crumb}" }
                    }
                }
                div { class: "write-head-actions",
                    if indicator == SaveIndicator::Failed {
                        button {
                            class: "write-status",
                            r#type: "button",
                            "data-tone": indicator.tone(),
                            title: if let SaveState::Failed { message, .. } = &save_state { message.clone() },
                            onclick: retry,
                            span { class: "write-status-dot", aria_hidden: "true" }
                            span { role: "status", "{indicator.label()}" }
                        }
                    } else {
                        span {
                            class: "write-status",
                            "data-tone": indicator.tone(),
                            "data-pulse": (indicator == SaveIndicator::Saving).then_some("true"),
                            span { class: "write-status-dot", aria_hidden: "true" }
                            span { role: "status", aria_live: "polite", "{indicator.label()}" }
                        }
                    }
                    if post_id.is_some() {
                        Link {
                            class: "{button_class(ButtonVariant::Ghost, ButtonSize::Md)} write-hide-narrow",
                            to: Route::PostPreview { slug: saved.slug.clone() },
                            "预览"
                        }
                    }
                    button {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Md),
                        r#type: "button",
                        aria_pressed: "{drawer_open()}",
                        aria_controls: "write-drawer",
                        // 窄屏只剩图标，名称由 aria-label 补出。
                        aria_label: "属性",
                        onclick: move |_| {
                            drawer_animate.set(true);
                            drawer_open.toggle();
                        },
                        Icon { name: IconName::PanelRight, size: 15 }
                        span { class: "write-hide-narrow", "属性" }
                    }
                    if published {
                        button {
                            class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                            r#type: "button",
                            disabled: !can_save,
                            title: save_hint,
                            onclick: move |_| submit("published"),
                            "更新"
                        }
                        Menu {
                            label: "更多操作",
                            items: vec![MenuItem::new("unpublish", "转为草稿").icon(IconName::EyeOff)],
                            on_select: move |_| {
                                if can_save {
                                    submit("draft");
                                }
                            },
                        }
                    } else {
                        button {
                            class: "{button_class(ButtonVariant::Ghost, ButtonSize::Md)} write-hide-narrow",
                            r#type: "button",
                            disabled: !can_save,
                            title: save_hint,
                            onclick: move |_| submit("draft"),
                            "存草稿"
                        }
                        button {
                            class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                            r#type: "button",
                            disabled: !can_save,
                            title: save_hint,
                            onclick: move |_| submit("published"),
                            "发布"
                        }
                    }
                }
            }

            if let Some(local) = recovery() {
                div { class: "write-banner", role: "alert",
                    Icon { name: IconName::RotateCcwClock, size: 18 }
                    div {
                        p { class: "write-banner-title", "这个标签页留有未保存的内容" }
                        p {
                            "本地副本与服务端版本不同。恢复会替换编辑器里的内容，保存后才生效；选择之前编辑器暂停输入。"
                        }
                        div { class: "write-banner-actions",
                            button {
                                class: button_class(ButtonVariant::Primary, ButtonSize::Sm),
                                r#type: "button",
                                disabled: !ready,
                                onclick: move |_| {
                                    form.set(local.clone());
                                    state.set_content(&local.content);
                                    recovery.set(None);
                                },
                                "恢复本地副本"
                            }
                            button {
                                class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                                r#type: "button",
                                onclick: move |_| recovery.set(None),
                                "使用服务端版本"
                            }
                        }
                    }
                }
            }

            div { class: "write-body",
                main { class: "write-main", "data-vt-scroll": "admin-write-content",
                    div { class: "write-doc",
                        LibraryLoadError { library: state.library }
                        if let Some(message) = phase.error() {
                            p { class: "write-notice", role: "alert",
                                span { "{message}" }
                                button {
                                    class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                                    r#type: "button",
                                    onclick: move |_| state.retry(),
                                    "重试"
                                }
                            }
                        }
                        if cache_failed() {
                            p { class: "write-notice", role: "alert",
                                "浏览器无法保存本地恢复副本，请先保存再离开。"
                            }
                        }
                        for err in (state.upload_errors)() {
                            p { key: "{err.id}", class: "write-notice", role: "alert",
                                span { "图片上传失败：{err.file_name} — {err.message}" }
                                IconButton {
                                    icon: IconName::X,
                                    label: "关闭提示",
                                    size: ButtonSize::Sm,
                                    onclick: {
                                        let id = err.id.clone();
                                        let mut upload_errors = state.upload_errors;
                                        move |_| {
                                            #[cfg(target_arch = "wasm32")]
                                            if let Some(handle) = &*state.editor.read() {
                                                handle.instance().remove_upload_by_upload_id(&id);
                                            }
                                            upload_errors.write().retain(|e| e.id != id);
                                        }
                                    },
                                }
                            }
                        }

                        label { class: "sr-only", r#for: "write-title", "标题" }
                        // 单段文本框随内容换行；回车不换行，直接进入正文。
                        textarea {
                            id: "write-title",
                            class: "write-title",
                            rows: "1",
                            placeholder: "文章标题",
                            readonly: recovering,
                            value: "{draft.title}",
                            oninput: move |e| form.write().title = e.value().replace(['\n', '\r'], ""),
                            onkeydown: move |e| {
                                if e.key() == Key::Enter && !e.data().is_composing() {
                                    e.prevent_default();
                                    focus_editor();
                                }
                            },
                        }
                        p { class: "write-meta",
                            if saved.slug.is_empty() {
                                span { "保存后生成链接" }
                            } else {
                                span { class: "write-meta-slug", "/post/{saved.slug}" }
                            }
                            if tag_count > 0 {
                                span { "{tag_count} 个标签" }
                            }
                            if let Some(date) = published_on {
                                span { "已发布于 {date}" }
                            } else {
                                span { "草稿 · 未发布" }
                            }
                        }
                        div { class: "write-rule", aria_hidden: "true" }

                        // inert 只在等待选择恢复时出现：属性存在即生效，不能写成 false。
                        div {
                            class: "write-editor-wrap",
                            "inert": recovering.then_some("true"),
                            "data-empty": show_starters.then_some("true"),
                            div { id: "tiptap-editor", class: "write-editor" }
                        }

                        if show_starters {
                            div { class: "write-starters", role: "group", aria_label: "起步模板",
                                for (name, hint, body) in STARTERS {
                                    button {
                                        key: "{name}",
                                        class: "write-starter",
                                        r#type: "button",
                                        onclick: move |_| {
                                            starter_dismissed.set(true);
                                            if !body.is_empty() {
                                                state.set_content(body);
                                            }
                                            focus_editor();
                                        },
                                        span { class: "write-starter-name", "{name}" }
                                        span { class: "write-starter-hint", "{hint}" }
                                    }
                                }
                            }
                            p { class: "write-starter-note",
                                "标题和正文都填写后才能保存；摘要、标签和封面可以稍后在「属性」里补。"
                            }
                        }
                    }
                }
                if drawer_open() {
                    div {
                        id: "write-drawer",
                        class: "write-drawer-slot",
                        "data-enter": drawer_animate().then_some("true"),
                        MetadataPanel { state, open: drawer_open, on_trash: move_to_trash }
                    }
                }
            }

            if state.phase.read().is_loading() && state.library.read().as_ref().is_none_or(|r| r.is_ok()) {
                div { class: "write-loading", WriteSkeleton {} }
            }

            // 正文插图：素材库多选。
            AssetPickerModal {
                visible: state.body_picker_visible,
                cover_uploading: state.body_picker_uploading,
                title: "选择图片",
                multi: true,
                on_select: move |picks: Vec<AssetSelection>| {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(handle) = &*state.editor.read() {
                        if let Ok(json) = serde_json::to_string(&picks) {
                            handle.instance().insert_images_from_library(&json);
                        }
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = picks;
                },
            }
        }
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
