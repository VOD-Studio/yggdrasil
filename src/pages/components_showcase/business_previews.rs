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
use crate::components::sql_result_table::SqlResultTable;
use crate::components::ui::{
    Button, ButtonSize, ButtonVariant, FilterTabs, Icon, IconButton, IconName, ModalShell,
    TablePager,
};

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
        "/images/samples/clay-circle.webp",
        "纸面与陶土.webp",
        "暖白纸面上的一轮陶土色圆",
    ),
    (
        "/images/samples/hairline-grid.webp",
        "细线网格.webp",
        "细线网格上连起的几个点",
    ),
    (
        "/images/samples/ink-dusk.webp",
        "夜色弧线.webp",
        "深色背景上层层展开的陶土色弧线",
    ),
    (
        "/images/samples/stacked.webp",
        "叠放的卡片.webp",
        "两张叠放的卡片",
    ),
    (
        "/images/samples/contours.webp",
        "等高线.webp",
        "起伏的等高线",
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

/// 演示弹窗头部：与正式弹窗同为 `.asset-modal-head`，关闭前多一个「恢复默认」。
#[component]
fn DemoModalHead(
    title: &'static str,
    on_reset: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "asset-modal-head",
            h2 { "{title}" }
            div { class: "flex items-center gap-1",
                Button { variant: ButtonVariant::Ghost, size: ButtonSize::Sm, icon: IconName::RotateCcw,
                    onclick: move |_| on_reset.call(()),
                    "恢复默认"
                }
                IconButton { icon: IconName::X, label: "关闭", size: ButtonSize::Sm, onclick: move |_| on_close.call(()) }
            }
        }
    }
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
                div { class: "showcase-demo-bar",
                    Button { variant: ButtonVariant::Secondary, size: ButtonSize::Sm,
                        onclick: move |_| { selected.set(Vec::new()); page.set(1); visible.set(true); },
                        "打开素材选择弹窗"
                    }
                    span { "本地演示，选择不会写入文章。" }
                }
            }
            if !visible() {
                div { class: "showcase-picker-panel",
                    PickerDemoContent { detail, selected, multi, query, page, confirmed, visible }
                }
            }
            if detail {
                ModalShell { visible, closing, title: "选择图片", overlay_padding: "p-4 sm:p-6", panel_class: "asset-modal asset-picker",
                    DemoModalHead { title: "选择图片",
                        on_reset: move |_| {
                            selected.set(sample_selected());
                            multi.set(true);
                            query.set(String::new());
                            page.set(1);
                            confirmed.set(Vec::new());
                            closing.set(true);
                            visible.set(false);
                        },
                        on_close: move |_| { closing.set(true); visible.set(false); },
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
    let mode = if multi() { "multi" } else { "single" };
    // 不带外框：卡片预览外包 .showcase-picker-panel，弹窗里直接排进 .asset-modal。
    rsx! {
        div { class: "asset-modal-bar",
            label { class: "field field-sm admin-filter-search asset-modal-search",
                Icon { name: IconName::Search, size: 14 }
                span { class: "sr-only", "搜索素材" }
                input {
                    r#type: "search",
                    placeholder: "文件名或替代文本",
                    value: "{query}",
                    oninput: move |e| { query.set(e.value()); page.set(1); },
                }
            }
            if detail {
                FilterTabs {
                    items: vec![("single", "单选"), ("multi", "多选")],
                    active_value: mode.to_string(),
                    on_change: move |value: String| { multi.set(value == "multi"); selected.set(Vec::new()); },
                    aria_label: "选择方式",
                    class: "",
                }
            }
        }
        PickerGallery { items, selected, multi: multi(), loading: false, uploading: false, error: None,
            uploading_preview: None,
            on_pick: move |picks: Vec<AssetSelection>| { confirmed.set(picks); visible.set(false); }
        }
        if total > 3 {
            div { class: "asset-modal-pager",
                TablePager { page: page(), total, per_page: 3, unit: "张",
                    on_change: move |next: i32| page.set(next.clamp(1, ((total + 2) / 3) as i32)),
                }
            }
        }
        if multi() {
            PickerSelectionFooter { selected, on_pick: move |picks: Vec<AssetSelection>| { confirmed.set(picks); visible.set(false); } }
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
                div { class: "showcase-demo-bar",
                    Button { variant: ButtonVariant::Secondary, size: ButtonSize::Sm, onclick: move |_| visible.set(true), "打开上传弹窗" }
                    Button { variant: ButtonVariant::Ghost, size: ButtonSize::Sm, onclick: move |_| { generation += 1; items.set(Vec::new()); }, "空状态" }
                    Button { variant: ButtonVariant::Ghost, size: ButtonSize::Sm, onclick: move |_| { generation += 1; items.set(sample_upload_items()); }, "代表状态" }
                    span { "本地演示，示例文件不会上传。" }
                }
            }
            if !visible() { UploadDemoPanel { items, generation } }
            if detail {
                ModalShell { visible, closing, title: "上传素材", panel_class: "asset-modal asset-uploader",
                    DemoModalHead { title: "上传素材",
                        on_reset: move |_| {
                            generation += 1;
                            items.set(sample_upload_items());
                            closing.set(true);
                            visible.set(false);
                        },
                        on_close: move |_| { closing.set(true); visible.set(false); },
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
                class: "asset-drop w-full border-0",
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
    let output_mode = if failed() { "error" } else { "ok" };
    rsx! {
        div { class: if detail { "showcase-runner-demo showcase-runner-detail" } else { "showcase-runner-demo" }, id: host_id,
            if detail {
                div { class: "showcase-demo-bar",
                    FilterTabs {
                        items: vec![("ok", "正常输出"), ("error", "错误输出")],
                        active_value: output_mode.to_string(),
                        on_change: move |value: String| failed.set(value == "error"),
                        aria_label: "输出示例",
                        class: "",
                    }
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
    let active = match preset() {
        SqlPreset::Posts => "posts",
        SqlPreset::Comments => "comments",
        SqlPreset::Metrics => "metrics",
    };

    rsx! {
        div { class: if detail { "showcase-sql-demo showcase-sql-detail" } else { "showcase-sql-demo" },
            if detail {
                div { class: "showcase-demo-bar",
                    FilterTabs {
                        items: vec![("posts", "文章"), ("comments", "评论"), ("metrics", "系统指标")],
                        active_value: active.to_string(),
                        on_change: move |value: String| preset.set(match value.as_str() {
                            "comments" => SqlPreset::Comments,
                            "metrics" => SqlPreset::Metrics,
                            _ => SqlPreset::Posts,
                        }),
                        aria_label: "示例查询",
                        class: "",
                    }
                    span { "点击任意行展开完整字段。" }
                }
            }
            // 与系统页 SQL 控制台的结果区同一结构。
            section { class: "admin-panel sys-section sys-result",
                div { class: "sys-section-head",
                    code { class: "showcase-sql-query", "{query_text}" }
                    span { "{stats_text}" }
                }
                div { key: "{preset() as usize}-{detail}",
                    SqlResultTable {
                        result,
                        initial_expanded: if detail && preset() == SqlPreset::Posts { Some(0) } else { None },
                    }
                }
            }
        }
    }
}
