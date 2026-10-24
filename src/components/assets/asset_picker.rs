//! 素材选择 modal（封面/头像「从素材库选择」、正文编辑器「素材库」插图共用）。
//!
//! 网格展示素材库（默认最新排序，支持文件名/alt 搜索）。
//! 单选模式（默认）：单击选中即回填并关闭，供封面/头像使用。
//! 多选模式（`multi: true`，正文插图）：单击切换勾选态（跨页/搜索保留），
//! 底部确认栏按勾选顺序批量回填。
//! 内嵌「上传新图」入口（复用 `upload_image_file`），上传成功后留在网格中供用户选择
//! （多选模式下自动加入选中集——上传即意图）。
//! 纯 Leptos 组件，不触碰 Tiptap；数据加载仅在浏览器端（Effect）发生。

use std::time::Duration;

use leptos::prelude::*;

use crate::api::assets::list_assets;
use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, Icon, IconButton, IconName, ModalShell, TablePager,
};
use crate::models::asset::{AssetDto, AssetFilter, AssetSort};
use crate::utils::error::failure_message;

/// 每页素材数，与素材管理页及服务端列表接口保持一致。
const ASSETS_PER_PAGE: i32 = 60;
/// 搜索输入防抖窗口，复用素材管理页的请求节流约定。
const SEARCH_DEBOUNCE_MS: u64 = 300;

/// 素材选中载荷：`on_select` 的回传单元。
///
/// 序列化形状即编辑器桥接契约 `[{ "src", "alt?" }]`（`insertImagesFromLibrary`），
/// 写字页确认后直接 `serde_json::to_string` 传给 JS 侧，无需二次映射。
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct AssetSelection {
    /// `/uploads/<path>` URL（序列化为编辑器 Image 节点的 `src`）。
    #[serde(rename = "src")]
    pub url: String,
    /// 图片替代文本（素材 alt）；None 时不序列化，JS 侧等同未提供。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
}

/// Grid data shared by the authenticated picker and the local showcase.
#[derive(Clone, PartialEq)]
pub struct PickerAsset {
    pub(crate) url: String,
    pub(crate) thumbnail: String,
    pub(crate) filename: String,
    pub(crate) alt: Option<String>,
    pub(crate) fresh: bool,
}

impl From<&AssetDto> for PickerAsset {
    fn from(dto: &AssetDto) -> Self {
        let url = format!("/uploads/{}", dto.asset.path);
        Self {
            thumbnail: format!("{url}?thumb=300x300"),
            url,
            filename: dto.asset.filename.clone(),
            alt: dto.asset.alt.clone(),
            fresh: false,
        }
    }
}

fn prepend_uploaded_asset(items: &mut Vec<PickerAsset>, url: String) {
    if items.iter().any(|item| item.url == url) {
        return;
    }
    items.insert(
        0,
        PickerAsset {
            thumbnail: url.clone(),
            url,
            filename: "新上传图片".to_string(),
            alt: None,
            fresh: true,
        },
    );
}

/// 切换素材勾选态：已选（按 URL 判重）则移除，未选则按点击顺序追加。
///
/// 以 URL 为唯一键：上传新图在网格刷新前后可能以不同条目形态出现
/// （`uploaded_url` 暂存 vs 素材列表行），URL 判重保证同图不会被选两次。
/// 抽成纯函数便于单元测试（见本文件 tests 模块）。
fn toggle_selection(selected: &mut Vec<AssetSelection>, item: AssetSelection) {
    if let Some(pos) = selected.iter().position(|s| s.url == item.url) {
        selected.remove(pos);
    } else {
        selected.push(item);
    }
}

