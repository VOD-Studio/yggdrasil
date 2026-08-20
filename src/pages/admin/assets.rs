//! 素材管理页面。
//!
//! 网格浏览 `uploads/` 已登记图片：搜索（文件名/alt）、引用状态筛选
//! （全部/引用中/未引用）、排序（最新/最大）、客户端分页。
//! 「被 N 处引用」徽标可点击，弹出引用明细浮层（文章/评论/头像分组，
//! 含跳转链接与草稿/回收站状态徽标）。
//! 缩略图直接复用 `serve_image` 的动态处理（`?thumb=300x300`），零额外成本。
//!
//! 缩略图采用与前台正文图一致的 `.blur-img` 双层结构（`?w=20` 占位 + `data-src`
//! 展示层），点击由全局注入的 `lightbox.js` 接管为灯箱预览（图集模式，当前页内
//! 左右切换，灯箱加载原图 = `data-src` 去 query）。数据异步到达/刷新后由
//! `use_effect` 调 `__initLightbox` 绑定；TS 端有 `data-lb-bound` 守卫，重复绑定幂等。

use dioxus::prelude::*;

// server fn 仅在 WASM 前端调用（全部包在 cfg(wasm32) 块内），server SSR 只编译类型。
use crate::api::assets::AssetListResponse;
#[cfg(target_arch = "wasm32")]
use crate::api::assets::{
    batch_delete_assets, delete_asset, list_assets, purge_orphan_assets, rebuild_assets_index,
    update_asset_alt,
};
#[cfg(target_arch = "wasm32")]
use crate::api::assets::{BatchDeleteAssetsResponse, PurgeOrphansResponse, RebuildAssetsResponse};
use crate::components::assets::AssetUploadModal;
use crate::components::empty_state::EmptyState;
use crate::components::forms::FormInput;
use crate::components::skeletons::assets_skeleton::AssetsSkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::{FilterTabs, Pagination, Popover, BADGE_BASE, MEDIA_BADGE_BASE};
#[cfg(target_arch = "wasm32")]
use crate::models::asset::{AssetFilter, AssetSort};
use crate::models::asset::{AssetRef, AssetRefPostStatus};
use crate::router::Route;
use crate::utils::format_bytes;
#[cfg(target_arch = "wasm32")]
use crate::utils::js::invoke_optional_global;
use std::collections::HashSet;

/// 每页素材数，与服务端 list.rs 的 PER_PAGE 对齐。
const ASSETS_PER_PAGE: i32 = 60;

/// 搜索输入防抖窗口：停顿该时长后才提交查询，避免逐键发请求。
/// 仅在 wasm32 的防抖任务中引用，server 编译门控掉。
#[cfg(target_arch = "wasm32")]
const SEARCH_DEBOUNCE_MS: u32 = 300;

/// 操作横幅/批量条退出动画时长 ms，与 input.css 的 .animate-row-leave（200ms）一一对应；
/// 退出动画放完后由 spawn 真正摘除节点（仅 wasm 端的延时任务引用，server 编译门控掉）。
#[cfg(target_arch = "wasm32")]
const BANNER_EXIT_MS: u32 = 200;

// —— 引用明细浮层 ——

/// Material Symbols 图标 path（Google 原版存档见 public/icons/；
/// 组件内 fill=currentColor 随明暗主题）。
const ARTICLE_ICON_PATH: &str = "M277-279h275v-60H277v60Zm0-171h406v-60H277v60Zm0-171h406v-60H277v60Zm-97 501q-24 0-42-18t-18-42v-600q0-24 18-42t42-18h600q24 0 42 18t18 42v600q0 24-18 42t-42 18H180Zm0-60h600v-600H180v600Zm0-600v600-600Z";
const COMMENT_ICON_PATH: &str = "M880-80 720-240H140q-24 0-42-18t-18-42v-520q0-24 18-42t42-18h680q24 0 42 18t18 42v740ZM140-300h606l74 80v-600H140v520Zm0 0v-520 520Z";
const ACCOUNT_ICON_PATH: &str = "M222-255q63-44 125-67.5T480-346q71 0 133.5 23.5T739-255q44-54 62.5-109T820-480q0-145-97.5-242.5T480-820q-145 0-242.5 97.5T140-480q0 61 19 116t63 109Zm160.5-234.5Q343-529 343-587t39.5-97.5Q422-724 480-724t97.5 39.5Q617-645 617-587t-39.5 97.5Q538-450 480-450t-97.5-39.5ZM480-80q-82 0-155-31.5t-127.5-86Q143-252 111.5-325T80-480q0-83 31.5-155.5t86-127Q252-817 325-848.5T480-880q83 0 155.5 31.5t127 86q54.5 54.5 86 127T880-480q0 82-31.5 155t-86 127.5q-54.5 54.5-127 86T480-80Zm107.5-76Q640-172 691-212q-51-36-104-55t-107-19q-54 0-107 19t-104 55q51 40 103.5 56T480-140q55 0 107.5-16Zm-52-375.5Q557-553 557-587t-21.5-55.5Q514-664 480-664t-55.5 21.5Q403-621 403-587t21.5 55.5Q446-510 480-510t55.5-21.5ZM480-587Zm0 374Z";
const LINK_ICON_PATH: &str = "M450-280H280q-83 0-141.5-58.5T80-480q0-83 58.5-141.5T280-680h170v60H280q-58.33 0-99.17 40.76-40.83 40.77-40.83 99Q140-422 180.83-381q40.84 41 99.17 41h170v60ZM325-450v-60h310v60H325Zm185 170v-60h170q58.33 0 99.17-40.76 40.83-40.77 40.83-99Q820-538 779.17-579q-40.84-41-99.17-41H510v-60h170q83 0 141.5 58.5T880-480q0 83-58.5 141.5T680-280H510Z";
const OPEN_NEW_ICON_PATH: &str = "M180-120q-24 0-42-18t-18-42v-600q0-24 18-42t42-18h279v60H180v600h600v-279h60v279q0 24-18 42t-42 18H180Zm202-219-42-43 398-398H519v-60h321v321h-60v-218L382-339Z";

