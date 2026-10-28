//! 抽屉：手机上的导航与目录面板。底部抽屉样式见 shell.css 的 `.sheet`，
//! 左侧抽屉（后台导航）见 admin.css 的 `.side-sheet`。
use leptos::prelude::*;

/// 关闭动画时长（ms），与 shell.css 的 `sheet-out` 一致。
const SHEET_EXIT_MS: u32 = 200;
/// 拖拽死区：手指移动超过 4px 才开始跟手，避免轻点误拖。
const DRAG_DEAD_ZONE: f64 = 4.0;
/// 向下拖过该距离松手即关闭。
const DRAG_CLOSE_DISTANCE: f64 = 80.0;

/// 左侧抽屉向左拖过该距离松手即关闭。
const SIDE_CLOSE_DISTANCE: f64 = 80.0;

/// 拖拽位移：死区内为 0，向上拖不跟手（抽屉已贴底）。
fn drag_offset(start_y: f64, current_y: f64) -> f64 {
    let dy = current_y - start_y;
    if dy <= DRAG_DEAD_ZONE {
        0.0
    } else {
        dy - DRAG_DEAD_ZONE
    }
}

/// 抽屉内容用来关闭抽屉（带退出动画、焦点还给触发按钮）。
#[derive(Clone, Copy)]
pub struct SheetClose(Callback<()>);

impl SheetClose {
    pub fn close(&self) {
        self.0.run(());
    }
}

/// 在 [`BottomSheet`] / [`SideSheet`] 的子组件里取得关闭句柄。
pub fn use_sheet_close() -> SheetClose {
    expect_context::<SheetClose>()
}

/// 两种抽屉共用的开合生命周期，返回 `(closing, close)`：
///
/// - `open` 翻转时锁定 / 解除页面滚动，打开时按 `focus_targets` 顺序聚焦第一个匹配项；
/// - 卸载时解除滚动锁（例如抽屉打开时跳到别的页面）；
/// - `close`：播放退出动画（[`SHEET_EXIT_MS`]）后真正关闭，焦点立即还给 `trigger_id`；
///   `on_close_start` 在动画开始前调用（清掉进行中的拖拽），`on_closed` 在关闭后调用（复位位移）；
/// - 通过 context 提供 [`SheetClose`] 给抽屉内容。
fn use_sheet_lifecycle(
    open: RwSignal<bool>,
    trigger_id: String,
    focus_targets: Vec<String>,
    on_close_start: impl Fn() + Send + Sync + 'static,
    on_closed: impl Fn() + Send + Sync + 'static,
) -> (RwSignal<bool>, Callback<()>) {
    let closing = RwSignal::new(false);
    let was_open = RwSignal::new(false);

    Effect::new(move |_| {
        let is_open = open.get();
        if is_open == was_open.get_untracked() {
            return;
        }
        was_open.set(is_open);
        lock_page_scroll(is_open);
        if is_open {
            focus_selectors(&focus_targets);
        }
    });
    // 卸载时（例如抽屉打开时跳到别的页面）解除滚动锁。
    on_cleanup(move || lock_page_scroll(false));

    let on_closed = std::sync::Arc::new(on_closed);
    let close = Callback::new(move |()| {
        if !open.get_untracked() || closing.get_untracked() {
            return;
        }
        closing.set(true);
        on_close_start();
        focus_selectors(&[format!("#{trigger_id}")]);
        let on_closed = on_closed.clone();
        leptos::task::spawn_local(async move {
            crate::utils::time::sleep_ms(SHEET_EXIT_MS).await;
            let _ = open.try_set(false);
            let _ = closing.try_set(false);
            on_closed();
        });
    });
    provide_context(SheetClose(close));
    (closing, close)
}

