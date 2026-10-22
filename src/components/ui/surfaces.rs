//! Positioned popovers, modal shells and the danger confirm dialog.
use dioxus::prelude::*;
/// Popover 遮罩与面板的层级(z-40 遮罩 < z-50 面板),与 Tooltip/lightbox 同 z-50。
const POPOVER_OVERLAY_CLASS: &str = "fixed inset-0 z-40";

/// 受控式通用 Popover(浮层)组件。
///
/// 与 [`Tooltip`] 对称——但 Tooltip 是纯 CSS hover、无状态;Popover 是点击触发、
/// 受控开关,用于承载确认框、轻量表单等交互内容。
///
/// ## 定位策略
///
/// 父容器常有 `overflow-hidden`(如表格面板),`position:absolute` 子节点
/// 会被裁掉,故面板用 **`position:fixed`**,以触发点击的**视口坐标**(`MouseEvent::
/// client_coordinates()`)作为锚点(参照 `theme.rs` 圆形展开动画的坐标用法)。无需
/// `getBoundingClientRect`/`node_ref`。
///
/// - `placement: "top"`(默认):面板底边贴点击点上方(`bottom: 100vh - y + gap`)。
/// - `placement: "bottom"`:面板顶边贴点击点下方(`top: y + gap`)。
/// - 水平由 `align` 决定:`"center"`(默认)面板中心对齐点击点(`left: x` +
///   `-translate-x-1/2`);`"start"` 面板左缘贴点向右延伸(`left: x`);
///   `"end"` 面板右缘贴点向左延伸(`right: 100vw - x`)。
///
/// 触发器贴近视口边缘时(如表格最右列的按钮),居中的宽面板会越出视口——
/// 此时用 `align` 让面板朝视口内侧延伸(与 Tooltip 的 align 同款约定,
/// 见 issue #14)。`end` 用 `right` 锚定右缘,与面板内容宽度无关,天然不越右缘。
///
/// ## 关闭路径(三条)
///
/// 1. 点遮罩(透明,仅作点击兜底)→ `on_close`。
/// 2. Escape 键 → `on_close`(组件内 `use_effect` 注册全局 keydown 监听,`use_drop` 清理)。
/// 3. 面板内确认/取消按钮调用 `on_close`。
///
/// ## Props
///
/// - `open`:受控开关;`false` 时组件不渲染任何内容(SSR 安全)。
/// - `anchor_x` / `anchor_y`:触发点击的视口坐标。
/// - `placement`:`"top"`(默认)/ `"bottom"`。
/// - `align`:`"center"`(默认)/ `"start"` / `"end"`(水平对齐,见上)。
/// - `children`:面板内容(确认框等)。
/// - `on_close`:任一关闭路径触发。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub fn Popover(
    open: bool,
    anchor_x: i32,
    anchor_y: i32,
    children: Element,
    on_close: EventHandler<()>,
    #[props(default = "top")] placement: &'static str,
    #[props(default = "center")] align: &'static str,
) -> Element {
    // Escape 关闭:组件 open 时注册全局 keydown 监听,关闭/卸载时移除。
    // 手写最小 listener 而非复用 use_event_listener——后者 handler 无参,拿不到
    // KeyboardEvent 的 key()。用 use_hook 持有 Closure,use_effect 注册,use_drop 清理。
    #[cfg(target_arch = "wasm32")]
    {
        use dioxus::prelude::{use_drop, use_effect, use_hook, use_reactive};
        use std::cell::RefCell;
        use std::rc::Rc;
        type EscState =
            Rc<RefCell<Option<wasm_bindgen::prelude::Closure<dyn FnMut(web_sys::KeyboardEvent)>>>>;
        let state: EscState = use_hook(|| Rc::new(RefCell::new(None)));
        let state_for_drop = state.clone();
        let open_for_effect = open;
        let on_close_for_esc = on_close;
        use_effect(use_reactive((&open_for_effect,), move |(is_open,)| {
            let Some(window) = web_sys::window() else {
                return;
            };
            if let Some(previous) = state.borrow_mut().take() {
                let _ = window.remove_event_listener_with_callback(
                    "keydown",
                    wasm_bindgen::JsCast::unchecked_ref(previous.as_ref()),
                );
            }
            if !is_open {
                return;
            }
            let on_close_for_esc = on_close_for_esc;
            // Closure 带 KeyboardEvent 参数:浏览器调用 handler 时传入事件对象,
            // 无需依赖已废弃的 window.event()。as_ref + unchecked_ref 转成 JS Function。
            let closure =
                wasm_bindgen::prelude::Closure::wrap(Box::new(move |ev: web_sys::KeyboardEvent| {
                    if ev.key() == "Escape" {
                        on_close_for_esc.call(());
                    }
                })
                    as Box<dyn FnMut(web_sys::KeyboardEvent)>);
            let _ = window.add_event_listener_with_callback(
                "keydown",
                wasm_bindgen::JsCast::unchecked_ref(closure.as_ref()),
            );
            *state.borrow_mut() = Some(closure);
        }));
        use_drop(move || {
            if let Some(closure) = state_for_drop.borrow_mut().take() {
                if let Some(window) = web_sys::window() {
                    let _ = window.remove_event_listener_with_callback(
                        "keydown",
                        wasm_bindgen::JsCast::unchecked_ref(closure.as_ref()),
                    );
                }
            }
        });
    }

    if !open {
        return rsx! {};
    }

    // 面板定位:placement 决定垂直方向;align 决定水平。end 用 right 锚定
    // (右缘 = 点击点),与面板宽度解耦。
    // 居中由 .popover-panel[data-align="center"] 的 translate 属性完成，
    // 与入场动画的 transform（只缩放）互不干扰。
    let (horizontal, align_attr) = match align {
        "start" => (format!("left: {x}px;", x = anchor_x), "start"),
        "end" => (format!("right: calc(100vw - {x}px);", x = anchor_x), "end"),
        _ => (format!("left: {x}px;", x = anchor_x), "center"),
    };
    let style = if placement == "bottom" {
        format!("top: {y}px; {horizontal}", y = anchor_y + 8)
    } else {
        // top:面板在点击点上方——用 bottom 锚定 viewport 底,差值即视口高度 - y + 间隙。
        // 视口高度用 100vh,纯 CSS 无需 JS 读取 scrollHeight。
        format!(
            "bottom: calc(100vh - {y}px + 8px); {horizontal}",
            y = anchor_y
        )
    };
    let placement_attr = if placement == "bottom" {
        "bottom"
    } else {
        "top"
    };

    rsx! {
        // 透明遮罩:拦截外部点击(点遮罩即关)。z-40 < 面板 z-50。
        div {
            class: "{POPOVER_OVERLAY_CLASS}",
            onclick: move |_| on_close.call(()),
        }
        // 面板:fixed 定位逃出 overflow-hidden 容器。
        div {
            class: "popover-panel",
            "data-placement": placement_attr,
            "data-align": align_attr,
            style: "{style}",
            {children}
        }
    }
}

