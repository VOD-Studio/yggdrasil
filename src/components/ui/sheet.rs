//! 抽屉：手机上的导航与目录面板。底部抽屉样式见 shell.css 的 `.sheet`，
//! 左侧抽屉（后台导航）见 admin.css 的 `.side-sheet`。
use dioxus::prelude::*;

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
        self.0.call(());
    }
}

/// 在 [`BottomSheet`] 的子组件里取得关闭句柄。
pub fn use_sheet_close() -> SheetClose {
    use_context::<SheetClose>()
}

/// 底部抽屉。
///
/// 从底部以 heavy 弹簧升起；点遮罩、按 Esc、点按或向下拖过 80px 把手关闭。打开时聚焦
/// 标有 `aria-current="page"` 的项，没有时聚焦第一个链接或按钮；关闭后焦点回到
/// `trigger_id` 指向的按钮。打开期间锁住页面滚动。子组件用 [`use_sheet_close`] 关闭。
#[component]
pub fn BottomSheet(
    open: Signal<bool>,
    id: String,
    trigger_id: String,
    label: String,
    children: Element,
) -> Element {
    let mut open = open;
    let mut closing = use_signal(|| false);
    let mut drag_start = use_signal(|| None::<f64>);
    let mut drag_dy = use_signal(|| 0.0_f64);
    let mut was_open = use_signal(|| false);

    let focus_id = id.clone();
    use_effect(move || {
        let is_open = open();
        if is_open == *was_open.peek() {
            return;
        }
        was_open.set(is_open);
        lock_page_scroll(is_open);
        if is_open {
            focus_selectors(&[
                format!("#{focus_id} [aria-current='page']"),
                format!("#{focus_id} .sheet-body :is(a[href], button)"),
            ]);
        }
    });
    // 卸载时（例如抽屉打开时跳到别的页面）解除滚动锁。
    use_drop(move || lock_page_scroll(false));

    let close = use_callback(move |()| {
        if !open() || closing() {
            return;
        }
        closing.set(true);
        drag_start.set(None);
        focus_selectors(&[format!("#{trigger_id}")]);
        spawn(async move {
            crate::utils::time::sleep_ms(SHEET_EXIT_MS).await;
            open.set(false);
            closing.set(false);
            drag_dy.set(0.0);
        });
    });
    use_context_provider(|| SheetClose(close));

    if !open() {
        return rsx! {};
    }

    let dy = drag_dy();
    let dragging = drag_start().is_some();
    let sheet_style = if dy > 0.0 {
        format!("transform: translateY({dy}px);")
    } else {
        String::new()
    };
    let handle_id = format!("{id}-handle");

    rsx! {
        div {
            class: "sheet-scrim",
            "data-closing": closing().then_some("true"),
            onclick: move |_| close.call(()),
        }
        div {
            id: "{id}",
            class: "sheet",
            role: "dialog",
            aria_modal: "true",
            aria_label: "{label}",
            "data-closing": closing().then_some("true"),
            "data-dragging": dragging.then_some("true"),
            style: "{sheet_style}",
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    e.prevent_default();
                    close.call(());
                }
            },
            button {
                id: "{handle_id}",
                class: "sheet-handle",
                r#type: "button",
                aria_label: "向下拖动或点按关闭",
                onpointerdown: move |e| {
                    drag_start.set(Some(e.client_coordinates().y));
                    // 捕获指针：手指移出把手后仍能收到移动与抬起事件。
                    #[cfg(target_arch = "wasm32")]
                    if let Some(el) = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.get_element_by_id(&handle_id))
                    {
                        let _ = el.set_pointer_capture(e.pointer_id());
                    }
                },
                onpointermove: move |e| {
                    if let Some(start) = drag_start() {
                        drag_dy.set(drag_offset(start, e.client_coordinates().y));
                    }
                },
                onpointerup: move |_| {
                    let moved = drag_dy();
                    drag_start.set(None);
                    if moved > DRAG_CLOSE_DISTANCE || moved == 0.0 {
                        close.call(());
                    } else {
                        drag_dy.set(0.0);
                    }
                },
                onpointercancel: move |_| {
                    drag_start.set(None);
                    drag_dy.set(0.0);
                },
            }
            div { class: "sheet-body", {children} }
        }
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
#[component]
pub fn SideSheet(
    open: Signal<bool>,
    id: String,
    trigger_id: String,
    label: String,
    children: Element,
) -> Element {
    let mut open = open;
    let mut closing = use_signal(|| false);
    let mut press = use_signal(|| None::<(f64, f64)>);
    let mut drag_dx = use_signal(|| None::<f64>);
    let mut was_open = use_signal(|| false);

    let focus_id = id.clone();
    use_effect(move || {
        let is_open = open();
        if is_open == *was_open.peek() {
            return;
        }
        was_open.set(is_open);
        lock_page_scroll(is_open);
        if is_open {
            focus_selectors(&[
                format!("#{focus_id} [aria-current='page']"),
                format!("#{focus_id} :is(a[href], button)"),
            ]);
        }
    });
    use_drop(move || lock_page_scroll(false));

    let close = use_callback(move |()| {
        if !open() || closing() {
            return;
        }
        closing.set(true);
        press.set(None);
        focus_selectors(&[format!("#{trigger_id}")]);
        spawn(async move {
            crate::utils::time::sleep_ms(SHEET_EXIT_MS).await;
            open.set(false);
            closing.set(false);
            drag_dx.set(None);
        });
    });
    use_context_provider(|| SheetClose(close));

    if !open() {
        return rsx! {};
    }

    let dx = drag_dx();
    let sheet_style = dx
        .map(|dx| format!("transform: translateX({dx}px);"))
        .unwrap_or_default();
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
    let panel_id = id.clone();

    rsx! {
        div {
            class: "sheet-scrim",
            "data-closing": closing().then_some("true"),
            onclick: move |_| close.call(()),
        }
        div {
            id: "{id}",
            class: "side-sheet",
            role: "dialog",
            aria_modal: "true",
            aria_label: "{label}",
            "data-closing": closing().then_some("true"),
            "data-dragging": dx.is_some().then_some("true"),
            style: "{sheet_style}",
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    e.prevent_default();
                    close.call(());
                }
            },
            onpointerdown: move |e| {
                let point = e.client_coordinates();
                press.set(Some((point.x, point.y)));
            },
            onpointermove: move |e| {
                let Some(start) = press() else {
                    return;
                };
                let point = e.client_coordinates();
                if let Some(offset) = side_drag_offset(start, (point.x, point.y)) {
                    // 开始跟手后捕获指针：手指移出抽屉仍能收到抬起事件。
                    #[cfg(target_arch = "wasm32")]
                    if drag_dx.peek().is_none() {
                        if let Some(el) = web_sys::window()
                            .and_then(|w| w.document())
                            .and_then(|d| d.get_element_by_id(&panel_id))
                        {
                            let _ = el.set_pointer_capture(e.pointer_id());
                        }
                    }
                    drag_dx.set(Some(offset));
                } else if drag_dx.peek().is_some() {
                    drag_dx.set(Some(0.0));
                }
            },
            onpointerup: move |_| {
                press.set(None);
                if drag_dx().is_some_and(|dx| -dx > SIDE_CLOSE_DISTANCE) {
                    close.call(());
                } else {
                    drag_dx.set(None);
                }
            },
            onpointercancel: move |_| {
                press.set(None);
                drag_dx.set(None);
            },
            {children}
        }
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
