//! 素材上传 modal（素材管理页内上传）。
//!
//! 三条入口（点击选择 / 拖拽 / 粘贴）收敛到同一个 [`enqueue_files`]：拿到的
//! `web_sys::File` 先过 [`validate_file`]（镜像服务端 5MiB / 四种 MIME 的硬限制，
//! 不合格立即记失败行、不发请求），合格项入共享队列后由 **worker 池**并发上传：
//! 在跑 worker 数上限 = 并发配置（「站点配置」面板 / `UPLOAD_CONCURRENCY` env 播种，
//! 挂载时经 `get_upload_settings` 拉取，失败回退默认 3）。每个 worker 张间停顿
//! `500ms × 当前并发数`——N 路并行时聚合速率恒 ≤ 2/s，与默认上传限流桶
//! （`RATE_LIMIT_UPLOAD_PER_SEC=2` / `BURST=15`）对齐：N=1 时与旧顺序版逐张 500ms
//! 完全一致，N 调大也不触发 429。上传管线与文章编辑器完全同源
//! （`upload_image_file` → `POST /api/upload`）。
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
//!
//! 注：worker 池共享状态用 `Rc<UploadPool>` 而非裸 Cell/RefCell——`use_hook` 每次渲染
//! 都会 clone 存储值（dioxus-core `use_hook_inner` 走 `.cloned()`），裸结构会被按值
//! 拷贝，各入口闭包拿到互不同步的副本导致 id 冲突与队列分裂；Rc 克隆共享同一实例。

use dioxus::prelude::*;

use crate::components::ui::SPINNER_SVG;

#[cfg(target_arch = "wasm32")]
use super::assets::format_bytes;
#[cfg(target_arch = "wasm32")]
use crate::tiptap_bridge::upload_image_file;
// 从 Dioxus 事件拿底层 web_sys::File（write.rs L30-34 同款 cfg 门控惯例）：
// - HasFileData：evt.files()（FormEvent / DragEvent 取文件）
// - WebFileExt：file.get_web_file()（FileData 取底层 web_sys::File）
#[cfg(target_arch = "wasm32")]
use dioxus::html::HasFileData;
#[cfg(target_arch = "wasm32")]
use dioxus::web::WebFileExt;

/// 单文件大小硬上限（5MiB），镜像服务端 `crate::utils::server::MAX_FILE_SIZE`。
#[cfg(any(test, target_arch = "wasm32"))]
const MAX_UPLOAD_BYTES: u64 = 5 * 1024 * 1024;
/// 允许的 MIME 白名单，镜像服务端 `api/upload.rs` 的 `ALLOWED_MIME_TYPES`。
#[cfg(any(test, target_arch = "wasm32"))]
const ALLOWED_MIME: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// 关闭/移除动画时长 ms，与 input.css 的 200ms 过渡/动画时长一一对应。
const EXIT_ANIM_MS: u32 = 200;

/// 单条上传状态机：Queued → Uploading → Done / Failed(原因)。
// 变体仅在 WASM 端构造，server SSR 只匹配渲染，非 wasm 构建放行 dead_code。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone, PartialEq)]
enum UploadStatus {
    Queued,
    Uploading,
    Done,
    Failed(String),
}

/// 列表行数据（纯数据，两端都可编译；`web_sys::File` 句柄不进 signal，
/// 由 WASM 端的 files 表以 id 关联保存，供重试取回）。
// 同 UploadStatus：实例仅在 WASM 端构造。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone, PartialEq)]
struct UploadItem {
    id: u64,
    name: String,
    /// 入队时已用 `format_bytes` 格式化好的可读大小。
    size: String,
    status: UploadStatus,
    /// 用户点了 ×：先播退出动画（animate-row-leave），EXIT_ANIM_MS 后真正摘除。
    removing: bool,
}

/// 一批入队（一次拖拽/选择/粘贴）的完成追踪：remaining 归零且 ≥1 成功时回调一次。
#[cfg(target_arch = "wasm32")]
struct BatchCtx {
    remaining: std::cell::Cell<usize>,
    any_done: std::cell::Cell<bool>,
}

