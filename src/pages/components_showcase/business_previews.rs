//! Local examples for business components. The production controllers stay in components/.
use dioxus::prelude::*;

use crate::api::database::sql_console::SqlResult;
use crate::components::assets::asset_picker::{PickerAsset, PickerGallery, PickerSelectionFooter};
use crate::components::assets::asset_upload::{
    UploadDropZoneContent, UploadItem, UploadPanel, UploadStatus,
};
use crate::components::assets::AssetSelection;
use crate::components::code_runner::runner::RunnerExample;
use crate::components::code_runner::CodeRunner;
use crate::components::forms::{FormInput, INPUT_INLINE_CLASS};
use crate::components::sql_result_table::SqlResultTable;
use crate::components::ui::{ModalShell, Pagination};
use crate::router::Route;

pub(super) fn preview(slug: &str, detail: bool) -> Option<Element> {
    Some(match slug {
        "asset-picker-modal" => rsx! { PickerDemo { detail } },
        "asset-upload-modal" => rsx! { UploadDemo { detail } },
        "code-runner" => rsx! { RunnerDemo { detail } },
        "sql-result-table" => rsx! { SqlTableDemo { detail } },
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SqlPreset {
    Posts,
    Comments,
    Metrics,
}

fn sample_sql_result(preset: SqlPreset, detail: bool) -> (SqlResult, &'static str, &'static str) {
    match preset {
        SqlPreset::Posts => {
            let columns = if detail {
                vec![
                    "id".to_string(),
                    "title".to_string(),
                    "slug".to_string(),
                    "published".to_string(),
                    "views".to_string(),
                    "deleted_at".to_string(),
                ]
            } else {
                vec![
                    "id".to_string(),
                    "title".to_string(),
                    "published".to_string(),
                    "views".to_string(),
                ]
            };
            let mut rows = vec![
                if detail {
                    vec![
                        serde_json::json!(108),
                        serde_json::json!("让想法生根"),
                        serde_json::json!("roots-of-thought"),
                        serde_json::json!(true),
                        serde_json::json!(1280),
                        serde_json::Value::Null,
                    ]
                } else {
                    vec![
                        serde_json::json!(108),
                        serde_json::json!("让想法生根"),
                        serde_json::json!(true),
                        serde_json::json!(1280),
                    ]
                },
                if detail {
                    vec![
                        serde_json::json!(107),
                        serde_json::json!("在时间的缝隙里种下文字"),
                        serde_json::json!("writing-in-time"),
                        serde_json::json!(true),
                        serde_json::json!(856),
                        serde_json::Value::Null,
                    ]
                } else {
                    vec![
                        serde_json::json!(107),
                        serde_json::json!("在时间的缝隙里种下文字"),
                        serde_json::json!(true),
                        serde_json::json!(856),
                    ]
                },
                if detail {
                    vec![
                        serde_json::json!(106),
                        serde_json::json!("未命名草稿：微风与山岗"),
                        serde_json::json!("draft-whispering-wind"),
                        serde_json::json!(false),
                        serde_json::json!(42),
                        serde_json::Value::Null,
                    ]
                } else {
                    vec![
                        serde_json::json!(106),
                        serde_json::json!("未命名草稿：微风与山岗"),
                        serde_json::json!(false),
                        serde_json::json!(42),
                    ]
                },
            ];
            if detail {
                rows.push(vec![
                    serde_json::json!(105),
                    serde_json::json!("已归档随笔：初春记事"),
                    serde_json::json!("archived-early-notes"),
                    serde_json::json!(false),
                    serde_json::json!(310),
                    serde_json::json!("2026-09-01 14:20"),
                ]);
            }
            let query = if detail {
                "SELECT id, title, slug, published, views, deleted_at FROM posts ORDER BY id DESC LIMIT 4;"
            } else {
                "SELECT id, title, published, views FROM posts LIMIT 3;"
            };
            let stats = if detail {
                "4 行 · 12ms"
            } else {
                "3 行 · 8ms"
            };
            (
                SqlResult {
                    columns,
                    rows,
                    statement_type: "SELECT".to_string(),
                    ..Default::default()
                },
                query,
                stats,
            )
        }
        SqlPreset::Comments => {
            let columns = vec![
                "id".to_string(),
                "post_id".to_string(),
                "author".to_string(),
                "content".to_string(),
                "is_approved".to_string(),
                "created_at".to_string(),
            ];
            let rows = vec![
                vec![
                    serde_json::json!(34),
                    serde_json::json!(108),
                    serde_json::json!("叶子"),
                    serde_json::json!("风吹过的时候，树叶也在点头。"),
                    serde_json::json!(true),
                    serde_json::json!("2026-09-28 09:12"),
                ],
                vec![
                    serde_json::json!(33),
                    serde_json::json!(108),
                    serde_json::json!("林中客"),
                    serde_json::json!("文字很有力量，期待下一篇更新！"),
                    serde_json::json!(true),
                    serde_json::json!("2026-09-28 09:25"),
                ],
                vec![
                    serde_json::json!(32),
                    serde_json::json!(107),
                    serde_json::json!("待审访客"),
                    serde_json::json!(
                        "这是一条待审核留言，用于验证长文本在表格中的截断展示与点击展开详情效果。"
                    ),
                    serde_json::json!(false),
                    serde_json::json!("2026-09-27 22:40"),
                ],
            ];
            (
                SqlResult {
                    columns,
                    rows,
                    statement_type: "SELECT".to_string(),
                    ..Default::default()
                },
                "SELECT id, post_id, author, content, is_approved, created_at FROM comments ORDER BY id DESC;",
                "3 行 · 9ms",
            )
        }
        SqlPreset::Metrics => {
            let columns = vec![
                "metric".to_string(),
                "category".to_string(),
                "value".to_string(),
                "unit".to_string(),
                "healthy".to_string(),
                "updated_at".to_string(),
            ];
            let rows = vec![
                vec![
                    serde_json::json!("db_pool_active"),
                    serde_json::json!("database"),
                    serde_json::json!(8),
                    serde_json::json!("conns"),
                    serde_json::json!(true),
                    serde_json::json!("2026-09-28 09:50"),
                ],
                vec![
                    serde_json::json!("cache_hit_rate"),
                    serde_json::json!("memory"),
                    serde_json::json!(94.6),
                    serde_json::json!("%"),
                    serde_json::json!(true),
                    serde_json::json!("2026-09-28 09:50"),
                ],
                vec![
                    serde_json::json!("p99_latency"),
                    serde_json::json!("query"),
                    serde_json::json!(18.2),
                    serde_json::json!("ms"),
                    serde_json::json!(true),
                    serde_json::json!("2026-09-28 09:50"),
                ],
                vec![
                    serde_json::json!("disk_usage_warn"),
                    serde_json::json!("storage"),
                    serde_json::json!(88.4),
                    serde_json::json!("%"),
                    serde_json::json!(false),
                    serde_json::json!("2026-09-28 09:50"),
                ],
            ];
            (
                SqlResult {
                    columns,
                    rows,
                    statement_type: "SELECT".to_string(),
                    ..Default::default()
                },
                "SELECT metric, category, value, unit, healthy, updated_at FROM system_metrics;",
                "4 行 · 15ms",
            )
        }
    }
}

#[component]
fn SqlTableDemo(detail: bool) -> Element {
    let mut preset = use_signal(|| SqlPreset::Posts);
    let (result, query_text, stats_text) = sample_sql_result(preset(), detail);

    rsx! {
        div { class: if detail { "showcase-sql-demo showcase-sql-detail" } else { "showcase-sql-demo" },
            if detail {
                div { class: "showcase-demo-actions",
                    button {
                        r#type: "button",
                        aria_pressed: "{preset() == SqlPreset::Posts}",
                        onclick: move |_| preset.set(SqlPreset::Posts),
                        "文章数据 (posts)"
                    }
                    button {
                        r#type: "button",
                        aria_pressed: "{preset() == SqlPreset::Comments}",
                        onclick: move |_| preset.set(SqlPreset::Comments),
                        "评论数据 (comments)"
                    }
                    button {
                        r#type: "button",
                        aria_pressed: "{preset() == SqlPreset::Metrics}",
                        onclick: move |_| preset.set(SqlPreset::Metrics),
                        "系统指标 (metrics)"
                    }
                    span { "点击任意行展开跨列完整详情；布尔、数字、文本与 NULL 呈现差异化渲染。" }
                }
            }
            div { class: "showcase-sql-window",
                div { class: "showcase-sql-window-bar",
                    div { class: "showcase-sql-window-dots",
                        span { class: "dot-close" }
                        span { class: "dot-min" }
                        span { class: "dot-max" }
                    }
                    div { class: "showcase-sql-query-tag",
                        span { class: "opacity-50 mr-1 select-none", "yggdrasil=#" }
                        span { class: "truncate", "{query_text}" }
                    }
                    div { class: "showcase-sql-meta-badge",
                        span { class: "badge-type", "SELECT" }
                        span { class: "badge-stats", "{stats_text}" }
                    }
                }
                div { class: "showcase-sql-content",
                    div { key: "{preset() as usize}-{detail}",
                        SqlResultTable {
                            result,
                            initial_expanded: if detail && preset() == SqlPreset::Posts { Some(0) } else { None },
                        }
                    }
                }
                if detail {
                    div { class: "showcase-sql-window-foot",
                        span { "PostgreSQL 16.2 · UTF-8" }
                        span { "提示：点击左侧箭头或行内容展开 / 收起完整字段" }
                    }
                }
            }
        }
    }
}
