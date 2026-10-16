//! 操作菜单：「⋯」等按钮触发的一列操作。
use super::controls::{button_class, ButtonSize, ButtonVariant};
use super::display::Tooltip;
use super::icons::{Icon, IconName};
use dioxus::prelude::*;

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

/// 图标按钮触发的操作菜单。
///
/// - 打开后焦点落在第一个可用项；↑/↓ 在可用项间循环（roving focus），Home/End 跳首尾；
/// - Enter/Space 选中并关闭，Esc 关闭，关闭后焦点回到触发按钮；Tab 关闭并自然流转；
/// - 点菜单外部关闭；不可用项变淡、写明原因，键盘与点击都跳过。
///
/// Props：
/// - `label`：触发按钮的读屏名称与悬浮提示，如「更多操作」
/// - `icon`：触发按钮图标，缺省 `Ellipsis`
/// - `items`：菜单项
/// - `on_select`：选中某项时回传其 `id`
/// - `placement`：`"bottom"`（默认）/ `"top"`；`align`：`"end"`（默认，右对齐）/ `"start"`
#[component]
pub fn Menu(
    label: String,
    items: Vec<MenuItem>,
    on_select: EventHandler<&'static str>,
    #[props(default = IconName::Ellipsis)] icon: IconName,
    #[props(default = ButtonSize::Sm)] size: ButtonSize,
    #[props(default = "bottom")] placement: &'static str,
    #[props(default = "end")] align: &'static str,
) -> Element {
    let menu_id = use_hook(|| MENU_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
    let trigger_id = format!("menu-{menu_id}-trigger");
    let panel_id = format!("menu-{menu_id}");
    let mut open = use_signal(|| false);
    let mut focused = use_signal(|| None::<usize>);

    // 打开或键盘移动后，把焦点放到当前项上。
    use_effect(move || {
        if let (true, Some(_i)) = (open(), focused()) {
            #[cfg(target_arch = "wasm32")]
            focus_by_id(&format!("menu-{menu_id}-item-{_i}"));
        }
    });

    let mut close = move |return_focus: bool| {
        open.set(false);
        focused.set(None);
        #[cfg(target_arch = "wasm32")]
        if return_focus {
            focus_by_id(&format!("menu-{menu_id}-trigger"));
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = return_focus;
    };

    let items_for_open = items.clone();
    let items_for_keys = items.clone();
    let placement = if placement == "top" { "top" } else { "bottom" };
    let align = if align == "start" { "start" } else { "end" };
    // 提示与菜单分处触发器两侧，互不遮挡。
    let tooltip_placement = if placement == "top" { "bottom" } else { "top" };
    let trigger_class = button_class(ButtonVariant::Ghost, size);
    let icon_px = if size == ButtonSize::Xs { 14 } else { 16 };

    rsx! {
        span { class: "menu-wrap",
            Tooltip { tip: label.clone(), placement: tooltip_placement,
                button {
                    id: "{trigger_id}",
                    class: "{trigger_class} btn-icon",
                    r#type: "button",
                    aria_label: "{label}",
                    aria_haspopup: "menu",
                    aria_expanded: "{open()}",
                    aria_controls: open().then(|| panel_id.clone()),
                    onclick: move |_| {
                        if open() {
                            close(false);
                        } else {
                            open.set(true);
                            focused.set(step_enabled(&items_for_open, None, 1));
                        }
                    },
                    onkeydown: {
                        let items = items.clone();
                        move |e: KeyboardEvent| {
                            if !open() && (e.key() == Key::ArrowDown || e.key() == Key::ArrowUp) {
                                e.prevent_default();
                                let delta = if e.key() == Key::ArrowUp { -1 } else { 1 };
                                open.set(true);
                                focused.set(step_enabled(&items, None, delta));
                            }
                        }
                    },
                    Icon { name: icon, size: icon_px }
                }
            }
            if open() {
                div { class: "fixed inset-0 z-40", onclick: move |_| close(false) }
                div {
                    id: "{panel_id}",
                    class: "menu",
                    role: "menu",
                    aria_labelledby: "{trigger_id}",
                    "data-placement": placement,
                    "data-align": align,
                    onkeydown: move |e: KeyboardEvent| {
                        let key = e.key();
                        if key == Key::ArrowDown || key == Key::ArrowUp {
                            e.prevent_default();
                            let delta = if key == Key::ArrowUp { -1 } else { 1 };
                            focused.set(step_enabled(&items_for_keys, focused(), delta));
                        } else if key == Key::Home {
                            e.prevent_default();
                            focused.set(step_enabled(&items_for_keys, None, 1));
                        } else if key == Key::End {
                            e.prevent_default();
                            focused.set(step_enabled(&items_for_keys, None, -1));
                        } else if key == Key::Escape {
                            e.prevent_default();
                            close(true);
                        } else if key == Key::Tab {
                            close(false);
                        }
                    },
                    for (i, item) in items.into_iter().enumerate() {
                        MenuRow {
                            key: "{item.id}",
                            id: format!("menu-{menu_id}-item-{i}"),
                            item,
                            focused: focused() == Some(i),
                            on_hover: move |_| focused.set(Some(i)),
                            on_pick: move |id| {
                                close(true);
                                on_select.call(id);
                            },
                        }
                    }
                }
            }
        }
    }
}

/// 菜单中的一项（可带前置分隔线）。
#[component]
fn MenuRow(
    id: String,
    item: MenuItem,
    focused: bool,
    on_hover: EventHandler<()>,
    on_pick: EventHandler<&'static str>,
) -> Element {
    let enabled = item.disabled.is_none();
    let reason = item.disabled.unwrap_or_default();
    let item_id = item.id;
    rsx! {
        if item.separated {
            div { class: "menu-separator", role: "separator" }
        }
        button {
            id,
            class: "menu-item",
            r#type: "button",
            role: "menuitem",
            tabindex: if focused { "0" } else { "-1" },
            "data-tone": if item.danger { "danger" } else { "" },
            aria_disabled: (!enabled).then_some("true"),
            title: reason,
            onmouseenter: move |_| {
                if enabled {
                    on_hover.call(());
                }
            },
            onclick: move |_| {
                if enabled {
                    on_pick.call(item_id);
                }
            },
            if let Some(icon) = item.icon {
                Icon { name: icon, size: 15 }
            }
            span { "{item.label}" }
            if let Some(keys) = item.shortcut {
                kbd { "{keys}" }
            }
        }
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
    fn Harness() -> Element {
        rsx! {
            Menu { label: "更多操作", items: items(), on_select: |_| {} }
        }
    }

    #[test]
    fn closed_menu_renders_only_the_trigger() {
        let html = dioxus::ssr::render_element(rsx! {
            Harness {}
        });
        assert!(html.contains("aria-haspopup=\"menu\""), "{html}");
        assert!(html.contains("aria-expanded=\"false\""), "{html}");
        assert!(!html.contains("role=\"menu\""), "{html}");
    }
}
