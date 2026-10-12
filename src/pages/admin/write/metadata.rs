//! 文章元信息面板，与正文编辑器共享表单快照。
use super::{cover::CoverUploader, state::PostDraft};
use crate::components::forms::FormInput;
use dioxus::prelude::*;

#[component]
pub(super) fn MetadataPanel(
    mut form: Signal<PostDraft>,
    mut sidebar_open: Signal<bool>,
    cover_uploading: Signal<bool>,
) -> Element {
    let parsed_tags = use_memo(move || form.read().parsed_tags());
    rsx! {
                    div { "data-vt-scroll": "admin-write-settings", class: "w-80 sm:w-88 flex-shrink-0 min-h-0 overflow-y-auto border-l border-[var(--color-paper-border)]/70 bg-[var(--color-paper-theme)] flex flex-col animate-section-enter",
                        // 侧栏标题栏
                        div { class: "px-5 py-4 border-b border-[var(--color-paper-border)]/60 flex items-center justify-between select-none",
                            div { class: "flex items-center gap-2",
                                svg {
                                    class: "w-4 h-4 text-[var(--color-paper-secondary)]",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    line { x1: "4", y1: "21", x2: "4", y2: "14" }
                                    line { x1: "4", y1: "10", x2: "4", y2: "3" }
                                    line { x1: "12", y1: "21", x2: "12", y2: "12" }
                                    line { x1: "12", y1: "8", x2: "12", y2: "3" }
                                    line { x1: "20", y1: "21", x2: "20", y2: "16" }
                                    line { x1: "20", y1: "12", x2: "20", y2: "3" }
                                    line { x1: "1", y1: "14", x2: "7", y2: "14" }
                                    line { x1: "9", y1: "8", x2: "15", y2: "8" }
                                    line { x1: "17", y1: "16", x2: "23", y2: "16" }
                                }
                                span { class: "text-xs font-bold uppercase tracking-wider text-[var(--color-paper-secondary)]",
                                    "文章属性"
                                }
                            }
                            button {
                                class: "text-[var(--color-paper-tertiary)] hover:text-[var(--color-paper-primary)] p-1 rounded-lg hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                                title: "收起侧栏",
                                onclick: move |_| sidebar_open.set(false),
                                svg {
                                    class: "w-4 h-4",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    line { x1: "18", y1: "6", x2: "6", y2: "18" }
                                    line { x1: "6", y1: "6", x2: "18", y2: "18" }
                                }
                            }
                        }

                        // Slug 节
                        div { class: "p-5 border-b border-[var(--color-paper-border)]/60 flex flex-col gap-2.5",
                            div { class: "flex items-center justify-between",
                                label { class: "flex items-center gap-1.5 text-xs font-semibold text-[var(--color-paper-secondary)]",
                                    svg {
                                        class: "w-3.5 h-3.5 text-[var(--color-paper-tertiary)]",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        path { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }
                                        path { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }
                                    }
                                    "永久链接 (Slug)"
                                }
                            }
                            div { class: "flex items-center gap-1 text-[11px] text-[var(--color-paper-tertiary)] font-mono px-1 truncate",
                                span { class: "text-[var(--color-paper-accent)] font-semibold", "/post/" }
                                span { class: "truncate",
                                    if form.read().slug.clone().trim().is_empty() {
                                        "(自动生成)"
                                    } else {
                                        "{form.read().slug.clone()}"
                                    }
                                }
                            }
                            FormInput {
                                r#type: "text",
                                placeholder: "自定义 URL，留空自动生成...",
                                value: form.read().slug.clone(),
                                oninput: move |v: String| form.write().slug = v,
                            }
                        }

                        // 标签节
                        div { class: "p-5 border-b border-[var(--color-paper-border)]/60 flex flex-col gap-2.5",
                            label { class: "flex items-center gap-1.5 text-xs font-semibold text-[var(--color-paper-secondary)]",
                                svg {
                                    class: "w-3.5 h-3.5 text-[var(--color-paper-tertiary)]",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z" }
                                    line { x1: "7", y1: "7", x2: "7.01", y2: "7" }
                                }
                                "文章标签"
                            }
                            FormInput {
                                r#type: "text",
                                placeholder: "输入标签，逗号或分号分隔...",
                                value: form.read().tags.clone(),
                                oninput: move |v: String| form.write().tags = v,
                            }
                            // 标签胶囊预览
                            if !parsed_tags().is_empty() {
                                div { class: "flex flex-wrap gap-1.5 mt-1",
                                    for tag in parsed_tags() {
                                        span {
                                            key: "{tag}",
                                            class: "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]/70 shadow-2xs",
                                            "#{tag}"
                                        }
                                    }
                                }
                            }
                        }

                        // 摘要节
                        div { class: "p-5 border-b border-[var(--color-paper-border)]/60 flex flex-col gap-2.5",
                            div { class: "flex items-center justify-between",
                                label { class: "flex items-center gap-1.5 text-xs font-semibold text-[var(--color-paper-secondary)]",
                                    svg {
                                        class: "w-3.5 h-3.5 text-[var(--color-paper-tertiary)]",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                                        polyline { points: "14 2 14 8 20 8" }
                                        line { x1: "16", y1: "13", x2: "8", y2: "13" }
                                        line { x1: "16", y1: "17", x2: "8", y2: "17" }
                                    }
                                    "文章摘要"
                                }
                                span { class: "text-[11px] text-[var(--color-paper-tertiary)]",
                                    "{form.read().summary.clone().chars().count()} 字"
                                }
                            }
                            textarea {
                                class: "w-full text-sm bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] placeholder-[var(--color-paper-tertiary)] focus:outline-none border border-[var(--color-paper-border)] focus:border-paper-accent focus:ring-1 focus:ring-paper-accent/30 rounded-2xl px-3.5 py-2.5 transition-all resize-none leading-relaxed",
                                placeholder: "选填，留空将自动从正文提取...",
                                rows: "3",
                                value: form.read().summary.clone(),
                                oninput: move |evt| form.write().summary = evt.value(),
                            }
                        }

                        // 封面图节
                        div { class: "p-5 flex flex-col gap-2.5",
                            div { class: "flex items-center justify-between",
                                label { class: "flex items-center gap-1.5 text-xs font-semibold text-[var(--color-paper-secondary)]",
                                    svg {
                                        class: "w-3.5 h-3.5 text-[var(--color-paper-tertiary)]",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        rect { x: "3", y: "3", width: "18", height: "18", rx: "2", ry: "2" }
                                        circle { cx: "8.5", cy: "8.5", r: "1.5" }
                                        polyline { points: "21 15 16 10 5 21" }
                                    }
                                    "文章封面"
                                }
                                span { class: "text-[11px] text-[var(--color-paper-tertiary)]",
                                    "21:9 推荐"
                                }
                            }
                            CoverUploader { form, cover_uploading }
                        }
                    }
    }
}
