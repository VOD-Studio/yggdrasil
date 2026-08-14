//! 素材选择 modal（封面/头像「从素材库选择」、正文编辑器「素材库」插图共用）。
//!
//! 网格展示素材库（默认最新排序，支持文件名/alt 搜索）。
//! 单选模式（默认）：单击选中即回填并关闭，供封面/头像使用。
//! 多选模式（`multi: true`，正文插图）：单击切换勾选态（跨页/搜索保留），
//! 底部确认栏按勾选顺序批量回填。
//! 内嵌「上传新图」入口（复用 `upload_image_file`），上传成功后留在网格中供用户选择
//! （多选模式下自动加入选中集——上传即意图）。
//! 纯 Dioxus 组件，不触碰 Tiptap；数据加载仅在 WASM 前端发生。

use crate::components::forms::{FormInput, INPUT_INLINE_CLASS};
use crate::components::ui::{Pagination, BTN_ICON, BTN_OUTLINE, BTN_PRIMARY, SPINNER_SVG};
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
/// 关闭过渡时长，与 input.css 的 modal-overlay / modal-panel 过渡保持一致。
const EXIT_ANIM_MS: u32 = 200;

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
            closing.set(false);
            uploaded_url.set(None);
            // 每次打开都是一次全新的选择会话（取消或确认后关闭均不留旧勾选）。
            selected.set(Vec::new());
        } else if *opened.peek() {
            #[cfg(target_arch = "wasm32")]
            request_generation_for_close.set(request_generation_for_close.get().wrapping_add(1));
            page.set(1);
            if !*closing.peek() {
                closing.set(true);
            }
            spawn(async move {
                crate::utils::time::sleep_ms(EXIT_ANIM_MS).await;
                closing.set(false);
            });
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

    let is_closing = closing();
    if !visible() && !is_closing {
        return rsx! {};
    }
    let current_page = page();
    let total_assets = total();
    let loading_now = loading();
    let show_pagination = total_assets > ASSETS_PER_PAGE as i64;

    rsx! {
        // 遮罩：点击关闭
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm sm:p-6 modal-overlay animate-modal-overlay-enter",
            class: if is_closing { "is-closing" } else { "" },
            onclick: move |_| {
                closing.set(true);
                visible.set(false);
            },
            // 面板：阻止点击穿透到遮罩
            div {
                class: "flex max-h-[80vh] min-h-0 w-full max-w-3xl flex-col overflow-hidden rounded-[2rem] border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] shadow-xl modal-panel animate-modal-panel-enter",
                role: "dialog",
                aria_modal: "true",
                aria_label: "{title}",
                onclick: move |evt| evt.stop_propagation(),

                // 头部：标题、搜索、上传与关闭按钮共享同一组内边距。
                div { class: "flex flex-wrap items-center gap-3 border-b border-[var(--color-paper-border)] p-6",
                    h2 { class: "shrink-0 text-lg font-bold text-[var(--color-paper-primary)]",
                        "{title}"
                    }
                    FormInput {
                        r#type: "search",
                        placeholder: "搜索文件名 / alt",
                        value: query(),
                        class: INPUT_INLINE_CLASS,
                        oninput: move |v: String| query.set(v),
                    }
                    // 上传新图：成功后保留在当前网格，用户点击后才选中
                    label {
                        class: "inline-flex shrink-0 cursor-pointer items-center justify-center {BTN_PRIMARY}",
                        class: if cover_uploading() { "pointer-events-none cursor-wait opacity-60" } else { "" },
                        "上传新图"
                        input {
                            r#type: "file",
                            accept: "image/jpeg,image/png,image/gif,image/webp",
                            class: "hidden",
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
                                            let preview_url =
                                                web_sys::Url::create_object_url_with_blob(&web_file).ok();
                                            uploading_preview.set(preview_url.clone());
                                            cover_uploading.set(true);
                                            error.set(None);
                                            spawn(async move {
                                                let result = crate::tiptap_bridge::upload_image_file(web_file)
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
                            },
                        }
                    }
                    button {
                        class: "{BTN_ICON} shrink-0 rounded-full",
                        aria_label: "关闭",
                        onclick: move |_| {
                            closing.set(true);
                            visible.set(false);
                        },
                        "×"
                    }
                }

                // 网格内容区：与头部统一使用 p-6，min-h-0 保证面板内部滚动。
                div { class: "relative min-h-0 flex-1 overflow-y-auto p-6",
                    // 翻页时保留旧网格，仅叠加半透明加载态，避免内容闪空。
                    if loading_now && !assets.read().is_empty() {
                        div {
                            class: "pointer-events-none absolute inset-0 z-10 flex items-start justify-center bg-[var(--color-paper-theme)]/35 pt-6 backdrop-blur-[1px]",
                            aria_live: "polite",
                            aria_label: "正在加载素材",
                            div { class: "inline-flex items-center gap-2 rounded-full bg-[var(--color-paper-entry)] px-3 py-1.5 text-xs text-[var(--color-paper-secondary)] shadow-sm",
                                span {
                                    class: "inline-flex h-4 w-4",
                                    dangerous_inner_html: SPINNER_SVG,
                                }
                                "加载中…"
                            }
                        }
                    }
                    if let Some(err) = error() {
                        div { class: "rounded-2xl bg-red-500/10 px-4 py-3 text-center text-sm text-red-600 dark:text-red-400",
                            "加载失败：{err}"
                        }
                    } else if loading_now && assets.read().is_empty() && !cover_uploading() {
                        div { class: "px-4 py-16 text-center text-sm text-[var(--color-paper-secondary)]",
                            "加载中..."
                        }
                    } else if assets.read().is_empty() && !cover_uploading() && uploaded_url().is_none() {
                        div { class: "px-4 py-16 text-center text-sm text-[var(--color-paper-secondary)]",
                            "素材库为空，点击「上传新图」添加"
                        }
                    } else {
                        div { class: "grid grid-cols-3 gap-4 sm:grid-cols-4",
                            if cover_uploading() {
                                div {
                                    class: "relative aspect-square overflow-hidden rounded-2xl border border-[var(--color-paper-accent)]/60 bg-[var(--color-paper-theme)] shadow-sm",
                                    aria_live: "polite",
                                    aria_label: "正在上传图片",
                                    if let Some(preview_url) = uploading_preview() {
                                        img {
                                            class: "h-full w-full scale-110 object-cover blur-md opacity-60",
                                            src: "{preview_url}",
                                            alt: "正在上传",
                                        }
                                    } else {
                                        div { class: "absolute inset-0 animate-pulse bg-[var(--color-paper-code-bg)]" }
                                    }
                                    div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-black/30 backdrop-blur-[2px]",
                                        span {
                                            class: "inline-flex h-5 w-5 text-white",
                                            dangerous_inner_html: SPINNER_SVG,
                                        }
                                        span { class: "text-xs font-medium text-white drop-shadow",
                                            "上传中"
                                        }
                                    }
                                }
                            }
                            if let Some(uploaded_url) = uploaded_url() {
                                {
                                    let uploaded_sel = AssetSelection {
                                        url: uploaded_url.clone(),
                                        alt: None,
                                    };
                                    // 多选模式：勾选顺序即插入顺序，badge 展示 1 基序号。
                                    let uploaded_order = if multi {
                                        selected.read().iter().position(|s| s.url == uploaded_url)
                                    } else {
                                        None
                                    };
                                    rsx! {
                                        button {
                                            key: "uploaded-{uploaded_url}",
                                            class: "group relative aspect-square cursor-pointer overflow-hidden rounded-2xl bg-[var(--color-paper-theme)] shadow-sm transition-all hover:shadow-md",
                                            class: if multi {
                                                if uploaded_order.is_some() {
                                                    "border-2 border-[var(--color-paper-accent)]"
                                                } else {
                                                    "border-2 border-[var(--color-paper-border)] hover:border-[var(--color-paper-primary)]"
                                                }
                                            } else {
                                                "border-2 border-[var(--color-paper-accent)]"
                                            },
                                            title: if multi { "新上传图片，点击切换选中" } else { "新上传图片，点击使用" },
                                            onclick: move |_| {
                                                if multi {
                                                    toggle_selection(&mut selected.write(), uploaded_sel);
                                                } else {
                                                    on_select.call(vec![uploaded_sel]);
                                                    closing.set(true);
                                                    visible.set(false);
                                                }
                                            },
                                            img {
                                                class: "h-full w-full object-cover",
                                                src: "{uploaded_url}",
                                                alt: "新上传图片",
                                            }
                                            span { class: "absolute inset-x-2 bottom-2 rounded-full bg-black/55 px-2 py-1 text-center text-xs font-medium text-white backdrop-blur-sm",
                                                "刚上传"
                                            }
                                            if let Some(n) = uploaded_order {
                                                span { class: "absolute right-2 top-2 flex h-6 w-6 items-center justify-center rounded-full bg-[var(--color-paper-accent)] text-xs font-bold text-[var(--color-paper-theme)] shadow",
                                                    "{n + 1}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            for asset in assets.read().iter() {
                                {
                                    let url = format!("/uploads/{}", asset.asset.path);
                                    let thumb = format!("{}?thumb=300x300", url);
                                    let selection = AssetSelection {
                                        url: url.clone(),
                                        alt: asset.asset.alt.clone(),
                                    };
                                    // 多选模式：勾选顺序即插入顺序，badge 展示 1 基序号。
                                    let order = if multi {
                                        selected.read().iter().position(|s| s.url == url)
                                    } else {
                                        None
                                    };
                                    rsx! {
                                        button {
                                            key: "{asset.asset.id}",
                                            class: "group relative aspect-square cursor-pointer overflow-hidden rounded-2xl bg-[var(--color-paper-theme)] transition-all hover:shadow-md",
                                            class: if multi {
                                                if order.is_some() {
                                                    "border-2 border-[var(--color-paper-accent)] shadow-sm"
                                                } else {
                                                    "border-2 border-[var(--color-paper-border)] hover:border-[var(--color-paper-primary)]"
                                                }
                                            } else {
                                                "border border-[var(--color-paper-border)] hover:border-[var(--color-paper-primary)]"
                                            },
                                            title: "{asset.asset.filename}",
                                            onclick: move |_| {
                                                if multi {
                                                    toggle_selection(&mut selected.write(), selection);
                                                } else {
                                                    on_select.call(vec![selection]);
                                                    closing.set(true);
                                                    visible.set(false);
                                                }
                                            },
                                            img {
                                                class: "h-full w-full object-cover",
                                                src: "{thumb}",
                                                alt: asset.asset.alt.clone().unwrap_or_else(|| { asset.asset.filename.clone() }),
                                                loading: "lazy",
                                            }
                                            if let Some(n) = order {
                                                span { class: "absolute right-2 top-2 flex h-6 w-6 items-center justify-center rounded-full bg-[var(--color-paper-accent)] text-xs font-bold text-[var(--color-paper-theme)] shadow",
                                                    "{n + 1}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if show_pagination {
                    {
                        let mut page_for_prev = page;
                        let loading_for_prev = loading;
                        let mut page_for_next = page;
                        let loading_for_next = loading;
                        let mut page_for_jump = page;
                        let loading_for_jump = loading;
                        rsx! {
                            div { class: "shrink-0 px-6 py-3 shadow-[inset_0_1px_0_var(--color-paper-border)]",
                                div { class: if loading_now { "pointer-events-none opacity-60" } else { "" },
                                    Pagination {
                                        variant: "admin",
                                        compact: true,
                                        current_page,
                                        total: total_assets,
                                        per_page: ASSETS_PER_PAGE,
                                        unit: "张",
                                        on_prev: move |_| {
                                            if !loading_for_prev() {
                                                page_for_prev.with_mut(|p| *p = (*p - 1).max(1));
                                            }
                                        },
                                        on_next: move |_| {
                                            if !loading_for_next() {
                                                page_for_next.with_mut(|p| *p += 1);
                                            }
                                        },
                                        on_jump: move |next_page: i32| {
                                            if !loading_for_jump() {
                                                page_for_jump.set(next_page.max(1));
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }

                // 多选确认栏：计数 + 清空 + 批量插入（勾选顺序即插入顺序）。
                if multi {
                    {
                        let count = selected.read().len();
                        let count_label = if count == 0 {
                            "未选择图片".to_string()
                        } else {
                            format!("已选 {count} 张")
                        };
                        let confirm_label = if count == 0 {
                            "插入图片".to_string()
                        } else {
                            format!("插入 {count} 张图片")
                        };
                        rsx! {
                            div { class: "flex shrink-0 items-center justify-between gap-3 px-6 py-3 shadow-[inset_0_1px_0_var(--color-paper-border)]",
                                span { class: "text-sm text-[var(--color-paper-secondary)]",
                                    "{count_label}"
                                }
                                div { class: "flex items-center gap-2",
                                    button {
                                        class: "{BTN_OUTLINE}",
                                        class: if count == 0 { "pointer-events-none opacity-50" } else { "" },
                                        disabled: count == 0,
                                        onclick: move |_| selected.set(Vec::new()),
                                        "清空"
                                    }
                                    button {
                                        class: "{BTN_PRIMARY}",
                                        class: if count == 0 { "pointer-events-none opacity-50" } else { "" },
                                        disabled: count == 0,
                                        onclick: move |_| {
                                            let picks = selected();
                                            if picks.is_empty() {
                                                return;
                                            }
                                            on_select.call(picks);
                                            closing.set(true);
                                            visible.set(false);
                                        },
                                        "{confirm_label}"
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

#[cfg(test)]
mod tests {
    use super::{AssetSelection, toggle_selection};

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
        assert_eq!(urls, ["/uploads/b.webp", "/uploads/a.webp", "/uploads/c.webp"]);
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
