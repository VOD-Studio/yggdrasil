//! 素材管理（`/admin/assets`）。
//!
//! 标题行（图片数、已引用、未引用、可清理；「⋯」菜单里重建索引与清理未引用，主操作
//! 上传）→ 引用状态选项卡（带数量）、文件名 / 替代文本搜索与排序 → 缩略图网格与底栏
//! 分页；右侧详情栏查看大图、文件信息、替代文本、引用来源，复制链接或删除（被引用的
//! 禁止删除）。未引用的素材可勾选，底部浮起批量删除条。窄屏详情浮于网格上方。
//! 缩略图走 `serve_image` 的动态缩放（`?thumb=300x300`）。样式见
//! styles/pages/admin-assets.css。

use dioxus::prelude::*;
use std::collections::HashSet;

use crate::api::assets::AssetListResponse;
use crate::components::assets::AssetUploadModal;
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::forms::{FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::components::skeletons::assets_skeleton::AssetsGridSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, ConfirmDialog, FilterTabs, Icon,
    IconButton, IconName, LoadError, Menu, MenuItem, TablePager,
};
use crate::models::asset::{AssetFilter, AssetSort};
use crate::utils::format_bytes;

/// 每页素材数，与服务端 list.rs 的 PER_PAGE 对齐。
const ASSETS_PER_PAGE: i32 = 60;
/// 搜索输入停顿多久后提交（毫秒）。
const SEARCH_DEBOUNCE_MS: u32 = 300;
/// 详情栏常驻的最小视口宽度；更窄时只在点开素材后浮出。
#[cfg(target_arch = "wasm32")]
const DETAIL_DOCK_MIN_WIDTH: f64 = 1100.0;

fn parse_filter(value: &str) -> AssetFilter {
    match value {
        "used" => AssetFilter::Used,
        "orphan" => AssetFilter::Orphan,
        _ => AssetFilter::All,
    }
}

/// 视口够宽时详情栏常驻，首屏自动选中第一张。
#[cfg(target_arch = "wasm32")]
fn detail_docked() -> bool {
    web_sys::window()
        .and_then(|w| w.inner_width().ok())
        .and_then(|w| w.as_f64())
        .is_some_and(|w| w >= DETAIL_DOCK_MIN_WIDTH)
}

