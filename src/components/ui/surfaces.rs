//! Positioned popovers, modal shells and the danger confirm dialog.
use leptos::ev;
use leptos::prelude::*;

/// Popover 遮罩与面板的层级(z-40 遮罩 < z-50 面板),与 Tooltip/lightbox 同 z-50。
const POPOVER_OVERLAY_CLASS: &str = "fixed inset-0 z-40";

/// 受控式通用 Popover(浮层)组件。
///
/// 与 [`Tooltip`](super::Tooltip) 对称——但 Tooltip 是纯 CSS hover、无状态;Popover 是点击触发、
/// 受控开关,用于承载确认框、轻量表单等交互内容。
///
/// ## 定位策略
///
/// 父容器常有 `overflow-hidden`(如表格面板),`position:absolute` 子节点
/// 会被裁掉,故面板用 **`position:fixed`**,以触发点击的**视口坐标**(`MouseEvent::
/// client_x()/client_y()`)作为锚点(参照 `theme.rs` 圆形展开动画的坐标用法)。无需
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
/// 2. Escape 键 → `on_close`(`open` 期间注册 window 级 keydown 监听,关闭/卸载时移除)。
/// 3. 面板内确认/取消按钮调用 `on_close`。
///
/// ## Props
///
/// - `open`:受控开关(值、信号或闭包);`false` 时组件不渲染任何内容(SSR 安全)。
/// - `anchor_x` / `anchor_y`:触发点击的视口坐标(值、信号或闭包)。
/// - `placement`:`"top"`(默认)/ `"bottom"`。
/// - `align`:`"center"`(默认)/ `"start"` / `"end"`(水平对齐,见上)。
/// - `children`:面板内容(确认框等)。
/// - `on_close`:任一关闭路径触发。
#[component]
pub fn Popover(
    #[prop(into)] open: Signal<bool>,
    #[prop(into)] anchor_x: Signal<i32>,
    #[prop(into)] anchor_y: Signal<i32>,
    children: ChildrenFn,
    #[prop(into)] on_close: Callback<()>,
    #[prop(default = "top".to_string(), into)] placement: String,
    #[prop(default = "center".to_string(), into)] align: String,
) -> impl IntoView {
    // Escape 关闭:组件 open 时注册全局 keydown 监听,关闭/卸载时移除。
    // 监听句柄是 Send + Sync 的普通值,可直接放进 StoredValue。
    let listener = StoredValue::new(None::<WindowListenerHandle>);
    Effect::new(move |_| {
        let is_open = open.get();
        if let Some(previous) = listener.try_update_value(|l| l.take()).flatten() {
            previous.remove();
        }
        if !is_open {
            return;
        }
        let handle = window_event_listener(ev::keydown, move |event| {
            if event.key() == "Escape" {
                on_close.run(());
            }
        });
        listener.set_value(Some(handle));
    });
    on_cleanup(move || {
        if let Some(Some(handle)) = listener.try_update_value(|l| l.take()) {
            handle.remove();
        }
    });

    // 面板定位:placement 决定垂直方向;align 决定水平。end 用 right 锚定
    // (右缘 = 点击点),与面板宽度解耦。
    // 居中由 .popover-panel[data-align="center"] 的 translate 属性完成，
    // 与入场动画的 transform（只缩放）互不干扰。
    let align_attr = match align.as_str() {
        "start" => "start",
        "end" => "end",
        _ => "center",
    };
    let placement_attr = if placement == "bottom" {
        "bottom"
    } else {
        "top"
    };
    let style = move || {
        let x = anchor_x.get();
        let y = anchor_y.get();
        let horizontal = match align_attr {
            "end" => format!("right: calc(100vw - {x}px);"),
            _ => format!("left: {x}px;"),
        };
        if placement_attr == "bottom" {
            format!("top: {}px; {horizontal}", y + 8)
        } else {
            // top:面板在点击点上方——用 bottom 锚定 viewport 底,差值即视口高度 - y + 间隙。
            // 视口高度用 100vh,纯 CSS 无需 JS 读取 scrollHeight。
            format!("bottom: calc(100vh - {y}px + 8px); {horizontal}")
        }
    };

    move || {
        open.get().then(|| {
            view! {
                // 透明遮罩:拦截外部点击(点遮罩即关)。z-40 < 面板 z-50。
                <div class=POPOVER_OVERLAY_CLASS on:click=move |_| on_close.run(())></div>
                // 面板:fixed 定位逃出 overflow-hidden 容器。
                <div
                    class="popover-panel"
                    data-placement=placement_attr
                    data-align=align_attr
                    style=style
                >
                    {children()}
                </div>
            }
        })
    }
}

/// 弹窗关闭/元素退出动画统一时长（ms），与 base.css 的 .modal-overlay / .modal-panel 关闭过渡对应；
/// 调用方需要在动画播放完毕后再真正卸载/摘除元素时复用本常量。
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

