//! 素材管理（`/admin/assets`）。
//!
//! 标题行（图片数、已引用、未引用、可清理；「⋯」菜单里重建索引与清理未引用，主操作
//! 上传）→ 引用状态选项卡（带数量）、文件名 / 替代文本搜索与排序 → 缩略图网格与底栏
//! 分页；右侧详情栏查看大图、文件信息、替代文本、引用来源，复制链接或删除（被引用的
//! 禁止删除）。未引用的素材可勾选，底部浮起批量删除条。窄屏详情浮于网格上方。
//! 缩略图走 `serve_image` 的动态缩放（`?thumb=300x300`）。样式见
//! styles/pages/admin-assets.css。
//!
//! 后台数据不进 SSR：列表在浏览器端由 Effect 拉取（带请求世代号防竞态），直开页面时
//! 首屏是骨架屏，水合后再加载。

use crate::components::admin_layout::AdminPageHead;
use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos_router::{lazy_route, LazyRoute};
use std::collections::HashSet;

use crate::api::assets::AssetListResponse;
use crate::components::assets::AssetUploadModal;
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::enhance_assets::LightboxAssets;
use crate::components::forms::{FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::components::skeletons::assets_skeleton::AssetsGridSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, ConfirmDialog, Icon, IconButton, IconName,
    LoadError, Menu, MenuItem, SegmentedControl, SegmentedItem, TablePager,
};
use crate::models::asset::{AssetDto, AssetFilter, AssetSort};
use crate::utils::error::failure_message;
use crate::utils::format_bytes;

mod card;
mod references;

use card::AssetTile;
use references::AssetDetail;

/// 每页素材数，与服务端 list.rs 的 PER_PAGE 对齐。
const ASSETS_PER_PAGE: i32 = 60;
/// 搜索输入停顿多久后提交（毫秒）。
const SEARCH_DEBOUNCE_MS: u32 = 300;

/// 素材管理路由：纯客户端数据，无 SSR 预取。
pub struct AssetsRoute;

#[lazy_route]
impl LazyRoute for AssetsRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <Assets/> }.into_any()
    }
}

fn parse_filter(value: &str) -> AssetFilter {
    match value {
        "used" => AssetFilter::Used,
        "orphan" => AssetFilter::Orphan,
        _ => AssetFilter::All,
    }
}

/// 视口够宽时详情栏常驻，首屏自动选中第一张。只在 Effect 内调用；服务端恒为 `false`。
fn detail_docked() -> bool {
    /// 详情栏常驻的最小视口宽度；更窄时只在点开素材后浮出。
    #[cfg(target_arch = "wasm32")]
    const DETAIL_DOCK_MIN_WIDTH: f64 = 1100.0;

    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.inner_width().ok())
            .and_then(|w| w.as_f64())
            .is_some_and(|w| w >= DETAIL_DOCK_MIN_WIDTH)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// 在本页数据里按 id 找素材。
fn find_asset(data: &Option<AssetListResponse>, id: &str) -> Option<AssetDto> {
    data.as_ref()
        .and_then(|r| r.assets.iter().find(|a| a.asset.id == id))
        .cloned()
}

/// 列表区的互斥状态；用 `Memo` 去重，同一状态内的数据刷新不会重建网格。
#[derive(Clone, Copy, PartialEq)]
enum Body {
    Error,
    Skeleton,
    /// 有搜索词但没有命中。
    EmptyFiltered,
    /// 「未引用」选项卡下没有素材。
    EmptyOrphan,
    EmptyAll,
    Grid,
}

