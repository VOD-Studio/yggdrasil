use crate::components::ui::LoadingButton;
use dioxus::prelude::*;

/// 自动备份设置卡片：启用开关 / 执行时间 / 保留份数 / 包含 uploads，
/// 以及上次自动备份结果与下次执行时间。
///
/// 时区模型：服务端只存 UTC "HH:MM"；卡片用 js_sys 在挂载回填与保存时
/// 做浏览器本地时区转换。SSR 与客户端首帧都是 view=None 的占位态，
/// 数据在 hydration 后落地，不存在两端渲染不一致。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn BackupSettingsCard() -> Element {
    #[cfg(target_arch = "wasm32")]
    use crate::api::settings::{get_backup_settings, update_backup_settings};
    use crate::components::forms::{TimePicker, ToggleSwitch};
    use crate::components::ui::{CollapsibleSettingsCard, BTN_ICON};
    use crate::models::settings::BackupSettingsView;
    #[cfg(target_arch = "wasm32")]
    use crate::utils::time::local_hhmm_to_utc;
    use crate::utils::time::utc_hhmm_to_local;

    // 服务端视图（设置 + 上次结果 + 下次执行）。SSR/首帧为 None（占位态）。
    let view = use_signal(|| Option::<BackupSettingsView>::None); // 表单草稿：与已存值天然分叉，是独立状态（非镜像）。
    let mut draft_enabled = use_signal(|| false);
    let mut draft_time = use_signal(String::new); // 浏览器本地 "HH:MM"
    let mut draft_retention = use_signal(|| "30".to_string());
    let mut draft_include_uploads = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut just_saved = use_signal(|| false);
    let mut card_error = use_signal(|| Option::<String>::None);

    // 挂载拉取并回填草稿（一次性种子回填，合法 effect）。
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            let mut view = view;
            match get_backup_settings().await {
                Ok(v) => {
                    draft_enabled.set(v.settings.auto_enabled);
                    draft_time.set(utc_hhmm_to_local(&v.settings.time_utc));
                    draft_retention.set(v.settings.retention_count.to_string());
                    draft_include_uploads.set(v.settings.include_uploads);
                    view.set(Some(v));
                }
                Err(e) => card_error.set(Some(e.to_string())),
            }
        });
    });

    // 派生：草稿是否与已存设置分叉（memo 避免每次渲染重复 parse）。
    let dirty = use_memo(move || {
        let guard = view.read();
        let Some(v) = guard.as_ref() else {
            return false;
        };
        let retention_changed = draft_retention()
            .trim()
            .parse::<i32>()
            .map(|d| d != v.settings.retention_count)
            .unwrap_or(true);
        draft_enabled() != v.settings.auto_enabled
            || retention_changed
            || draft_include_uploads() != v.settings.include_uploads
            || draft_time() != utc_hhmm_to_local(&v.settings.time_utc)
    });

    // 预格式化状态行文本（render 内联算派生值，不存 signal）。
    let (last_run_label, next_run_label) = {
        let v = view.read();
        let last = v.as_ref().and_then(|v| v.last_run.as_ref()).map(|r| {
            let at = rfc3339_to_local_display(&r.at);
            if r.ok {
                match &r.file {
                    Some(f) => format!("{at} 成功（{f}）"),
                    None => format!("{at} 成功"),
                }
            } else {
                format!("{at} 失败：{}", r.error.clone().unwrap_or_default())
            }
        });
        let next = v
            .as_ref()
            .and_then(|v| v.next_run_at.as_ref())
            .map(|t| rfc3339_to_local_display(t));
        (last, next)
    };
    let loaded = view.read().is_some();

    rsx! {
        CollapsibleSettingsCard {
            title: "自动备份".to_string(),
            summary: if !loaded { "正在加载设置…".to_string() } else if draft_enabled() { format!("已开启 · 每天 {} 执行", draft_time()) } else { "已关闭".to_string() },
            enabled: draft_enabled(),
            on_toggle: move |_| just_saved.set(false),
            div { class: "border-t border-paper-border p-5 space-y-5",

                if !loaded {
                    // SSR/首帧占位：与客户端初始渲染一致，fetch 落地后替换。
                    div { class: "text-xs text-paper-secondary py-2", "正在加载设置…" }
                } else {
                    // 启用开关行（toggle 样式镜像回收站设置卡片）
                    div { class: "flex items-center justify-between gap-4",
                        div { class: "min-w-0",
                            div { class: "text-sm font-medium text-paper-primary", "启用自动备份" }
                            div { class: "text-xs text-paper-secondary mt-1",
                                "到达设定时间后由后台任务执行"
                            }
                        }
                        ToggleSwitch {
                            checked: draft_enabled(),
                            ontoggle: move |_| {
                                draft_enabled.set(!draft_enabled());
                                just_saved.set(false);
                            },
                        }
                    }

                    // 执行时间（本地时间显示/编辑，存 UTC）
                    div { class: "space-y-3",
                        div { class: "min-w-0",
                            div { class: "text-sm font-medium text-paper-primary", "执行时间" }
                            div { class: "text-xs text-paper-secondary mt-1",
                                "按浏览器本地时间显示与编辑，服务端以 UTC 存储"
                            }
                        }
                        div { class: "flex items-center gap-3",
                            TimePicker {
                                value: draft_time(),
                                onchange: move |v: String| {
                                    draft_time.set(v);
                                    just_saved.set(false);
                                },
                            }
                            span { class: "text-xs text-paper-secondary", "本地时间" }
                        }
                    }

                    // 保留份数（步进器镜像回收站保留天数）
                    div { class: "space-y-3",
                        div { class: "min-w-0",
                            div { class: "text-sm font-medium text-paper-primary", "保留份数" }
                            div { class: "text-xs text-paper-secondary mt-1",
                                "自动备份保留的最近份数，超出后最旧的连配对素材包一起删除（1–365）"
                            }
                        }
                        div { class: "flex items-center gap-3",
                            div { class: "flex items-center rounded-lg border border-paper-border bg-paper-entry overflow-hidden",
                                button {
                                    class: "{BTN_ICON}",
                                    r#type: "button",
                                    aria_label: "减少保留份数",
                                    onclick: move |_| {
                                        let cur: i32 = draft_retention().trim().parse().unwrap_or(30);
                                        let next = cur.saturating_sub(1).max(1);
                                        draft_retention.set(next.to_string());
                                        just_saved.set(false);
                                    },
                                    "−"
                                }
                                input {
                                    r#type: "number",
                                    min: "1",
                                    max: "365",
                                    class: "w-14 h-9 px-1 text-center text-sm tabular-nums text-paper-primary bg-transparent border-0 focus:outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none",
                                    value: "{draft_retention()}",
                                    oninput: move |e| {
                                        draft_retention.set(e.value());
                                        just_saved.set(false);
                                    },
                                }
                                button {
                                    class: "{BTN_ICON}",
                                    r#type: "button",
                                    aria_label: "增加保留份数",
                                    onclick: move |_| {
                                        let cur: i32 = draft_retention().trim().parse().unwrap_or(30);
                                        let next = cur.saturating_add(1).min(365);
                                        draft_retention.set(next.to_string());
                                        just_saved.set(false);
                                    },
                                    "+"
                                }
                            }
                            span { class: "text-xs text-paper-secondary", "份" }
                        }
                    }

                    // 包含 uploads 开关
                    div { class: "flex items-center justify-between gap-4",
                        div { class: "min-w-0",
                            div { class: "text-sm font-medium text-paper-primary",
                                "包含 uploads 素材"
                            }
                            div { class: "text-xs text-paper-secondary mt-1",
                                "每次备份附带 uploads/ 打包（tar.gz，排除可重建的转码缓存）"
                            }
                        }
                        ToggleSwitch {
                            checked: draft_include_uploads(),
                            ontoggle: move |_| {
                                draft_include_uploads.set(!draft_include_uploads());
                                just_saved.set(false);
                            },
                        }
                    }

                    // 状态行：上次结果 / 下次执行
                    if last_run_label.is_some() || next_run_label.is_some() {
                        div { class: "text-xs text-paper-secondary space-y-1 border-t border-paper-border pt-3",
                            if let Some(label) = last_run_label {
                                div { "上次自动备份：{label}" }
                            }
                            if let Some(label) = next_run_label {
                                div { "下次执行：{label}（本地时间）" }
                            }
                        }
                    }

                    if let Some(err) = card_error() {
                        div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-3 text-sm text-red-700 dark:text-red-300",
                            "{err}"
                        }
                    }

                    // 底部操作行
                    div { class: "flex items-center justify-between gap-4 pt-1",
                        if just_saved() {
                            span { class: "inline-flex items-center gap-1.5 text-xs text-paper-accent",
                                svg {
                                    class: "w-3.5 h-3.5",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2.5",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M5 13l4 4L19 7",
                                    }
                                }
                                "已保存"
                            }
                        } else if dirty() {
                            span { class: "text-xs text-paper-secondary", "有未保存的更改" }
                        } else {
                            span { class: "text-xs text-transparent select-none", "·" }
                        }
                        LoadingButton {
                            label: "保存设置".to_string(),
                            loading: saving(),
                            disabled: just_saved() || !dirty(),
                            variant: "sm",
                            onclick: move |_| {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    let enabled = draft_enabled();
                                    let time_utc = local_hhmm_to_utc(&draft_time());
                                    let retention: i32 =
                                        draft_retention().trim().parse().unwrap_or(30);
                                    let include = draft_include_uploads();
                                    saving.set(true);
                                    spawn(async move {
                                        let mut view = view;
                                        match update_backup_settings(enabled, time_utc, retention, include).await
                                        {
                                            Ok(v) => {
                                                // 以服务端正典值回填草稿（normalize/clamp 后的值）。
                                                draft_enabled.set(v.settings.auto_enabled);
                                                draft_time.set(utc_hhmm_to_local(&v.settings.time_utc));
                                                draft_retention
                                                    .set(v.settings.retention_count.to_string());
                                                draft_include_uploads.set(v.settings.include_uploads);
                                                view.set(Some(v));
                                                just_saved.set(true);
                                                card_error.set(None);
                                            }
                                            Err(e) => card_error.set(Some(e.to_string())),
                                        }
                                        saving.set(false);
                                    });
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// RFC3339 → 浏览器本地「YYYY-MM-DD HH:MM」。解析失败原样返回。
/// 仅在 view 已落地（hydration 后）的渲染路径出现，无 SSR 不一致问题。
fn rfc3339_to_local_display(iso: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let ms = js_sys::Date::parse(iso);
        if ms.is_nan() {
            return iso.to_string();
        }
        let d = js_sys::Date::new(&ms.into());
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            d.get_full_year(),
            d.get_month() + 1,
            d.get_date(),
            d.get_hours(),
            d.get_minutes()
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        iso.to_string()
    }
}