/// 底部抽屉。
///
/// 从底部以 heavy 弹簧升起；点遮罩、按 Esc、点按或向下拖过 80px 把手关闭。打开时聚焦
/// 标有 `aria-current="page"` 的项，没有时聚焦第一个链接或按钮；关闭后焦点回到
/// `trigger_id` 指向的按钮。打开期间锁住页面滚动。子组件用 [`use_sheet_close`] 关闭。
///
/// Props：
/// - `open`：开关（读写，关闭动画结束后由组件置回 `false`）
/// - `id` / `trigger_id` / `label`：面板 id、触发按钮 id 与无障碍名称
/// - `children`：抽屉内容（打开时才渲染，每次打开重新创建）
#[component]
pub fn BottomSheet(
    open: RwSignal<bool>,
    #[prop(into)] id: String,
    #[prop(into)] trigger_id: String,
    #[prop(into)] label: String,
    children: ChildrenFn,
) -> impl IntoView {
    let drag_start = RwSignal::new(None::<f64>);
    let drag_dy = RwSignal::new(0.0_f64);

    let (closing, close) = use_sheet_lifecycle(
        open,
        trigger_id,
        vec![
            format!("#{id} [aria-current='page']"),
            format!("#{id} .sheet-body :is(a[href], button)"),
        ],
        move || drag_start.set(None),
        move || {
            let _ = drag_dy.try_set(0.0);
        },
    );

    move || {
        open.get().then(|| {
            let id = id.clone();
            let handle_id = format!("{id}-handle");
            view! {
                <div
                    class="sheet-scrim"
                    data-closing=move || closing.get().then_some("true")
                    on:click=move |_| close.run(())
                ></div>
                <div
                    id=id
                    class="sheet"
                    role="dialog"
                    aria-modal="true"
                    aria-label=label.clone()
                    data-closing=move || closing.get().then_some("true")
                    data-dragging=move || drag_start.get().is_some().then_some("true")
                    style=move || {
                        let dy = drag_dy.get();
                        if dy > 0.0 { format!("transform: translateY({dy}px);") } else { String::new() }
                    }
                    on:keydown=move |e| {
                        if e.key() == "Escape" {
                            e.prevent_default();
                            close.run(());
                        }
                    }
                >
                    <button
                        id=handle_id.clone()
                        class="sheet-handle"
                        type="button"
                        aria-label="向下拖动或点按关闭"
                        on:pointerdown=move |e| {
                            drag_start.set(Some(e.client_y() as f64));
                            // 捕获指针：手指移出把手后仍能收到移动与抬起事件。
                            #[cfg(target_arch = "wasm32")]
                            if let Some(el) = web_sys::window()
                                .and_then(|w| w.document())
                                .and_then(|d| d.get_element_by_id(&handle_id))
                            {
                                let _ = el.set_pointer_capture(e.pointer_id());
                            }
                        }
                        on:pointermove=move |e| {
                            if let Some(start) = drag_start.get_untracked() {
                                drag_dy.set(drag_offset(start, e.client_y() as f64));
                            }
                        }
                        on:pointerup=move |_| {
                            let moved = drag_dy.get_untracked();
                            drag_start.set(None);
                            if moved > DRAG_CLOSE_DISTANCE || moved == 0.0 {
                                close.run(());
                            } else {
                                drag_dy.set(0.0);
                            }
                        }
                        on:pointercancel=move |_| {
                            drag_start.set(None);
                            drag_dy.set(0.0);
                        }
                    ></button>
                    <div class="sheet-body">{children()}</div>
                </div>
            }
        })
    }
}

/// 左侧抽屉的拖拽位移（≤ 0）：只认向左、且水平位移大于竖直位移的拖动（竖直方向留给
/// 抽屉内滚动），并减去 4px 死区。返回 `None` 表示这次按下还不算拖拽。
fn side_drag_offset(start: (f64, f64), current: (f64, f64)) -> Option<f64> {
    let dx = current.0 - start.0;
    let dy = current.1 - start.1;
    if dx >= -DRAG_DEAD_ZONE || dx.abs() <= dy.abs() {
        return None;
    }
    Some(dx + DRAG_DEAD_ZONE)
}

/// 打开期间锁住页面滚动（与底部抽屉共用 `html.sheet-open`）。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
fn lock_page_scroll(locked: bool) {
    #[cfg(target_arch = "wasm32")]
    if let Some(html) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    {
        let _ = html.class_list().toggle_with_force("sheet-open", locked);
    }
}

/// 聚焦第一个能匹配到的选择器（仅 wasm）。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
fn focus_selectors(selectors: &[String]) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        if let Some(el) = selectors
            .iter()
            .find_map(|selector| document.query_selector(selector).ok().flatten())
            .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = el.focus();
        }
    }
}

