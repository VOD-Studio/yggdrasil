//! 素材选择 modal（封面/头像「从素材库选择」、正文编辑器「素材库」插图共用）。
//!
//! 网格展示素材库（默认最新排序，支持文件名/alt 搜索）。
//! 单选模式（默认）：单击选中即回填并关闭，供封面/头像使用。
//! 多选模式（`multi: true`，正文插图）：单击切换勾选态（跨页/搜索保留），
//! 底部确认栏按勾选顺序批量回填。
//! 内嵌「上传新图」入口（复用 `upload_image_file`），上传成功后留在网格中供用户选择
//! （多选模式下自动加入选中集——上传即意图）。
//! 纯 Dioxus 组件，不触碰 Tiptap；数据加载仅在 WASM 前端发生。

use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, Icon, IconButton, IconName, ModalShell, TablePager,
};
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::assets::list_assets;
use crate::models::asset::AssetDto;
#[cfg(target_arch = "wasm32")]
use crate::models::asset::{AssetFilter, AssetSort};

/// 每页素材数，与素材管理页及服务端列表接口保持一致。
const ASSETS_PER_PAGE: i32 = 60;
/// 搜索输入防抖窗口，复用素材管理页的请求节流约定。
#[cfg(target_arch = "wasm32")]
const SEARCH_DEBOUNCE_MS: u32 = 300;

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
pub(crate) struct PickerAsset {
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
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn AssetPickerModal(
    mut visible: Signal<bool>,
    on_select: EventHandler<Vec<AssetSelection>>,
    cover_uploading: Signal<bool>,
    #[props(default = "选择封面图")] title: &'static str,
    #[props(default)] multi: bool,
) -> Element {
    #[allow(unused_mut)]
    let mut assets: Signal<Vec<AssetDto>> = use_signal(Vec::new);
    #[allow(unused_mut)]
    let mut loading = use_signal(|| false);
    let mut query = use_signal(String::new);
    #[allow(unused_mut)]
    let mut debounced_query = use_signal(String::new);
    #[allow(unused_mut)]
    let mut error = use_signal(|| None::<String>);
    let mut page = use_signal(|| 1_i32);
    #[allow(unused_mut)]
    let mut total = use_signal(|| 0_i64);
    // 上传中的本地对象 URL 只用于当前 modal 预览，上传结束立即释放。
    let mut uploading_preview = use_signal(|| None::<String>);
    // 关闭时保留 DOM 播放淡出；动画结束后再卸载，避免弹窗瞬间消失。
    let mut closing = use_signal(|| false);
    let mut opened = use_signal(|| false);
    // 上传完成后先把新图留在弹窗网格中，用户点击后才真正应用头像。
    let mut uploaded_url = use_signal(|| None::<String>);
    // 多选模式的选中集：按勾选顺序排列，跨页/搜索/上传保留；每次打开时重置。
    // 以 URL 判重（见 toggle_selection）。单选模式不使用。
    let mut selected: Signal<Vec<AssetSelection>> = use_signal(Vec::new);

    #[cfg(target_arch = "wasm32")]
    let request_generation = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(0_u64)));

    #[cfg(target_arch = "wasm32")]
    let request_generation_for_close = request_generation.clone();
    use_effect(move || {
        if visible() {
            opened.set(true);
            uploaded_url.set(None);
            // 每次打开都是一次全新的选择会话（取消或确认后关闭均不留旧勾选）。
            selected.set(Vec::new());
        } else if *opened.peek() {
            #[cfg(target_arch = "wasm32")]
            request_generation_for_close.set(request_generation_for_close.get().wrapping_add(1));
            page.set(1);
        }
    });

    // 搜索防抖：保留输入框原值，停顿 300ms 后才提交查询并回到第 1 页。
    // 每次新输入都会递增请求代际，立即丢弃尚未返回的旧查询结果。
    #[cfg(target_arch = "wasm32")]
    let request_generation_for_debounce = request_generation.clone();
    use_effect(move || {
        let q = query();
        #[cfg(target_arch = "wasm32")]
        {
            request_generation_for_debounce
                .set(request_generation_for_debounce.get().wrapping_add(1));
            spawn(async move {
                crate::utils::time::sleep_ms(SEARCH_DEBOUNCE_MS).await;
                if *query.peek() == q {
                    if *debounced_query.peek() != q {
                        debounced_query.set(q);
                    }
                    page.set(1);
                }
            });
        }
    });

    // 打开、搜索词或页码变化时加载当前页；请求代际保证只有最后一次响应能落地。
    #[cfg(target_arch = "wasm32")]
    let request_generation_for_load = request_generation.clone();
    use_effect(move || {
        let open = visible();
        let q = debounced_query();
        let requested_page = page();
        if open {
            #[cfg(target_arch = "wasm32")]
            {
                let request_generation = request_generation_for_load.clone();
                let request_id = request_generation.get().wrapping_add(1);
                request_generation.set(request_id);
                spawn(async move {
                    if request_generation.get() != request_id {
                        return;
                    }
                    loading.set(true);
                    error.set(None);
                    let result =
                        list_assets(AssetFilter::All, q, AssetSort::CreatedDesc, requested_page)
                            .await;
                    if request_generation.get() != request_id {
                        return;
                    }
                    match result {
                        Ok(resp) => {
                            let last_page = ((resp.total + ASSETS_PER_PAGE as i64 - 1)
                                / ASSETS_PER_PAGE as i64)
                                .max(1) as i32;
                            if resp.assets.is_empty() && requested_page > last_page {
                                page.set(last_page);
                                return;
                            }
                            assets.set(resp.assets);
                            total.set(resp.total);
                            error.set(None);
                        }
                        Err(e) => error.set(Some(e.to_string())),
                    }
                    if request_generation.get() == request_id {
                        loading.set(false);
                    }
                });
            }
        }
    });

    let current_page = page();
    let total_assets = total();
    let loading_now = loading();
    let show_pagination = total_assets > ASSETS_PER_PAGE as i64;
    let mut gallery_items: Vec<PickerAsset> = assets.read().iter().map(PickerAsset::from).collect();
    if let Some(url) = uploaded_url() {
        prepend_uploaded_asset(&mut gallery_items, url);
    }

    rsx! {
        ModalShell {
            visible,
            closing,
            title,
            overlay_padding: "p-4 sm:p-6",
            panel_class: "asset-modal asset-picker",

            div { class: "asset-modal-head",
                h2 { "{title}" }
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
            div { class: "asset-modal-bar",
                label { class: "field field-sm admin-filter-search asset-modal-search",
                    Icon { name: IconName::Search, size: 14 }
                    span { class: "sr-only", "搜索素材" }
                    input {
                        r#type: "search",
                        placeholder: "文件名或替代文本",
                        value: "{query}",
                        oninput: move |e| query.set(e.value()),
                    }
                }
                // 上传新图：成功后留在网格里，单选模式由用户再点一次确认。
                label {
                    class: "{button_class(ButtonVariant::Secondary, ButtonSize::Sm)} asset-modal-upload",
                    "aria-disabled": cover_uploading().then_some("true"),
                    Icon { name: IconName::Upload, size: 14 }
                    if cover_uploading() { "上传中…" } else { "上传新图片" }
                    input {
                        r#type: "file",
                        accept: "image/jpeg,image/png,image/gif,image/webp",
                        class: "sr-only",
                        disabled: cover_uploading(),
                            onchange: move |evt| {
                            #[cfg(target_arch = "wasm32")]
                            {
                                use dioxus::web::WebFileExt;
                                if cover_uploading() {
                                    return;
                                }
                                if let Some(file) = evt.files().into_iter().next() {
                    if let Some(web_file) = file.get_web_file() {
                        // 与 AssetUploadModal 的 worker 池入队校验（upload_pool::validate_file）
                        // 同一份规则：不合格直接提示、不发请求——此前这里只靠 accept 属性
                        // （客户端可绕过）+ 服务端兜底校验，缺了与批量上传一致的客户端预检。
                        if let Err(msg) = crate::components::assets::upload_pool::validate_file(
                        &web_file.type_(),
                        web_file.size() as u64,
                        ) {
                        error.set(Some(msg));
                        } else {
                        let preview_url =
                            web_sys::Url::create_object_url_with_blob(&web_file).ok();
                        uploading_preview.set(preview_url.clone());
                        cover_uploading.set(true);
                        error.set(None);
                        spawn(async move {
                            let result = crate::bridges::tiptap::upload_image_file(web_file)
                                .await;
                            if let Some(preview_url) = preview_url.as_deref() {
                                let _ = web_sys::Url::revoke_object_url(preview_url);
                            }
                            match result {
                                Ok(url) => {
                                    // 多选模式：上传即意图，新图直接进入选中集
                                    // （URL 判重，网格刷新后同图不会重复选中）。
                                    if multi {
                                        toggle_selection(
                                            &mut selected.write(),
                                            AssetSelection {
                                                url: url.clone(),
                                                alt: None,
                                            },
                                        );
                                    }
                                    uploaded_url.set(Some(url));
                                    uploading_preview.set(None);
                                    cover_uploading.set(false);
                                }
                                Err(msg) => {
                                    error.set(Some(msg));
                                    uploading_preview.set(None);
                                    cover_uploading.set(false);
                                }
                            }
                        });
                        }
                    }
                                }
                            }
                        },
                    }
                }
            }

                PickerGallery {
                    items: gallery_items.clone(),
                    selected,
                    multi,
                    loading: loading_now,
                    uploading: cover_uploading(),
                    error: error(),
                    uploading_preview: uploading_preview(),
                    on_pick: move |picks| {
                        on_select.call(picks);
                        closing.set(true);
                        visible.set(false);
                    },
                }
                if show_pagination {
                    div { class: "asset-modal-pager", "aria-busy": loading_now.then_some("true"),
                        TablePager {
                            page: current_page,
                            total: total_assets,
                            per_page: ASSETS_PER_PAGE,
                            unit: "张",
                            on_change: move |next: i32| {
                                if !loading() {
                                    page.set(next.max(1));
                                }
                            },
                        }
                    }
                }

                if multi {
                    PickerSelectionFooter { selected, on_pick: move |picks| {
                        on_select.call(picks);
                        closing.set(true);
                        visible.set(false);
                    } }
                }

        }
    }
}

