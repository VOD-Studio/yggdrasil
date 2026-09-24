//! Local examples for business components. The production controllers stay in components/.
use dioxus::prelude::*;

use crate::components::assets::asset_picker::{PickerAsset, PickerGallery, PickerSelectionFooter};
use crate::components::assets::asset_upload::{
    UploadDropZoneContent, UploadItem, UploadPanel, UploadStatus,
};
use crate::components::assets::AssetSelection;
use crate::components::code_runner::runner::RunnerExample;
use crate::components::code_runner::CodeRunner;
use crate::components::forms::{FormInput, INPUT_INLINE_CLASS};
use crate::components::ui::{ModalShell, Pagination};
use crate::router::Route;

pub(super) fn preview(slug: &str, detail: bool) -> Option<Element> {
    Some(match slug {
        "asset-picker-modal" => rsx! { PickerDemo { detail } },
        "asset-upload-modal" => rsx! { UploadDemo { detail } },
        "code-runner" => rsx! { RunnerDemo { detail } },
        _ => return None,
    })
}

const IMAGES: [(&str, &str, &str); 5] = [
    (
        "/images/empty-state/dog-camera.webp",
        "相机里的小狗.webp",
        "相机里的两只小狗",
    ),
    (
        "/images/xiantiaoxiaogou_02.webp",
        "光里的院子.webp",
        "院子里的光",
    ),
    (
        "/images/xiantiaoxiaogou_03.webp",
        "午后散步.webp",
        "午后的散步",
    ),
    (
        "/images/xiantiaoxiaogou_input.webp",
        "手绘小狗.webp",
        "小狗插画",
    ),
    (
        "/images/xiantiaoxiaogou_input_bg.webp",
        "草地背景.webp",
        "绿色草地",
    ),
];

fn picker_page(query: &str, page: i32) -> (Vec<PickerAsset>, i64) {
    let query = query.trim().to_lowercase();
    let matches: Vec<PickerAsset> = IMAGES
        .iter()
        .filter(|(_, name, alt)| {
            query.is_empty()
                || name.to_lowercase().contains(&query)
                || alt.to_lowercase().contains(&query)
        })
        .map(|(url, name, alt)| PickerAsset {
            url: (*url).to_string(),
            thumbnail: (*url).to_string(),
            filename: (*name).to_string(),
            alt: Some((*alt).to_string()),
            fresh: false,
        })
        .collect();
    let total = matches.len() as i64;
    let start = ((page - 1).max(0) as usize) * 3;
    (matches.into_iter().skip(start).take(3).collect(), total)
}

fn sample_selected() -> Vec<AssetSelection> {
    IMAGES[..2]
        .iter()
        .map(|(url, _, alt)| AssetSelection {
            url: (*url).to_string(),
            alt: Some((*alt).to_string()),
        })
        .collect()
}