/// 素材选择 modal。
///
/// - `visible`：显隐控制（父组件持有，选中/点遮罩/× 都会置 false）。
/// - `on_select`：选中回填。单选模式恰含一个元素；多选模式按勾选顺序排列。
/// - `cover_uploading`：modal 内上传新图时置位，供父页面拦截保存（与 CoverUploader 语义一致）。
/// - `title`：modal 标题；封面场景缺省为「选择封面图」，头像/正文场景各自传入。
/// - `multi`：多选模式（正文插图）。网格项变为勾选切换，底部确认栏批量回填。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub fn AssetPickerModal(
    visible: RwSignal<bool>,
    #[prop(into)] on_select: Callback<Vec<AssetSelection>>,
    cover_uploading: RwSignal<bool>,
    #[prop(default = "选择封面图".to_string(), into)] title: String,
    #[prop(optional)] multi: bool,
) -> impl IntoView {
    let assets = RwSignal::new(Vec::<AssetDto>::new());
    let loading = RwSignal::new(false);
    let query = RwSignal::new(String::new());
    let debounced_query = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let page = RwSignal::new(1_i32);
    let total = RwSignal::new(0_i64);
    // 上传中的本地对象 URL 只用于当前 modal 预览，上传结束立即释放。
    let uploading_preview = RwSignal::new(None::<String>);
    // 关闭时保留 DOM 播放淡出；动画结束后再卸载，避免弹窗瞬间消失。
    let closing = RwSignal::new(false);
    let opened = RwSignal::new(false);
    // 上传完成后先把新图留在弹窗网格中，用户点击后才真正应用头像。
    let uploaded_url = RwSignal::new(None::<String>);
    // 多选模式的选中集：按勾选顺序排列，跨页/搜索/上传保留；每次打开时重置。
    // 以 URL 判重（见 toggle_selection）。单选模式不使用。
    let selected = RwSignal::new(Vec::<AssetSelection>::new());

    // 请求代际：任何新输入 / 翻页 / 关闭都会递增，旧响应据此丢弃。
    let generation = StoredValue::new(0_u64);
    let bump_generation = move || {
        generation
            .try_update_value(|g| {
                *g = g.wrapping_add(1);
                *g
            })
            .unwrap_or(0)
    };
    let current_generation = move || generation.try_get_value().unwrap_or(0);
    // 防抖定时器句柄：新输入前清除旧的，卸载时清除。
    let debounce_timer = StoredValue::new(None::<TimeoutHandle>);

    Effect::new(move |_| {
        if visible.get() {
            opened.set(true);
            uploaded_url.set(None);
            // 每次打开都是一次全新的选择会话（取消或确认后关闭均不留旧勾选）。
            selected.set(Vec::new());
        } else if opened.get_untracked() {
            bump_generation();
            page.set(1);
        }
    });

    // 搜索防抖：保留输入框原值，停顿 300ms 后才提交查询并回到第 1 页。
    // 每次新输入都会递增请求代际，立即丢弃尚未返回的旧查询结果。
    Effect::new(move |_| {
        let q = query.get();
        bump_generation();
        if let Some(Some(old)) = debounce_timer.try_update_value(Option::take) {
            old.clear();
        }
        let handle = set_timeout_with_handle(
            move || {
                if query.try_get_untracked().as_deref() == Some(q.as_str()) {
                    if debounced_query.try_get_untracked().as_deref() != Some(q.as_str()) {
                        let _ = debounced_query.try_set(q);
                    }
                    let _ = page.try_set(1);
                }
            },
            Duration::from_millis(SEARCH_DEBOUNCE_MS),
        );
        if let Ok(handle) = handle {
            debounce_timer.set_value(Some(handle));
        }
    });

    // 打开、搜索词或页码变化时加载当前页；请求代际保证只有最后一次响应能落地。
    Effect::new(move |_| {
        let open = visible.get();
        let q = debounced_query.get();
        let requested_page = page.get();
        if !open {
            return;
        }
        let request_id = bump_generation();
        leptos::task::spawn_local(async move {
            if current_generation() != request_id {
                return;
            }
            let _ = loading.try_set(true);
            let _ = error.try_set(None);
            let result =
                list_assets(AssetFilter::All, q, AssetSort::CreatedDesc, requested_page).await;
            if current_generation() != request_id {
                return;
            }
            match result {
                Ok(resp) => {
                    let last_page = ((resp.total + ASSETS_PER_PAGE as i64 - 1)
                        / ASSETS_PER_PAGE as i64)
                        .max(1) as i32;
                    if resp.assets.is_empty() && requested_page > last_page {
                        let _ = page.try_set(last_page);
                        return;
                    }
                    let _ = assets.try_set(resp.assets);
                    let _ = total.try_set(resp.total);
                    let _ = error.try_set(None);
                }
                Err(e) => {
                    let _ = error.try_set(Some(failure_message("素材加载失败", &e)));
                }
            }
            if current_generation() == request_id {
                let _ = loading.try_set(false);
            }
        });
    });

    // 卸载：丢弃在途请求结果、清防抖定时器、释放尚未回收的预览对象 URL。
    on_cleanup(move || {
        bump_generation();
        if let Some(Some(timer)) = debounce_timer.try_update_value(Option::take) {
            timer.clear();
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(Some(preview_url)) = uploading_preview.try_get_untracked() {
            let _ = web_sys::Url::revoke_object_url(&preview_url);
        }
    });

    let show_pagination = move || total.get() > ASSETS_PER_PAGE as i64;
    let gallery_items = Memo::new(move |_| {
        let mut items: Vec<PickerAsset> =
            assets.with(|list| list.iter().map(PickerAsset::from).collect());
        if let Some(url) = uploaded_url.get() {
            prepend_uploaded_asset(&mut items, url);
        }
        items
    });

    let close = move || {
        closing.set(true);
        visible.set(false);
    };
    let pick_and_close = move |picks: Vec<AssetSelection>| {
        on_select.run(picks);
        close();
    };

    view! {
        <ModalShell
            visible=visible
            closing=closing
            title=title.clone()
            overlay_padding="p-4 sm:p-6"
            panel_class="asset-modal asset-picker"
        >
            <div class="asset-modal-head">
                <h2>{title.clone()}</h2>
                <IconButton
                    icon=IconName::X
                    label="关闭"
                    size=ButtonSize::Sm
                    onclick=move |_| close()
                />
            </div>
            <div class="asset-modal-bar">
                <label class="field field-sm admin-filter-search asset-modal-search">
                    <Icon name=IconName::Search size=14/>
                    <span class="sr-only">"搜索素材"</span>
                    <input
                        type="search"
                        placeholder="文件名或替代文本"
                        prop:value=move || query.get()
                        on:input=move |e| query.set(event_target_value(&e))
                    />
                </label>
                // 上传新图：成功后留在网格里，单选模式由用户再点一次确认。
                <label
                    class=format!(
                        "{} asset-modal-upload",
                        button_class(ButtonVariant::Secondary, ButtonSize::Sm),
                    )
                    aria-disabled=move || cover_uploading.get().then_some("true")
                >
                    <Icon name=IconName::Upload size=14/>
                    {move || if cover_uploading.get() { "上传中…" } else { "上传新图片" }}
                    <input
                        type="file"
                        accept="image/jpeg,image/png,image/gif,image/webp"
                        class="sr-only"
                        disabled=move || cover_uploading.get()
                        on:change=move |evt| {
                            #[cfg(target_arch = "wasm32")]
                            start_inline_upload(
                                &evt,
                                multi,
                                cover_uploading,
                                uploading_preview,
                                uploaded_url,
                                selected,
                                error,
                            );
                            #[cfg(not(target_arch = "wasm32"))]
                            let _ = evt;
                        }
                    />
                </label>
            </div>

            <PickerGallery
                items=gallery_items
                selected=selected
                multi=multi
                loading=loading
                uploading=cover_uploading
                error=error
                uploading_preview=uploading_preview
                on_pick=pick_and_close
            />
            <Show when=show_pagination>
                <div
                    class="asset-modal-pager"
                    aria-busy=move || loading.get().then_some("true")
                >
                    <TablePager
                        page=page
                        total=total
                        per_page=ASSETS_PER_PAGE
                        unit="张"
                        on_change=move |next: i32| {
                            if !loading.get_untracked() {
                                page.set(next.max(1));
                            }
                        }
                    />
                </div>
            </Show>

            <Show when=move || multi>
                <PickerSelectionFooter selected=selected on_pick=pick_and_close/>
            </Show>
        </ModalShell>
    }
}

/// 选择文件后的内嵌上传：预检 → 预览 → 上传 → 回填网格。仅浏览器端。
#[cfg(target_arch = "wasm32")]
fn start_inline_upload(
    evt: &web_sys::Event,
    multi: bool,
    cover_uploading: RwSignal<bool>,
    uploading_preview: RwSignal<Option<String>>,
    uploaded_url: RwSignal<Option<String>>,
    selected: RwSignal<Vec<AssetSelection>>,
    error: RwSignal<Option<String>>,
) {
    if cover_uploading.get_untracked() {
        return;
    }
    let Some(web_file) = event_target::<web_sys::HtmlInputElement>(evt)
        .files()
        .and_then(|list| list.item(0))
    else {
        return;
    };
    // 与 AssetUploadModal 的 worker 池入队校验（upload_pool::validate_file）同一份规则：
    // 不合格直接提示、不发请求，补上与批量上传一致的客户端预检。
    if let Err(msg) = super::upload_pool::validate_file(&web_file.type_(), web_file.size() as u64) {
        error.set(Some(format!("上传失败：{msg}")));
        return;
    }
    let preview_url = web_sys::Url::create_object_url_with_blob(&web_file).ok();
    uploading_preview.set(preview_url.clone());
    cover_uploading.set(true);
    error.set(None);
    leptos::task::spawn_local(async move {
        let result = crate::bridges::tiptap::upload_image_file(web_file).await;
        if let Some(preview_url) = preview_url.as_deref() {
            let _ = web_sys::Url::revoke_object_url(preview_url);
        }
        match result {
            Ok(url) => {
                // 多选模式：上传即意图，新图直接进入选中集
                // （URL 判重，网格刷新后同图不会重复选中）。
                if multi {
                    let _ = selected.try_update(|list| {
                        toggle_selection(
                            list,
                            AssetSelection {
                                url: url.clone(),
                                alt: None,
                            },
                        )
                    });
                }
                let _ = uploaded_url.try_set(Some(url));
                let _ = uploading_preview.try_set(None);
                let _ = cover_uploading.try_set(false);
            }
            Err(msg) => {
                let _ = error.try_set(Some(format!("上传失败：{msg}")));
                let _ = uploading_preview.try_set(None);
                let _ = cover_uploading.try_set(false);
            }
        }
    });
}

/// The actual picker grid is shared; callers provide already loaded display rows.
#[component]
pub(crate) fn PickerGallery(
    #[prop(into)] items: Signal<Vec<PickerAsset>>,
    selected: RwSignal<Vec<AssetSelection>>,
    multi: bool,
    #[prop(into)] loading: Signal<bool>,
    #[prop(into)] uploading: Signal<bool>,
    #[prop(into)] error: Signal<Option<String>>,
    #[prop(into)] uploading_preview: Signal<Option<String>>,
    #[prop(into)] on_pick: Callback<Vec<AssetSelection>>,
) -> impl IntoView {
    // 分支判别量用 Memo 去重：只有错误 / 读取中 / 空 / 网格切换时才重建主体，
    // 网格内部的增删与勾选由各自的响应式绑定原地更新。
    let branch = Memo::new(move |_| {
        let empty = items.with(Vec::is_empty);
        if error.with(Option::is_some) {
            0
        } else if loading.get() && empty && !uploading.get() {
            1
        } else if empty && !uploading.get() {
            2
        } else {
            3
        }
    });
    view! {
        <div class="asset-modal-body" aria-busy=move || loading.get().then_some("true")>
            {move || match branch.get() {
                0 => {
                    view! {
                        <p class="asset-modal-error" role="alert">
                            {move || error.get().unwrap_or_default()}
                        </p>
                    }
                        .into_any()
                }
                1 => {
                    view! {
                        <p class="asset-modal-empty" role="status">
                            "正在读取素材…"
                        </p>
                    }
                        .into_any()
                }
                2 => {
                    view! { <p class="asset-modal-empty">"素材库还是空的，先上传一张图片。"</p> }
                        .into_any()
                }
                _ => {
                    view! {
                        <ul class="asset-pick-grid">
                            {move || {
                                uploading
                                    .get()
                                    .then(|| {
                                        view! {
                                            <li
                                                class="asset-pick"
                                                data-uploading="true"
                                                aria-live="polite"
                                                aria-label="正在上传图片"
                                            >
                                                <span class="asset-pick-thumb">
                                                    {move || {
                                                        uploading_preview
                                                            .get()
                                                            .map(|preview_url| {
                                                                view! { <img src=preview_url alt=""/> }
                                                            })
                                                    }} <span class="asset-pick-progress">"上传中"</span>
                                                </span>
                                                <span class="asset-pick-name">"新图片"</span>
                                            </li>
                                        }
                                    })
                            }}
                            <For
                                each=move || items.get()
                                key=|item| {
                                    (
                                        item.url.clone(),
                                        item.thumbnail.clone(),
                                        item.filename.clone(),
                                        item.alt.clone(),
                                        item.fresh,
                                    )
                                }
                                children=move |item| {
                                    view! {
                                        <PickerItem
                                            item=item
                                            selected=selected
                                            multi=multi
                                            on_pick=on_pick
                                        />
                                    }
                                }
                            />
                        </ul>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

/// 网格中的单个素材：单选点击即回填，多选点击切换勾选并显示勾选序号。
#[component]
fn PickerItem(
    item: PickerAsset,
    selected: RwSignal<Vec<AssetSelection>>,
    multi: bool,
    on_pick: Callback<Vec<AssetSelection>>,
) -> impl IntoView {
    let PickerAsset {
        url,
        thumbnail,
        filename,
        alt,
        fresh,
    } = item;
    let selection = AssetSelection {
        url: url.clone(),
        alt,
    };
    let order = {
        let url = url.clone();
        move || {
            if multi {
                selected.with(|list| list.iter().position(|value| value.url == url))
            } else {
                None
            }
        }
    };
    let order_for_checked = order.clone();
    let order_for_aria = order.clone();
    let title_attr = filename.clone();
    let aria_label = filename.clone();
    view! {
        <li>
            <button
                class="asset-pick"
                type="button"
                role=if multi { "checkbox" } else { "button" }
                aria-checked=move || multi.then(|| order_for_aria().is_some().to_string())
                data-checked=move || order_for_checked().is_some().then_some("true")
                title=title_attr
                aria-label=aria_label
                on:click=move |_| {
                    if multi {
                        selected.update(|list| toggle_selection(list, selection.clone()));
                    } else {
                        on_pick.run(vec![selection.clone()]);
                    }
                }
            >
                <span class="asset-pick-thumb">
                    <img src=thumbnail alt="" loading="lazy"/>
                    {multi
                        .then(|| {
                            view! {
                                <span class="asset-pick-order" aria-hidden="true">
                                    {move || order().map(|index| (index + 1).to_string())}
                                </span>
                            }
                        })} {fresh.then(|| view! { <span class="asset-pick-fresh">"刚上传"</span> })}
                </span>
                <span class="asset-pick-name">{filename}</span>
            </button>
        </li>
    }
}

/// 多选模式底栏：已选计数、清空与确认插入。
#[component]
pub(crate) fn PickerSelectionFooter(
    selected: RwSignal<Vec<AssetSelection>>,
    #[prop(into)] on_pick: Callback<Vec<AssetSelection>>,
) -> impl IntoView {
    let count = move || selected.with(Vec::len);
    view! {
        <div class="asset-modal-foot">
            <span>
                {move || {
                    match count() {
                        0 => "可以多选，插入顺序与勾选顺序一致".to_string(),
                        n => format!("已选 {n} 张 · 按勾选顺序插入"),
                    }
                }}
            </span>
            <div class="asset-modal-foot-actions">
                <button
                    class=button_class(ButtonVariant::Ghost, ButtonSize::Md)
                    type="button"
                    disabled=move || count() == 0
                    on:click=move |_| selected.set(Vec::new())
                >
                    "清空"
                </button>
                <button
                    class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                    type="button"
                    disabled=move || count() == 0
                    on:click=move |_| {
                        let picks = selected.get_untracked();
                        if !picks.is_empty() {
                            on_pick.run(picks);
                        }
                    }
                >
                    {move || match count() {
                        0 => "插入".to_string(),
                        n => format!("插入 {n} 张"),
                    }}
                </button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(url: &str) -> AssetSelection {
        AssetSelection {
            url: url.to_string(),
            alt: None,
        }
    }

    fn picker_asset(url: &str, filename: &str) -> PickerAsset {
        PickerAsset {
            url: url.to_string(),
            thumbnail: format!("{url}?thumb=300x300"),
            filename: filename.to_string(),
            alt: Some("封面".to_string()),
            fresh: false,
        }
    }

    #[test]
    fn toggle_appends_in_click_order() {
        let mut selected = Vec::new();
        toggle_selection(&mut selected, sel("/uploads/b.webp"));
        toggle_selection(&mut selected, sel("/uploads/a.webp"));
        toggle_selection(&mut selected, sel("/uploads/c.webp"));
        let urls: Vec<&str> = selected.iter().map(|s| s.url.as_str()).collect();
        assert_eq!(
            urls,
            ["/uploads/b.webp", "/uploads/a.webp", "/uploads/c.webp"]
        );
    }

    #[test]
    fn toggle_removes_existing_by_url() {
        let mut selected = vec![sel("/uploads/a.webp"), sel("/uploads/b.webp")];
        toggle_selection(&mut selected, sel("/uploads/a.webp"));
        assert_eq!(selected, vec![sel("/uploads/b.webp")]);
    }

    #[test]
    fn toggle_dedupes_same_url_regardless_of_alt() {
        // 同一 URL 先以 alt=None（刚上传形态）入选，网格刷新后同图带 alt 再点：
        // 应移除（视为同一张图），而非追加重复条目。
        let mut selected = vec![sel("/uploads/a.webp")];
        toggle_selection(
            &mut selected,
            AssetSelection {
                url: "/uploads/a.webp".to_string(),
                alt: Some("封面".to_string()),
            },
        );
        assert!(selected.is_empty());
    }

    #[test]
    fn uploaded_asset_does_not_duplicate_a_loaded_grid_url() {
        let mut items = vec![picker_asset("/uploads/a.webp", "a.webp")];
        prepend_uploaded_asset(&mut items, "/uploads/a.webp".to_string());
        assert_eq!(items.len(), 1);
        prepend_uploaded_asset(&mut items, "/uploads/b.webp".to_string());
        assert_eq!(
            items
                .iter()
                .map(|item| item.url.as_str())
                .collect::<Vec<_>>(),
            ["/uploads/b.webp", "/uploads/a.webp"]
        );
    }

    #[test]
    fn selection_serializes_to_editor_bridge_shape() {
        // 桥接契约：insertImagesFromLibrary 的载荷为 [{"src","alt"?}]，
        // JS 侧 parseLibraryInsertItems 依赖字段名 src/alt 与 alt 缺省不序列化。
        let picks = vec![
            AssetSelection {
                url: "/uploads/a.webp".to_string(),
                alt: Some("封面".to_string()),
            },
            sel("/uploads/b.webp"),
        ];
        let json = serde_json::to_string(&picks).expect("AssetSelection 必然可序列化");
        assert_eq!(
            json,
            r#"[{"src":"/uploads/a.webp","alt":"封面"},{"src":"/uploads/b.webp"}]"#
        );
    }

    fn render_gallery(multi: bool, selected: Vec<AssetSelection>) -> String {
        Owner::new().with(|| {
            let items = RwSignal::new(vec![
                picker_asset("/uploads/a.webp", "a.webp"),
                PickerAsset {
                    fresh: true,
                    ..picker_asset("/uploads/b.webp", "新上传图片")
                },
            ]);
            let selected = RwSignal::new(selected);
            view! {
                <PickerGallery
                    items=items
                    selected=selected
                    multi=multi
                    loading=false
                    uploading=false
                    error={None::<String>}
                    uploading_preview={None::<String>}
                    on_pick=Callback::new(|_| {})
                />
            }
            .to_html()
        })
    }

    #[test]
    fn single_gallery_renders_button_role_and_fresh_badge() {
        let html = render_gallery(false, Vec::new());
        assert!(html.contains("role=\"button\""), "{html}");
        assert!(html.contains("asset-pick-fresh"), "{html}");
        assert!(!html.contains("asset-pick-order"), "{html}");
    }

    #[test]
    fn multi_gallery_marks_checked_items_with_order() {
        let html = render_gallery(true, vec![sel("/uploads/b.webp")]);
        assert!(html.contains("role=\"checkbox\""), "{html}");
        assert_eq!(html.matches("data-checked=\"true\"").count(), 1, "{html}");
        assert!(html.contains("aria-checked=\"true\""), "{html}");
        assert!(html.contains("aria-checked=\"false\""), "{html}");
    }

    #[test]
    fn gallery_shows_empty_and_error_states() {
        let html = Owner::new().with(|| {
            view! {
                <PickerGallery
                    items={Vec::<PickerAsset>::new()}
                    selected=RwSignal::new(Vec::new())
                    multi=false
                    loading=false
                    uploading=false
                    error={None::<String>}
                    uploading_preview={None::<String>}
                    on_pick=Callback::new(|_| {})
                />
            }
            .to_html()
        });
        assert!(html.contains("素材库还是空的"), "{html}");
        let html = Owner::new().with(|| {
            view! {
                <PickerGallery
                    items={Vec::<PickerAsset>::new()}
                    selected=RwSignal::new(Vec::new())
                    multi=false
                    loading=false
                    uploading=false
                    error=Some("素材加载失败，请稍后再试".to_string())
                    uploading_preview={None::<String>}
                    on_pick=Callback::new(|_| {})
                />
            }
            .to_html()
        });
        assert!(html.contains("role=\"alert\""), "{html}");
        assert!(html.contains("素材加载失败"), "{html}");
    }
}