/// 弹窗打开时把焦点交给标题为 `title` 的最上层面板（仅 wasm）。
#[cfg(target_arch = "wasm32")]
fn focus_modal_panel(document: &web_sys::Document, title: &str) {
    use wasm_bindgen::JsCast;
    let Ok(panels) =
        document.query_selector_all("[data-ygg-modal-panel][data-ygg-modal-open='true']")
    else {
        return;
    };
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
/// `Effect` 兜底补置 `closing` 为时已晚，元素已被提前卸载，动画会变成
/// 瞬间消失而非淡出）。因此 `visible` / `closing` 是与调用方共享的 `RwSignal`，
/// 而非本组件内部私有状态；`opened`（mount 守卫）才是本组件私有的。
///
/// 面板上的点击不会冒泡：面板内的站内链接因此收不到文档级路由协调器，会退化为整页跳转，
/// 弹窗内容里需要导航时请用 `crate::navigation::push`。
///
/// ## Props
///
/// - `visible` / `closing`：与调用方共享的显隐 / 关闭动画信号。
/// - `title`：面板的 `aria-label`（不渲染可见标题——各调用方的标题栏自行渲染）。
/// - `role`：面板角色，默认 `dialog`；危险确认传 `alertdialog`。
/// - `overlay_padding`：遮罩内边距（响应式内边距因弹窗尺寸而异，默认 `"p-6"`）。
/// - `panel_class`：在共享面板底座上追加的宽度等布局类（默认 `"w-full max-w-lg"`）。
/// - `children`：面板内容（含调用方自己的标题栏、× 按钮等）；显示期间渲染，每次打开重新创建。
#[component]
pub fn ModalShell(
    visible: RwSignal<bool>,
    closing: RwSignal<bool>,
    #[prop(into)] title: String,
    #[prop(default = "dialog".to_string(), into)] role: String,
    #[prop(default = "p-6".to_string(), into)] overlay_padding: String,
    #[prop(default = "w-full max-w-lg".to_string(), into)] panel_class: String,
    children: ChildrenFn,
) -> impl IntoView {
    // 公共弹窗在打开时接管焦点，Escape 关闭后还给触发器；监听随显隐同步清理。
    let listener = StoredValue::new(None::<WindowListenerHandle>);
    // 打开前获得焦点的元素（JS 句柄，非 Send），关闭后归还焦点。
    #[cfg(target_arch = "wasm32")]
    let previous_focus: StoredValue<Option<web_sys::Element>, LocalStorage> =
        StoredValue::new_local(None);
    {
        let title = title.clone();
        Effect::new(move |_| {
            let is_visible = visible.get();
            if let Some(old) = listener.try_update_value(|l| l.take()).flatten() {
                old.remove();
            }
            if is_visible {
                #[cfg(target_arch = "wasm32")]
                if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                    previous_focus.set_value(document.active_element());
                    focus_modal_panel(&document, &title);
                }
                let title = title.clone();
                let handle = window_event_listener(ev::keydown, move |event| {
                    let key = event.key();
                    if key == "Escape" {
                        closing.set(true);
                        visible.set(false);
                    } else if key == "Tab" {
                        #[cfg(target_arch = "wasm32")]
                        trap_tab(&event, &title);
                        #[cfg(not(target_arch = "wasm32"))]
                        let _ = &title;
                    }
                });
                listener.set_value(Some(handle));
            } else {
                #[cfg(target_arch = "wasm32")]
                if let Some(element) = previous_focus.try_update_value(|p| p.take()).flatten() {
                    use wasm_bindgen::JsCast;
                    if let Some(element) = element.dyn_ref::<web_sys::HtmlElement>() {
                        let _ = element.focus();
                    }
                }
            }
        });
    }
    on_cleanup(move || {
        if let Some(Some(old)) = listener.try_update_value(|l| l.take()) {
            old.remove();
        }
    });

    // 是否曾开过弹窗：mount 时 visible=false 也会跑一次 Effect，不加此守卫会
    // 在页面加载后 EXIT_ANIM_MS 内渲染一层透明遮罩（opacity:0 仍拦截点击）吞掉首次点击。
    let opened = RwSignal::new(false);

    // visible 翻转驱动关闭动画：关闭入口（× / 遮罩 / Esc）会同步先置 closing 再翻
    // visible（见本组件 doc）；这里只负责重开复位与 EXIT_ANIM_MS 后的复位卸载。
    // 闭包只订阅 visible，closing/opened 全用 get_untracked 防自触发循环。
    Effect::new(move |_| {
        if visible.get() {
            opened.set(true);
            closing.set(false);
        } else if opened.get_untracked() {
            // 非交互路径的 visible 翻转（理论上不存在）兜底补置 closing。
            if !closing.get_untracked() {
                closing.set(true);
            }
            leptos::task::spawn_local(async move {
                crate::utils::time::sleep_ms(EXIT_ANIM_MS).await;
                let _ = closing.try_set(false);
            });
        }
    });

    // 用 Memo 去重：visible → closing 的过渡期内 `shown` 恒为 true，子树原地保留，
    // `.is-closing` 类才能让 CSS 过渡从可见态播放。
    let shown = Memo::new(move |_| visible.get() || closing.get());
    move || {
        shown.get().then(|| {
            let overlay_class = {
                let overlay_padding = overlay_padding.clone();
                move || {
                    let is_closing = closing.get();
                    format!(
                        "{MODAL_OVERLAY_BASE} {overlay_padding}{}",
                        if is_closing { " is-closing" } else { "" }
                    )
                }
            };
            view! {
                // 遮罩：点击关闭。
                <div
                    class=overlay_class
                    on:click=move |_| {
                        closing.set(true);
                        visible.set(false);
                    }
                >
                    // 面板：阻止点击穿透到遮罩。
                    <div
                        class=format!("{MODAL_PANEL_BASE} {panel_class}")
                        role=role.clone()
                        tabindex="-1"
                        data-ygg-modal-panel="true"
                        data-ygg-modal-open=move || if visible.get() { "true" } else { "false" }
                        aria-modal="true"
                        aria-label=title.clone()
                        on:click=move |evt| evt.stop_propagation()
                    >
                        {children()}
                    </div>
                </div>
            }
        })
    }
}

