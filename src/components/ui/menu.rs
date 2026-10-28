//! 操作菜单：「⋯」等按钮触发的一列操作。
use super::controls::{button_class, ButtonSize, ButtonVariant};
use super::display::Tooltip;
use super::icons::{Icon, IconName};
use leptos::ev::KeyboardEvent;
use leptos::prelude::*;

static MENU_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 菜单里的一项。`id` 回传给 `on_select`，用于区分点了哪一项。
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub id: &'static str,
    pub label: String,
    pub icon: Option<IconName>,
    /// 红色语气：删除、移入回收站等。
    pub danger: bool,
    /// 不可用时变淡、跳过键盘移动，并以悬浮提示写明原因。
    pub disabled: Option<&'static str>,
    pub shortcut: Option<&'static str>,
    /// 在本项之前画一条分隔线。
    pub separated: bool,
}

impl MenuItem {
    pub fn new(id: &'static str, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            icon: None,
            danger: false,
            disabled: None,
            shortcut: None,
            separated: false,
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    #[allow(dead_code)] // 随页面重写逐步启用
    pub fn disabled(mut self, reason: &'static str) -> Self {
        self.disabled = Some(reason);
        self
    }

    #[allow(dead_code)] // 随页面重写逐步启用
    pub fn shortcut(mut self, keys: &'static str) -> Self {
        self.shortcut = Some(keys);
        self
    }

    pub fn separated(mut self) -> Self {
        self.separated = true;
        self
    }
}

/// 在可用项之间移动焦点的目标序号；当前项不可用或不在列表时落到首项或末项。
fn step_enabled(items: &[MenuItem], cur: Option<usize>, delta: i32) -> Option<usize> {
    let enabled: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.disabled.is_none())
        .map(|(i, _)| i)
        .collect();
    if enabled.is_empty() {
        return None;
    }
    let len = enabled.len() as i32;
    match cur.and_then(|c| enabled.iter().position(|&i| i == c)) {
        Some(pos) => Some(enabled[(pos as i32 + delta).rem_euclid(len) as usize]),
        None if delta < 0 => enabled.last().copied(),
        None => enabled.first().copied(),
    }
}

/// 按 id 聚焦元素（仅 wasm）。
#[cfg(target_arch = "wasm32")]
fn focus_by_id(element_id: &str) {
    use wasm_bindgen::JsCast;
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(element_id))
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = el.focus();
    }
}

/// 测量触发器视口位置，返回 fixed 面板的内联定位样式与实际展开方向（仅 wasm）。
///
/// 菜单挂在 `overflow: clip` 的表格面板里时，绝对定位会被裁掉，故面板用 fixed，
/// 锚点取触发器矩形；默认向下的菜单在下方放不下时翻到上方。
#[cfg(target_arch = "wasm32")]
fn measure_panel(
    trigger_id: &str,
    item_count: usize,
    placement: &'static str,
    align: &'static str,
) -> Option<(String, &'static str)> {
    let document = web_sys::window()?.document()?;
    let rect = document
        .get_element_by_id(trigger_id)?
        .get_bounding_client_rect();
    let root = document.document_element()?;
    let (width, height) = (root.client_width() as f64, root.client_height() as f64);
    let placement = if placement == "bottom"
        && crate::components::forms::measure_flip(trigger_id, item_count)
    {
        "top"
    } else {
        placement
    };
    let vertical = if placement == "top" {
        format!("bottom: {}px;", height - rect.top() + 6.0)
    } else {
        format!("top: {}px;", rect.bottom() + 6.0)
    };
    let horizontal = if align == "start" {
        format!("left: {}px;", rect.left())
    } else {
        format!("right: {}px;", width - rect.right())
    };
    Some((format!("{vertical} {horizontal}"), placement))
}

