//! 素材上传 modal（素材管理页内上传）。
//!
//! UI 外壳（遮罩/面板/关闭动效经 [`crate::components::ui::ModalShell`]，逐文件状态列表
//! 在此渲染）；上传状态机、worker 池并发调度与校验规则见
//! [`crate::components::assets::upload_pool`]。
//!
//! Esc 关闭由 `ModalShell` 负责（显示期间监听 keydown）；粘贴监听挂在 window 上，
//! 只在 mount 注册一次、`on_cleanup` 移除，handler 内用 `get_untracked` 守卫——
//! modal 关闭后粘贴绝不触发上传。无文件的文本粘贴不拦截（不 prevent_default），
//! 搜索框等正常粘贴不受影响。
//!
//! 组件始终挂载（`visible` 只控制渲染早退），上传中途允许关闭弹窗：worker 与
//! signals 随组件实例存活，后台续传，每批（一次拖拽/选择/粘贴）收尾时 ≥1 个成功仍照常
//! 回调 `on_uploaded`；组件卸载时 worker 池释放文件句柄并停止。

use leptos::prelude::*;

use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, Icon, IconButton, IconName, ModalShell, EXIT_ANIM_MS,
};

#[cfg(target_arch = "wasm32")]
use super::upload_pool::{enqueue_files, set_status, UploadPool};
pub(crate) use super::upload_pool::{UploadItem, UploadStatus};

#[cfg(target_arch = "wasm32")]
use crate::bridges::tiptap::upload_image_file;

/// 把 `FileList` 展开成 `Vec<File>`（拖拽 / 选择 / 粘贴三个入口共用）。
#[cfg(target_arch = "wasm32")]
fn collect_files(list: Option<web_sys::FileList>) -> Vec<web_sys::File> {
    let Some(list) = list else {
        return Vec::new();
    };
    (0..list.length()).filter_map(|i| list.item(i)).collect()
}

