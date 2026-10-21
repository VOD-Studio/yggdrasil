//! 素材上传 modal（素材管理页内上传）。
//!
//! UI 外壳（遮罩/面板/关闭动效经 [`crate::components::ui::ModalShell`]，逐文件状态列表
//! 在此渲染）；上传状态机、worker 池并发调度与校验规则见
//! [`crate::components::assets::upload_pool`]。
//!
//! Esc / 粘贴监听挂在 window 上，只在 mount 注册一次、`use_drop` 移除；handler 内用
//! `visible.peek()` 守卫——modal 关闭后粘贴绝不触发上传。无文件的文本粘贴不拦截
//! （不 prevent_default），搜索框等正常粘贴不受影响。原生监听器触发时 Dioxus 的
//! scope 栈为空（`spawn` 会因 `current_scope_id` unwrap 空栈而 panic），渲染期捕获的
//! 组件 scope id 在粘贴 handler 里经 `Runtime::in_scope` 显式进栈后再入队，使粘贴路径
//! 与 Dioxus 事件入口（拖拽/选择）的 spawn 语义完全一致。
//!
//! 组件始终挂载（`visible` 只控制渲染早退），上传中途允许关闭弹窗：spawn 的 worker 与
//! signals 随组件实例存活，后台续传，每批（一次拖拽/选择/粘贴）收尾时 ≥1 个成功仍照常
//! 回调 `on_uploaded`。

use dioxus::prelude::*;

use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, Icon, IconButton, IconName, ModalShell, EXIT_ANIM_MS,
};

#[cfg(target_arch = "wasm32")]
use super::upload_pool::{enqueue_files, set_status, UploadPool};
pub(crate) use super::upload_pool::{UploadItem, UploadStatus};

#[cfg(target_arch = "wasm32")]
use crate::bridges::tiptap::upload_image_file;
// 从 Dioxus 事件拿底层 web_sys::File（write.rs L30-34 同款 cfg 门控惯例）：
// - HasFileData：evt.files()（FormEvent / DragEvent 取文件）
// - WebFileExt：file.get_web_file()（FileData 取底层 web_sys::File）
#[cfg(target_arch = "wasm32")]
use dioxus::html::HasFileData;
#[cfg(target_arch = "wasm32")]
use dioxus::web::WebFileExt;