/// 图标按钮触发的操作菜单。
///
/// - 打开后焦点落在第一个可用项；↑/↓ 在可用项间循环（roving focus），Home/End 跳首尾；
/// - Enter/Space 选中并关闭，Esc 关闭，关闭后焦点回到触发按钮；Tab 关闭并自然流转；
/// - 点菜单外部关闭；不可用项变淡、写明原因，键盘与点击都跳过。
///
/// 触发器与菜单项的 `id`（`menu-N-trigger` 等）在挂载后由客户端计数分配：全局计数器在服务端是
/// 进程级的，直接写进 SSR HTML 会与水合后的客户端计数不一致。
///
/// Props：
/// - `label`：触发按钮的读屏名称与悬浮提示，如「更多操作」
/// - `icon`：触发按钮图标，缺省 `Ellipsis`
/// - `items`：菜单项（`Vec<MenuItem>` 或返回它的闭包 / 信号）
/// - `on_select`：选中某项时回传其 `id`
/// - `placement`：`"bottom"`（默认）/ `"top"`；`align`：`"end"`（默认，右对齐）/ `"start"`
#[component]
pub fn Menu(
    #[prop(into)] label: String,
    #[prop(into)] items: Signal<Vec<MenuItem>>,
    #[prop(into)] on_select: Callback<&'static str>,
    #[prop(default = IconName::Ellipsis)] icon: IconName,
    #[prop(default = ButtonSize::Sm)] size: ButtonSize,
    #[prop(default = "bottom".to_string(), into)] placement: String,
    #[prop(default = "end".to_string(), into)] align: String,
) -> impl IntoView {
    // 挂载后才分配序号，保证 SSR HTML 与水合一致（见组件文档）。
    let menu_id = RwSignal::new(None::<usize>);
    Effect::new(move |_| {
        if menu_id.get_untracked().is_none() {
            menu_id.set(Some(
                MENU_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            ));
        }
    });
    let trigger_id = move || menu_id.get().map(|n| format!("menu-{n}-trigger"));
    let panel_id = move || menu_id.get().map(|n| format!("menu-{n}"));
    let item_dom_id = move |i: usize| {
        format!(
            "menu-{}-item-{i}",
            menu_id.get_untracked().unwrap_or_default()
        )
    };

    let open = RwSignal::new(false);
    // 打开时测得的 fixed 定位与实际方向；SSR 与首帧回退到声明的 placement。
    let panel_pos = RwSignal::new(None::<(String, &'static str)>);
    let focused = RwSignal::new(None::<usize>);

    // 打开或键盘移动后，把焦点放到当前项上。
    Effect::new(move |_| {
        if let (true, Some(_i)) = (open.get(), focused.get()) {
            #[cfg(target_arch = "wasm32")]
            focus_by_id(&item_dom_id(_i));
        }
    });

    let close = move |return_focus: bool| {
        open.set(false);
        focused.set(None);
        panel_pos.set(None);
        #[cfg(target_arch = "wasm32")]
        if return_focus {
            if let Some(id) = trigger_id() {
                focus_by_id(&id);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = return_focus;
    };

    let placement: &'static str = if placement == "top" { "top" } else { "bottom" };
    let align: &'static str = if align == "start" { "start" } else { "end" };
    // 展开前测量触发器矩形，得到 fixed 面板的定位（仅 wasm；SSR 无 DOM）。
    let measure = move || {
        #[cfg(target_arch = "wasm32")]
        if let Some(id) = trigger_id() {
            let item_count = items.with_untracked(|items| items.len());
            panel_pos.set(measure_panel(&id, item_count, placement, align));
        }
    };
    let actual_placement = move || {
        panel_pos
            .with(|pos| pos.as_ref().map(|(_, actual)| *actual))
            .unwrap_or(placement)
    };
    let panel_style = move || panel_pos.with(|pos| pos.as_ref().map(|(style, _)| style.clone()));
    // 提示与菜单分处触发器两侧，互不遮挡。
    let tooltip_placement = Signal::derive(move || {
        if actual_placement() == "top" {
            "bottom"
        } else {
            "top"
        }
        .to_string()
    });
    let trigger_class = button_class(ButtonVariant::Ghost, size);
    let icon_px = if size == ButtonSize::Xs { 14 } else { 16 };

    let open_with = move |delta: i32| {
        measure();
        open.set(true);
        focused.set(items.with_untracked(|items| step_enabled(items, None, delta)));
    };

    view! {
        <span class="menu-wrap">
            <Tooltip tip=label.clone() placement=tooltip_placement>
                <button
                    id=trigger_id
                    class=format!("{trigger_class} btn-icon")
                    type="button"
                    aria-label=label
                    aria-haspopup="menu"
                    aria-expanded=move || open.get().to_string()
                    aria-controls=move || if open.get() { panel_id() } else { None }
                    on:click=move |_| {
                        if open.get_untracked() {
                            close(false);
                        } else {
                            open_with(1);
                        }
                    }
                    on:keydown=move |e: KeyboardEvent| {
                        if open.get_untracked() {
                            return;
                        }
                        match e.key().as_str() {
                            "ArrowDown" => {
                                e.prevent_default();
                                open_with(1);
                            }
                            "ArrowUp" => {
                                e.prevent_default();
                                open_with(-1);
                            }
                            _ => {}
                        }
                    }
                >
                    <Icon name=icon size=icon_px/>
                </button>
            </Tooltip>
            {move || {
                open.get()
                    .then(|| {
                        view! {
                            <div
                                class="fixed inset-0 z-40"
                                on:click=move |_| close(false)
                                // fixed 面板不随页面滚动，滚轮一动就收起，避免与触发器脱节。
                                on:wheel=move |_| close(false)
                            ></div>
                            <div
                                id=panel_id
                                class="menu"
                                role="menu"
                                aria-labelledby=trigger_id
                                data-placement=actual_placement
                                data-align=align
                                style=panel_style
                                on:keydown=move |e: KeyboardEvent| {
                                    let step = |from: Option<usize>, delta: i32| {
                                        items.with_untracked(|items| step_enabled(items, from, delta))
                                    };
                                    match e.key().as_str() {
                                        "ArrowDown" => {
                                            e.prevent_default();
                                            focused.set(step(focused.get_untracked(), 1));
                                        }
                                        "ArrowUp" => {
                                            e.prevent_default();
                                            focused.set(step(focused.get_untracked(), -1));
                                        }
                                        "Home" => {
                                            e.prevent_default();
                                            focused.set(step(None, 1));
                                        }
                                        "End" => {
                                            e.prevent_default();
                                            focused.set(step(None, -1));
                                        }
                                        "Escape" => {
                                            e.prevent_default();
                                            close(true);
                                        }
                                        "Tab" => close(false),
                                        _ => {}
                                    }
                                }
                            >
                                {move || {
                                    items
                                        .get()
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, item)| {
                                            view! {
                                                <MenuRow
                                                    id=item_dom_id(i)
                                                    item=item
                                                    focused=Signal::derive(move || focused.get() == Some(i))
                                                    on_hover=Callback::new(move |()| focused.set(Some(i)))
                                                    on_pick=Callback::new(move |id: &'static str| {
                                                        close(true);
                                                        on_select.run(id);
                                                    })
                                                />
                                            }
                                        })
                                        .collect_view()
                                }}
                            </div>
                        }
                    })
            }}
        </span>
    }
}

/// 菜单中的一项（可带前置分隔线）。
#[component]
fn MenuRow(
    id: String,
    item: MenuItem,
    #[prop(into)] focused: Signal<bool>,
    #[prop(into)] on_hover: Callback<()>,
    #[prop(into)] on_pick: Callback<&'static str>,
) -> impl IntoView {
    let enabled = item.disabled.is_none();
    let reason = item.disabled.unwrap_or_default();
    let item_id = item.id;
    view! {
        {item.separated.then(|| view! { <div class="menu-separator" role="separator"></div> })}
        <button
            id=id
            class="menu-item"
            type="button"
            role="menuitem"
            tabindex=move || if focused.get() { "0" } else { "-1" }
            data-tone=if item.danger { "danger" } else { "" }
            aria-disabled=(!enabled).then_some("true")
            title=reason
            on:mouseenter=move |_| {
                if enabled {
                    on_hover.run(());
                }
            }
            on:click=move |_| {
                if enabled {
                    on_pick.run(item_id);
                }
            }
        >
            {item.icon.map(|icon| view! { <Icon name=icon size=15/> })}
            <span>{item.label}</span>
            {item.shortcut.map(|keys| view! { <kbd>{keys}</kbd> })}
        </button>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<MenuItem> {
        vec![
            MenuItem::new("edit", "编辑"),
            MenuItem::new("archive", "归档").disabled("已归档"),
            MenuItem::new("copy", "复制链接"),
            MenuItem::new("trash", "移入回收站").danger().separated(),
        ]
    }

    #[test]
    fn step_enabled_skips_disabled_items_and_wraps() {
        let items = items();
        assert_eq!(step_enabled(&items, None, 1), Some(0));
        assert_eq!(step_enabled(&items, None, -1), Some(3));
        assert_eq!(step_enabled(&items, Some(0), 1), Some(2)); // 跳过不可用的「归档」
        assert_eq!(step_enabled(&items, Some(3), 1), Some(0)); // 末尾回绕
        assert_eq!(step_enabled(&items, Some(0), -1), Some(3)); // 首位回绕
        assert_eq!(step_enabled(&items, Some(1), 1), Some(0)); // 当前项不可用时从首项起
    }

    #[test]
    fn step_enabled_handles_all_disabled() {
        let items = vec![MenuItem::new("a", "A").disabled("不可用")];
        assert_eq!(step_enabled(&items, None, 1), None);
    }

    #[component]
    fn Harness() -> impl IntoView {
        view! { <Menu label="更多操作" items=items() on_select=|_| {}/> }
    }

    #[test]
    fn closed_menu_renders_only_the_trigger() {
        let html = Owner::new().with(|| view! { <Harness/> }.to_html());
        assert!(html.contains("aria-haspopup=\"menu\""), "{html}");
        assert!(html.contains("aria-expanded=\"false\""), "{html}");
        assert!(!html.contains("role=\"menu\""), "{html}");
    }
}
