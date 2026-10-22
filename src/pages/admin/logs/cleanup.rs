//! 日志清理策略：保留天数与最大行数，在标题行「清理策略」展开的面板里编辑。
use crate::api::logs::{get_log_settings, update_log_settings};
use crate::components::forms::NumberInput;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, IconButton, IconName,
};
use crate::models::log::LogSettings;
use crate::utils::error::failure_message;
use dioxus::prelude::*;

#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn CleanupPolicy(on_close: EventHandler<()>) -> Element {
    let toast = use_toast();
    let mut saved = use_signal(|| None::<LogSettings>);
    let mut days = use_signal(|| 7_i64);
    let mut rows = use_signal(|| 100_000_i64);
    let mut saving = use_signal(|| false);
    use_hook(move || {
        spawn(async move {
            if let Ok(s) = get_log_settings().await {
                days.set(s.retention_days as i64);
                rows.set(s.max_rows as i64);
                saved.set(Some(s));
            }
        });
    });
    let dirty =
        saved().is_some_and(|s| s.retention_days as i64 != days() || s.max_rows as i64 != rows());
    let save = move |_| {
        saving.set(true);
        spawn(async move {
            match update_log_settings(days() as i32, rows() as i32).await {
                Ok(s) => {
                    days.set(s.retention_days as i64);
                    rows.set(s.max_rows as i64);
                    saved.set(Some(s));
                    toast.success("清理策略已保存");
                }
                Err(e) => toast.error(failure_message("保存失败", &e)),
            }
            saving.set(false);
        });
    };
    rsx! {
        section { class: "admin-panel logs-cleanup", aria_labelledby: "logs-cleanup-h",
            div { class: "logs-cleanup-head",
                div {
                    h2 { id: "logs-cleanup-h", "清理策略" }
                    p { "后台任务定期清理，保留期与行数上限先到者生效。" }
                }
                IconButton { icon: IconName::X, label: "收起", size: ButtonSize::Sm, onclick: move |_| on_close.call(()) }
            }
            div { class: "logs-cleanup-fields",
                label { r#for: "logs-days", "保留天数" }
                NumberInput {
                    id: "logs-days",
                    value: days(),
                    min: 1,
                    max: 90,
                    unit: "天",
                    onchange: move |v| days.set(v),
                }
                label { r#for: "logs-rows", "最大行数" }
                NumberInput {
                    id: "logs-rows",
                    value: rows(),
                    min: 1000,
                    max: 1_000_000,
                    step: 10_000,
                    unit: "行",
                    onchange: move |v| rows.set(v),
                }
                button {
                    class: button_class(ButtonVariant::Primary, ButtonSize::Sm),
                    r#type: "button",
                    disabled: !dirty || saving(),
                    onclick: save,
                    if saving() { "保存中…" } else { "保存" }
                }
            }
        }
    }
}
