//! 文章属性抽屉：永久链接、标签、摘要、封面与字数；与正文编辑器共享表单。
use super::{cover::CoverUploader, state::WriteEditorState};
use crate::components::post::post_meta::group_digits;
use crate::components::ui::{IconButton, IconName};
use dioxus::prelude::*;

/// 正文字数（不计空白）与约略阅读分钟数。
pub(super) fn word_stats(markdown: &str) -> (usize, usize) {
    let chars = markdown.chars().filter(|c| !c.is_whitespace()).count();
    (chars, (chars / 300).max(1))
}

#[component]
pub(super) fn MetadataPanel(
    state: WriteEditorState,
    mut open: Signal<bool>,
    on_trash: EventHandler<()>,
) -> Element {
    let mut form = state.form;
    let mut tag_input = use_signal(String::new);
    let tags = use_memo(move || form.read().parsed_tags());
    let summary_len = use_memo(move || form.read().summary.chars().count());
    let (words, minutes) = word_stats(&form.read().content);
    let mut commit_tag = move || {
        let pending = tag_input.peek().clone();
        if !pending.trim().is_empty() {
            form.write().add_tags(&pending);
        }
        tag_input.set(String::new());
    };

    rsx! {
        aside { class: "write-drawer", "data-vt-scroll": "admin-write-settings", aria_label: "文章属性",
            div { class: "write-drawer-head",
                h2 { "文章属性" }
                IconButton {
                    icon: IconName::X,
                    label: "收起属性",
                    size: crate::components::ui::ButtonSize::Sm,
                    tooltip_align: "end",
                    onclick: move |_| open.set(false),
                }
            }

            div { class: "write-prop",
                label { class: "write-prop-label", r#for: "write-slug",
                    span { "永久链接" }
                    span { class: "write-prop-hint", "留空自动生成" }
                }
                div { class: "field write-slug",
                    span { aria_hidden: "true", "/post/" }
                    input {
                        id: "write-slug",
                        r#type: "text",
                        spellcheck: "false",
                        autocomplete: "off",
                        value: "{form.read().slug}",
                        oninput: move |e| form.write().slug = e.value(),
                    }
                }
            }

            div { class: "write-prop",
                label { class: "write-prop-label", r#for: "write-tag-input",
                    span { "标签" }
                    span { class: "write-prop-hint", "回车或逗号添加" }
                }
                div { class: "field write-tags",
                    for tag in tags() {
                        span { key: "{tag}", class: "write-tag",
                            "{tag}"
                            button {
                                r#type: "button",
                                aria_label: "移除标签 {tag}",
                                onclick: {
                                    let tag = tag.clone();
                                    move |_| form.write().remove_tag(&tag)
                                },
                                crate::components::ui::Icon { name: IconName::X, size: 11 }
                            }
                        }
                    }
                    input {
                        id: "write-tag-input",
                        r#type: "text",
                        placeholder: if tags().is_empty() { "添加标签…" } else { "" },
                        value: "{tag_input}",
                        oninput: move |e| {
                            let value = e.value();
                            // 输入或粘贴里带分隔符时直接拆成标签。
                            if value.contains([',', '，', ';', '；']) {
                                tag_input.set(value);
                                commit_tag();
                            } else {
                                tag_input.set(value);
                            }
                        },
                        onkeydown: move |e| {
                            if e.data().is_composing() {
                                return;
                            }
                            if e.key() == Key::Enter {
                                e.prevent_default();
                                commit_tag();
                            } else if e.key() == Key::Backspace && tag_input.peek().is_empty() {
                                if let Some(last) = tags.peek().last().cloned() {
                                    form.write().remove_tag(&last);
                                }
                            }
                        },
                        onblur: move |_| commit_tag(),
                    }
                }
            }

            div { class: "write-prop",
                label { class: "write-prop-label", r#for: "write-summary",
                    span { "摘要" }
                    span { class: "write-prop-hint tabular-nums", "{summary_len} 字" }
                }
                textarea {
                    id: "write-summary",
                    class: "field field-textarea write-summary",
                    rows: "5",
                    placeholder: "选填，留空将从正文提取",
                    value: "{form.read().summary}",
                    oninput: move |e| form.write().summary = e.value(),
                }
            }

            div { class: "write-prop",
                div { class: "write-prop-label",
                    span { "封面" }
                    span { class: "write-prop-hint", "推荐 21:9" }
                }
                CoverUploader { form, cover_uploading: state.cover_uploading }
            }

            div { class: "write-drawer-foot",
                span { class: "tabular-nums", "{group_digits(words as u32)} 字 · 约 {minutes} 分钟" }
                if state.post_id.is_some() {
                    button {
                        class: "write-trash",
                        r#type: "button",
                        onclick: move |_| on_trash.call(()),
                        "移到回收站"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::word_stats;

    #[test]
    fn word_stats_ignore_whitespace_and_round_up_to_one_minute() {
        assert_eq!(word_stats("你好 世界\n"), (4, 1));
        assert_eq!(word_stats(&"字".repeat(900)), (900, 3));
    }
}