/// Worker 池共享状态：id 分配、重试句柄表、待传队列、在跑 worker 数。
/// `use_hook` 持 `Rc<UploadPool>`：渲染期 clone 的是 Rc（共享同一实例），各入口闭包
/// 与 worker 看到的永远是同一份队列/句柄表。
#[cfg(target_arch = "wasm32")]
struct UploadPool {
    next_id: std::cell::Cell<u64>,
    files: std::cell::RefCell<Vec<(u64, web_sys::File)>>,
    queue: std::cell::RefCell<std::collections::VecDeque<(u64, std::rc::Rc<BatchCtx>)>>,
    active_workers: std::cell::Cell<u32>,
}

/// 预校验：MIME 白名单 + 5MiB 上限。失败返回可读原因（直接展示在行内，不发请求）。
#[cfg(any(test, target_arch = "wasm32"))]
fn validate_file(mime: &str, size: u64) -> Result<(), String> {
    if !ALLOWED_MIME.contains(&mime) {
        return Err("不支持的文件类型（仅 JPEG / PNG / GIF / WebP）".into());
    }
    if size > MAX_UPLOAD_BYTES {
        return Err("大小超过 5MB 限制".into());
    }
    Ok(())
}

/// 更新单条状态：write guard 在函数内立即释放，绝不跨 await 持有。
#[cfg(target_arch = "wasm32")]
fn set_status(items: &mut Signal<Vec<UploadItem>>, id: u64, status: UploadStatus) {
    let mut guard = items.write();
    if let Some(it) = guard.iter_mut().find(|it| it.id == id) {
        it.status = status;
    }
}

/// 单个 worker 的消费循环：队列空即退出并归还名额。
///
/// 张间停顿 `500ms × 当前并发数`：N 个 worker 并行时聚合速率恒 ≤ 2/s（停顿随 N
/// 线性放大），与默认上传限流桶（`RATE_LIMIT_UPLOAD_PER_SEC=2` / `BURST=15`）对齐——
/// N=1 时与旧顺序版逐张 500ms 完全一致，N 调大也不会触发 429。
#[cfg(target_arch = "wasm32")]
async fn worker_loop(
    mut items: Signal<Vec<UploadItem>>,
    pool: std::rc::Rc<UploadPool>,
    concurrency: Signal<i32>,
    on_uploaded: EventHandler<()>,
) {
    loop {
        // borrow 不出块，guard 不跨 await。
        let next = pool.queue.borrow_mut().pop_front();
        let Some((id, batch)) = next else { break };
        // 句柄可能已被「×」移除：跳过上传但仍计入批次完成度（否则该批永不合拢，
        // 其他成功项的 on_uploaded 无法触发）。
        let file = pool
            .files
            .borrow()
            .iter()
            .find(|(fid, _)| *fid == id)
            .map(|(_, f)| f.clone());
        if let Some(file) = file {
            set_status(&mut items, id, UploadStatus::Uploading);
            match upload_image_file(file).await {
                Ok(_) => {
                    set_status(&mut items, id, UploadStatus::Done);
                    batch.any_done.set(true);
                }
                Err(msg) => set_status(&mut items, id, UploadStatus::Failed(msg)),
            }
        }
        // 本批最后一条收尾且 ≥1 成功 → 回调一次（父组件刷新网格）。
        let remaining = batch.remaining.get() - 1;
        batch.remaining.set(remaining);
        if remaining == 0 && batch.any_done.get() {
            on_uploaded.call(());
        }
        // 队列未空则停顿压速率；停顿随并发数线性放大（见 fn doc），live 读取
        // 让面板改动即时生效。clamp(1, 32) 纯防御：服务端已钳到 1–8，
        // 此处只防异常值导致 500*n 溢出或路径级长停。
        if !pool.queue.borrow().is_empty() {
            let n = (*concurrency.peek()).clamp(1, 32) as u32;
            crate::utils::time::sleep_ms(500 * n).await;
        }
    }
    pool.active_workers.set(pool.active_workers.get() - 1);
}

