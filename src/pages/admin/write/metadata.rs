//! 文章属性抽屉：永久链接、标签、摘要、封面与字数；与正文编辑器共享表单。
use super::{
    cover::CoverUploader,
    state::{PostDraft, WriteEditorState},
};
use crate::components::forms::FormTextarea;
use crate::components::post::post_meta::group_digits;
use crate::components::ui::{ButtonSize, Icon, IconButton, IconName};
use leptos::prelude::*;

/// 正文字数（不计空白）与约略阅读分钟数。
pub(super) fn word_stats(markdown: &str) -> (usize, usize) {
    let chars = markdown.chars().filter(|c| !c.is_whitespace()).count();
    (chars, (chars / 300).max(1))
}

#[component]
pub(super) fn MetadataPanel(
    state: WriteEditorState,
    open: RwSignal<bool>,
    #[prop(into)] on_trash: Callback<()>,
) -> impl IntoView {
    let form = state.form;
    let tag_input = RwSignal::new(String::new());
    let tags = Memo::new(move |_| form.with(PostDraft::parsed_tags));
    let summary_len = Memo::new(move |_| form.with(|f| f.summary.chars().count()));
    let stats = Memo::new(move |_| form.with(|f| word_stats(&f.content)));
    let commit_tag = move || {
        let pending = tag_input.get_untracked();
        if !pending.trim().is_empty() {
            form.update(|f| f.add_tags(&pending));
        }
        tag_input.set(String::new());
    };

    view! {
        <aside class="write-drawer" data-vt-scroll="admin-write-settings" aria-label="文章属性">
            <div class="write-drawer-head">
                <h2>"文章属性"</h2>
                <IconButton
                    icon=IconName::X
                    label="收起属性"
                    size=ButtonSize::Sm
                    tooltip_align="end"
                    onclick=move |_| open.set(false)
                />
            </div>

            <div class="write-prop">
                <label class="write-prop-label" for="write-slug">
                    <span>"永久链接"</span>
                    <span class="write-prop-hint">"留空自动生成"</span>
                </label>
                <div class="field write-slug">
                    <span aria-hidden="true">"/post/"</span>
                    <input
                        id="write-slug"
                        type="text"
                        spellcheck="false"
                        autocomplete="off"
                        value=form.with_untracked(|f| f.slug.clone())
                        prop:value=move || form.with(|f| f.slug.clone())
                        on:input=move |e| {
                            let value = event_target_value(&e);
                            form.update(|f| f.slug = value);
                        }
                    />
                </div>
            </div>

            <div class="write-prop">
                <label class="write-prop-label" for="write-tag-input">
                    <span>"标签"</span>
                    <span class="write-prop-hint">"回车或逗号添加"</span>
                </label>
                <div class="field write-tags">
                    <For each=move || tags.get() key=|tag| tag.clone() let:tag>
                        <span class="write-tag">
                            {tag.clone()}
                            <button
                                type="button"
                                aria-label=format!("移除标签 {tag}")
                                on:click={
                                    let tag = tag.clone();
                                    move |_| form.update(|f| f.remove_tag(&tag))
                                }
                            >
                                <Icon name=IconName::X size=11/>
                            </button>
                        </span>
                    </For>
                    <input
                        id="write-tag-input"
                        type="text"
                        placeholder=move || if tags.with(Vec::is_empty) { "添加标签…" } else { "" }
                        prop:value=move || tag_input.get()
                        on:input=move |e| {
                            let value = event_target_value(&e);
                            // 输入或粘贴里带分隔符时直接拆成标签。
                            let split = value.contains([',', '，', ';', '；']);
                            tag_input.set(value);
                            if split {
                                commit_tag();
                            }
                        }
                        on:keydown=move |e| {
                            if e.is_composing() {
                                return;
                            }
                            if e.key() == "Enter" {
                                e.prevent_default();
                                commit_tag();
                            } else if e.key() == "Backspace" && tag_input.with_untracked(String::is_empty) {
                                if let Some(last) = tags.with_untracked(|t| t.last().cloned()) {
                                    form.update(|f| f.remove_tag(&last));
                                }
                            }
                        }
                        on:blur=move |_| commit_tag()
                    />
                </div>
            </div>

            <div class="write-prop">
                <label class="write-prop-label" for="write-summary">
                    <span>"摘要"</span>
                    <span class="write-prop-hint tabular-nums">{move || summary_len.get()} " 字"</span>
                </label>
                <FormTextarea
                    id="write-summary"
                    class="field field-textarea write-summary"
                    rows=5u32
                    placeholder="选填，留空将从正文提取"
                    value=Signal::derive(move || form.with(|f| f.summary.clone()))
                    oninput=move |v: String| form.update(|f| f.summary = v)
                />
            </div>

            <div class="write-prop">
                <div class="write-prop-label">
                    <span>"封面"</span>
                    <span class="write-prop-hint">"推荐 21:9"</span>
                </div>
                <CoverUploader form cover_uploading=state.cover_uploading/>
            </div>

            <div class="write-drawer-foot">
                <span class="tabular-nums">
                    {move || {
                        let (words, minutes) = stats.get();
                        format!("{} 字 · 约 {minutes} 分钟", group_digits(words as u32))
                    }}
                </span>
                {state
                    .post_id
                    .is_some()
                    .then(|| {
                        view! {
                            <button class="write-trash" type="button" on:click=move |_| on_trash.run(())>
                                "移到回收站"
                            </button>
                        }
                    })}
            </div>
        </aside>
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