/// 浮层行内 Material Symbols 图标（标准 960 视口 + currentColor，明暗主题自适应）。
fn ref_icon(d: &'static str, class: &'static str) -> Element {
    rsx! {
        svg {
            class,
            xmlns: "http://www.w3.org/2000/svg",
            height: "24px",
            view_box: "0 -960 960 960",
            width: "24px",
            fill: "currentColor",
            path { d }
        }
    }
}

/// 引用明细浮层的一行：图标 + 文案 + 状态徽标 + 跳转。
///
/// 跳转规则：
/// - 已发布文章 → 原生 `<a target="_blank">` 新标签整页打开前台文章页
///   （规避 issue #36：admin→前台跨布局 Link + 目标页 suspend 触发 Dioxus 0.7
///   arena 双重回收 bug；新标签也不打断后台上下文）。
/// - 草稿/回收站文章 → 同 AdminLayout 的 WriteEdit 客户端导航（同布局链安全）；
///   页面随导航卸载，浮层随之销毁，无需显式关闭。
/// - 用户头像无管理页可链，纯展示；友链 → /admin/friends（同布局客户端导航）。
fn render_ref_row(r: &AssetRef, close: Callback<()>) -> Element {
    const ICON_CLASS: &str = "w-4 h-4 shrink-0 text-[var(--color-paper-tertiary)]";
    const ROW_CLASS: &str = "flex items-center gap-2.5 -mx-1.5 px-1.5 py-1.5 rounded-xl hover:bg-[var(--color-paper-theme)] transition-colors group/row";
    const TEXT_CLASS: &str = "flex-1 min-w-0 truncate text-sm text-[var(--color-paper-primary)]";
    const OPEN_CLASS: &str = "w-3.5 h-3.5 shrink-0 text-[var(--color-paper-tertiary)] opacity-0 group-hover/row:opacity-100 transition-opacity";
    match r {
        AssetRef::Post {
            post_id,
            title,
            slug,
            status,
        } => {
            let content = rsx! {
                {ref_icon(ARTICLE_ICON_PATH, ICON_CLASS)}
                span { class: TEXT_CLASS, "{title}" }
                if let Some((badge_label, badge_class)) = status.badge() {
                    span { class: "{BADGE_BASE} shrink-0 {badge_class}", "{badge_label}" }
                }
            };
            if *status == AssetRefPostStatus::Published {
                rsx! {
                    a {
                        class: ROW_CLASS,
                        href: "/post/{slug}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        title: "新标签打开文章页",
                        onclick: move |_| close(()),
                        {content}
                        {ref_icon(OPEN_NEW_ICON_PATH, OPEN_CLASS)}
                    }
                }
            } else {
                rsx! {
                    Link {
                        class: ROW_CLASS,
                        to: Route::WriteEdit { id: *post_id },
                        {content}
                    }
                }
            }
        }
        AssetRef::Comment {
            author_name,
            post_id,
            post_title,
            post_slug,
            post_status,
            ..
        } => {
            let content = rsx! {
                {ref_icon(COMMENT_ICON_PATH, ICON_CLASS)}
                span { class: TEXT_CLASS,
                    span { class: "text-[var(--color-paper-secondary)]", "{author_name} 在 " }
                    "《{post_title}》"
                }
                if let Some((badge_label, badge_class)) = post_status.badge() {
                    span { class: "{BADGE_BASE} shrink-0 {badge_class}", "{badge_label}" }
                }
            };
            if *post_status == AssetRefPostStatus::Published {
                rsx! {
                    a {
                        class: ROW_CLASS,
                        href: "/post/{post_slug}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        title: "新标签打开所属文章",
                        onclick: move |_| close(()),
                        {content}
                        {ref_icon(OPEN_NEW_ICON_PATH, OPEN_CLASS)}
                    }
                }
            } else {
                rsx! {
                    Link {
                        class: ROW_CLASS,
                        to: Route::WriteEdit { id: *post_id },
                        {content}
                    }
                }
            }
        }
        AssetRef::UserAvatar { label, .. } => rsx! {
            div { class: "flex items-center gap-2.5 -mx-1.5 px-1.5 py-1.5 rounded-xl",
                {ref_icon(ACCOUNT_ICON_PATH, ICON_CLASS)}
                span { class: TEXT_CLASS,
                    span { class: "text-[var(--color-paper-secondary)]", "用户头像 · " }
                    "{label}"
                }
            }
        },
        AssetRef::FriendAvatar { name, .. } => rsx! {
            Link {
                class: ROW_CLASS,
                to: Route::FriendsAdmin {},
                {ref_icon(LINK_ICON_PATH, ICON_CLASS)}
                span { class: TEXT_CLASS,
                    span { class: "text-[var(--color-paper-secondary)]", "友链头像 · " }
                    "{name}"
                }
            }
        },
    }
}