/// 弹窗关闭/元素退出动画统一时长（ms），与 input.css 里 `.modal-panel` / `.animate-row-leave`
/// 等退出过渡动画时长一一对应；调用方需要在动画播放完毕后再真正卸载/摘除元素时复用本常量。
pub const EXIT_ANIM_MS: u32 = 200;

/// 弹窗遮罩基础类（不含内边距——不同弹窗尺寸的响应式内边距不同，经 `overlay_padding` 传入）。
const MODAL_OVERLAY_BASE: &str = "dialog-overlay modal-overlay animate-modal-overlay-enter";
/// 弹窗面板基础类（不含宽度——不同弹窗尺寸不同，经 `panel_class` 传入追加）。
const MODAL_PANEL_BASE: &str = "dialog-panel modal-panel animate-modal-panel-enter";

/// 焦点锁定：Tab / Shift+Tab 在最上层打开的弹窗内循环，焦点不会跑到页面背后。
#[cfg(target_arch = "wasm32")]
pub(crate) fn trap_tab(event: &web_sys::KeyboardEvent, title: &str) {
    use wasm_bindgen::JsCast;
    const FOCUSABLE: &str = "a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])";
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Ok(panels) =
        document.query_selector_all("[data-ygg-modal-panel][data-ygg-modal-open='true']")
    else {
        return;
    };
    let panel = (0..panels.length())
        .rev()
        .filter_map(|i| panels.item(i))
        .filter_map(|node| node.dyn_into::<web_sys::Element>().ok())
        .find(|panel| panel.get_attribute("aria-label").as_deref() == Some(title));
    let Some(panel) = panel else {
        return;
    };
    let Ok(nodes) = panel.query_selector_all(FOCUSABLE) else {
        return;
    };
    let focusable: Vec<web_sys::HtmlElement> = (0..nodes.length())
        .filter_map(|i| nodes.item(i))
        .filter_map(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
        .filter(|el| el.offset_parent().is_some())
        .collect();
    let (Some(first), Some(last)) = (focusable.first(), focusable.last()) else {
        event.prevent_default();
        return;
    };
    let active = document.active_element();
    let inside = active
        .as_ref()
        .is_some_and(|el| panel.contains(Some(el.unchecked_ref())));
    let at = |target: &web_sys::HtmlElement| {
        active
            .as_ref()
            .is_some_and(|el| el.is_same_node(Some(target.unchecked_ref())))
    };
    if event.shift_key()
        && (!inside
            || at(first)
            || active
                .as_ref()
                .is_some_and(|el| el.is_same_node(Some(panel.unchecked_ref()))))
    {
        event.prevent_default();
        let _ = last.focus();
    } else if !event.shift_key() && (!inside || at(last)) {
        event.prevent_default();
        let _ = first.focus();
    }
}

/// 通用弹窗外壳：遮罩 + 面板容器 + 开合动画状态机（`opened` / `closing` / 退出计时）。
///
/// 收敛 `AssetUploadModal` / `AssetPickerModal` 曾各自照抄的一份遮罩 + 面板 + 开合动画
/// 状态机，顺带补齐两者曾意外分叉的 `role="dialog"` / `aria-modal` / `aria-label`
/// （此前只有 `AssetPickerModal` 有）。调用方仍各自负责标题栏、内容区等具体内容
/// （经 `children` 传入），以及自身域相关的开关副作用（如重置搜索词/选中集）。
///
/// ## 关闭路径与同帧动画
///
/// 本组件自身处理遮罩点击关闭。调用方若在 `children` 内还有别的关闭入口
/// （标题栏 × 按钮、选中即关闭等），必须用与本组件相同的写法**同步**先置
/// `closing.set(true)` 再置 `visible.set(false)`——同一帧内让存活元素换上
/// `.is-closing` 类，过渡动画才能从可见态播放（只翻 `visible`、等下一帧的
/// `use_effect` 兜底补置 `closing` 为时已晚，元素已被提前卸载，动画会变成
/// 瞬间消失而非淡出）。因此 `visible` / `closing` 是与调用方共享的 `Signal`，
/// 而非本组件内部私有状态；`opened`（mount 守卫）才是本组件私有的。
///
/// ## Props
///
/// - `visible` / `closing`：与调用方共享的显隐 / 关闭动画信号。
/// - `title`：面板的 `aria-label`（不渲染可见标题——各调用方的标题栏自行渲染）。
/// - `role`：面板角色，默认 `dialog`；危险确认传 `alertdialog`。
/// - `overlay_padding`：遮罩内边距（响应式内边距因弹窗尺寸而异，默认 `"p-6"`）。
/// - `panel_class`：在共享面板底座上追加的宽度等布局类（默认 `"w-full max-w-lg"`）。
/// - `children`：面板内容（含调用方自己的标题栏、× 按钮等）。
#[component]
pub fn ModalShell(
    mut visible: Signal<bool>,
    mut closing: Signal<bool>,
    title: &'static str,
    #[props(default = "dialog")] role: &'static str,
    #[props(default = "p-6")] overlay_padding: &'static str,
    #[props(default = "w-full max-w-lg")] panel_class: &'static str,
    children: Element,
) -> Element {
    // 公共弹窗在打开时接管焦点，Escape 关闭后还给触发器；监听随显隐同步清理。
    #[cfg(target_arch = "wasm32")]
    {
        use std::cell::RefCell;
        use std::rc::Rc;
        use wasm_bindgen::JsCast;

        type KeyListener =
            Rc<RefCell<Option<wasm_bindgen::prelude::Closure<dyn FnMut(web_sys::KeyboardEvent)>>>>;
        let listener: KeyListener = use_hook(|| Rc::new(RefCell::new(None)));
        let previous_focus: Rc<RefCell<Option<web_sys::Element>>> =
            use_hook(|| Rc::new(RefCell::new(None)));
        let listener_for_drop = listener.clone();
        let mut close_visible = visible;
        let mut close_closing = closing;
        use_effect(move || {
            let is_visible = visible();
            let Some(window) = web_sys::window() else {
                return;
            };
            if let Some(old) = listener.borrow_mut().take() {
                let _ = window
                    .remove_event_listener_with_callback("keydown", old.as_ref().unchecked_ref());
            }
            if is_visible {
                if let Some(document) = window.document() {
                    *previous_focus.borrow_mut() = document.active_element();
                    if let Ok(panels) = document
                        .query_selector_all("[data-ygg-modal-panel][data-ygg-modal-open='true']")
                    {
                        for index in (0..panels.length()).rev() {
                            if let Some(panel) = panels
                                .item(index)
                                .and_then(|node| node.dyn_into::<web_sys::Element>().ok())
                            {
                                if panel.get_attribute("aria-label").as_deref() == Some(title) {
                                    if let Some(element) = panel.dyn_ref::<web_sys::HtmlElement>() {
                                        let _ = element.focus();
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
                let closure = wasm_bindgen::prelude::Closure::wrap(Box::new(
                    move |event: web_sys::KeyboardEvent| {
                        if event.key() == "Escape" {
                            close_closing.set(true);
                            close_visible.set(false);
                        } else if event.key() == "Tab" {
                            trap_tab(&event, title);
                        }
                    },
                )
                    as Box<dyn FnMut(web_sys::KeyboardEvent)>);
                let _ = window
                    .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
                *listener.borrow_mut() = Some(closure);
            } else if let Some(element) = previous_focus.borrow_mut().take() {
                if let Some(element) = element.dyn_ref::<web_sys::HtmlElement>() {
                    let _ = element.focus();
                }
            }
        });
        use_drop(move || {
            if let Some(old) = listener_for_drop.borrow_mut().take() {
                if let Some(window) = web_sys::window() {
                    let _ = window.remove_event_listener_with_callback(
                        "keydown",
                        old.as_ref().unchecked_ref(),
                    );
                }
            }
        });
    }

    // 是否曾开过弹窗：mount 时 visible=false 也会跑一次 use_effect，不加此守卫会
    // 在页面加载后 EXIT_ANIM_MS 内渲染一层透明遮罩（opacity:0 仍拦截点击）吞掉首次点击。
    let mut opened = use_signal(|| false);

    // visible 翻转驱动关闭动画：关闭入口（× / 遮罩 / Esc）会同步先置 closing 再翻
    // visible（见本组件 doc）；这里只负责重开复位与 EXIT_ANIM_MS 后的复位卸载。
    // 闭包只订阅 visible，closing/opened 全用 peek 防自触发循环。
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

    // closing() 的订阅读是必需的——closing.set 靠这个订阅触发重渲染，peek 不会重绘。
    let is_closing = closing();
    if !visible() && !is_closing {
        return rsx! {};
    }

    rsx! {
        // 遮罩：点击关闭。
        div {
            class: "{MODAL_OVERLAY_BASE} {overlay_padding}",
            class: if is_closing { "is-closing" } else { "" },
            onclick: move |_| {
                closing.set(true);
                visible.set(false);
            },
            // 面板：阻止点击穿透到遮罩。
            div {
                class: "{MODAL_PANEL_BASE} {panel_class}",
                role,
                tabindex: "-1",
                "data-ygg-modal-panel": "true",
                "data-ygg-modal-open": if visible() { "true" } else { "false" },
                aria_modal: "true",
                aria_label: "{title}",
                onclick: move |evt| evt.stop_propagation(),
                {children}
            }
        }
    }
}

/// 危险操作确认对话框（`alertdialog`）：左侧 danger 图标、标题与说明，底栏「取消」与
/// danger 主按钮。点确认先关闭再回调 `on_confirm`，执行与结果提示由调用方负责。
/// `title` 同时作为面板的无障碍名称，同一时刻打开的对话框标题不能重复。
#[component]
pub fn ConfirmDialog(
    mut visible: Signal<bool>,
    title: &'static str,
    #[props(into)] description: String,
    confirm_label: &'static str,
    #[props(default = super::IconName::Trash)] icon: super::IconName,
    on_confirm: EventHandler<()>,
) -> Element {
    use super::{button_class, ButtonSize, ButtonVariant, Icon};

    let mut closing = use_signal(|| false);
    let mut close = move || {
        closing.set(true);
        visible.set(false);
    };
    rsx! {
        ModalShell {
            visible,
            closing,
            title,
            role: "alertdialog",
            overlay_padding: "p-4",
            panel_class: "confirm-dialog",
            div { class: "confirm-dialog-body",
                span { class: "confirm-dialog-icon", aria_hidden: "true", Icon { name: icon, size: 18 } }
                div {
                    h2 { "{title}" }
                    p { "{description}" }
                }
            }
            div { class: "confirm-dialog-actions",
                button {
                    class: button_class(ButtonVariant::Ghost, ButtonSize::Md),
                    r#type: "button",
                    onclick: move |_| close(),
                    "取消"
                }
                button {
                    class: button_class(ButtonVariant::Danger, ButtonSize::Md),
                    r#type: "button",
                    onclick: move |_| {
                        close();
                        on_confirm.call(());
                    },
                    "{confirm_label}"
                }
            }
        }
    }
}
