//! 设置页各分区共用的保存协调与版式组件。
//!
//! 各分区只管自己的草稿与已存值：经 [`use_settings_slot`] 登记「是否有改动」以及
//! 保存、放弃两个回调。页面底部的浮起保存条读取 [`SettingsBus`]：点「保存设置」只让
//! 有改动的分区提交，分区提交完毕调用 [`SettingsBus::finish`] 报告结果；点「放弃」
//! 让所有分区把草稿还原为已存值。

use std::future::Future;

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::SettingsSection;
use crate::components::ui::use_toast;
use crate::utils::error::failure_message;

/// 保存条与各分区之间的共享状态。
#[derive(Clone, Copy, PartialEq)]
pub(super) struct SettingsBus {
    /// 有改动的分区。
    pub dirty: RwSignal<Vec<SettingsSection>>,
    /// 正在提交的分区。
    pub saving: RwSignal<Vec<SettingsSection>>,
    /// 最近一次提交失败的分区（再次提交或放弃时清除）。
    pub failed: RwSignal<Vec<SettingsSection>>,
    /// 每点一次「保存设置」加一。
    pub save_gen: RwSignal<u32>,
    /// 每点一次「放弃」加一。
    pub reset_gen: RwSignal<u32>,
}

impl SettingsBus {
    pub fn new() -> Self {
        Self {
            dirty: RwSignal::new(Vec::new()),
            saving: RwSignal::new(Vec::new()),
            failed: RwSignal::new(Vec::new()),
            save_gen: RwSignal::new(0),
            reset_gen: RwSignal::new(0),
        }
    }

    /// 分区提交结束：移出「提交中」，失败时记入失败列表。
    pub fn finish(self, section: SettingsSection, ok: bool) {
        self.saving.update(|list| list.retain(|s| *s != section));
        if !ok {
            self.failed.update(|list| {
                if !list.contains(&section) {
                    list.push(section);
                }
            });
        }
    }

    /// 保存条：提交全部有改动的分区。
    pub fn save_all(self) {
        if !self.saving.with_untracked(|list| list.is_empty()) {
            return;
        }
        self.failed.set(Vec::new());
        self.save_gen.update(|g| *g += 1);
    }

    /// 保存条：放弃全部改动。
    pub fn reset_all(self) {
        self.failed.set(Vec::new());
        self.reset_gen.update(|g| *g += 1);
    }
}

fn set_member(list: RwSignal<Vec<SettingsSection>>, section: SettingsSection, on: bool) {
    let present = list
        .try_with_untracked(|l| l.contains(&section))
        .unwrap_or(on);
    if on && !present {
        list.update(|l| l.push(section));
    } else if !on && present {
        list.update(|l| l.retain(|s| *s != section));
    }
}

/// 登记一个分区。`save` 负责发起提交，结束时调用 `finish`；`reset` 把草稿还原。
pub(super) fn use_settings_slot(
    section: SettingsSection,
    dirty: Memo<bool>,
    save: Callback<()>,
    reset: Callback<()>,
) -> SettingsBus {
    let bus = expect_context::<SettingsBus>();
    Effect::new(move |_| set_member(bus.dirty, section, dirty.get()));
    let seen_save = StoredValue::new(bus.save_gen.get_untracked());
    Effect::new(move |_| {
        let generation = bus.save_gen.get();
        if generation == seen_save.get_value() {
            return;
        }
        seen_save.set_value(generation);
        if dirty.get_untracked() {
            set_member(bus.saving, section, true);
            save.run(());
        }
    });
    let seen_reset = StoredValue::new(bus.reset_gen.get_untracked());
    Effect::new(move |_| {
        let generation = bus.reset_gen.get();
        if generation == seen_reset.get_value() {
            return;
        }
        seen_reset.set_value(generation);
        reset.run(());
    });
    on_cleanup(move || {
        set_member(bus.dirty, section, false);
        set_member(bus.saving, section, false);
    });
    bus
}

/// 一个分区的草稿、已存值与读取状态。
pub(super) struct SectionState<T: Send + Sync + 'static> {
    pub draft: RwSignal<T>,
    pub saved: RwSignal<Option<T>>,
    pub load_failed: RwSignal<bool>,
    pub bus: SettingsBus,
    pub section: SettingsSection,
}

impl<T: Send + Sync + 'static> Clone for SectionState<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for SectionState<T> {}

impl<T: Clone + PartialEq + Send + Sync + 'static> SectionState<T> {
    /// 已读到服务端的值，可以编辑。
    pub fn ready(&self) -> Signal<bool> {
        let saved = self.saved;
        Signal::derive(move || saved.with(|s| s.is_some()))
    }

    /// 读取失败。
    pub fn load_failed(&self) -> Signal<bool> {
        self.load_failed.into()
    }

    /// 最近一次提交失败。
    pub fn failed(&self) -> Signal<bool> {
        let (bus, section) = (self.bus, self.section);
        Signal::derive(move || bus.failed.with(|list| list.contains(&section)))
    }

    /// 草稿中某个字段的响应式读取。
    pub fn field<U: Send + Sync + 'static>(
        &self,
        read: impl Fn(&T) -> U + Send + Sync + 'static,
    ) -> Signal<U> {
        let draft = self.draft;
        Signal::derive(move || draft.with(&read))
    }

    /// 修改草稿。
    pub fn edit(&self, change: impl FnOnce(&mut T)) {
        self.draft.update(change);
    }
}