#[component]
fn PickerDemo(detail: bool) -> Element {
    let mut visible = use_signal(|| false);
    let mut closing = use_signal(|| false);
    let mut selected = use_signal(sample_selected);
    let mut multi = use_signal(|| true);
    let mut query = use_signal(String::new);
    let mut page = use_signal(|| 1_i32);
    let mut confirmed: Signal<Vec<AssetSelection>> = use_signal(Vec::new);
    let result_label = confirmed
        .read()
        .iter()
        .map(|item| item.url.clone())
        .collect::<Vec<_>>()
        .join("、");
    rsx! {
        div { class: if detail { "showcase-picker-demo showcase-picker-detail" } else { "showcase-picker-demo" },
            if detail {
                div { class: "showcase-demo-actions",
                    button { r#type: "button", onclick: move |_| { selected.set(Vec::new()); page.set(1); visible.set(true); }, "打开素材选择弹窗" }
                    span { "本地演示，选择不会写入文章。" }
                }
            }
            if !visible() {
                PickerDemoContent { detail, selected, multi, query, page, confirmed, visible }
            }
            if detail {
                ModalShell { visible, closing, title: "素材选择演示", panel_class: "w-full max-w-3xl min-h-0",
                    div { class: "showcase-demo-actions", span { "素材选择 · 本地演示" }
                        button { r#type: "button", onclick: move |_| {
                            selected.set(sample_selected());
                            multi.set(true);
                            query.set(String::new());
                            page.set(1);
                            confirmed.set(Vec::new());
                            closing.set(true);
                            visible.set(false);
                        }, "↻ 恢复默认" }
                        button { r#type: "button", aria_label: "关闭", onclick: move |_| { closing.set(true); visible.set(false); }, "×" }
                    }
                    PickerDemoContent { detail: true, selected, multi, query, page, confirmed, visible }
                }
            }
            if detail && !confirmed.read().is_empty() {
                p { class: "showcase-demo-result", "已确认：{result_label}" }
            }
        }
    }
}

#[component]
fn PickerDemoContent(
    detail: bool,
    mut selected: Signal<Vec<AssetSelection>>,
    mut multi: Signal<bool>,
    mut query: Signal<String>,
    mut page: Signal<i32>,
    mut confirmed: Signal<Vec<AssetSelection>>,
    mut visible: Signal<bool>,
) -> Element {
    let (items, total) = picker_page(&query(), page());
    let mut page_prev = page;
    let mut page_next = page;
    let mut page_jump = page;
    rsx! {
        div { class: "showcase-picker-panel",
            div { class: "showcase-picker-toolbar",
                FormInput { r#type: "search", placeholder: "搜索文件名 / alt", value: query(), class: INPUT_INLINE_CLASS,
                    oninput: move |value: String| { query.set(value); page.set(1); }
                }
                if detail {
                    div { class: "showcase-picker-mode-switch",
                        button { r#type: "button", aria_pressed: "{!multi()}", onclick: move |_| { multi.set(false); selected.set(Vec::new()); }, "单选" }
                        button { r#type: "button", aria_pressed: "{multi()}", onclick: move |_| { multi.set(true); selected.set(Vec::new()); }, "多选" }
                    }
                }
            }
            PickerGallery { items, selected, multi: multi(), loading: false, uploading: false, error: None,
                uploading_preview: None,
                on_pick: move |picks: Vec<AssetSelection>| { confirmed.set(picks); visible.set(false); }
            }
            if total > 3 {
                div { class: "showcase-picker-pagination shrink-0 shadow-[inset_0_1px_0_var(--color-paper-border)]",
                    Pagination::<Route> { variant: "admin", compact: true, current_page: page(), total,
                        per_page: 3, unit: "张",
                        on_prev: move |_| page_prev.set((page_prev() - 1).max(1)),
                        on_next: move |_| page_next.set((page_next() + 1).min(((total + 2) / 3) as i32)),
                        on_jump: move |next: i32| page_jump.set(next.clamp(1, ((total + 2) / 3) as i32)),
                    }
                }
            }
            if multi() {
                PickerSelectionFooter { selected, on_pick: move |picks: Vec<AssetSelection>| { confirmed.set(picks); visible.set(false); } }
            }
        }
    }
}

#[component]
fn UploadDemo(detail: bool) -> Element {
    let mut visible = use_signal(|| false);
    let mut closing = use_signal(|| false);
    let mut items = use_signal(sample_upload_items);
    let mut generation = use_signal(|| 0_u64);
    rsx! {
        div { class: if detail { "showcase-upload-demo showcase-upload-detail" } else { "showcase-upload-demo" },
            if detail {
                div { class: "showcase-demo-actions",
                    button { r#type: "button", onclick: move |_| visible.set(true), "打开上传弹窗" }
                    button { r#type: "button", onclick: move |_| { generation += 1; items.set(Vec::new()); }, "空状态" }
                    button { r#type: "button", onclick: move |_| { generation += 1; items.set(sample_upload_items()); }, "代表状态" }
                    span { "本地演示，示例文件不会上传。" }
                }
            }
            if !visible() { UploadDemoPanel { items, generation } }
            if detail {
                ModalShell { visible, closing, title: "上传素材演示", panel_class: "w-full max-w-3xl min-h-0",
                    div { class: "showcase-demo-actions", span { "素材上传 · 本地演示" }
                        button { r#type: "button", onclick: move |_| {
                            generation += 1;
                            items.set(sample_upload_items());
                            closing.set(true);
                            visible.set(false);
                        }, "↻ 恢复默认" }
                        button { r#type: "button", aria_label: "关闭", onclick: move |_| { closing.set(true); visible.set(false); }, "×" }
                    }
                    UploadDemoPanel { items, generation }
                }
            }
        }
    }
}

fn sample_upload_items() -> Vec<UploadItem> {
    [
        ("封面.webp", "1.2 MB", UploadStatus::Done),
        ("相册-02.png", "860 KB", UploadStatus::Uploading),
        ("草稿插图.jpg", "420 KB", UploadStatus::Queued),
        (
            "超大原图.png",
            "8.4 MB",
            UploadStatus::Failed("大小超过 5MB 限制".to_string()),
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (name, size, status))| UploadItem {
        id: index as u64 + 1,
        name: name.to_string(),
        size: size.to_string(),
        status,
        removing: false,
    })
    .collect()
}

#[component]
fn UploadDemoPanel(mut items: Signal<Vec<UploadItem>>, generation: Signal<u64>) -> Element {
    rsx! {
        UploadPanel { items: items(),
            on_retry: move |id: u64| {
                let ticket = *generation.peek();
                if let Some(item) = items.write().iter_mut().find(|item| item.id == id) {
                    item.status = UploadStatus::Uploading;
                }
                spawn(async move {
                    crate::utils::time::sleep_ms(350).await;
                    if *generation.peek() != ticket { return; }
                    if let Some(item) = items.write().iter_mut().find(|item| item.id == id) {
                        item.status = UploadStatus::Done;
                    }
                });
            },
            on_remove: move |id: u64| items.write().retain(|item| item.id != id),
            button { r#type: "button",
                class: "flex w-full flex-col items-center justify-center gap-2 rounded-2xl border border-dashed border-[var(--color-paper-border)] bg-[var(--color-paper-theme)] px-6 py-8 cursor-pointer hover:border-[var(--color-paper-primary)]",
                onclick: move |_| {
                    let next = items.read().iter().map(|item| item.id).max().unwrap_or(0) + 1;
                    items.write().push(UploadItem { id: next, name: format!("示例文件-{next}.webp"), size: "540 KB".to_string(), status: UploadStatus::Queued, removing: false });
                },
                UploadDropZoneContent { title: "加入示例文件", hint: "本地状态演示，不读取电脑文件或发起上传。" }
            }
        }
    }
}

#[component]
fn RunnerDemo(detail: bool) -> Element {
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
    let mut visible = use_signal(move || detail);
    let mut failed = use_signal(|| false);
    let host_id = if detail {
        "showcase-runner-detail"
    } else {
        "showcase-runner-card"
    };

    #[cfg(target_arch = "wasm32")]
    {
        use std::{cell::RefCell, rc::Rc};
        use wasm_bindgen::{closure::Closure, JsCast};
        type Watch = (
            web_sys::IntersectionObserver,
            Closure<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>,
        );
        let watcher = use_hook(|| Rc::new(RefCell::new(None::<Watch>)));
        let target_watcher = watcher.clone();
        use_effect(move || {
            if visible() {
                return;
            }
            let Some(target) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id(host_id))
            else {
                return;
            };
            let callback = Closure::new(
                move |entries: js_sys::Array, observer: web_sys::IntersectionObserver| {
                    if entries.iter().any(|value| {
                        value
                            .unchecked_into::<web_sys::IntersectionObserverEntry>()
                            .is_intersecting()
                    }) {
                        observer.disconnect();
                        visible.set(true);
                    }
                },
            );
            if let Ok(observer) =
                web_sys::IntersectionObserver::new(callback.as_ref().unchecked_ref())
            {
                observer.observe(&target);
                target_watcher.borrow_mut().replace((observer, callback));
            } else {
                visible.set(true);
            }
        });
        use_drop(move || {
            if let Some((observer, _callback)) = watcher.borrow_mut().take() {
                observer.disconnect();
            }
        });
    }

    let sample = if failed() {
        RunnerExample {
            stdout: "开始整理 3 个条目…\n".to_string(),
            stderr: "示例错误：第 2 个条目缺少标题\n".to_string(),
            status: "退出码: 1 · 输出示例".to_string(),
            error: "运行错误示例".to_string(),
        }
    } else {
        RunnerExample {
            stdout: "开始整理 3 个条目…\n✓ 已整理 3 个条目\n".to_string(),
            stderr: "".to_string(),
            status: "退出码: 0 · 输出示例".to_string(),
            error: String::new(),
        }
    };
    rsx! {
        div { class: if detail { "showcase-runner-demo showcase-runner-detail" } else { "showcase-runner-demo" }, id: host_id,
            if detail {
                div { class: "showcase-demo-actions",
                    button { r#type: "button", aria_pressed: "{!failed()}", onclick: move |_| failed.set(false), "正常输出" }
                    button { r#type: "button", aria_pressed: "{failed()}", onclick: move |_| failed.set(true), "错误输出" }
                    span { "固定输出示例，与编辑后的代码无关；不会执行程序。" }
                }
            }
            if visible() {
                div { key: "runner-{failed()}",
                    CodeRunner { source: "print('让想法生根')\n".to_string(), language: "python".to_string(), overrides: None,
                        instance_id: if detail { 3101 } else { 3100 }, example_output: Some(sample) }
                }
            } else {
                div { class: "showcase-browser-placeholder", "运行器进入视野后加载编辑器和终端…" }
            }
        }
    }
}