/// 左侧抽屉。
///
/// 从左侧以 heavy 弹簧滑出；点遮罩、按 Esc 或向左拖过 80px 关闭（4px 死区，竖直拖动
/// 留给内容滚动）。打开时聚焦 `aria-current="page"` 的项，没有时聚焦第一个链接或按钮；
/// 关闭后焦点回到 `trigger_id`。打开期间锁住页面滚动。子组件用 [`use_sheet_close`] 关闭。
///
/// Props 同 [`BottomSheet`]。
#[component]
pub fn SideSheet(
    open: RwSignal<bool>,
    #[prop(into)] id: String,
    #[prop(into)] trigger_id: String,
    #[prop(into)] label: String,
    children: ChildrenFn,
) -> impl IntoView {
    let press = RwSignal::new(None::<(f64, f64)>);
    let drag_dx = RwSignal::new(None::<f64>);

    let (closing, close) = use_sheet_lifecycle(
        open,
        trigger_id,
        vec![
            format!("#{id} [aria-current='page']"),
            format!("#{id} :is(a[href], button)"),
        ],
        move || press.set(None),
        move || {
            let _ = drag_dx.try_set(None);
        },
    );

    move || {
        open.get().then(|| {
            let id = id.clone();
            #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
            let panel_id = id.clone();
            view! {
                <div
                    class="sheet-scrim"
                    data-closing=move || closing.get().then_some("true")
                    on:click=move |_| close.run(())
                ></div>
                <div
                    id=id
                    class="side-sheet"
                    role="dialog"
                    aria-modal="true"
                    aria-label=label.clone()
                    data-closing=move || closing.get().then_some("true")
                    data-dragging=move || drag_dx.get().is_some().then_some("true")
                    style=move || {
                        drag_dx
                            .get()
                            .map(|dx| format!("transform: translateX({dx}px);"))
                            .unwrap_or_default()
                    }
                    on:keydown=move |e| {
                        if e.key() == "Escape" {
                            e.prevent_default();
                            close.run(());
                        }
                    }
                    on:pointerdown=move |e| {
                        press.set(Some((e.client_x() as f64, e.client_y() as f64)));
                    }
                    on:pointermove=move |e| {
                        let Some(start) = press.get_untracked() else {
                            return;
                        };
                        let point = (e.client_x() as f64, e.client_y() as f64);
                        if let Some(offset) = side_drag_offset(start, point) {
                            // 开始跟手后捕获指针：手指移出抽屉仍能收到抬起事件。
                            #[cfg(target_arch = "wasm32")]
                            if drag_dx.get_untracked().is_none() {
                                if let Some(el) = web_sys::window()
                                    .and_then(|w| w.document())
                                    .and_then(|d| d.get_element_by_id(&panel_id))
                                {
                                    let _ = el.set_pointer_capture(e.pointer_id());
                                }
                            }
                            drag_dx.set(Some(offset));
                        } else if drag_dx.get_untracked().is_some() {
                            drag_dx.set(Some(0.0));
                        }
                    }
                    on:pointerup=move |_| {
                        press.set(None);
                        if drag_dx.get_untracked().is_some_and(|dx| -dx > SIDE_CLOSE_DISTANCE) {
                            close.run(());
                        } else {
                            drag_dx.set(None);
                        }
                    }
                    on:pointercancel=move |_| {
                        press.set(None);
                        drag_dx.set(None);
                    }
                >
                    {children()}
                </div>
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{drag_offset, side_drag_offset};

    #[test]
    fn side_drag_offset_only_tracks_leftward_horizontal_drags() {
        assert_eq!(side_drag_offset((100.0, 100.0), (97.0, 100.0)), None); // 死区内
        assert_eq!(side_drag_offset((100.0, 100.0), (140.0, 100.0)), None); // 向右
        assert_eq!(side_drag_offset((100.0, 100.0), (80.0, 160.0)), None); // 竖直为主
        assert_eq!(side_drag_offset((100.0, 100.0), (50.0, 110.0)), Some(-46.0));
    }

    #[test]
    fn drag_offset_ignores_small_moves_and_upward_drags() {
        assert_eq!(drag_offset(100.0, 103.0), 0.0); // 死区内
        assert_eq!(drag_offset(100.0, 90.0), 0.0); // 向上
        assert_eq!(drag_offset(100.0, 150.0), 46.0); // 减去 4px 死区
    }
}
