//! 日志清理策略：保留天数与最大行数，在标题行「清理策略」展开的面板里编辑。
use crate::api::logs::{get_log_settings, update_log_settings};
use crate::components::forms::NumberInput;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, IconButton, IconName,
};
use crate::models::log::LogSettings;
use crate::utils::error::failure_message;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// 清理策略面板。
///
/// Props：
/// - `on_close`：点击「收起」时的回调
#[component]
pub(super) fn CleanupPolicy(#[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let toast = use_toast();
    let saved = RwSignal::new(None::<LogSettings>);
    let days = RwSignal::new(7_i64);
    let rows = RwSignal::new(100_000_i64);
    let saving = RwSignal::new(false);
    // 挂载后拉取一次当前设置；失败时保留默认值（与原版一致，不打扰用户）。
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(s) = get_log_settings().await {
                days.try_set(i64::from(s.retention_days));
                rows.try_set(i64::from(s.max_rows));
                saved.try_set(Some(s));
            }
        });
    });
    let dirty = move || {
        saved.with(|s| {
            s.as_ref().is_some_and(|s| {
                i64::from(s.retention_days) != days.get() || i64::from(s.max_rows) != rows.get()
            })
        })
    };
    let save = move |_| {
        saving.set(true);
        let (retention_days, max_rows) = (days.get_untracked() as i32, rows.get_untracked() as i32);
        spawn_local(async move {
            match update_log_settings(retention_days, max_rows).await {
                Ok(s) => {
                    days.try_set(i64::from(s.retention_days));
                    rows.try_set(i64::from(s.max_rows));
                    saved.try_set(Some(s));
                    toast.success("清理策略已保存");
                }
                Err(e) => toast.error(failure_message("保存失败", &e)),
            }
            saving.try_set(false);
        });
    };
    view! {
        <section class="admin-panel logs-cleanup" aria-labelledby="logs-cleanup-h">
            <div class="logs-cleanup-head">
                <div>
                    <h2 id="logs-cleanup-h">"清理策略"</h2>
                    <p>"后台任务定期清理，保留期与行数上限先到者生效。"</p>
                </div>
                <IconButton
                    icon=IconName::X
                    label="收起"
                    size=ButtonSize::Sm
                    onclick=move |_| on_close.run(())
                />
            </div>
            <div class="logs-cleanup-fields">
                <label for="logs-days">"保留天数"</label>
                <NumberInput
                    id="logs-days"
                    value=days
                    min=1
                    max=90
                    unit="天"
                    onchange=move |v: i64| days.set(v)
                />
                <label for="logs-rows">"最大行数"</label>
                <NumberInput
                    id="logs-rows"
                    value=rows
                    min=1000
                    max=1_000_000
                    step=10_000
                    unit="行"
                    onchange=move |v: i64| rows.set(v)
                />
                <button
                    class=button_class(ButtonVariant::Primary, ButtonSize::Sm)
                    type="button"
                    disabled=move || !dirty() || saving.get()
                    on:click=save
                >
                    {move || if saving.get() { "保存中…" } else { "保存" }}
                </button>
            </div>
        </section>
    }
}