/// 素材上传 modal。
///
/// - `visible`：显隐控制（父组件持有；× / Esc / 点遮罩都会置 false）。
/// - `on_uploaded`：一批上传完成且 ≥1 个成功时回调一次（父组件刷新网格）。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn AssetUploadModal(mut visible: Signal<bool>, on_uploaded: EventHandler<()>) -> Element {
    let mut items: Signal<Vec<UploadItem>> = use_signal(Vec::new);
    let mut drag_active = use_signal(|| false);
    // 关闭动画状态：visible 翻 false 时先置 closing 播退出动画，EXIT_ANIM_MS 后再卸载内容。
    let mut closing = use_signal(|| false);
    // worker 池共享状态（WASM-only）：use_hook 持 Rc，跨渲染共享同一实例。
    #[cfg(target_arch = "wasm32")]
    let pool = use_hook(|| std::rc::Rc::new(UploadPool::new()));
    // 并发配置：默认值先行，挂载后经 admin server fn 拉服务端值覆盖（面板改了
    // 无需刷新页面——下次打开本站页面即生效；进行中的 worker live 读取停顿）。
    let mut concurrency: Signal<i32> =
        use_signal(|| crate::models::settings::DEFAULT_UPLOAD_CONCURRENCY);

    // 挂载即读服务端并发配置；失败静默回退默认值，不阻塞上传。
    #[cfg(target_arch = "wasm32")]
    {
        use_effect(move || {
            spawn(async move {
                if let Ok(s) = crate::api::settings::get_upload_settings().await {
                    concurrency.set(s.concurrency);
                }
            });
        });
    }

    // window 全局监听（Esc 关闭 + 粘贴上传）：组件始终挂载，故只在 mount 注册一次，
    // use_drop 移除；handler 内 peek 守卫——modal 关闭后 Esc/粘贴都不响应。
    // 骨架照抄 ui.rs Popover（use_hook 持 Closure + use_effect 注册 + use_drop 移除）。
    #[cfg(target_arch = "wasm32")]
    {
        use std::cell::RefCell;
        use std::rc::Rc;
        type KeyClosure = wasm_bindgen::prelude::Closure<dyn FnMut(web_sys::KeyboardEvent)>;
        type PasteClosure = wasm_bindgen::prelude::Closure<dyn FnMut(web_sys::ClipboardEvent)>;
        type Listeners = Rc<RefCell<Option<(KeyClosure, PasteClosure)>>>;
        let listeners: Listeners = use_hook(|| Rc::new(RefCell::new(None)));
        let listeners_for_drop = listeners.clone();
        // 组件 scope id（渲染期捕获）：原生 window 监听器触发时 Dioxus 的 scope 栈
        // 为空，`spawn` 会因 current_scope_id unwrap 空栈而 panic（与 sql_console
        // 的 Ctrl+Enter 同源），监听器里用 Runtime::in_scope 重建 scope 上下文。
        let scope_id = dioxus::core::Runtime::current()
            .try_current_scope_id()
            .unwrap_or(dioxus::core::ScopeId::ROOT);
        // effect 闭包 move 捕获的是这份克隆，原值留给下方 drop/change 事件闭包。
        let pool_for_paste = pool.clone();
        use_effect(move || {
            let Some(window) = web_sys::window() else {
                return;
            };
            let on_keydown =
                wasm_bindgen::prelude::Closure::wrap(Box::new(move |ev: web_sys::KeyboardEvent| {
                    if !*visible.peek() {
                        return;
                    }
                    if ev.key() == "Escape" {
                        closing.set(true);
                        visible.set(false);
                    }
                })
                    as Box<dyn FnMut(web_sys::KeyboardEvent)>);
            // 内层 paste 闭包再持一份克隆（FnMut effect 的捕获不可被 move 出）。
            let pool_in_paste = pool_for_paste.clone();
            let on_paste =
                wasm_bindgen::prelude::Closure::wrap(Box::new(move |ev: web_sys::ClipboardEvent| {
                    if !*visible.peek() {
                        return;
                    }
                    let Some(dt) = ev.clipboard_data() else {
                        return;
                    };
                    let Some(list) = dt.files() else {
                        return;
                    };
                    // 文本粘贴（无文件）不拦截：不 prevent_default，搜索框正常粘贴。
                    if list.length() == 0 {
                        return;
                    }
                    ev.prevent_default();
                    let collected: Vec<web_sys::File> =
                        (0..list.length()).filter_map(|i| list.item(i)).collect();
                    if !collected.is_empty() {
                        // 原生监听里无 scope 上下文：进入组件 scope 使 enqueue 内的
                        // spawn 与 Dioxus 事件入口（拖拽/选择）行为完全一致。
                        // Rc 句柄逐事件克隆进内层闭包（外层 FnMut 的捕获不可 move 出，
                        // 同 sql_console 的 on_run_shortcut 闭包包装考虑）。
                        let pool_call = pool_in_paste.clone();
                        dioxus::core::Runtime::current().in_scope(scope_id, move || {
                            enqueue_files(items, pool_call, concurrency, on_uploaded, collected);
                        });
                    }
                })
                    as Box<dyn FnMut(web_sys::ClipboardEvent)>);
            use wasm_bindgen::JsCast;
            let _ = window
                .add_event_listener_with_callback("keydown", on_keydown.as_ref().unchecked_ref());
            let _ =
                window.add_event_listener_with_callback("paste", on_paste.as_ref().unchecked_ref());
            *listeners.borrow_mut() = Some((on_keydown, on_paste));
        });
        use_drop(move || {
            if let Some((on_keydown, on_paste)) = listeners_for_drop.borrow_mut().take() {
                if let Some(window) = web_sys::window() {
                    use wasm_bindgen::JsCast;
                    let _ = window.remove_event_listener_with_callback(
                        "keydown",
                        on_keydown.as_ref().unchecked_ref(),
                    );
                    let _ = window.remove_event_listener_with_callback(
                        "paste",
                        on_paste.as_ref().unchecked_ref(),
                    );
                }
            }
        });
    }

    // 每个 move 事件闭包独占一份 Rc 克隆（move 闭包按整个变量捕获）。
    #[cfg(target_arch = "wasm32")]
    let (pool_for_drop, pool_for_change) = (pool.clone(), pool.clone());
    // 行列表内层 for 闭包用：状态区 keyed 包装引入的嵌套闭包无法 move 外层闭包捕获的
    // Rc（FnMut 链，E0507），故在组件体预克隆一份，供内层闭包从组件体直接捕获。
    #[cfg(target_arch = "wasm32")]
    let (pool_for_rows, pool_for_remove) = (pool.clone(), pool.clone());

    // 快照当前列表（小 Vec，clone 成本可忽略；status 变化驱动重渲染）。
    let items_snapshot = items.read().clone();

    rsx! {
        ModalShell {
            visible,
            closing,
            title: "上传素材",
            panel_class: "asset-modal asset-uploader",

                // 头部（照抄 AssetPickerModal 头部类）。
                div { class: "asset-modal-head",
                    h2 { "上传素材" }
                    IconButton {
                        icon: IconName::X,
                        label: "关闭",
                        size: ButtonSize::Sm,
                        onclick: move |_| {
                            closing.set(true);
                            visible.set(false);
                        },
                    }
                }

                    UploadPanel {
                        items: items_snapshot,
                        on_retry: move |item_id: u64| {
                            #[cfg(target_arch = "wasm32")]
                            if let Some(file) = pool_for_rows.find_file(item_id) {
                                set_status(&mut items, item_id, UploadStatus::Uploading);
                                spawn(async move {
                                    match upload_image_file(file).await {
                                        Ok(_) => {
                                            set_status(&mut items, item_id, UploadStatus::Done);
                                            on_uploaded.call(());
                                        }
                                        Err(msg) => set_status(&mut items, item_id, UploadStatus::Failed(msg)),
                                    }
                                });
                            }
                        },
                        on_remove: move |item_id: u64| {
                            if let Some(item) = items.write().iter_mut().find(|item| item.id == item_id) {
                                item.removing = true;
                            }
                            #[cfg(target_arch = "wasm32")]
                            pool_for_remove.remove_file(item_id);
                            spawn(async move {
                                crate::utils::time::sleep_ms(EXIT_ANIM_MS).await;
                                items.write().retain(|item| item.id != item_id);
                            });
                        },

                    // 拖放区 = label：点击天然触发隐藏 file input（同 CoverUploader），无需 JS。
                    // 内部内容包一层 pointer-events-none，防子元素进出触发 dragleave 高亮闪烁。
                    label {
                        class: "asset-drop",
                        "data-drag": drag_active().then_some("true"),
                        // ondragover 必须 prevent_default，否则浏览器直接打开文件。
                        ondragover: move |evt| {
                            evt.prevent_default();
                            drag_active.set(true);
                        },
                        ondragenter: move |evt| {
                            evt.prevent_default();
                        },
                        ondragleave: move |_| {
                            drag_active.set(false);
                        },
                        ondrop: move |evt| {
                            evt.prevent_default();
                            drag_active.set(false);
                            #[cfg(target_arch = "wasm32")]
                            {
                                let collected: Vec<web_sys::File> = evt
                                    .files()
                                    .into_iter()
                                    .filter_map(|f| f.get_web_file())
                                    .collect();
                                if !collected.is_empty() {
                                    enqueue_files(
                                        items,
                                        pool_for_drop.clone(),
                                        concurrency,
                                        on_uploaded,
                                        collected,
                                    );
                                }
                            }
                        },
                        UploadDropZoneContent {}
                        input {
                            r#type: "file",
                            accept: "image/jpeg,image/png,image/gif,image/webp",
                            multiple: true,
                            class: "sr-only",
                            onchange: move |evt| {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    let collected: Vec<web_sys::File> = evt
                                        .files()
                                        .into_iter()
                                        .filter_map(|f| f.get_web_file())
                                        .collect();
                                    if !collected.is_empty() {
                                        enqueue_files(
                                            items,
                                            pool_for_change.clone(),
                                            concurrency,
                                            on_uploaded,
                                            collected,
                                        );
                                    }
                                }
                            },
                        }
                    }



                }
                UploadFooter {
                    items: items.read().clone(),
                    on_done: move |_| {
                        closing.set(true);
                        visible.set(false);
                    },
                }
        }
    }
}