/// 素材管理入口组件。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn Assets() -> Element {
    let toast = use_toast();
    let mut filter = use_signal(|| "all".to_string());
    let mut query = use_signal(String::new);
    let mut debounced_query = use_signal(String::new);
    let mut sort = use_signal(|| AssetSort::CreatedDesc);
    let mut page = use_signal(|| 1_i32);
    let mut reload = use_signal(|| 0_u32);
    let mut data = use_signal(|| None::<AssetListResponse>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| false);
    // 详情栏里的素材与勾选（勾选跨翻页保留，删除成功后清空）。
    let mut active = use_signal(|| None::<String>);
    let mut checked = use_signal(HashSet::<String>::new);
    let mut confirm_delete = use_signal(|| false);
    let mut confirm_batch = use_signal(|| false);
    let mut confirm_purge = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut upload_open = use_signal(|| false);

    // 搜索防抖：停顿后才提交，并回到第 1 页。
    use_effect(move || {
        let q = query();
        spawn(async move {
            crate::utils::time::sleep_ms(SEARCH_DEBOUNCE_MS).await;
            if *query.peek() == q && *debounced_query.peek() != q {
                debounced_query.set(q);
                page.set(1);
            }
        });
    });

    use_effect(move || {
        let f = parse_filter(&filter());
        let q = debounced_query();
        let s = sort();
        let p = page();
        let _ = reload();
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            loading.set(true);
            match crate::api::assets::list_assets(f, q, s, p).await {
                Ok(resp) => {
                    // 详情里的素材不在本页时，宽屏换成第一张，窄屏收起。
                    let keep = active
                        .peek()
                        .as_ref()
                        .is_some_and(|id| resp.assets.iter().any(|a| &a.asset.id == id));
                    if !keep {
                        active.set(
                            resp.assets
                                .first()
                                .filter(|_| detail_docked())
                                .map(|a| a.asset.id.clone()),
                        );
                    }
                    data.set(Some(resp));
                    error.set(false);
                }
                Err(_) => error.set(true),
            }
            loading.set(false);
        });
    });

    // 详情大图换了之后重新绑定灯箱（data-lb-bound 守卫保证幂等）。灯箱脚本
    // P5 §8.2 起按需注入（本页 rsx 声明 LightboxAssets）。
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        if active().is_none() {
            return;
        }
        spawn(async move {
            crate::utils::time::sleep_ms(30).await;
            if let Some(window) = web_sys::window() {
                let selectors = js_sys::Array::of1(&".assets-lightbox".into());
                let value: wasm_bindgen::JsValue = js_sys::Object::from(selectors).into();
                let _ = js_sys::Reflect::set(&window, &"__lightboxSelectors".into(), &value);
                crate::utils::js::invoke_optional_global(&window, "__initLightbox", &[value]);
            }
        });
    });

    let mut refresh = move || reload += 1;
    let mut delete_one = move |id: String| {
        busy.set(true);
        spawn(async move {
            match crate::api::assets::delete_asset(id.clone()).await {
                Ok(res) if res.success => {
                    toast.success(res.message);
                    checked.write().remove(&id);
                    active.set(None);
                    refresh();
                }
                Ok(res) => toast.error(res.message),
                Err(e) => toast.error(format!("删除失败：{e}")),
            }
            busy.set(false);
        });
    };
    let mut delete_checked = move || {
        let ids: Vec<String> = checked.peek().iter().cloned().collect();
        busy.set(true);
        spawn(async move {
            match crate::api::assets::batch_delete_assets(ids).await {
                Ok(res) => {
                    let mut msg = res.message;
                    if res.freed_bytes > 0 {
                        msg.push_str(&format!("，释放 {}", format_bytes(res.freed_bytes)));
                    }
                    if res.failures > 0 {
                        msg.push_str(&format!("（{} 项失败）", res.failures));
                        toast.error(msg);
                    } else {
                        toast.success(msg);
                    }
                    checked.write().clear();
                    refresh();
                }
                Err(e) => toast.error(format!("批量删除失败：{e}")),
            }
            busy.set(false);
        });
    };
    let mut purge = move || {
        busy.set(true);
        spawn(async move {
            match crate::api::assets::purge_orphan_assets().await {
                Ok(res) => {
                    let mut msg = format!(
                        "已清理 {} 张未引用素材，释放 {}",
                        res.deleted_count,
                        format_bytes(res.freed_bytes)
                    );
                    if res.failures > 0 {
                        msg.push_str(&format!("（{} 个文件删除失败）", res.failures));
                    }
                    toast.success(msg);
                    refresh();
                }
                Err(e) => toast.error(format!("清理失败：{e}")),
            }
            busy.set(false);
        });
    };
    let mut rebuild = move || {
        busy.set(true);
        spawn(async move {
            match crate::api::assets::rebuild_assets_index().await {
                Ok(res) => {
                    toast.success(res.message);
                    refresh();
                }
                Err(e) => toast.error(format!("重建失败：{e}")),
            }
            busy.set(false);
        });
    };

    let resp = data.read().clone();
    let (assets, total, used, orphan, purgeable, purgeable_bytes) = match resp.as_ref() {
        Some(r) => (
            r.assets.clone(),
            r.total,
            r.used_count,
            r.orphan_count,
            r.purgeable_count,
            r.purgeable_bytes,
        ),
        None => (Vec::new(), 0, 0, 0, 0, 0),
    };
    let summary = resp.as_ref().map(|_| {
        let mut s = format!("{} 张图片 · 已引用 {used} · 未引用 {orphan}", used + orphan);
        if purgeable > 0 {
            s.push_str(&format!(" · 可清理 {purgeable}"));
        }
        s
    });
    let detail = active().and_then(|id| assets.iter().find(|a| a.asset.id == id).cloned());
    let selection = checked();
    let filtering = !debounced_query().is_empty();
    let mut menu = vec![MenuItem::new("rebuild", "重建索引").icon(IconName::RefreshCw)];
    let mut purge_item = MenuItem::new("purge", "清理未引用…")
        .icon(IconName::Trash)
        .danger()
        .separated();
    if purgeable == 0 {
        purge_item = purge_item.disabled("没有上传满 7 天的未引用素材");
    }
    menu.push(purge_item);

    rsx! {
        // P5 §8.2：素材详情灯箱按需注入（脚本未就绪时迟到调用入队兜底）。
        crate::components::enhance_assets::LightboxAssets {}
        div { class: "admin-assets", "data-detail": detail.is_some().then_some("true"),
            div { class: "admin-assets-main",
                div { class: "admin-page-head",
                    div {
                        h1 { "素材" }
                        p { if let Some(summary) = summary { "{summary}" } else { "\u{00a0}" } }
                    }
                    div { class: "admin-page-actions",
                        Menu {
                            label: if busy() { "处理中…" } else { "更多操作" },
                            size: ButtonSize::Md,
                            items: menu,
                            on_select: move |id: &'static str| match id {
                                "rebuild" => rebuild(),
                                "purge" => confirm_purge.set(true),
                                _ => {}
                            },
                        }
                        button {
                            class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                            r#type: "button",
                            onclick: move |_| upload_open.set(true),
                            Icon { name: IconName::Upload, size: 15 }
                            "上传"
                        }
                    }
                }

                div { class: "admin-toolbar",
                    FilterTabs {
                        class: "",
                        aria_label: "引用状态",
                        items: vec![("all", "全部"), ("used", "已引用"), ("orphan", "未引用")],
                        counts: if resp.is_some() { vec![used + orphan, used, orphan] } else { Vec::new() },
                        active_value: filter(),
                        on_change: move |v: String| {
                            filter.set(v);
                            page.set(1);
                        },
                    }
                    div { class: "admin-toolbar-filters",
                        label { class: "field field-sm admin-filter-search",
                            Icon { name: IconName::Search, size: 14 }
                            span { class: "sr-only", "搜索素材" }
                            input {
                                r#type: "search",
                                placeholder: "文件名或替代文本",
                                value: "{query}",
                                oninput: move |e| query.set(e.value()),
                                onkeydown: move |e| {
                                    if e.key() == Key::Escape && !query().is_empty() {
                                        e.prevent_default();
                                        query.set(String::new());
                                    }
                                },
                            }
                        }
                        FormSelect {
                            id: "admin-assets-sort",
                            value: sort(),
                            options: vec![(AssetSort::CreatedDesc, "最新上传"), (AssetSort::SizeDesc, "文件最大")],
                            trigger_class: FORM_SELECT_COMPACT_CLASS,
                            aria_label: "排序",
                            onchange: move |v: AssetSort| {
                                sort.set(v);
                                page.set(1);
                            },
                        }
                    }
                }

                if error() {
                    LoadError { title: "素材加载失败", on_retry: move |_| refresh() }
                } else if resp.is_none() {
                    AssetsGridSkeleton {}
                } else if assets.is_empty() {
                    div { class: "admin-panel",
                        if filtering {
                            EmptyState {
                                icon: IconName::SearchX,
                                title: "没有符合条件的素材",
                                description: "换个文件名或替代文本再试。",
                                action: EmptyStateAction {
                                    label: "清除搜索".to_string(),
                                    onclick: Callback::new(move |_| query.set(String::new())),
                                },
                            }
                        } else if filter() == "orphan" {
                            EmptyState {
                                icon: IconName::Image,
                                title: "没有未引用的素材",
                                description: "每张图片都有文章、笔记、评论或头像在用。",
                            }
                        } else {
                            EmptyState {
                                icon: IconName::Image,
                                title: "还没有素材",
                                description: "在编辑器里插入的图片，或在这里上传的图片，都会出现在这里。",
                                action: EmptyStateAction {
                                    label: "上传图片".to_string(),
                                    onclick: Callback::new(move |_| upload_open.set(true)),
                                },
                            }
                        }
                    }
                } else {
                    ul { class: "asset-grid", "aria-busy": loading().then_some("true"),
                        for item in assets.iter().cloned() {
                            card::AssetTile {
                                key: "{item.asset.id}",
                                active: active().as_deref() == Some(item.asset.id.as_str()),
                                checked: selection.contains(&item.asset.id),
                                on_open: move |id| active.set(Some(id)),
                                on_check: move |(id, on): (String, bool)| {
                                    if on {
                                        checked.write().insert(id);
                                    } else {
                                        checked.write().remove(&id);
                                    }
                                },
                                asset: item,
                            }
                        }
                    }
                    div { class: "admin-panel asset-pager",
                        TablePager {
                            page: page(),
                            total,
                            per_page: ASSETS_PER_PAGE,
                            unit: "张",
                            on_change: move |p| page.set(p),
                        }
                    }
                }

                if !selection.is_empty() {
                    div { class: "batch-bar", role: "toolbar", aria_label: "批量操作",
                        span { class: "batch-bar-count", "已选 {selection.len()} 张" }
                        button {
                            class: "{button_class(ButtonVariant::Ghost, ButtonSize::Sm)} batch-bar-danger",
                            r#type: "button",
                            disabled: busy(),
                            onclick: move |_| confirm_batch.set(true),
                            "删除所选"
                        }
                        span { class: "batch-bar-sep", aria_hidden: "true" }
                        IconButton {
                            icon: IconName::X,
                            label: "取消选择",
                            size: ButtonSize::Sm,
                            onclick: move |_| checked.write().clear(),
                        }
                    }
                }
            }

            if let Some(item) = detail.clone() {
                div { class: "asset-detail-slot",
                    references::AssetDetail {
                        key: "{item.asset.id}",
                        on_changed: move |_| refresh(),
                        on_delete: move |_| confirm_delete.set(true),
                        on_close: move |_| active.set(None),
                        asset: item,
                    }
                }
            }

            ConfirmDialog {
                visible: confirm_delete,
                title: "删除这张图片？",
                description: format!(
                    "「{}」会从素材库和磁盘上删除，无法恢复。",
                    detail.as_ref().map(|d| d.asset.filename.clone()).unwrap_or_default()
                ),
                confirm_label: "删除",
                on_confirm: move |_| {
                    if let Some(id) = active() {
                        delete_one(id);
                    }
                },
            }
            ConfirmDialog {
                visible: confirm_batch,
                title: "删除所选图片？",
                description: format!("{} 张未引用的图片会从素材库和磁盘上删除，无法恢复。", selection.len()),
                confirm_label: "删除",
                on_confirm: move |_| delete_checked(),
            }
            ConfirmDialog {
                visible: confirm_purge,
                title: "清理未引用的素材？",
                description: format!(
                    "{purgeable} 张上传满 7 天且没有任何引用的图片会被删除，释放 {}。上传不满 7 天的保留，以免误删还没保存的草稿里的图片。",
                    format_bytes(purgeable_bytes)
                ),
                confirm_label: "清理",
                on_confirm: move |_| purge(),
            }
            AssetUploadModal { visible: upload_open, on_uploaded: move |_| refresh() }
        }
    }
}

mod card;
mod references;
