//! 前台笔记骨架屏：列表 / 笔记本 / 单篇阅读三种版式，以及路由切换时的整页占位。
//!
//! 原先与 `pages/notes.rs` 同处一文件；`FrontendLayout` 的路由级 fallback 要按路径选择
//! 骨架屏，而页面本身是懒加载分包，因此占位组件放在主包的 `skeletons` 层，
//! 笔记页的 `Suspense` fallback 同样从这里引用，保证两处视觉一致。

use leptos::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 笔记流占位：日期标签、两条随记与一块卡片。
#[component]
pub fn NotesStreamSkeleton() -> impl IntoView {
    view! {
        <div class="notes-stream-skeleton" aria-hidden="true">
            <SkeletonBox class="h-3 w-24"/>
            {(0..2)
                .map(|_| {
                    view! {
                        <div class="note-moment">
                            <SkeletonBox class="h-3 w-10"/>
                            <div>
                                <SkeletonBox class="h-4 w-full"/>
                                <SkeletonBox class="h-4 mt-2 w-3/4"/>
                            </div>
                        </div>
                    }
                })
                .collect_view()}
            <SkeletonBox class="h-36 w-full rounded-[10px] mt-3"/>
        </div>
    }
}

/// 列表页整页占位：页头、左栏与笔记流，与真实布局对齐。
#[component]
pub fn NotesPageSkeleton() -> impl IntoView {
    view! {
        <div class="notes" role="status" aria-label="正在加载笔记">
            <div class="notes-header" aria-hidden="true">
                <div>
                    <h1 class="notes-title">"笔记"</h1>
                    <SkeletonBox class="h-4 mt-3 w-72 max-w-full"/>
                </div>
            </div>
            <div class="notes-layout" aria-hidden="true">
                <div class="notes-side">
                    <SkeletonBox class="h-[34px] w-full rounded-lg"/>
                    <div class="notes-side-group">
                        {(0..3)
                            .map(|_| view! { <SkeletonBox class="h-8 w-full rounded-lg mt-1"/> })
                            .collect_view()}
                    </div>
                </div>
                <div class="notes-stream">
                    <NotesStreamSkeleton/>
                </div>
            </div>
        </div>
    }
}

/// 笔记本页占位：标题、简介、开始阅读按钮与条目列表。
#[component]
pub fn NotebookSkeleton() -> impl IntoView {
    view! {
        <div role="status" aria-label="正在加载笔记本">
            <div aria-hidden="true">
                <SkeletonBox class="h-3 w-20"/>
                <SkeletonBox class="h-9 mt-8 w-56"/>
                <SkeletonBox class="h-4 mt-4 w-full"/>
                <SkeletonBox class="h-10 mt-6 w-36 rounded-[10px]"/>
                <div class="notebook-entries">
                    {(0..4)
                        .map(|_| {
                            view! {
                                <div class="notebook-entry">
                                    <SkeletonBox class="h-5 w-7"/>
                                    <div>
                                        <SkeletonBox class="h-5 w-3/5"/>
                                        <SkeletonBox class="h-3 mt-3 w-full"/>
                                    </div>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
        </div>
    }
}

/// 阅读页占位：左栏目录与正文。
#[component]
pub fn NoteReaderSkeleton() -> impl IntoView {
    view! {
        <div class="notes-layout note-reader" role="status" aria-label="正在加载笔记">
            <div class="notes-side note-rail" aria-hidden="true">
                <SkeletonBox class="h-3 w-20"/>
                {(0..4)
                    .map(|_| view! { <SkeletonBox class="h-7 w-full rounded-lg mt-2"/> })
                    .collect_view()}
            </div>
            <div class="note-document" aria-hidden="true">
                <SkeletonBox class="h-5 w-24 rounded-full"/>
                <SkeletonBox class="h-9 mt-4 w-4/5"/>
                <SkeletonBox class="h-3 mt-4 w-48"/>
                <div class="flex flex-col gap-3 mt-10">
                    {(0..6)
                        .map(|_| view! { <SkeletonBox class="h-3.5 w-full"/> })
                        .collect_view()}
                </div>
            </div>
        </div>
    }
}

/// 前台笔记路径的三种版式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotesLayout {
    /// `/notes`：笔记列表。
    List,
    /// `/notes/book/:id`：笔记本。
    Notebook,
    /// `/notes/:slug`：单篇笔记。
    Note,
}

/// 按路径区分版式；忽略查询串、片段与结尾的 `/`。
/// 与路由表一致：`/notes/book/:id` 恰好两段才是笔记本，其余单段 `/notes/<x>` 都是笔记
/// （含 slug 恰为 `book` 的笔记）。
fn notes_layout(path: &str) -> NotesLayout {
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let mut segments = path
        .strip_prefix("/notes")
        .unwrap_or_default()
        .split('/')
        .filter(|segment| !segment.is_empty());
    match (segments.next(), segments.next(), segments.next()) {
        (Some("book"), Some(_), None) => NotesLayout::Notebook,
        (Some(_), None, _) => NotesLayout::Note,
        _ => NotesLayout::List,
    }
}

/// 路由切换时的整页占位：按目标路径选择（`path` 为 `pathname`，可带查询串）。
#[component]
pub fn NotesRouteSkeleton(#[prop(into)] path: String) -> impl IntoView {
    match notes_layout(&path) {
        NotesLayout::Note => view! {
            <div class="notes">
                <NoteReaderSkeleton/>
            </div>
        }
        .into_any(),
        NotesLayout::Notebook => view! {
            <div class="notebook">
                <NotebookSkeleton/>
            </div>
        }
        .into_any(),
        NotesLayout::List => view! { <NotesPageSkeleton/> }.into_any(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_layout_distinguishes_list_notebook_and_note() {
        assert_eq!(notes_layout("/notes"), NotesLayout::List);
        assert_eq!(notes_layout("/notes/"), NotesLayout::List);
        assert_eq!(notes_layout("/notes/book/3"), NotesLayout::Notebook);
        assert_eq!(notes_layout("/notes/hello"), NotesLayout::Note);
        assert_eq!(notes_layout("/notes/hello?book=3"), NotesLayout::Note);
        assert_eq!(notes_layout("/notes/book"), NotesLayout::Note);
        assert_eq!(notes_layout("/notes/book/3/x"), NotesLayout::List);
    }

    #[test]
    fn notes_route_skeleton_uses_the_matching_wrapper() {
        let html = |path: &str| {
            let path = path.to_string();
            Owner::new().with(|| view! { <NotesRouteSkeleton path=path/> }.to_html())
        };
        assert!(html("/notes/book/3").contains(r#"class="notebook""#));
        assert!(html("/notes/hello").contains("note-reader"));
        assert!(html("/notes").contains("notes-header"));
    }
}