/// 素材管理入口组件。
// 交互逻辑全部 cfg(wasm32) 门控，server SSR 编译时一批绑定未使用，按 CoverUploader 惯例放行。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
#[component]
pub fn Assets() -> Element {
    // 筛选/搜索/排序/分页状态：全部客户端驱动（单路由 + signal，对齐「全部文章」模式）。
    let mut filter = use_signal(|| "all".to_string());
    let mut query = use_signal(String::new);
    let mut sort = use_signal(|| "created".to_string());
    let mut page = use_signal(|| 1_i32);

    #[allow(unused_mut)]
    let mut data: Signal<Option<AssetListResponse>> = use_signal(|| None);
    #[allow(unused_mut)]
    let mut loading: Signal<bool> = use_signal(|| true);
    #[allow(unused_mut)]
    let mut error: Signal<Option<String>> = use_signal(|| None);

    // 操作结果横幅（删除/清理/alt 编辑的反馈）。
    #[allow(unused_mut)]
    let mut op_message: Signal<Option<String>> = use_signal(|| None);
    // 操作横幅退出动画态：× 后置 true 播 .animate-row-leave，BANNER_EXIT_MS 后真正摘除。
    #[allow(unused_mut)]
    let mut op_closing = use_signal(|| false);
    // 待二次确认的删除目标（素材 id）与一键清理确认态。
    let mut confirm_delete: Signal<Option<String>> = use_signal(|| None);
    let mut purge_confirm = use_signal(|| false);
    // 重建索引进行中状态。
    let mut rebuilding = use_signal(|| false);
    // alt 内联编辑：目标素材 id + 输入框值。
    let mut editing_alt: Signal<Option<String>> = use_signal(|| None);
    let mut alt_input = use_signal(String::new);
    // 重载触发器：操作成功后 +1 让 effect 重新请求。
    let mut reload = use_signal(|| 0_i32);
    // 已加载视图标识（filter|query|sort|page）：fetch 成功后更新，作为网格 key。
    // 同视图内的 reload（删除/上传/alt/重建/清理）不改变它 → 走 keyed diff 不重播动画。
    let mut loaded_view = use_signal(String::new);
    // 多选：选中素材 id 集合 + 批量删除确认态。仅未引用素材可选（被引用的禁删，
    // 与单删保护语义一致）；选择跨翻页/筛选保留，批量删除成功后整体清空。
    let mut selected_ids: Signal<HashSet<String>> = use_signal(HashSet::new);
    let mut batch_confirm = use_signal(|| false);
    // 批量操作条退出动画态：选择清空后置 true 播 .animate-row-leave，延迟真正清空
    // selected_ids，使退出动画期间内容不退化为"已选 0 张"（保持消失前最后一帧）。
    let mut batch_closing = use_signal(|| false);
    // 页内上传 modal 显隐。
    let mut upload_open = use_signal(|| false);
    // 引用明细浮层：打开的素材 id + 视口锚点坐标 + 水平对齐（点击时按视口宽度预算）。
    let mut refs_popover: Signal<Option<(String, i32, i32, &'static str)>> = use_signal(|| None);
    // —— 操作横幅与批量条的进出动画 ——
    // 出现走 .animate-row-enter（挂载即播）；消失先置 closing=true 切到 .animate-row-leave，
    // BANNER_EXIT_MS 后由 spawn 真正摘除。peek 门控：摘除前若被新消息/新选择覆盖（closing
    // 已被重置为 false）则放弃摘除，避免覆盖掉用户刚触发的新内容。
    // 三 helper 用 Callback（Copy，可在多处 onclick 中重复引用）；普通闭包因 Signal
    // 的 set(&mut self) 而成为 FnMut、非 Copy，无法跨 move 处理器共享。
    // show_op(String)：弹新消息（先重置 closing，取消上一条还在播的退出动画）。
    let show_op = Callback::new(move |msg: String| {
        op_closing.set(false);
        op_message.set(Some(msg));
    });
    // dismiss_op(())：× 收起操作横幅（播退出动画，延迟摘除；peek 防覆盖）。
    let dismiss_op = Callback::new(move |()| {
        op_closing.set(true);
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            crate::utils::time::sleep_ms(BANNER_EXIT_MS).await;
            if *op_closing.peek() {
                op_message.set(None);
                op_closing.set(false);
            }
        });
    });
    // dismiss_batch(())：选择清空时收起批量条（延迟清空 selected_ids，使退出动画
    // 期间内容不退化为"已选 0 张"；peek 防新选择覆盖刚发起的收起）。
    let dismiss_batch = Callback::new(move |()| {
        batch_closing.set(true);
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            crate::utils::time::sleep_ms(BANNER_EXIT_MS).await;
            if *batch_closing.peek() {
                selected_ids.set(HashSet::new());
                batch_confirm.set(false);
                batch_closing.set(false);
            }
        });
    });

    // dismiss_refs(())：关闭引用明细浮层（遮罩/Escape/行内跳转共用）。
    let dismiss_refs = Callback::new(move |()| {
        refs_popover.set(None);
    });
    // 搜索防抖：query 是输入框原始值（受控绑定），debounced_query 才是请求参数。
    // 停顿 300ms 无新输入才提交；每次击键重启本 effect 并新 spawn 一个延时任务，
    // 旧任务醒来后用 peek 比对当前 query，已过期则静默丢弃。提交时连带重置页码——
    // 若放在 oninput 里，page 变化会立刻用旧关键词抢发一次请求。
    let mut debounced_query = use_signal(String::new);
    use_effect(move || {
        let q = query();
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            crate::utils::time::sleep_ms(SEARCH_DEBOUNCE_MS).await;
            if *query.peek() == q && *debounced_query.peek() != q {
                debounced_query.set(q);
                page.set(1);
            }
        });
    });

    // 数据加载：任一查询条件或 reload 变化时重新请求。筛选/搜索/排序变化时重置到第 1 页。
    use_effect(move || {
        let f = filter();
        let q = debounced_query();
        let s = sort();
        let p = page();
        let _ = reload();

        #[cfg(target_arch = "wasm32")]
        {
            let filter_enum = match f.as_str() {
                "used" => AssetFilter::Used,
                "orphan" => AssetFilter::Orphan,
                _ => AssetFilter::All,
            };
            let sort_enum = if s == "size" {
                AssetSort::SizeDesc
            } else {
                AssetSort::CreatedDesc
            };
            // 视图 key 在 spawn 前构造（q 随后被 move 进 list_assets）。reload 故意不参与：
            // 同视图重操作只 diff 网格，不重播入场动画。
            let view_key = format!("{f}|{q}|{s}|{p}");
            // 任何重新取数都关闭引用明细浮层（引用数据可能已变化）。
            refs_popover.set(None);
            spawn(async move {
                loading.set(true);
                error.set(None);
                match list_assets(filter_enum, q, sort_enum, p).await {
                    Ok(resp) => {
                        // 先 key 后数据：即使两次 set 未被批处理，网格也只在最终
                        // key 下挂载一次，不会 "" → 正式 key 二次挂载闪双动画。
                        loaded_view.set(view_key);
                        data.set(Some(resp));
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
                loading.set(false);
            });
        }
    });

    // 灯箱初始化：lightbox.js 由 Dioxus.toml 全局注入。网格随数据异步渲染，
    // 需在数据到达（DOM 提交后）调 __initLightbox 绑定；筛选/翻页/刷新重建节点后
    // 重跑此 effect 重新绑定。TS 端 data-lb-bound 守卫保证重复绑定幂等。
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        // 订阅 data：取数/刷新后网格重渲染，effect 在 DOM 更新后运行。
        if data.read().is_none() {
            return;
        }
        let window =
            web_sys::window().expect("assets use_effect 仅在 WASM 浏览器上下文执行：无 window");
        // 双保险契约（同 PostContent）：先设全局配置，lightbox.js 若尚未加载完，
        // 其 IIFE 尾部读到配置自启动；已加载则下方直接调用兜底。
        let selectors = js_sys::Array::of1(&".assets-lightbox".into());
        let selectors_val = js_sys::Object::from(selectors).into();
        let _ = js_sys::Reflect::set(&window, &"__lightboxSelectors".into(), &selectors_val);
        invoke_optional_global(&window, "__initLightbox", &[selectors_val]);
    });

    let resp = data.read();
    let (assets, total, used_count, orphan_count, purgeable_count, purgeable_bytes) =
        match resp.as_ref() {
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
    let all_count = used_count + orphan_count;
    drop(resp);

    // Dioxus 格式化段不支持内联 if 块表达式，条件 class 提前算好。
    let sort_btn_base = "text-xs font-mono tracking-widest uppercase cursor-pointer px-3 py-2 rounded-full border transition-colors";
    let sort_active = "border-[var(--color-paper-primary)] text-[var(--color-paper-primary)]";
    let sort_idle = "border-[var(--color-paper-border)] text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)]";
    let sort_created_class = if sort() == "created" {
        format!("{sort_btn_base} {sort_active}")
    } else {
        format!("{sort_btn_base} {sort_idle}")
    };
    let sort_size_class = if sort() == "size" {
        format!("{sort_btn_base} {sort_active}")
    } else {
        format!("{sort_btn_base} {sort_idle}")
    };

    // 多选派生值：本页可删（未引用）素材 id、是否已全选本页、是否有任何选择
    // （驱动未选中卡片的勾选框常显）。全选/取消本页闭包各需一份 id 列表拷贝。
    let page_orphan_ids: Vec<String> = assets
        .iter()
        .filter(|item| item.ref_count == 0)
        .map(|item| item.asset.id.clone())
        .collect();
    let any_selected = !selected_ids().is_empty();
    let all_page_selected =
        !page_orphan_ids.is_empty() && page_orphan_ids.iter().all(|id| selected_ids().contains(id));
    let page_orphan_ids_for_toggle = page_orphan_ids.clone();

    rsx! {
        // min-h-full：AdminLayout 卡片是 flex 列滚动容器，main（flex-1）的高度为 definite，
        // 故本根节点可解析百分比最小高度——内容不足一页时撑满 main 内容盒，
        // 配合下方分页的 mt-auto wrapper 把分页条吸附到卡片底部。
        div { class: "animate-page-enter min-h-full flex flex-col",
            h1 { class: "animate-row-enter text-3xl font-extrabold tracking-tight mb-2",
                "素材管理"
            }
            p {
                class: "animate-row-enter text-sm text-[var(--color-paper-secondary)] mb-8",
                style: "animation-delay: 60ms",
                "管理文章编辑器上传的图片。共 {all_count} 张，引用中 {used_count} 张，未引用 {orphan_count} 张。"
            }

            // 顶栏：筛选 tabs + 搜索 + 排序
            div {
                class: "animate-row-enter flex flex-wrap items-end justify-between gap-4",
                style: "animation-delay: 120ms",
                FilterTabs {
                    items: vec![("all", "全部"), ("used", "引用中"), ("orphan", "未引用")],
                    active_value: filter(),
                    on_change: move |v: String| {
                        filter.set(v);
                        page.set(1);
                    },
                }
                // 右侧控件与 FilterTabs 同加 mb-6：items-end 对齐的是 margin box，
                // 两者底边同落在 tabs 下划线处，到下方横幅/网格的距离均为 24px。
                div { class: "flex items-center gap-3 mb-6",
                    // 页内上传：主 CTA（实心 paper-primary，对齐 AssetPickerModal「上传新图」先例）。
                    button {
                        class: "text-xs font-medium cursor-pointer px-3 py-2 rounded-full bg-[var(--color-paper-primary)] text-[var(--color-paper-theme)] hover:opacity-80 transition-opacity",
                        onclick: move |_| upload_open.set(true),
                        "上传素材"
                    }
                    // 重建索引：以磁盘为准全量自愈（存量回填/不一致修复）。
                    button {
                        class: "text-xs font-medium cursor-pointer px-3 py-2 rounded-full border border-[var(--color-paper-border)] text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] hover:border-[var(--color-paper-primary)] transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: rebuilding(),
                        title: "扫描 uploads/ 全量文件，同步素材注册表与文章引用（幂等，可随时重跑）",
                        onclick: move |_| {
                            rebuilding.set(true);
                            #[cfg(target_arch = "wasm32")]
                            spawn(async move {
                                match rebuild_assets_index().await {
                                    Ok(RebuildAssetsResponse { message, .. }) => {
                                        show_op(message);
                                        reload.set(reload() + 1);
                                    }
                                    Err(e) => show_op(format!("重建失败：{e}")),
                                }
                                rebuilding.set(false);
                            });
                        },
                        if rebuilding() {
                            "重建中..."
                        } else {
                            "重建索引"
                        }
                    }
                    FormInput {
                        r#type: "search",
                        placeholder: "搜索文件名 / alt",
                        value: query(),
                        class: "w-56 text-sm px-4 py-2 border border-paper-border rounded-2xl bg-paper-entry text-paper-primary placeholder:text-paper-tertiary focus:outline-none focus:border-paper-accent focus:ring-1 focus:ring-paper-accent/30 transition-colors",
                        oninput: move |v: String| query.set(v),
                    }
                    button {
                        class: "{sort_created_class}",
                        onclick: move |_| {
                            sort.set("created".to_string());
                            page.set(1);
                        },
                        "最新"
                    }
                    button {
                        class: "{sort_size_class}",
                        onclick: move |_| {
                            sort.set("size".to_string());
                            page.set(1);
                        },
                        "最大"
                    }
                    // 一键清理孤儿：仅 7 天保护窗外的无引用素材；两步确认。
                    if purgeable_count > 0 {
                        if purge_confirm() {
                            button {
                                class: "text-xs font-medium cursor-pointer px-3 py-2 rounded-full bg-red-500 text-white hover:bg-red-600 transition-colors",
                                onclick: move |_| {
                                    purge_confirm.set(false);
                                    #[cfg(target_arch = "wasm32")]
                                    spawn(async move {
                                        match purge_orphan_assets().await {
                                            Ok(
                                                PurgeOrphansResponse { deleted_count, freed_bytes, failures, .. },
                                            ) => {
                                                let mut msg = format!(
                                                    "已清理 {} 张未引用素材，释放 {}",
                                                    deleted_count,
                                                    format_bytes(freed_bytes),
                                                );
                                                if failures > 0 {
                                                    msg.push_str(
                                                        &format!("（{} 个文件删除失败）", failures),
                                                    );
                                                }
                                                show_op(msg);
                                                reload.set(reload() + 1);
                                            }
                                            Err(e) => show_op(format!("清理失败：{e}")),
                                        }
                                    });
                                },
                                "确认清理 {purgeable_count} 张（{format_bytes(purgeable_bytes)}）"
                            }
                            button {
                                class: "text-xs cursor-pointer px-3 py-2 rounded-full border border-[var(--color-paper-border)] text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors",
                                onclick: move |_| purge_confirm.set(false),
                                "取消"
                            }
                        } else {
                            button {
                                class: "text-xs font-medium cursor-pointer px-3 py-2 rounded-full border border-amber-500/50 text-amber-600 dark:text-amber-400 hover:bg-amber-500/10 transition-colors",
                                title: "仅清理无引用且上传超过 7 天的素材（保护未保存的草稿）",
                                onclick: move |_| purge_confirm.set(true),
                                "清理未引用（{purgeable_count} 张 · {format_bytes(purgeable_bytes)}）"
                            }
                        }
                    }
                }
            }

            // 操作结果横幅
            if let Some(msg) = op_message() {
                // mt-4 会与 FilterTabs 自带的 mb-6 叠加成大空洞；改用 mb-6 后
                // 上方 = tabs 标准间距 24px，下方与网格 mt-2 塌陷同为 24px，对称。
                div { class: "mb-6 flex items-center justify-between gap-4 rounded-2xl border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] px-4 py-3 text-sm text-[var(--color-paper-primary)] shadow-sm",
                    class: if op_closing() { "animate-row-leave" } else { "animate-row-enter" },
                    span { "{msg}" }
                    button {
                        class: "text-[var(--color-paper-tertiary)] hover:text-[var(--color-paper-primary)] cursor-pointer",
                        onclick: move |_| dismiss_op(()),
                        "×"
                    }
                }
            }

            // 多选批量操作条：出现即与操作横幅同槽位（顶栏与网格之间）。
            if any_selected {
                div { class: "mb-6 flex items-center gap-3 rounded-2xl border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] px-4 py-3 text-sm shadow-sm",
                    class: if batch_closing() { "animate-row-leave" } else { "animate-row-enter" },
                    span { class: "text-sm text-[var(--color-paper-secondary)]",
                        "已选 {selected_ids().len()} 张"
                    }
                    button {
                        class: "text-xs cursor-pointer text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors",
                        onclick: move |_| {
                            let mut s = selected_ids();
                            if all_page_selected {
                                for id in &page_orphan_ids_for_toggle {
                                    s.remove(id);
                                }
                            } else {
                                for id in &page_orphan_ids_for_toggle {
                                    s.insert(id.clone());
                                }
                            }
                            if s.is_empty() {
                                // 取消本页正好清空 → 收起批量条（延迟清空播退出动画）
                                dismiss_batch(());
                            } else {
                                batch_closing.set(false);
                                selected_ids.set(s);
                            }
                        },
                        if all_page_selected {
                            "取消本页"
                        } else {
                            "全选本页"
                        }
                    }
                    button {
                        class: "text-xs cursor-pointer text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors",
                        onclick: move |_| {
                            batch_confirm.set(false);
                            dismiss_batch(());
                        },
                        "清除"
                    }
                    div { class: "flex-1" }
                    if batch_confirm() {
                        button {
                            class: "text-xs font-medium cursor-pointer px-3 py-2 rounded-full bg-red-500 text-white hover:bg-red-600 transition-colors",
                            onclick: move |_| {
                                batch_confirm.set(false);
                                let ids: Vec<String> = selected_ids().iter().cloned().collect();
                                #[cfg(target_arch = "wasm32")]
                                spawn(async move {
                                    match batch_delete_assets(ids).await {
                                        Ok(resp) => {
                                            let BatchDeleteAssetsResponse {
                                                message,
                                                freed_bytes,
                                                failures,
                                                deleted_count,
                                                ..
                                            } = resp;
                                            let mut msg = message;
                                            if freed_bytes > 0 {
                                                msg.push_str(
                                                    &format!("，释放 {}", format_bytes(freed_bytes)),
                                                );
                                            }
                                            if failures > 0 {
                                                msg.push_str(&format!("（{failures} 项失败）"));
                                            }
                                            show_op(msg);
                                            if deleted_count > 0 {
                                                reload.set(reload() + 1);
                                                dismiss_batch(());
                                            }
                                        }
                                        Err(e) => show_op(format!("批量删除失败：{e}")),
                                    }
                                });
                            },
                            "确认删除 {selected_ids().len()} 张"
                        }
                        button {
                            class: "text-xs cursor-pointer px-3 py-2 rounded-full border border-[var(--color-paper-border)] text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors",
                            onclick: move |_| batch_confirm.set(false),
                            "取消"
                        }
                    } else {
                        button {
                            class: "text-xs font-medium cursor-pointer px-3 py-2 rounded-full border border-red-500/50 text-red-600 dark:text-red-400 hover:bg-red-500/10 transition-colors",
                            onclick: move |_| batch_confirm.set(true),
                            "删除所选"
                        }
                    }
                }
            }

            // 内容区
            if let Some(err) = error() {
                div { class: "mt-8 text-sm text-red-500", "加载失败：{err}" }
            } else if loading() && assets.is_empty() {
                DelayedSkeleton { AssetsSkeleton {} }
            } else if assets.is_empty() {
                EmptyState {
                    title: "暂无素材".to_string(),
                    description: "在编辑器中上传图片后会自动出现在这里".to_string(),
                }
            } else {
                // 网格：缩略图卡片（assets-lightbox 为 __initLightbox 的根选择器）。
                // loaded_view 作 key 强制 remount：视图切换（tab/搜索/排序/翻页）后新数据
                // 落地时整网重挂载、卡片重播阶梯入场；同视图内 reload（删除/上传/alt）
                // key 不变，走普通 keyed diff 不重播。std::iter::once 先例见 post_detail.rs。
                for view_key in std::iter::once(loaded_view()) {
                    div {
                        key: "{view_key}",
                        class: "assets-lightbox grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6 gap-4 mt-2",
                        for (idx, asset) in assets.iter().enumerate() {
                            {
                                let a = &asset.asset;
                                let thumb = format!("/uploads/{}?thumb=300x300", a.path);
                                let placeholder = format!("/uploads/{}?w=20", a.path);
                                let img_alt = a.alt.clone().unwrap_or_else(|| a.filename.clone());
                                let is_orphan = asset.ref_count == 0;
                                let is_selected = selected_ids().contains(&a.id);
                                // 选中卡片加主题色描边（ring 与原有 border 叠加，不动布局）。
                                let card_ring = if is_selected {
                                    "ring-2 ring-[var(--color-paper-accent)]"
                                } else {
                                    ""
                                };
                                // 阶梯入场 delay：30ms 步长、300ms 封顶——前 10 张阶梯、其余同播，
                                // 60 张/页时尾部不晚于 ~750ms 全部到位（对齐 asset_upload 的 idx*30 先例）。
                                let enter_delay = (idx * 30).min(300);
                                // 勾选框：未引用素材才可删可选。选中或有任何选择时常显，
                                // 否则随卡片 hover 显现（与卡片操作按钮同一显现语言）。
                                let checkbox_class = if is_selected {
                                    "absolute top-2 right-2 z-10 w-6 h-6 flex items-center justify-center rounded-full text-xs cursor-pointer transition-all bg-[var(--color-paper-accent)] text-white border border-[var(--color-paper-accent)]"
                                } else if any_selected {
                                    "absolute top-2 right-2 z-10 w-6 h-6 flex items-center justify-center rounded-full text-xs cursor-pointer transition-all bg-black/40 backdrop-blur-sm text-white border border-white/60"
                                } else {
                                    "absolute top-2 right-2 z-10 w-6 h-6 flex items-center justify-center rounded-full text-xs cursor-pointer transition-all bg-black/40 backdrop-blur-sm text-white border border-white/60 opacity-0 group-hover:opacity-100"
                                };
                                // z-10：.blur-img-full 带 z-index:1，不提升会被展示层盖住（灯箱改造的回归）。
                                let badge_tone = if is_orphan {
                                    "bg-amber-500/80 text-white"
                                } else {
                                    "bg-black/50 text-white"
                                };
                                rsx! {
                                    div {
                                        key: "{a.id}",
                                        class: "group relative rounded-2xl overflow-hidden border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] shadow-sm hover:shadow-md hover:-translate-y-1 transition-all duration-300 animate-row-enter {card_ring}",
                                        style: "animation-delay: {enter_delay}ms",
                                        // blur-img 双层结构（对齐前台正文图）：?w=20 模糊占位 +
                                        // data-src 展示层（IO 懒加载）；点击由 lightbox.js 接管为灯箱
                                        // （图集模式，原图 = data-src 去 query）。不加 lightbox-single。
                                        // data-error-text：缩略图重试耗尽仍失败（本地文件丢失等）时
                                        // 卡片占位与灯箱错误态显示的定制文案。
                                        div {
                                            class: "blur-img !rounded-none aspect-square m-0 cursor-pointer bg-[var(--color-paper-theme)]",
                                            "data-error-text": "本地文件已丢失",
                                            img { class: "blur-img-placeholder", src: "{placeholder}", alt: "" }
                                            img {
                                                class: "blur-img-full",
                                                "data-src": "{thumb}",
                                                alt: "{img_alt}",
                                            }
                                        }
                                        // 引用徽标：未引用静态展示；引用中可点击 →
                                        // 引用明细浮层（stop_propagation 防触发灯箱）。
                                        if is_orphan {
                                            span {
                                                class: "absolute top-2 left-2 z-10 {MEDIA_BADGE_BASE} backdrop-blur-sm {badge_tone}",
                                                "未引用"
                                            }
                                        } else {
                                            button {
                                                class: "absolute top-2 left-2 z-10 {MEDIA_BADGE_BASE} backdrop-blur-sm {badge_tone} cursor-pointer hover:brightness-125 transition",
                                                title: "查看被谁引用",
                                                onclick: {
                                                    let id = a.id.clone();
                                                    move |evt| {
                                                        evt.stop_propagation();
                                                        #[cfg(target_arch = "wasm32")]
                                                        {
                                                            let coords = evt.client_coordinates();
                                                            let x = coords.x as i32;
                                                            let y = coords.y as i32;
                                                            // 动态 align：靠视口左/右缘的卡片让面板
                                                            // 朝内侧延伸（240px ≈ 面板宽 w-80 的 3/4）。
                                                            let vw = web_sys::window()
                                                                .and_then(|w| w.inner_width().ok())
                                                                .and_then(|v| v.as_f64())
                                                                .unwrap_or(1280.0)
                                                                as i32;
                                                            let align = if x < 240 {
                                                                "start"
                                                            } else if x > vw - 240 {
                                                                "end"
                                                            } else {
                                                                "center"
                                                            };
                                                            refs_popover.set(Some((id.clone(), x, y, align)));
                                                        }
                                                    }
                                                },
                                                "被 {asset.ref_count} 处引用"
                                            }
                                        }
                                        // 多选勾选框（仅未引用素材；stop_propagation 防触发灯箱）
                                        if is_orphan {
                                            button {
                                                class: "{checkbox_class}",
                                                title: if is_selected { "取消选择" } else { "选择" },
                                                onclick: {
                                                    let id = a.id.clone();
                                                    move |evt| {
                                                        evt.stop_propagation();
                                                        let mut s = selected_ids();
                                                        if s.contains(&id) {
                                                            s.remove(&id);
                                                        } else {
                                                            s.insert(id.clone());
                                                        }
                                                        if s.is_empty() {
                                                            // 取消最后一个勾选 → 收起批量条（延迟清空播退出动画）
                                                            dismiss_batch(());
                                                        } else {
                                                            batch_closing.set(false);
                                                            selected_ids.set(s);
                                                        }
                                                    }
                                                },
                                                if is_selected {
                                                    "✓"
                                                }
                                            }
                                        }
                                        div { class: "p-3",
                                            p {
                                                class: "text-xs font-medium truncate text-[var(--color-paper-primary)]",
                                                title: "{a.filename}",
                                                "{a.filename}"
                                            }
                                            p { class: "text-[10px] font-mono text-[var(--color-paper-tertiary)] mt-0.5",
                                                "{a.width}×{a.height} · {format_bytes(a.size_bytes)}"
                                            }
                                            if let Some(alt_text) = &a.alt {
                                                p {
                                                    class: "text-[10px] truncate text-[var(--color-paper-secondary)] mt-0.5",
                                                    title: "{alt_text}",
                                                    "alt: {alt_text}"
                                                }
                                            }

                                            // 操作区：确认删除 / alt 编辑 / 常规三按钮 三态互斥
                                            if confirm_delete().as_deref() == Some(a.id.as_str()) {
                                                div { class: "flex items-center gap-2 mt-2",
                                                    button {
                                                        class: "text-[10px] font-medium cursor-pointer px-2 py-1 rounded-full bg-red-500 text-white hover:bg-red-600 transition-colors",
                                                        onclick: {
                                                            let id = a.id.clone();
                                                            move |_| {
                                                                confirm_delete.set(None);
                                                                let id = id.clone();
                                                                #[cfg(target_arch = "wasm32")]
                                                                spawn(async move {
                                                                    match delete_asset(id).await {
                                                                        Ok(resp) => {
                                                                            // 行已不在 DB（refs 为空的业务拒绝 = 素材不存在）
                                                                            // 说明网格是过期数据，同样触发刷新自愈。
                                                                            let stale = !resp.success && resp.refs.is_empty();
                                                                            show_op(resp.message);
                                                                            if resp.success || stale {
                                                                                reload.set(reload() + 1);
                                                                            }
                                                                        }
                                                                        Err(e) => show_op(format!("删除失败：{e}")),
                                                                    }
                                                                });
                                                            }
                                                        },
                                                        "确认删除"
                                                    }
                                                    button {
                                                        class: "text-[10px] cursor-pointer px-2 py-1 rounded-full border border-[var(--color-paper-border)] text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors",
                                                        onclick: move |_| confirm_delete.set(None),
                                                        "取消"
                                                    }
                                                }
                                            } else if editing_alt().as_deref() == Some(a.id.as_str()) {
                                                div { class: "flex items-center gap-1 mt-2",
                                                    FormInput {
                                                        r#type: "text",
                                                        placeholder: "alt 文本",
                                                        value: alt_input(),
                                                        class: "flex-1 min-w-0 text-[10px] px-2 py-1 rounded-full border border-paper-border bg-paper-entry text-paper-primary placeholder:text-paper-tertiary focus:outline-none focus:border-paper-accent transition-colors",
                                                        oninput: move |v: String| alt_input.set(v),
                                                    }
                                                    button {
                                                        class: "text-[10px] font-medium cursor-pointer px-2 py-1 rounded-full bg-[var(--color-paper-primary)] text-[var(--color-paper-theme)] hover:opacity-80 transition-opacity",
                                                        onclick: {
                                                            let id = a.id.clone();
                                                            move |_| {
                                                                let id = id.clone();
                                                                let alt = alt_input();
                                                                editing_alt.set(None);
                                                                #[cfg(target_arch = "wasm32")]
                                                                spawn(async move {
                                                                    match update_asset_alt(id, alt).await {
                                                                        Ok(resp) => {
                                                                            show_op(resp.message);
                                                                            if resp.success {
                                                                                reload.set(reload() + 1);
                                                                            }
                                                                        }
                                                                        Err(e) => show_op(format!("保存失败：{e}")),
                                                                    }
                                                                });
                                                            }
                                                        },
                                                        "存"
                                                    }
                                                    button {
                                                        class: "text-[10px] cursor-pointer px-2 py-1 rounded-full border border-[var(--color-paper-border)] text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors",
                                                        onclick: move |_| editing_alt.set(None),
                                                        "×"
                                                    }
                                                }
                                            } else {
                                                div { class: "flex items-center gap-2 mt-2 opacity-0 group-hover:opacity-100 transition-opacity",
                                                    button {
                                                        class: "text-[10px] cursor-pointer text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors",
                                                        title: "复制图片相对路径",
                                                        onclick: {
                                                            let url = format!("/uploads/{}", a.path);
                                                            move |_| {
                                                                #[cfg(target_arch = "wasm32")]
                                                                if let Some(window) = web_sys::window() {
                                                                    let _ = window
                                                                        .navigator()
                                                                        .clipboard()
                                                                        .write_text(&url);
                                                                    show_op(format!("已复制 {url}"));
                                                                }
                                                            }
                                                        },
                                                        "复制路径"
                                                    }
                                                    button {
                                                        class: "text-[10px] cursor-pointer text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors",
                                                        title: "编辑 alt",
                                                        onclick: {
                                                            let id = a.id.clone();
                                                            let current_alt = a.alt.clone().unwrap_or_default();
                                                            move |_| {
                                                                alt_input.set(current_alt.clone());
                                                                editing_alt.set(Some(id.clone()));
                                                            }
                                                        },
                                                        "alt"
                                                    }
                                                    if asset.ref_count > 0 {
                                                        {
                                                            let refs_tip = asset
                                                                .refs
                                                                .iter()
                                                                .map(|r| r.describe())
                                                                .collect::<Vec<_>>()
                                                                .join("、");
                                                            rsx! {
                                                                span {
                                                                    class: "text-[10px] text-[var(--color-paper-tertiary)] cursor-not-allowed",
                                                                    title: "被引用：{refs_tip}",
                                                                    "删除"
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        button {
                                                            class: "text-[10px] cursor-pointer text-red-500/70 hover:text-red-500 transition-colors",
                                                            onclick: {
                                                                let id = a.id.clone();
                                                                move |_| confirm_delete.set(Some(id.clone()))
                                                            },
                                                            "删除"
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
                }

                // mt-auto 吸收上方自由空间：内容短时分页贴卡片底（main 的 py-12 保留 48px
                // 底距）；内容超一页时 auto margin 为 0，分页跟在网格后随卡片滚动。
                // wrapper 是 flex item 建立独立格式化上下文，Pagination nav 自带的 mt-6
                // 不会穿透塌陷，24px 间距恒在。
                div { class: "mt-auto",
                    Pagination {
                        variant: "admin",
                        current_page: page(),
                        total,
                        per_page: ASSETS_PER_PAGE,
                        unit: "张",
                        on_prev: move |_| page.set((page() - 1).max(1)),
                        on_next: move |_| page.set(page() + 1),
                        on_jump: move |p: i32| page.set(p),
                    }
                }
            }

            // 引用明细浮层：页面根部渲染（fixed 定位须避开卡片 transform 动画祖先）；
            // 数据复用已加载列表（服务端按 文章→评论→头像 分组序返回），点开零额外请求。
            if let Some((ref_id, anchor_x, anchor_y, pop_align)) = refs_popover() {
                if let Some(target) = assets.iter().find(|item| item.asset.id == ref_id) {
                    {
                        let posts: Vec<&AssetRef> = target
                            .refs
                            .iter()
                            .filter(|r| matches!(r, AssetRef::Post { .. }))
                            .collect();
                        let comments: Vec<&AssetRef> = target
                            .refs
                            .iter()
                            .filter(|r| matches!(r, AssetRef::Comment { .. }))
                            .collect();
                        let avatars: Vec<&AssetRef> = target
                            .refs
                            .iter()
                            .filter(|r| {
                                matches!(
                                    r,
                                    AssetRef::UserAvatar { .. } | AssetRef::FriendAvatar { .. }
                                )
                            })
                            .collect();
                        let total = target.ref_count;
                        rsx! {
                            Popover {
                                open: true,
                                anchor_x,
                                anchor_y,
                                placement: "bottom",
                                align: pop_align,
                                on_close: move |_| dismiss_refs(()),
                                div { class: "w-80 max-w-[calc(100vw-2rem)] max-h-[60vh] overflow-y-auto",
                                    p { class: "text-sm font-medium text-[var(--color-paper-primary)]",
                                        "引用此素材 · {total} 处"
                                    }
                                    if target.refs.is_empty() {
                                        // ref_count 与明细正常必然一致；不一致（极端竞态）时兜底。
                                        p { class: "mt-2 text-xs text-[var(--color-paper-tertiary)]",
                                            "未找到引用明细，请刷新后重试"
                                        }
                                    }
                                    if !posts.is_empty() {
                                        p { class: "mt-3 mb-1 text-[10px] font-mono tracking-widest text-[var(--color-paper-tertiary)]",
                                            "文章 · {posts.len()}"
                                        }
                                        for r in posts {
                                            {render_ref_row(r, dismiss_refs)}
                                        }
                                    }
                                    if !comments.is_empty() {
                                        p { class: "mt-3 mb-1 text-[10px] font-mono tracking-widest text-[var(--color-paper-tertiary)]",
                                            "评论 · {comments.len()}"
                                        }
                                        for r in comments {
                                            {render_ref_row(r, dismiss_refs)}
                                        }
                                    }
                                    if !avatars.is_empty() {
                                        p { class: "mt-3 mb-1 text-[10px] font-mono tracking-widest text-[var(--color-paper-tertiary)]",
                                            "头像 · {avatars.len()}"
                                        }
                                        for r in avatars {
                                            {render_ref_row(r, dismiss_refs)}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 页内上传 modal：始终挂载，visible 控制渲染；上传成功刷新网格
            // （不重置页码/排序，按当前排序自然刷新——「最新」下新图在头部）。
            AssetUploadModal {
                visible: upload_open,
                on_uploaded: move |_| reload.set(reload() + 1),
            }
        }
    }
}
