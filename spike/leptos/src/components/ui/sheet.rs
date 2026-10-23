//! 抽屉：手机上的导航与目录面板。底部抽屉样式见 shell.css 的 `.sheet`。
use leptos::prelude::*;

/// 关闭动画时长（ms），与 shell.css 的 `sheet-out` 一致。
const SHEET_EXIT_MS: u32 = 200;
/// 拖拽死区：手指移动超过 4px 才开始跟手，避免轻点误拖。
const DRAG_DEAD_ZONE: f64 = 4.0;
/// 向下拖过该距离松手即关闭。
const DRAG_CLOSE_DISTANCE: f64 = 80.0;

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

/// 在 [`BottomSheet`] 的子组件里取得关闭句柄。
pub fn use_sheet_close() -> SheetClose {
    expect_context::<SheetClose>()
}

/// 底部抽屉。
///
/// 从底部以 heavy 弹簧升起；点遮罩、按 Esc、点按或向下拖过 80px 把手关闭。打开时聚焦
/// 标有 `aria-current="page"` 的项，没有时聚焦第一个链接或按钮；关闭后焦点回到
/// `trigger_id` 指向的按钮。打开期间锁住页面滚动。子组件用 [`use_sheet_close`] 关闭。
#[component]
pub fn BottomSheet(
    open: RwSignal<bool>,
    #[prop(into)] id: String,
    #[prop(into)] trigger_id: String,
    #[prop(into)] label: String,
    children: ChildrenFn,
) -> impl IntoView {
    let closing = RwSignal::new(false);
    let drag_start = RwSignal::new(None::<f64>);
    let drag_dy = RwSignal::new(0.0_f64);
    let was_open = RwSignal::new(false);

    let focus_id = id.clone();
    Effect::new(move |_| {
        let is_open = open.get();
        if is_open == was_open.get_untracked() {
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
    on_cleanup(move || lock_page_scroll(false));

    let close = Callback::new(move |()| {
        if !open.get_untracked() || closing.get_untracked() {
            return;
        }
        closing.set(true);
        drag_start.set(None);
        focus_selectors(&[format!("#{trigger_id}")]);
        leptos::task::spawn_local(async move {
            crate::utils::time::sleep_ms(SHEET_EXIT_MS).await;
            open.set(false);
            closing.set(false);
            drag_dy.set(0.0);
        });
    });
    provide_context(SheetClose(close));

    let id = StoredValue::new(id);
    let label = StoredValue::new(label);
    let children = StoredValue::new(children);

    view! {
        <Show when=move || open.get()>
            {move || {
                let id = id.get_value();
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
                        aria-label=label.get_value()
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
                        <div class="sheet-body">{children.with_value(|c| c())}</div>
                    </div>
                }
            }}
        </Show>
    }
}

/// 打开期间锁住页面滚动（`html.sheet-open`）。
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
