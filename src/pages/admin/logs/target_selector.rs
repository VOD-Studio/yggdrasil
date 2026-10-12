//! target selector behavior of the log page.
use dioxus::prelude::*;

static TARGET_SELECT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// target 筛选选择器：触发器 + 下拉列表面板。
///
/// 选项为动态 target 列表（含「全部 target」清空项与暂无 target 提示），
/// 列表项采用等宽字体展示 Rust module 路径。
/// 触发器与面板尺寸、圆角、层级和动画完全对齐 [`FormSelect`] 与 [`INPUT_INLINE_CLASS`]。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn TargetSelect(
    value: Option<String>,
    options: Vec<String>,
    onchange: EventHandler<Option<String>>,
) -> Element {
    let id_prefix = use_hook(|| TARGET_SELECT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
    let trigger_id = format!("target-select-{id_prefix}");

    #[cfg(target_arch = "wasm32")]
    let trigger_id_click = trigger_id.clone();
    #[cfg(target_arch = "wasm32")]
    let trigger_id_keys = trigger_id.clone();

    let mut open = use_signal(|| false);
    #[allow(unused_mut)]
    let mut flip_up = use_signal(|| false);

    let current_label = value.clone().unwrap_or_else(|| "全部 target".to_string());
    let chevron_rotate = if open() { "rotate-180" } else { "" };
    let total_options = options.len() + 1;

    let placement_cls = if flip_up() {
        "bottom-full mb-1.5 origin-bottom"
    } else {
        "top-full mt-1.5 origin-top"
    };

    rsx! {
        div { class: "relative flex-shrink-0 w-full sm:w-60",
            button {
                id: "{trigger_id}",
                r#type: "button",
                class: "relative inline-flex w-full items-center justify-between cursor-pointer select-none text-left pl-4 pr-10 py-2 border border-paper-border rounded-2xl bg-paper-entry text-paper-primary hover:bg-paper-theme focus:outline-none focus:border-paper-accent focus:ring-1 focus:ring-paper-accent/30 transition-colors duration-200",
                aria_haspopup: "listbox",
                aria_expanded: "{open()}",
                aria_label: "按 target 筛选",
                onclick: move |_| {
                    if !open() {
                        #[cfg(target_arch = "wasm32")]
                        flip_up.set(crate::components::forms::measure_flip(&trigger_id_click, total_options));
                        open.set(true);
                    } else {
                        open.set(false);
                    }
                },
                onkeydown: move |e: KeyboardEvent| {
                    let key = e.key();
                    let is_space = matches!(&key, Key::Character(s) if s == " ");
                    if !open() {
                        if key == Key::ArrowDown || key == Key::ArrowUp || key == Key::Enter || is_space {
                            e.prevent_default();
                            #[cfg(target_arch = "wasm32")]
                            flip_up.set(crate::components::forms::measure_flip(&trigger_id_keys, total_options));
                            open.set(true);
                        }
                    } else if key == Key::Escape || key == Key::Tab {
                        open.set(false);
                    }
                },
                span { class: "truncate", "{current_label}" }
                svg {
                    class: "pointer-events-none absolute right-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-paper-secondary transition-transform duration-200 {chevron_rotate}",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M6 9l6 6 6-6",
                    }
                }
            }

            if open() {
                // 透明遮罩：拦截外部点击关闭
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |_| open.set(false),
                }
                ul {
                    class: "absolute left-0 z-50 w-72 max-w-[calc(100vw_-_2rem)] max-h-60 overflow-y-auto rounded-2xl border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] p-1.5 shadow-lg animate-popover-enter-edge {placement_cls} flex flex-col gap-0.5",
                    role: "listbox",
                    aria_labelledby: "{trigger_id}",
                    // 「全部」选项（清除 target 筛选）
                    {
                        let selected = value.is_none();
                        rsx! {
                            li {
                                button {
                                    r#type: "button",
                                    class: if selected { "w-full flex items-center justify-between gap-2 px-3 py-2 rounded-xl cursor-pointer select-none transition-colors hover:bg-[var(--color-paper-accent-soft)] text-paper-accent text-sm" } else { "w-full flex items-center justify-between gap-2 px-3 py-2 rounded-xl cursor-pointer select-none transition-colors hover:bg-[var(--color-paper-accent-soft)] text-[var(--color-paper-primary)] text-sm" },
                                    role: "option",
                                    aria_selected: "{selected}",
                                    onmousedown: move |e| e.prevent_default(),
                                    onclick: move |_| {
                                        onchange.call(None);
                                        open.set(false);
                                    },
                                    span { class: "truncate", "全部 target" }
                                    if selected {
                                        svg {
                                            class: "w-4 h-4 flex-shrink-0 text-paper-accent",
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
                                    }
                                }
                            }
                        }
                    }
                    if options.is_empty() {
                        li { class: "px-3 py-2 text-xs text-[var(--color-paper-tertiary)] select-none",
                            "暂无 target（日志落库后出现）"
                        }
                    }
                    for t in options {
                        {
                            let selected = value.as_deref() == Some(t.as_str());
                            let pick = t.clone();
                            rsx! {
                                li { key: "{t}",
                                    button {
                                        r#type: "button",
                                        class: if selected { "w-full flex items-center justify-between gap-2 px-3 py-2 rounded-xl cursor-pointer select-none transition-colors hover:bg-[var(--color-paper-accent-soft)] font-mono text-xs text-paper-accent" } else { "w-full flex items-center justify-between gap-2 px-3 py-2 rounded-xl cursor-pointer select-none transition-colors hover:bg-[var(--color-paper-accent-soft)] font-mono text-xs text-[var(--color-paper-primary)]" },
                                        role: "option",
                                        aria_selected: "{selected}",
                                        onmousedown: move |e| e.prevent_default(),
                                        onclick: move |_| {
                                            onchange.call(Some(pick.clone()));
                                            open.set(false);
                                        },
                                        span { class: "truncate", "{t}" }
                                        if selected {
                                            svg {
                                                class: "w-4 h-4 flex-shrink-0 text-paper-accent",
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
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