/// 素材上传 modal。
///
/// - `visible`：显隐控制（父组件持有；× / Esc / 点遮罩都会置 false）。
/// - `on_uploaded`：一批上传完成且 ≥1 个成功时回调一次（父组件刷新网格）。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub fn AssetUploadModal(
    visible: RwSignal<bool>,
    #[prop(into)] on_uploaded: Callback<()>,
) -> impl IntoView {
    let items = RwSignal::new(Vec::<UploadItem>::new());
    let drag_active = RwSignal::new(false);
    // 关闭动画状态：visible 翻 false 时先置 closing 播退出动画，EXIT_ANIM_MS 后再卸载内容。
    let closing = RwSignal::new(false);
    // worker 池共享状态（WASM-only）：持 Rc，含 `web_sys::File`，不是 Send + Sync，放本地存储。
    #[cfg(target_arch = "wasm32")]
    let pool = StoredValue::new_local(std::rc::Rc::new(UploadPool::new()));
    // 并发配置：默认值先行，挂载后经 admin server fn 拉服务端值覆盖（面板改了
    // 无需刷新页面——下次打开本站页面即生效；进行中的 worker live 读取停顿）。
    let concurrency = RwSignal::new(crate::models::settings::DEFAULT_UPLOAD_CONCURRENCY);

    // 挂载即读服务端并发配置；失败静默回退默认值，不阻塞上传。
    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            if let Ok(s) = crate::api::settings::get_upload_settings().await {
                let _ = concurrency.try_set(s.concurrency);
            }
        });
    });

    // window 粘贴监听：组件始终挂载，故只在 mount 注册一次，on_cleanup 移除；
    // handler 内 get_untracked 守卫——modal 关闭后粘贴不响应。
    #[cfg(target_arch = "wasm32")]
    {
        let paste_handle = StoredValue::new(None::<WindowListenerHandle>);
        Effect::new(move |_| {
            if paste_handle.with_value(Option::is_some) {
                return;
            }
            let handle = window_event_listener(leptos::ev::paste, move |ev| {
                if !visible.try_get_untracked().unwrap_or(false) {
                    return;
                }
                let Some(dt) = ev.clipboard_data() else {
                    return;
                };
                let collected = collect_files(dt.files());
                // 文本粘贴（无文件）不拦截：不 prevent_default，搜索框正常粘贴。
                if collected.is_empty() {
                    return;
                }
                ev.prevent_default();
                enqueue_files(items, pool.get_value(), concurrency, on_uploaded, collected);
            });
            paste_handle.set_value(Some(handle));
        });
        on_cleanup(move || {
            if let Some(Some(handle)) = paste_handle.try_update_value(Option::take) {
                handle.remove();
            }
        });
    }
    // 卸载：释放文件句柄、清空队列并让在跑的 worker 退出。
    #[cfg(target_arch = "wasm32")]
    on_cleanup(move || {
        if let Some(pool) = pool.try_get_value() {
            pool.dispose();
        }
    });

    let on_retry = move |item_id: u64| {
        #[cfg(target_arch = "wasm32")]
        if let Some(file) = pool.get_value().find_file(item_id) {
            set_status(items, item_id, UploadStatus::Uploading);
            leptos::task::spawn_local(async move {
                match upload_image_file(file).await {
                    Ok(_) => {
                        if set_status(items, item_id, UploadStatus::Done) {
                            on_uploaded.run(());
                        }
                    }
                    Err(msg) => {
                        set_status(items, item_id, UploadStatus::Failed(msg));
                    }
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = item_id;
    };
    let on_remove = move |item_id: u64| {
        items.update(|list| {
            if let Some(item) = list.iter_mut().find(|item| item.id == item_id) {
                item.removing = true;
            }
        });
        #[cfg(target_arch = "wasm32")]
        pool.get_value().remove_file(item_id);
        leptos::task::spawn_local(async move {
            crate::utils::time::sleep_ms(EXIT_ANIM_MS).await;
            let _ = items.try_update(|list| list.retain(|item| item.id != item_id));
        });
    };

    view! {
        <ModalShell
            visible=visible
            closing=closing
            title="上传素材"
            panel_class="asset-modal asset-uploader"
        >
            // 头部（照抄 AssetPickerModal 头部类）。
            <div class="asset-modal-head">
                <h2>"上传素材"</h2>
                <IconButton
                    icon=IconName::X
                    label="关闭"
                    size=ButtonSize::Sm
                    onclick=move |_| {
                        closing.set(true);
                        visible.set(false);
                    }
                />
            </div>

            <UploadPanel items=items on_retry=on_retry on_remove=on_remove>
                // 拖放区 = label：点击天然触发隐藏 file input（同 CoverUploader），无需 JS。
                // 内部内容包一层 pointer-events-none，防子元素进出触发 dragleave 高亮闪烁。
                <label
                    class="asset-drop"
                    data-drag=move || drag_active.get().then_some("true")
                    // dragover 必须 prevent_default，否则浏览器直接打开文件。
                    on:dragover=move |evt| {
                        evt.prevent_default();
                        drag_active.set(true);
                    }
                    on:dragenter=move |evt| {
                        evt.prevent_default();
                    }
                    on:dragleave=move |_| {
                        drag_active.set(false);
                    }
                    on:drop=move |evt| {
                        evt.prevent_default();
                        drag_active.set(false);
                        #[cfg(target_arch = "wasm32")]
                        {
                            let collected = collect_files(
                                evt.data_transfer().and_then(|dt| dt.files()),
                            );
                            if !collected.is_empty() {
                                enqueue_files(
                                    items,
                                    pool.get_value(),
                                    concurrency,
                                    on_uploaded,
                                    collected,
                                );
                            }
                        }
                    }
                >
                    <UploadDropZoneContent/>
                    <input
                        type="file"
                        accept="image/jpeg,image/png,image/gif,image/webp"
                        multiple=true
                        class="sr-only"
                        on:change=move |evt| {
                            #[cfg(target_arch = "wasm32")]
                            {
                                let collected = collect_files(
                                    event_target::<web_sys::HtmlInputElement>(&evt).files(),
                                );
                                if !collected.is_empty() {
                                    enqueue_files(
                                        items,
                                        pool.get_value(),
                                        concurrency,
                                        on_uploaded,
                                        collected,
                                    );
                                }
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            let _ = evt;
                        }
                    />
                </label>
            </UploadPanel>
            <UploadFooter
                items=items
                on_done=move |_| {
                    closing.set(true);
                    visible.set(false);
                }
            />
        </ModalShell>
    }
}

/// 底栏：完成、上传中与失败张数，「完成」关闭弹窗。
#[component]
fn UploadFooter(
    #[prop(into)] items: Signal<Vec<UploadItem>>,
    #[prop(into)] on_done: Callback<()>,
) -> impl IntoView {
    let summary = move || {
        items.with(|list| {
            let done = list
                .iter()
                .filter(|i| i.status == UploadStatus::Done)
                .count();
            let busy = list
                .iter()
                .filter(|i| matches!(i.status, UploadStatus::Queued | UploadStatus::Uploading))
                .count();
            let failed = list
                .iter()
                .filter(|i| matches!(i.status, UploadStatus::Failed(_)))
                .count();
            upload_summary(done, busy, failed)
        })
    };
    view! {
        <div class="asset-modal-foot">
            <span role="status">{summary}</span>
            <button
                class=button_class(ButtonVariant::Secondary, ButtonSize::Md)
                type="button"
                on:click=move |_| on_done.run(())
            >
                "完成"
            </button>
        </div>
    }
}

/// 「1 张完成 · 1 张上传中 · 1 张失败」；没有文件时给出操作提示。
fn upload_summary(done: usize, busy: usize, failed: usize) -> String {
    let mut parts = Vec::new();
    if done > 0 {
        parts.push(format!("{done} 张完成"));
    }
    if busy > 0 {
        parts.push(format!("{busy} 张上传中"));
    }
    if failed > 0 {
        parts.push(format!("{failed} 张失败"));
    }
    if parts.is_empty() {
        "可以一次选择多张，也可以直接粘贴".to_string()
    } else {
        parts.join(" · ")
    }
}

/// 上传面板主体：`children`（拖放区）在上，逐文件状态列表在下。
///
/// 图鉴预览与 [`AssetUploadModal`] 共用：列表数据由调用方以信号提供，重试 / 移除经回调上抛。
#[component]
pub(crate) fn UploadPanel(
    #[prop(into)] items: Signal<Vec<UploadItem>>,
    #[prop(into)] on_retry: Callback<u64>,
    #[prop(into)] on_remove: Callback<u64>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="asset-modal-body asset-upload-body">
            {children()} <UploadRows items=items on_retry=on_retry on_remove=on_remove/>
        </div>
    }
}

/// 拖放区的内容（图标 + 标题 + 提示），文案可由图鉴预览覆盖。
#[component]
pub(crate) fn UploadDropZoneContent(
    #[prop(default = "拖拽图片到这里，或点击选择")] title: &'static str,
    #[prop(default = "也可以粘贴 · JPEG / PNG / GIF / WebP · 单张不超过 5MB")] hint: &'static str,
) -> impl IntoView {
    view! {
        <span class="asset-drop-content">
            <Icon name=IconName::Upload size=22/>
            <span class="asset-drop-title">{title}</span>
            <span class="asset-drop-hint">{hint}</span>
        </span>
    }
}

/// 逐文件状态列表；空列表不渲染 `<ul>`。
///
/// 行元素以 id 为键保持稳定（入场动画只在插入时播放），状态 / 退出标记经 `items`
/// 按 id 回查后原地更新属性。
#[component]
pub(crate) fn UploadRows(
    #[prop(into)] items: Signal<Vec<UploadItem>>,
    #[prop(into)] on_retry: Callback<u64>,
    #[prop(into)] on_remove: Callback<u64>,
) -> impl IntoView {
    move || {
        (!items.with(Vec::is_empty)).then(|| {
            view! {
                <ul class="asset-upload-rows">
                    <For
                        each=move || items.get()
                        key=|item| item.id
                        children=move |item| {
                            view! {
                                <UploadRow
                                    id=item.id
                                    name=item.name
                                    size=item.size
                                    items=items
                                    on_retry=on_retry
                                    on_remove=on_remove
                                />
                            }
                        }
                    />
                </ul>
            }
        })
    }
}

/// 单行：名称 / 大小固定，状态随 `items` 中同 id 条目更新。
#[component]
fn UploadRow(
    id: u64,
    name: String,
    size: String,
    items: Signal<Vec<UploadItem>>,
    on_retry: Callback<u64>,
    on_remove: Callback<u64>,
) -> impl IntoView {
    // 行被移除后 `For` 会随后销毁本组件；期间回查不到条目时沿用上一次的值。
    let current = Memo::new(move |prev: Option<&UploadItem>| {
        items
            .with(|list| list.iter().find(|item| item.id == id).cloned())
            .or_else(|| prev.cloned())
            .expect("行组件创建时条目必然存在")
    });
    let status = move || current.get().status;
    let state = move || match status() {
        UploadStatus::Queued => "queued",
        UploadStatus::Uploading => "uploading",
        UploadStatus::Done => "done",
        UploadStatus::Failed(_) => "failed",
    };
    let detail = {
        let size = size.clone();
        move || match status() {
            UploadStatus::Uploading => view! {
                <span class="asset-upload-bar" aria-hidden="true">
                    <span></span>
                </span>
            }
            .into_any(),
            UploadStatus::Failed(message) => {
                let title_attr = message.clone();
                view! {
                    <span class="asset-upload-error" title=title_attr>
                        {message}
                    </span>
                }
                .into_any()
            }

            _ => view! { <span class="asset-upload-size">{size.clone()}</span> }.into_any(),
        }
    };
    let trailing = move || match status() {
        UploadStatus::Queued => view! { "等待中" }.into_any(),
        UploadStatus::Uploading => view! { "上传中" }.into_any(),
        UploadStatus::Done => view! {
            <Icon name=IconName::Check size=13/>
            "已上传"
        }
        .into_any(),
        UploadStatus::Failed(_) => view! {
            <button
                class=button_class(ButtonVariant::Ghost, ButtonSize::Xs)
                type="button"
                on:click=move |_| on_retry.run(id)
            >
                "重试"
            </button>
            <IconButton
                icon=IconName::X
                label="移除"
                size=ButtonSize::Xs
                onclick=move |_| on_remove.run(id)
            />
        }
        .into_any(),
    };
    let title_attr = name.clone();
    view! {
        <li
            class="asset-upload-row"
            data-state=state
            data-removing=move || current.get().removing.then_some("true")
        >
            <span class="asset-upload-thumb" aria-hidden="true"></span>
            <span class="asset-upload-text">
                <span class="asset-upload-name" title=title_attr>
                    {name}
                </span>
                {detail}
            </span>
            <span class="asset-upload-status">{trailing}</span>
        </li>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_summary_lists_only_nonzero_states() {
        assert_eq!(upload_summary(1, 1, 1), "1 张完成 · 1 张上传中 · 1 张失败");
        assert_eq!(upload_summary(3, 0, 0), "3 张完成");
        assert_eq!(upload_summary(0, 0, 0), "可以一次选择多张，也可以直接粘贴");
    }

    fn item(id: u64, status: UploadStatus, removing: bool) -> UploadItem {
        UploadItem {
            id,
            name: format!("图-{id}.png"),
            size: "1.0 MB".to_string(),
            status,
            removing,
        }
    }

    #[test]
    fn rows_render_state_markers_and_actions() {
        let html = Owner::new().with(|| {
            let items = RwSignal::new(vec![
                item(1, UploadStatus::Done, false),
                item(2, UploadStatus::Uploading, false),
                item(3, UploadStatus::Failed("大小超过 5MB 限制".into()), true),
            ]);
            view! {
                <UploadRows
                    items=items
                    on_retry=Callback::new(|_| {})
                    on_remove=Callback::new(|_| {})
                />
            }
            .to_html()
        });
        assert!(html.contains("data-state=\"done\""), "{html}");
        assert!(html.contains("data-state=\"uploading\""), "{html}");
        assert!(html.contains("data-state=\"failed\""), "{html}");
        assert_eq!(html.matches("data-removing=\"true\"").count(), 1, "{html}");
        assert!(html.contains("class=\"asset-upload-bar\""), "{html}");
        assert!(html.contains("重试"), "{html}");
        assert!(html.contains("已上传"), "{html}");
    }

    #[test]
    fn empty_rows_render_no_list() {
        let html = Owner::new().with(|| {
            let items = RwSignal::new(Vec::<UploadItem>::new());
            view! {
                <UploadRows
                    items=items
                    on_retry=Callback::new(|_| {})
                    on_remove=Callback::new(|_| {})
                />
            }
            .to_html()
        });
        assert!(!html.contains("asset-upload-rows"), "{html}");
    }
}