/// 三入口收敛点：校验入队 + 按需补足 worker。仅 WASM 端存在（`web_sys::File` /
/// `spawn` / `upload_image_file` 都是 WASM-only 符号）。
#[cfg(target_arch = "wasm32")]
fn enqueue_files(
    mut items: Signal<Vec<UploadItem>>,
    pool: std::rc::Rc<UploadPool>,
    concurrency: Signal<i32>,
    on_uploaded: EventHandler<()>,
    new_files: Vec<web_sys::File>,
) {
    // 1) 校验入队：不合格直接记 Failed（不发请求）；合格记 Queued 并留存句柄供重试。
    let mut valid_ids = Vec::new();
    for file in new_files {
        let id = pool.next_id.get() + 1;
        pool.next_id.set(id);
        let item = UploadItem {
            id,
            name: file.name(),
            size: format_bytes(file.size() as i64),
            removing: false,
            status: match validate_file(&file.type_(), file.size() as u64) {
                Ok(()) => {
                    pool.files.borrow_mut().push((id, file));
                    valid_ids.push(id);
                    UploadStatus::Queued
                }
                Err(msg) => UploadStatus::Failed(msg),
            },
        };
        items.write().push(item);
    }
    if valid_ids.is_empty() {
        return;
    }

    // 2) 一批一个 BatchCtx 追踪完成度；id 入共享队列后补足 worker：在跑数低于
    //    并发上限且队列非空时逐个 spawn（worker 队列空自退，不会超额驻留）。
    let batch = std::rc::Rc::new(BatchCtx {
        remaining: std::cell::Cell::new(valid_ids.len()),
        any_done: std::cell::Cell::new(false),
    });
    {
        let mut q = pool.queue.borrow_mut();
        for id in valid_ids {
            q.push_back((id, batch.clone()));
        }
    }
    // clamp 上界与 worker_loop 的停顿同款防御（正常值 1–8）。
    let target = (*concurrency.peek()).clamp(1, 32) as u32;
    while pool.active_workers.get() < target && !pool.queue.borrow().is_empty() {
        pool.active_workers.set(pool.active_workers.get() + 1);
        spawn(worker_loop(items, pool.clone(), concurrency, on_uploaded));
    }
}

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
    // 是否曾开过弹窗：mount 时 visible=false 也会跑一次 use_effect，不加此守卫会
    // 在页面加载后 200ms 内渲染一层透明遮罩（opacity:0 仍拦截点击）吞掉首次点击。
    let mut opened = use_signal(|| false);
    // worker 池共享状态（WASM-only）：use_hook 持 Rc，跨渲染共享同一实例。
    #[cfg(target_arch = "wasm32")]
    let pool = use_hook(|| {
        std::rc::Rc::new(UploadPool {
            next_id: std::cell::Cell::new(0_u64),
            files: std::cell::RefCell::new(Vec::new()),
            queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            active_workers: std::cell::Cell::new(0),
        })
    });
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
    let pool_for_rows = pool.clone();

    // visible 翻转驱动关闭动画：关闭入口（× / 遮罩 / Esc）会同步先置 closing 再翻
    // visible（同帧渲染在存活元素上换 .is-closing 类 → transition 从可见态出发）；
    // 这里只负责重开复位与 EXIT_ANIM_MS 后的复位卸载。闭包只订阅 visible，
    // closing/opened 全用 peek 防自触发循环。
    use_effect(move || {
        if visible() {
            opened.set(true);
            closing.set(false);
        } else if *opened.peek() {
            // 非交互路径的 visible 翻转（理论上不存在）兜底补置 closing。
            if !*closing.peek() {
                closing.set(true);
            }
            spawn(async move {
                crate::utils::time::sleep_ms(EXIT_ANIM_MS).await;
                closing.set(false);
            });
        }
    });

    // closing() 的订阅读是必需的——closing.set 靠这个订阅触发重渲染，peek 会不重绘。
    let is_closing = closing();
    if !visible() && !is_closing {
        return rsx! {};
    }

    // 快照当前列表（小 Vec，clone 成本可忽略；status 变化驱动重渲染）。
    let items_snapshot = items.read().clone();

    rsx! {
        // 遮罩：点击关闭（照抄 AssetPickerModal）。
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-6 modal-overlay animate-modal-overlay-enter",
            class: if is_closing { "is-closing" } else { "" },
            onclick: move |_| {
                closing.set(true);
                visible.set(false);
            },
            // 面板：阻止点击穿透到遮罩。
            div {
                class: "w-full max-w-lg max-h-[80vh] flex flex-col rounded-[2rem] bg-[var(--color-paper-entry)] border border-[var(--color-paper-border)] shadow-xl overflow-hidden modal-panel animate-modal-panel-enter",
                onclick: move |evt| evt.stop_propagation(),

                // 头部（照抄 AssetPickerModal 头部类）。
                div { class: "flex items-center gap-3 px-6 py-4 border-b border-[var(--color-paper-border)]",
                    h2 { class: "text-lg font-bold text-[var(--color-paper-primary)]",
                        "上传素材"
                    }
                    div { class: "flex-1" }
                    button {
                        class: "shrink-0 w-8 h-8 flex items-center justify-center rounded-full text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-theme)] transition-colors cursor-pointer",
                        aria_label: "关闭",
                        onclick: move |_| {
                            closing.set(true);
                            visible.set(false);
                        },
                        "×"
                    }
                }

                // 主体：拖放区 + 逐文件状态列表。
                div { class: "flex-1 overflow-y-auto p-6 flex flex-col gap-4",
                    // 拖放区 = label：点击天然触发隐藏 file input（同 CoverUploader），无需 JS。
                    // 内部内容包一层 pointer-events-none，防子元素进出触发 dragleave 高亮闪烁。
                    label {
                        class: "flex flex-col items-center justify-center gap-2 px-6 py-10 border border-dashed rounded-2xl cursor-pointer transition-colors",
                        class: if drag_active() { "border-[var(--color-paper-primary)] bg-[var(--color-paper-theme)]" } else { "border-[var(--color-paper-border)] bg-[var(--color-paper-theme)] hover:border-[var(--color-paper-primary)]" },
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
                        div { class: "pointer-events-none flex flex-col items-center gap-2",
                            // 上传图标（Feather 风格线框，照抄 CoverUploader）。
                            svg {
                                class: "w-8 h-8 text-[var(--color-paper-tertiary)]",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.8",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                                polyline { points: "17 8 12 3 7 8" }
                                line {
                                    x1: "12",
                                    y1: "3",
                                    x2: "12",
                                    y2: "15",
                                }
                            }
                            p { class: "text-sm font-medium text-[var(--color-paper-primary)]",
                                "拖拽图片到这里，或点击选择"
                            }
                            p { class: "text-xs text-[var(--color-paper-tertiary)]",
                                "支持 Ctrl/⌘+V 粘贴 · JPEG / PNG / GIF / WebP · 单张 ≤ 5MB"
                            }
                        }
                        input {
                            r#type: "file",
                            accept: "image/jpeg,image/png,image/gif,image/webp",
                            multiple: true,
                            class: "hidden",
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

                    // 逐文件状态列表。
                    if !items_snapshot.is_empty() {
                        div { class: "flex flex-col gap-2",
                            for (idx, item) in items_snapshot.iter().enumerate() {
                                {
                                    let item_id = item.id;
                                    // 状态切换 key（q/u/d/f）：status 变化强制重挂载状态区 → 重放 status-pop。
                                    let status_key = match &item.status {
                                        UploadStatus::Queued => "q",
                                        UploadStatus::Uploading => "u",
                                        UploadStatus::Done => "d",
                                        UploadStatus::Failed(_) => "f",
                                    };
                                    rsx! {
                                        div {
                                            key: "{item_id}",
                                            class: "flex items-center gap-3 rounded-2xl border border-[var(--color-paper-border)] bg-[var(--color-paper-theme)] px-4 py-3",
                                            class: if item.removing { "animate-row-leave" } else { "animate-row-enter" },
                                            // 进场阶梯 delay（复用 .animate-row-enter）；removing 时清空，避免退出动画被残留 delay 推迟。
                                            style: if item.removing { String::new() } else { format!("animation-delay:{}ms", idx * 30) },
                                            // 左：文件名 + 大小（min-w-0 + truncate 防长文件名撑破行）。
                                            div { class: "flex-1 min-w-0",
                                                p {
                                                    class: "text-sm truncate text-[var(--color-paper-primary)]",
                                                    title: "{item.name}",
                                                    "{item.name}"
                                                }
                                                p { class: "text-xs font-mono text-[var(--color-paper-tertiary)]", "{item.size}" }
                                            }
                                            // 右：状态 + 操作。外层 keyed（status 变化强制重挂载）→ 重放 status-pop。
                                            for _ in std::iter::once(()) {
                                                {
                                                    // for 闭包体内声明 Rc 局部：下层 onclick 的 move 捕获落在本层
                                                    // 局部（可自由 move），而非跨层 move 外层闭包捕获（FnMut 链 E0507）。
                                                    #[cfg(target_arch = "wasm32")]
                                                    let (pool_for_retry, pool_for_remove) =
                                                        (pool_for_rows.clone(), pool_for_rows.clone());
                                                    rsx! {
                                                        div {
                                                            key: "{status_key}",
                                                            class: "flex items-center gap-2 shrink-0 animate-status-pop",
                                                            match &item.status {
                                                                UploadStatus::Queued => rsx! {
                                                                    span { class: "text-xs text-[var(--color-paper-tertiary)] shrink-0", "等待中" }
                                                                },
                                                                UploadStatus::Uploading => rsx! {
                                                                    span { class: "flex items-center gap-1.5 text-xs text-[var(--color-paper-secondary)] shrink-0",
                                                                        span {
                                                                            class: "inline-block w-3.5 h-3.5",
                                                                            dangerous_inner_html: SPINNER_SVG,
                                                                        }
                                                                        "上传中"
                                                                    }
                                                                },
                                                                UploadStatus::Done => rsx! {
                                                                    span { class: "text-xs text-emerald-600 dark:text-emerald-400 shrink-0", "✓ 已上传" }
                                                                },
                                                                UploadStatus::Failed(msg) => rsx! {
                                                                    // 失败原因文字展示（不只靠颜色），过长截断 + title 全文。
                                                                    span { class: "text-xs text-red-500 shrink-0 max-w-56 truncate", title: "{msg}", "{msg}" }
                                                                    button {
                                                                        class: "text-xs cursor-pointer text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] shrink-0",
                                                                        onclick: move |_| {
                                                                            #[cfg(target_arch = "wasm32")]
                                                                            {
                                                                                // 从 files 表取回句柄重发本条。
                                                                                let file = pool_for_retry
                                                                                    .files
                                                                                    .borrow()
                                                                                    .iter()
                                                                                    .find(|(fid, _)| *fid == item_id)
                                                                                    .map(|(_, f)| f.clone());
                                                                                if let Some(file) = file {
                                                                                    set_status(&mut items, item_id, UploadStatus::Uploading);
                                                                                    spawn(async move {
                                                                                        match upload_image_file(file).await {
                                                                                            Ok(_) => {
                                                                                                set_status(&mut items, item_id, UploadStatus::Done);
                                                                                                on_uploaded.call(());
                                                                                            }
                                                                                            Err(msg) => {
                                                                                                set_status(&mut items, item_id, UploadStatus::Failed(msg));
                                                                                            }
                                                                                        }
                                                                                    });
                                                                                }
                                                                            }
                                                                        },
                                                                        "重试"
                                                                    }
                                                                    button {
                                                                        class: "text-xs cursor-pointer text-[var(--color-paper-tertiary)] hover:text-[var(--color-paper-primary)] transition-colors shrink-0",
                                                                        aria_label: "移除",
                                                                        onclick: move |_| {
                                                                            // 先标记 removing 播退出动画；文件句柄立即释放，批任务取不到句柄会
                                                                            // 跳过该条（不会在淡出行上写状态）；EXIT_ANIM_MS 后真正摘除列表项。
                                                                            {
                                                                                let mut guard = items.write();
                                                                                if let Some(it) = guard.iter_mut().find(|it| it.id == item_id) {
                                                                                    it.removing = true;
                                                                                }
                                                                            }
                                                                            #[cfg(target_arch = "wasm32")]
                                                                            pool_for_remove
                                                                                .files
                                                                                .borrow_mut()
                                                                                .retain(|(fid, _)| *fid != item_id);
                                                                            spawn(async move {
                                                                                crate::utils::time::sleep_ms(EXIT_ANIM_MS).await;
                                                                                items.write().retain(|it| it.id != item_id);
                                                                            });
                                                                        },
                                                                        "×"
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
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_file, ALLOWED_MIME, MAX_UPLOAD_BYTES};

    /// 四种支持的类型在上限边缘全部接受。
    #[test]
    fn validate_file_accepts_supported_types_at_limit() {
        for mime in ALLOWED_MIME {
            assert!(
                validate_file(mime, MAX_UPLOAD_BYTES).is_ok(),
                "{mime} 应被接受"
            );
        }
    }

    /// svg 不在白名单（服务端同样拒绝）。
    #[test]
    fn validate_file_rejects_svg() {
        assert!(validate_file("image/svg+xml", 1024).is_err());
    }

    /// 空 MIME（浏览器给不出类型）也拒绝。
    #[test]
    fn validate_file_rejects_empty_mime() {
        assert!(validate_file("", 1024).is_err());
    }

    /// 上限 +1 字节拒绝。
    #[test]
    fn validate_file_rejects_oversize() {
        assert!(validate_file("image/png", MAX_UPLOAD_BYTES + 1).is_err());
    }
}