/// 危险操作确认对话框（`alertdialog`）：左侧 danger 图标、标题与说明，底栏「取消」与
/// danger 主按钮。点确认先关闭再回调 `on_confirm`，执行与结果提示由调用方负责。
/// `title` 同时作为面板的无障碍名称，同一时刻打开的对话框标题不能重复。
///
/// `description` 可传字符串、信号或闭包（说明里带当前对象名时随状态更新）。
#[component]
pub fn ConfirmDialog(
    visible: RwSignal<bool>,
    #[prop(into)] title: String,
    #[prop(into)] description: Signal<String>,
    #[prop(into)] confirm_label: String,
    #[prop(default = super::IconName::Trash)] icon: super::IconName,
    #[prop(into)] on_confirm: Callback<()>,
) -> impl IntoView {
    use super::{button_class, ButtonSize, ButtonVariant, Icon};

    let closing = RwSignal::new(false);
    let close = move || {
        closing.set(true);
        visible.set(false);
    };
    let modal_title = title.clone();
    view! {
        <ModalShell
            visible=visible
            closing=closing
            title=modal_title
            role="alertdialog"
            overlay_padding="p-4"
            panel_class="confirm-dialog"
        >
            <div class="confirm-dialog-body">
                <span class="confirm-dialog-icon" aria-hidden="true">
                    <Icon name=icon size=18/>
                </span>
                <div>
                    <h2>{title.clone()}</h2>
                    <p>{move || description.get()}</p>
                </div>
            </div>
            <div class="confirm-dialog-actions">
                <button
                    class=button_class(ButtonVariant::Ghost, ButtonSize::Md)
                    type="button"
                    on:click=move |_| close()
                >
                    "取消"
                </button>
                <button
                    class=button_class(ButtonVariant::Danger, ButtonSize::Md)
                    type="button"
                    on:click=move |_| {
                        close();
                        on_confirm.run(());
                    }
                >
                    {confirm_label.clone()}
                </button>
            </div>
        </ModalShell>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_overlays_render_nothing() {
        let html = Owner::new().with(|| {
            let visible = RwSignal::new(false);
            let closing = RwSignal::new(false);
            view! {
                <ModalShell visible closing title="标题">"内容"</ModalShell>
                <Popover open=false anchor_x=0 anchor_y=0 on_close=|_| {}>"浮层"</Popover>
            }
            .to_html()
        });
        assert!(!html.contains("modal-overlay"), "{html}");
        assert!(!html.contains("popover-panel"), "{html}");
    }

    #[test]
    fn open_modal_renders_dialog_semantics_and_popover_style() {
        let html = Owner::new().with(|| {
            let visible = RwSignal::new(true);
            let closing = RwSignal::new(false);
            view! {
                <ModalShell visible closing title="标题" role="alertdialog">"内容"</ModalShell>
                <Popover open=true anchor_x=10 anchor_y=20 placement="bottom" align="end" on_close=|_| {}>
                    "浮层"
                </Popover>
            }
            .to_html()
        });
        assert!(html.contains("role=\"alertdialog\""), "{html}");
        assert!(html.contains("aria-label=\"标题\""), "{html}");
        assert!(html.contains("data-ygg-modal-open=\"true\""), "{html}");
        assert!(
            html.contains("top: 28px; right: calc(100vw - 10px);"),
            "{html}"
        );
    }
}
