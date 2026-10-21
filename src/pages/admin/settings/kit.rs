//! 设置页各分区共用的保存协调与版式组件。
//!
//! 各分区只管自己的草稿与已存值：经 [`use_settings_slot`] 登记「是否有改动」以及
//! 保存、放弃两个回调。页面底部的浮起保存条读取 [`SettingsBus`]：点「保存设置」只让
//! 有改动的分区提交，分区提交完毕调用 [`SettingsBus::finish`] 报告结果；点「放弃」
//! 让所有分区把草稿还原为已存值。

use std::future::Future;

use dioxus::prelude::*;

use super::SettingsSection;
use crate::components::ui::use_toast;

/// 保存条与各分区之间的共享状态。
#[derive(Clone, Copy, PartialEq)]
pub(super) struct SettingsBus {
    /// 有改动的分区。
    pub dirty: Signal<Vec<SettingsSection>>,
    /// 正在提交的分区。
    pub saving: Signal<Vec<SettingsSection>>,
    /// 最近一次提交失败的分区（再次提交或放弃时清除）。
    pub failed: Signal<Vec<SettingsSection>>,
    /// 每点一次「保存设置」加一。
    pub save_gen: Signal<u32>,
    /// 每点一次「放弃」加一。
    pub reset_gen: Signal<u32>,
}

impl SettingsBus {
    pub fn new() -> Self {
        Self {
            dirty: Signal::new(Vec::new()),
            saving: Signal::new(Vec::new()),
            failed: Signal::new(Vec::new()),
            save_gen: Signal::new(0),
            reset_gen: Signal::new(0),
        }
    }

    /// 分区提交结束：移出「提交中」，失败时记入失败列表。
    pub fn finish(mut self, section: SettingsSection, ok: bool) {
        self.saving.with_mut(|list| list.retain(|s| *s != section));
        if !ok {
            self.failed.with_mut(|list| {
                if !list.contains(&section) {
                    list.push(section);
                }
            });
        }
    }

    /// 保存条：提交全部有改动的分区。
    pub fn save_all(mut self) {
        if !self.saving.peek().is_empty() {
            return;
        }
        self.failed.set(Vec::new());
        self.save_gen += 1;
    }

    /// 保存条：放弃全部改动。
    pub fn reset_all(mut self) {
        self.failed.set(Vec::new());
        self.reset_gen += 1;
    }
}

fn set_member(mut list: Signal<Vec<SettingsSection>>, section: SettingsSection, on: bool) {
    let present = list.peek().contains(&section);
    if on && !present {
        list.write().push(section);
    } else if !on && present {
        list.write().retain(|s| *s != section);
    }
}

/// 登记一个分区。`save` 负责发起提交，结束时调用 `finish`；`reset` 把草稿还原。
pub(super) fn use_settings_slot(
    section: SettingsSection,
    dirty: Memo<bool>,
    save: Callback<()>,
    reset: Callback<()>,
) -> SettingsBus {
    let bus: SettingsBus = use_context();
    use_effect(move || set_member(bus.dirty, section, dirty()));
    let mut seen_save = use_signal(|| *bus.save_gen.peek());
    use_effect(move || {
        let generation = (bus.save_gen)();
        if generation == *seen_save.peek() {
            return;
        }
        seen_save.set(generation);
        if *dirty.peek() {
            set_member(bus.saving, section, true);
            save.call(());
        }
    });
    let mut seen_reset = use_signal(|| *bus.reset_gen.peek());
    use_effect(move || {
        let generation = (bus.reset_gen)();
        if generation == *seen_reset.peek() {
            return;
        }
        seen_reset.set(generation);
        reset.call(());
    });
    use_drop(move || {
        set_member(bus.dirty, section, false);
        set_member(bus.saving, section, false);
    });
    bus
}

/// 一个分区的草稿、已存值与读取状态。
#[derive(Clone, Copy, PartialEq)]
pub(super) struct SectionState<T: 'static> {
    pub draft: Signal<T>,
    pub saved: Signal<Option<T>>,
    pub load_failed: Signal<bool>,
    pub bus: SettingsBus,
    pub section: SettingsSection,
}

impl<T: Clone + PartialEq + 'static> SectionState<T> {
    /// 已读到服务端的值，可以编辑。
    pub fn ready(&self) -> bool {
        self.saved.read().is_some()
    }

    /// 最近一次提交失败。
    pub fn failed(&self) -> bool {
        self.bus.failed.read().contains(&self.section)
    }
}