/// The actual picker grid is shared; callers provide already loaded display rows.
#[component]
pub(crate) fn PickerGallery(
    items: Vec<PickerAsset>,
    mut selected: Signal<Vec<AssetSelection>>,
    multi: bool,
    loading: bool,
    uploading: bool,
    error: Option<String>,
    uploading_preview: Option<String>,
    on_pick: EventHandler<Vec<AssetSelection>>,
) -> Element {
    rsx! {
        div { class: "asset-modal-body", "aria-busy": loading.then_some("true"),
            if let Some(error) = error {
                p { class: "asset-modal-error", role: "alert", "加载失败：{error}" }
            } else if loading && items.is_empty() && !uploading {
                p { class: "asset-modal-empty", role: "status", "正在读取素材…" }
            } else if items.is_empty() && !uploading {
                p { class: "asset-modal-empty", "素材库还是空的，先上传一张图片。" }
            } else {
                ul { class: "asset-pick-grid",
                    if uploading {
                        li { class: "asset-pick", "data-uploading": "true", aria_live: "polite", aria_label: "正在上传图片",
                            span { class: "asset-pick-thumb",
                                if let Some(preview_url) = uploading_preview {
                                    img { src: "{preview_url}", alt: "" }
                                }
                                span { class: "asset-pick-progress", "上传中" }
                            }
                            span { class: "asset-pick-name", "新图片" }
                        }
                    }
                    for item in items.iter() {
                        {
                            let selection = AssetSelection { url: item.url.clone(), alt: item.alt.clone() };
                            let order = if multi { selected.read().iter().position(|value| value.url == item.url) } else { None };
                            rsx! {
                                li { key: "{item.url}",
                                    button {
                                        class: "asset-pick",
                                        r#type: "button",
                                        role: if multi { "checkbox" } else { "button" },
                                        aria_checked: multi.then(|| order.is_some().to_string()),
                                        "data-checked": order.is_some().then_some("true"),
                                        title: "{item.filename}",
                                        aria_label: "{item.filename}",
                                        onclick: move |_| {
                                            if multi {
                                                toggle_selection(&mut selected.write(), selection.clone());
                                            } else {
                                                on_pick.call(vec![selection.clone()]);
                                            }
                                        },
                                        span { class: "asset-pick-thumb",
                                            img { src: "{item.thumbnail}", alt: "", loading: "lazy" }
                                            if multi {
                                                span { class: "asset-pick-order", aria_hidden: "true",
                                                    if let Some(index) = order { "{index + 1}" }
                                                }
                                            }
                                            if item.fresh {
                                                span { class: "asset-pick-fresh", "刚上传" }
                                            }
                                        }
                                        span { class: "asset-pick-name", "{item.filename}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn PickerSelectionFooter(
    mut selected: Signal<Vec<AssetSelection>>,
    on_pick: EventHandler<Vec<AssetSelection>>,
) -> Element {
    let count = selected.read().len();
    rsx! {
        div { class: "asset-modal-foot",
            span {
                if count == 0 {
                    "可以多选，插入顺序与勾选顺序一致"
                } else {
                    "已选 {count} 张 · 按勾选顺序插入"
                }
            }
            div { class: "asset-modal-foot-actions",
                button {
                    class: button_class(ButtonVariant::Ghost, ButtonSize::Md),
                    r#type: "button",
                    disabled: count == 0,
                    onclick: move |_| selected.set(Vec::new()),
                    "清空"
                }
                button {
                    class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                    r#type: "button",
                    disabled: count == 0,
                    onclick: move |_| {
                        let picks = selected();
                        if !picks.is_empty() {
                            on_pick.call(picks);
                        }
                    },
                    if count == 0 { "插入" } else { "插入 {count} 张" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{prepend_uploaded_asset, toggle_selection, AssetSelection, PickerAsset};

    fn sel(url: &str) -> AssetSelection {
        AssetSelection {
            url: url.to_string(),
            alt: None,
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
        let mut items = vec![PickerAsset {
            url: "/uploads/a.webp".to_string(),
            thumbnail: "/uploads/a.webp?thumb=300x300".to_string(),
            filename: "a.webp".to_string(),
            alt: Some("封面".to_string()),
            fresh: false,
        }];
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
}