/// 素材管理入口组件。
#[component]
pub fn Assets() -> impl IntoView {
    let toast = use_toast();
    let filter = RwSignal::new("all".to_string());
    let query = RwSignal::new(String::new());
    let debounced_query = RwSignal::new(String::new());
    let sort = RwSignal::new(AssetSort::CreatedDesc);
    let page = RwSignal::new(1_i32);
    let reload = RwSignal::new(0_u32);
    let data = RwSignal::new(None::<AssetListResponse>);
    let loading = RwSignal::new(true);
    let error = RwSignal::new(false);
    // 详情栏里的素材与勾选（勾选跨翻页保留，删除成功后清空）。
    let active = RwSignal::new(None::<String>);
    let checked = RwSignal::new(HashSet::<String>::new());
    let confirm_delete = RwSignal::new(false);
    let confirm_batch = RwSignal::new(false);
    let confirm_purge = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let upload_open = RwSignal::new(false);

    // 搜索防抖：停顿后才提交，并回到第 1 页。
    Effect::new(move |_| {
        let q = query.get();
        leptos::task::spawn_local(async move {
            crate::utils::time::sleep_ms(SEARCH_DEBOUNCE_MS).await;
            if query.try_get_untracked().as_deref() == Some(q.as_str())
                && debounced_query.try_get_untracked().as_deref() != Some(q.as_str())
            {
                debounced_query.try_set(q);
                page.try_set(1);
            }
        });
    });

    // 请求世代号：筛选、翻页、刷新都会递增，只有最后一次请求的响应能落地。
    let generation = StoredValue::new(0_u64);
    let bump_generation = move || {
        generation
            .try_update_value(|g| {
                *g = g.wrapping_add(1);
                *g
            })
            .unwrap_or(0)
    };
    on_cleanup(move || {
        bump_generation();
    });

    Effect::new(move |_| {
        let f = parse_filter(&filter.get());
        let q = debounced_query.get();
        let s = sort.get();
        let p = page.get();
        let _ = reload.get();
        let request_id = bump_generation();
        leptos::task::spawn_local(async move {
            loading.try_set(true);
            let result = crate::api::assets::list_assets(f, q, s, p).await;
            if generation.try_get_value() != Some(request_id) {
                return;
            }
            match result {
                Ok(resp) => {
                    // 详情里的素材不在本页时，宽屏换成第一张，窄屏收起。
                    let keep = active.try_with_untracked(|id| {
                        id.as_ref()
                            .is_some_and(|id| resp.assets.iter().any(|a| &a.asset.id == id))
                    });
                    if keep == Some(false) {
                        active.try_set(
                            resp.assets
                                .first()
                                .filter(|_| detail_docked())
                                .map(|a| a.asset.id.clone()),
                        );
                    }
                    data.try_set(Some(resp));
                    error.try_set(false);
                }
                Err(_) => {
                    error.try_set(true);
                }
            }
            loading.try_set(false);
        });
    });

    // 详情大图换了之后重新绑定灯箱（data-lb-bound 守卫保证幂等）。灯箱脚本
    // P5 §8.2 起按需注入（本页声明 LightboxAssets）。
    Effect::new(move |_| {
        let has_active = active.get().is_some();
        #[cfg(target_arch = "wasm32")]
        if has_active {
            leptos::task::spawn_local(async move {
                crate::utils::time::sleep_ms(30).await;
                if let Some(window) = web_sys::window() {
                    let selectors = js_sys::Array::of1(&".assets-lightbox".into());
                    let value: wasm_bindgen::JsValue = js_sys::Object::from(selectors).into();
                    let _ = js_sys::Reflect::set(&window, &"__lightboxSelectors".into(), &value);
                    crate::utils::js::invoke_optional_global(&window, "__initLightbox", &[value]);
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = has_active;
    });

    let refresh = move || {
        reload.try_update(|n| *n += 1);
    };
    let delete_one = move |id: String| {
        busy.set(true);
        leptos::task::spawn_local(async move {
            match crate::api::assets::delete_asset(id.clone()).await {
                Ok(res) if res.success => {
                    toast.success(res.message);
                    checked.try_update(|c| {
                        c.remove(&id);
                    });
                    active.try_set(None);
                    refresh();
                }
                Ok(res) => toast.error(res.message),
                Err(e) => toast.error(failure_message("删除失败", &e)),
            }
            busy.try_set(false);
        });
    };
    let delete_checked = move || {
        let ids: Vec<String> = checked.get_untracked().iter().cloned().collect();
        busy.set(true);
        leptos::task::spawn_local(async move {
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
                    checked.try_update(HashSet::clear);
                    refresh();
                }
                Err(e) => toast.error(failure_message("批量删除失败", &e)),
            }
            busy.try_set(false);
        });
    };
    let purge = move || {
        busy.set(true);
        leptos::task::spawn_local(async move {
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
                Err(e) => toast.error(failure_message("清理失败", &e)),
            }
            busy.try_set(false);
        });
    };
    let rebuild = move || {
        busy.set(true);
        leptos::task::spawn_local(async move {
            match crate::api::assets::rebuild_assets_index().await {
                Ok(res) => {
                    toast.success(res.message);
                    refresh();
                }
                Err(e) => toast.error(failure_message("重建失败", &e)),
            }
            busy.try_set(false);
        });
    };

    // 列表统计；尚未加载时全为 0。
    let loaded = Memo::new(move |_| data.with(Option::is_some));
    let used = Memo::new(move |_| data.with(|d| d.as_ref().map_or(0, |r| r.used_count)));
    let orphan = Memo::new(move |_| data.with(|d| d.as_ref().map_or(0, |r| r.orphan_count)));
    let purgeable = Memo::new(move |_| data.with(|d| d.as_ref().map_or(0, |r| r.purgeable_count)));
    let purgeable_bytes =
        Memo::new(move |_| data.with(|d| d.as_ref().map_or(0, |r| r.purgeable_bytes)));
    let total = Memo::new(move |_| data.with(|d| d.as_ref().map_or(0, |r| r.total)));
    let summary = Memo::new(move |_| {
        loaded.get().then(|| {
            let (used, orphan, purgeable) = (used.get(), orphan.get(), purgeable.get());
            let mut s = format!("{} 张图片 · 已引用 {used} · 未引用 {orphan}", used + orphan);
            if purgeable > 0 {
                s.push_str(&format!(" · 可清理 {purgeable}"));
            }
            s
        })
    });
    // 详情栏只在素材仍在本页时显示；按 id 重建详情组件。
    let detail_id = Memo::new(move |_| {
        active.get().filter(|id| {
            data.with(|d| {
                d.as_ref()
                    .is_some_and(|r| r.assets.iter().any(|a| &a.asset.id == id))
            })
        })
    });
    let detail_name = Memo::new(move |_| {
        detail_id
            .get()
            .and_then(|id| data.with(|d| find_asset(d, &id)))
            .map(|a| a.asset.filename)
            .unwrap_or_default()
    });
    let selection_count = Memo::new(move |_| checked.with(HashSet::len));
    let has_selection = Memo::new(move |_| selection_count.get() > 0);

    let menu_items = Signal::derive(move || {
        let mut purge_item = MenuItem::new("purge", "清理未引用…")
            .icon(IconName::Trash)
            .danger()
            .separated();
        if purgeable.get() == 0 {
            purge_item = purge_item.disabled("没有上传满 7 天的未引用素材");
        }
        vec![
            MenuItem::new("rebuild", "重建索引").icon(IconName::RefreshCw),
            purge_item,
        ]
    });
    let filter_items = Signal::derive(move || {
        let loaded = loaded.get();
        let (used, orphan) = (used.get(), orphan.get());
        [
            ("all", "全部", used + orphan),
            ("used", "已引用", used),
            ("orphan", "未引用", orphan),
        ]
        .into_iter()
        .map(|(value, label, n)| {
            let item = SegmentedItem::new(value.to_string(), label);
            if loaded {
                item.count(n)
            } else {
                item
            }
        })
        .collect::<Vec<_>>()
    });

    let body = Memo::new(move |_| {
        if error.get() {
            Body::Error
        } else if !loaded.get() {
            Body::Skeleton
        } else if data.with(|d| d.as_ref().is_some_and(|r| r.assets.is_empty())) {
            if debounced_query.with(|q| !q.is_empty()) {
                Body::EmptyFiltered
            } else if filter.with(|f| f == "orphan") {
                Body::EmptyOrphan
            } else {
                Body::EmptyAll
            }
        } else {
            Body::Grid
        }
    });

    view! {
        // P5 §8.2：素材详情灯箱按需注入（脚本未就绪时迟到调用入队兜底）。
        <LightboxAssets/>
        <div class="admin-assets" data-detail=move || detail_id.get().map(|_| "true")>
            <div class="admin-assets-main">
                <AdminPageHead
                    title="素材"
                    actions=move || {
                        view! {
                            <Menu
                                label="更多操作"
                                size=ButtonSize::Md
                                items=menu_items
                                on_select=move |id: &'static str| match id {
                                    "rebuild" => rebuild(),
                                    "purge" => confirm_purge.set(true),
                                    _ => {}
                                }
                            />
                            <button
                                class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                                type="button"
                                on:click=move |_| upload_open.set(true)
                            >
                                <Icon name=IconName::Upload size=15/>
                                "上传"
                            </button>
                        }
                    }
                >
                    {move || summary.get()}
                </AdminPageHead>

                <div class="admin-toolbar">
                    <SegmentedControl
                        aria_label="引用状态"
                        items=filter_items
                        value=filter
                        on_change=move |v: String| {
                            filter.set(v);
                            page.set(1);
                        }
                    />
                    <div class="admin-toolbar-filters">
                        <label class="field field-sm admin-filter-search">
                            <Icon name=IconName::Search size=14/>
                            <span class="sr-only">"搜索素材"</span>
                            <input
                                type="search"
                                placeholder="文件名或替代文本"
                                prop:value=move || query.get()
                                on:input=move |e| query.set(event_target_value(&e))
                                on:keydown=move |e: KeyboardEvent| {
                                    if e.key() == "Escape" && query.with_untracked(|q| !q.is_empty()) {
                                        e.prevent_default();
                                        query.set(String::new());
                                    }
                                }
                            />
                        </label>
                        <FormSelect
                            id="admin-assets-sort"
                            value=sort
                            options=vec![
                                (AssetSort::CreatedDesc, "最新上传"),
                                (AssetSort::SizeDesc, "文件最大"),
                            ]
                            trigger_class=FORM_SELECT_COMPACT_CLASS
                            aria_label="排序"
                            onchange=move |v: AssetSort| {
                                sort.set(v);
                                page.set(1);
                            }
                        />
                    </div>
                </div>

                {move || match body.get() {
                    Body::Error => {
                        view! { <LoadError title="素材加载失败" on_retry=move |_| refresh()/> }
                            .into_any()
                    }
                    Body::Skeleton => view! { <AssetsGridSkeleton/> }.into_any(),
                    Body::EmptyFiltered => {
                        view! {
                            <div class="admin-panel">
                                <EmptyState
                                    icon=IconName::SearchX
                                    title="没有符合条件的素材"
                                    description="换个文件名或替代文本再试。"
                                    action=EmptyStateAction::new(
                                        "清除搜索",
                                        move |_| query.set(String::new()),
                                    )
                                />
                            </div>
                        }
                            .into_any()
                    }
                    Body::EmptyOrphan => {
                        view! {
                            <div class="admin-panel">
                                <EmptyState
                                    icon=IconName::Image
                                    title="没有未引用的素材"
                                    description="每张图片都有文章、笔记、评论或头像在用。"
                                />
                            </div>
                        }
                            .into_any()
                    }
                    Body::EmptyAll => {
                        view! {
                            <div class="admin-panel">
                                <EmptyState
                                    icon=IconName::Image
                                    title="还没有素材"
                                    description="在编辑器里插入的图片，或在这里上传的图片，都会出现在这里。"
                                    action=EmptyStateAction::new(
                                        "上传图片",
                                        move |_| upload_open.set(true),
                                    )
                                />
                            </div>
                        }
                            .into_any()
                    }
                    Body::Grid => {
                        view! {
                            <ul class="asset-grid" aria-busy=move || loading.get().then_some("true")>
                                <For
                                    each=move || {
                                        data.with(|d| {
                                            d.as_ref().map(|r| r.assets.clone()).unwrap_or_default()
                                        })
                                    }
                                    key=|item| item.asset.id.clone()
                                    children=move |item| {
                                        let id = item.asset.id.clone();
                                        // 同一素材的格子跨刷新复用，内容随列表更新；
                                        // 素材离开本页时 fallback 保证重算不会取空。
                                        let asset = Memo::new({
                                            let id = id.clone();
                                            move |_| {
                                                data.with(|d| find_asset(d, &id))
                                                    .unwrap_or_else(|| item.clone())
                                            }
                                        });
                                        let is_active = Signal::derive({
                                            let id = id.clone();
                                            move || {
                                                active.with(|a| a.as_deref() == Some(id.as_str()))
                                            }
                                        });
                                        let is_checked = Signal::derive(move || {
                                            checked.with(|c| c.contains(&id))
                                        });
                                        view! {
                                            <AssetTile
                                                asset=asset
                                                active=is_active
                                                checked=is_checked
                                                on_open=move |id: String| active.set(Some(id))
                                                on_check=move |(id, on): (String, bool)| {
                                                    checked
                                                        .update(|c| {
                                                            if on {
                                                                c.insert(id);
                                                            } else {
                                                                c.remove(&id);
                                                            }
                                                        });
                                                }
                                            />
                                        }
                                    }
                                />
                            </ul>
                            <div class="admin-panel asset-pager">
                                <TablePager
                                    page=page
                                    total=total
                                    per_page=ASSETS_PER_PAGE
                                    unit="张"
                                    on_change=move |p| page.set(p)
                                />
                            </div>
                        }
                            .into_any()
                    }
                }}

                {move || {
                    has_selection
                        .get()
                        .then(|| {
                            view! {
                                <div class="batch-bar" role="toolbar" aria-label="批量操作">
                                    <span class="batch-bar-count">
                                        {move || format!("已选 {} 张", selection_count.get())}
                                    </span>
                                    <button
                                        class=format!(
                                            "{} batch-bar-danger",
                                            button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                                        )
                                        type="button"
                                        disabled=move || busy.get()
                                        on:click=move |_| confirm_batch.set(true)
                                    >
                                        "删除所选"
                                    </button>
                                    <span class="batch-bar-sep" aria-hidden="true"></span>
                                    <IconButton
                                        icon=IconName::X
                                        label="取消选择"
                                        size=ButtonSize::Sm
                                        onclick=move |_| {
                                            checked.update(HashSet::clear);
                                        }
                                    />
                                </div>
                            }
                        })
                }}
            </div>

            {move || {
                detail_id
                    .get()
                    .and_then(|id| {
                        let fallback = data.with_untracked(|d| find_asset(d, &id))?;
                        let asset = Memo::new(move |_| {
                            data.with(|d| find_asset(d, &id)).unwrap_or_else(|| fallback.clone())
                        });
                        Some(
                            view! {
                                <div class="asset-detail-slot">
                                    <AssetDetail
                                        asset=asset
                                        on_changed=move |_| refresh()
                                        on_delete=move |_| confirm_delete.set(true)
                                        on_close=move |_| active.set(None)
                                    />
                                </div>
                            },
                        )
                    })
            }}

            <ConfirmDialog
                visible=confirm_delete
                title="删除这张图片？"
                description=move || {
                    format!("「{}」会从素材库和磁盘上删除，无法恢复。", detail_name.get())
                }
                confirm_label="删除"
                on_confirm=move |_| {
                    if let Some(id) = active.get_untracked() {
                        delete_one(id);
                    }
                }
            />
            <ConfirmDialog
                visible=confirm_batch
                title="删除所选图片？"
                description=move || {
                    format!(
                        "{} 张未引用的图片会从素材库和磁盘上删除，无法恢复。",
                        selection_count.get(),
                    )
                }
                confirm_label="删除"
                on_confirm=move |_| delete_checked()
            />
            <ConfirmDialog
                visible=confirm_purge
                title="清理未引用的素材？"
                description=move || {
                    format!(
                        "{} 张上传满 7 天且没有任何引用的图片会被删除，释放 {}。上传不满 7 天的保留，以免误删还没保存的草稿里的图片。",
                        purgeable.get(),
                        format_bytes(purgeable_bytes.get()),
                    )
                }
                confirm_label="清理"
                on_confirm=move |_| purge()
            />
            <AssetUploadModal visible=upload_open on_uploaded=move |_| refresh()/>
        </div>
    }
}