/// 读取一次已存值并登记到保存条。`save` 提交草稿并返回服务端规范化后的值。
pub(super) fn use_section<T, L, LF, S, SF>(
    section: SettingsSection,
    load: L,
    save: S,
) -> SectionState<T>
where
    T: Clone + PartialEq + Default + 'static,
    L: FnOnce() -> LF + 'static,
    LF: Future<Output = Result<T, ServerFnError>> + 'static,
    S: Fn(T) -> SF + Copy + 'static,
    SF: Future<Output = Result<T, ServerFnError>> + 'static,
{
    let toast = use_toast();
    let bus: SettingsBus = use_context();
    let mut draft = use_signal(T::default);
    let mut saved = use_signal(|| None::<T>);
    let mut load_failed = use_signal(|| false);
    use_hook(move || {
        spawn(async move {
            match load().await {
                Ok(value) => {
                    draft.set(value.clone());
                    saved.set(Some(value));
                }
                Err(_) => load_failed.set(true),
            }
        });
    });
    let dirty = use_memo(move || saved.read().as_ref().is_some_and(|s| *s != *draft.read()));
    let on_save = Callback::new(move |()| {
        let value = draft();
        spawn(async move {
            match save(value).await {
                Ok(next) => {
                    draft.set(next.clone());
                    saved.set(Some(next));
                    bus.finish(section, true);
                }
                Err(e) => {
                    toast.error(format!("「{}」保存失败：{e}", section.label()));
                    bus.finish(section, false);
                }
            }
        });
    });
    let on_reset = Callback::new(move |()| {
        if let Some(value) = saved() {
            draft.set(value);
        }
    });
    use_settings_slot(section, dirty, on_save, on_reset);
    SectionState {
        draft,
        saved,
        load_failed,
        bus,
        section,
    }
}

/// 分区卡片：标题、说明与「需重启」标记，正文为若干设置行。
#[component]
pub(super) fn SettingsCard(
    section: SettingsSection,
    #[props(into)] description: String,
    #[props(default)] failed: bool,
    #[props(default = true)] ready: bool,
    #[props(default)] load_failed: bool,
    children: Element,
) -> Element {
    let heading = format!("{}-h", section.dom_id());
    rsx! {
        section {
            id: "{section.dom_id()}",
            class: "admin-panel settings-card",
            "data-failed": failed.then_some("true"),
            aria_labelledby: "{heading}",
            div { class: "settings-card-head",
                div {
                    h2 { id: "{heading}", "{section.label()}" }
                    p { "{description}" }
                }
                if section.needs_restart() {
                    span { class: "settings-restart", "需重启" }
                }
            }
            if load_failed {
                p { class: "settings-load-error", role: "alert", "读取失败，请刷新页面重试。" }
            } else if ready {
                div { class: "settings-rows", {children} }
            } else {
                div { class: "settings-rows", aria_busy: "true",
                    for i in 0..2 {
                        div { key: "{i}", class: "settings-row",
                            crate::components::skeletons::atoms::SkeletonBox { class: "h-3.5 w-20 rounded" }
                            crate::components::skeletons::atoms::SkeletonBox { class: "h-9 w-full rounded-lg" }
                        }
                    }
                }
            }
        }
    }
}

/// 标签在左、控件与说明在右的一行。
#[component]
pub(super) fn SettingRow(
    #[props(into)] label: String,
    #[props(default)] html_for: Option<String>,
    #[props(default)] hint: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "settings-row",
            if let Some(id) = html_for {
                label { class: "settings-label", r#for: "{id}", "{label}" }
            } else {
                span { class: "settings-label", "{label}" }
            }
            div { class: "settings-control",
                {children}
                if let Some(hint) = hint {
                    p { class: "settings-hint", "{hint}" }
                }
            }
        }
    }
}

/// 开关行：标签、说明与右侧开关。
#[component]
pub(super) fn SwitchRow(
    #[props(into)] label: String,
    #[props(into)] hint: String,
    checked: bool,
    ontoggle: Callback<()>,
) -> Element {
    rsx! {
        div { class: "settings-row settings-switch-row",
            span { class: "settings-label", "{label}" }
            p { class: "settings-hint", "{hint}" }
            crate::components::forms::ToggleSwitch { checked, aria_label: label.clone(), ontoggle }
        }
    }
}