/// 底栏：完成、上传中与失败张数，「完成」关闭弹窗。
#[component]
fn UploadFooter(items: Vec<UploadItem>, on_done: EventHandler<()>) -> Element {
    let done = items
        .iter()
        .filter(|i| i.status == UploadStatus::Done)
        .count();
    let busy = items
        .iter()
        .filter(|i| matches!(i.status, UploadStatus::Queued | UploadStatus::Uploading))
        .count();
    let failed = items
        .iter()
        .filter(|i| matches!(i.status, UploadStatus::Failed(_)))
        .count();
    let summary = upload_summary(done, busy, failed);
    rsx! {
        div { class: "asset-modal-foot",
            span { role: "status", "{summary}" }
            button {
                class: button_class(ButtonVariant::Secondary, ButtonSize::Md),
                r#type: "button",
                onclick: move |_| on_done.call(()),
                "完成"
            }
        }
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

#[component]
pub(crate) fn UploadPanel(
    items: Vec<UploadItem>,
    on_retry: EventHandler<u64>,
    on_remove: EventHandler<u64>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "asset-modal-body asset-upload-body",
            {children}
            UploadRows { items, on_retry, on_remove }
        }
    }
}

#[component]
pub(crate) fn UploadDropZoneContent(
    #[props(default = "拖拽图片到这里，或点击选择")] title: &'static str,
    #[props(default = "也可以粘贴 · JPEG / PNG / GIF / WebP · 单张不超过 5MB")] hint: &'static str,
) -> Element {
    rsx! {
        span { class: "asset-drop-content",
            Icon { name: IconName::Upload, size: 22 }
            span { class: "asset-drop-title", "{title}" }
            span { class: "asset-drop-hint", "{hint}" }
        }
    }
}