/// 读取一次已存值并登记到保存条。`save` 提交草稿并返回服务端规范化后的值。
pub(super) fn use_section<T, L, LF, S, SF>(
    section: SettingsSection,
    load: L,
    save: S,
) -> SectionState<T>
where
    T: Clone + PartialEq + Default + Send + Sync + 'static,
    L: FnOnce() -> LF + 'static,
    LF: Future<Output = Result<T, ServerFnError>> + 'static,
    S: Fn(T) -> SF + Copy + Send + Sync + 'static,
    SF: Future<Output = Result<T, ServerFnError>> + 'static,
{
    let toast = use_toast();
    let bus = expect_context::<SettingsBus>();
    let draft = RwSignal::new(T::default());
    let saved = RwSignal::new(None::<T>);
    let load_failed = RwSignal::new(false);
    // Effect 只在浏览器里运行：服务端渲染不请求。
    let mut load = Some(load);
    Effect::new(move |_| {
        let Some(load) = load.take() else {
            return;
        };
        spawn_local(async move {
            match load().await {
                Ok(value) => {
                    draft.set(value.clone());
                    saved.set(Some(value));
                }
                Err(_) => load_failed.set(true),
            }
        });
    });
    let dirty =
        Memo::new(move |_| saved.with(|s| s.as_ref().is_some_and(|s| draft.with(|d| s != d))));
    let on_save = Callback::new(move |()| {
        let value = draft.get_untracked();
        spawn_local(async move {
            match save(value).await {
                Ok(next) => {
                    draft.set(next.clone());
                    saved.set(Some(next));
                    bus.finish(section, true);
                }
                Err(e) => {
                    toast.error(failure_message(
                        &format!("「{}」保存失败", section.label()),
                        &e,
                    ));
                    bus.finish(section, false);
                }
            }
        });
    });
    let on_reset = Callback::new(move |()| {
        if let Some(value) = saved.get_untracked() {
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
    #[prop(into)] description: String,
    #[prop(optional, into)] failed: Signal<bool>,
    #[prop(into)] ready: Signal<bool>,
    #[prop(into)] load_failed: Signal<bool>,
    children: ChildrenFn,
) -> impl IntoView {
    let heading = format!("{}-h", section.dom_id());
    view! {
        <section
            id=section.dom_id()
            class="admin-panel settings-card"
            data-failed=move || failed.get().then_some("true")
            aria-labelledby=heading.clone()
        >
            <div class="settings-card-head">
                <div>
                    <h2 id=heading.clone()>{section.label()}</h2>
                    <p>{description}</p>
                </div>
                {section.needs_restart().then(|| view! { <span class="settings-restart">"需重启"</span> })}
            </div>
            {move || {
                if load_failed.get() {
                    view! {
                        <p class="settings-load-error" role="alert">"读取失败，请刷新页面重试。"</p>
                    }
                        .into_any()
                } else if ready.get() {
                    view! { <div class="settings-rows">{children()}</div> }.into_any()
                } else {
                    view! {
                        <div class="settings-rows" aria-busy="true">
                            {(0..2)
                                .map(|_| {
                                    view! {
                                        <div class="settings-row">
                                            <crate::components::skeletons::atoms::SkeletonBox class="h-3.5 w-20 rounded"/>
                                            <crate::components::skeletons::atoms::SkeletonBox class="h-9 w-full rounded-lg"/>
                                        </div>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}

/// 标签在左、控件与说明在右的一行。
#[component]
pub(super) fn SettingRow(
    #[prop(into)] label: String,
    #[prop(optional, into)] html_for: Option<String>,
    #[prop(optional, into)] hint: Option<String>,
    children: Children,
) -> impl IntoView {
    let label_view = match html_for {
        Some(id) => view! { <label class="settings-label" for=id>{label}</label> }.into_any(),
        None => view! { <span class="settings-label">{label}</span> }.into_any(),
    };
    view! {
        <div class="settings-row">
            {label_view} <div class="settings-control">
                {children()} {hint.map(|hint| view! { <p class="settings-hint">{hint}</p> })}
            </div>
        </div>
    }
}

/// 开关行：标签、说明与右侧开关。
#[component]
pub(super) fn SwitchRow(
    #[prop(into)] label: String,
    #[prop(into)] hint: String,
    #[prop(into)] checked: Signal<bool>,
    #[prop(into)] ontoggle: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="settings-row settings-switch-row">
            <span class="settings-label">{label.clone()}</span>
            <p class="settings-hint">{hint}</p>
            <crate::components::forms::ToggleSwitch checked aria_label=label ontoggle/>
        </div>
    }
}