#[component]
pub(crate) fn UploadRows(
    items: Vec<UploadItem>,
    on_retry: EventHandler<u64>,
    on_remove: EventHandler<u64>,
) -> Element {
    if items.is_empty() {
        return rsx! {};
    }
    rsx! {
        ul { class: "asset-upload-rows",
            for item in items.iter() {
                {
                    let id = item.id;
                    let state = match item.status {
                        UploadStatus::Queued => "queued",
                        UploadStatus::Uploading => "uploading",
                        UploadStatus::Done => "done",
                        UploadStatus::Failed(_) => "failed",
                    };
                    rsx! {
                        li {
                            key: "{id}",
                            class: "asset-upload-row",
                            "data-state": state,
                            "data-removing": item.removing.then_some("true"),
                            span { class: "asset-upload-thumb", aria_hidden: "true" }
                            span { class: "asset-upload-text",
                                span { class: "asset-upload-name", title: "{item.name}", "{item.name}" }
                                match &item.status {
                                    UploadStatus::Uploading => rsx! {
                                        span { class: "asset-upload-bar", aria_hidden: "true", span {} }
                                    },
                                    UploadStatus::Failed(message) => rsx! {
                                        span { class: "asset-upload-error", title: "{message}", "{message}" }
                                    },
                                    _ => rsx! {
                                        span { class: "asset-upload-size", "{item.size}" }
                                    },
                                }
                            }
                            span { class: "asset-upload-status",
                                match &item.status {
                                    UploadStatus::Queued => rsx! { "等待中" },
                                    UploadStatus::Uploading => rsx! { "上传中" },
                                    UploadStatus::Done => rsx! {
                                        Icon { name: IconName::Check, size: 13 }
                                        "已上传"
                                    },
                                    UploadStatus::Failed(_) => rsx! {
                                        button {
                                            class: button_class(ButtonVariant::Ghost, ButtonSize::Xs),
                                            r#type: "button",
                                            onclick: move |_| on_retry.call(id),
                                            "重试"
                                        }
                                        IconButton {
                                            icon: IconName::X,
                                            label: "移除",
                                            size: ButtonSize::Xs,
                                            onclick: move |_| on_remove.call(id),
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::upload_summary;

    #[test]
    fn upload_summary_lists_only_nonzero_states() {
        assert_eq!(upload_summary(1, 1, 1), "1 张完成 · 1 张上传中 · 1 张失败");
        assert_eq!(upload_summary(3, 0, 0), "3 张完成");
        assert_eq!(upload_summary(0, 0, 0), "可以一次选择多张，也可以直接粘贴");
    }
}
