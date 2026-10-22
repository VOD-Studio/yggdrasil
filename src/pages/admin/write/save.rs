//! Saving uses a captured form snapshot and one result path for create/update.
use super::state::{PostDraft, SaveState, WriteEditorState};
use crate::bridges::library::library_ready;
use crate::components::ui::Toaster;
#[cfg(target_arch = "wasm32")]
use crate::utils::error::failure_message;
use dioxus::prelude::*;

#[derive(Debug, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
struct PostSubmission {
    title: String,
    slug: Option<String>,
    summary: Option<String>,
    markdown: String,
    status: String,
    tags: Vec<String>,
    cover_image: Option<String>,
}

fn optional_text(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn prepare(draft: &PostDraft, markdown: String) -> Result<PostSubmission, &'static str> {
    let Some(title) = optional_text(&draft.title) else {
        return Err("标题不能为空");
    };
    if markdown.contains("](blob:") || markdown.contains("data-upload-state") {
        return Err("检测到未完成上传的图片，请处理后保存");
    }
    if markdown.trim().is_empty() {
        return Err("内容不能为空");
    }
    Ok(PostSubmission {
        title,
        slug: optional_text(&draft.slug),
        summary: optional_text(&draft.summary),
        markdown,
        status: draft.status.clone(),
        tags: draft.parsed_tags(),
        cover_image: optional_text(&draft.cover_image),
    })
}

/// 保存为给定状态（`draft` / `published`）。成功后留在本页：更新已保存基线与时间，
/// 新文章换到编辑路由；失败时保留表单，状态指示可按原状态重试。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables, unused_mut))]
pub(super) fn submit(state: WriteEditorState, toast: Toaster, status: &'static str) {
    let mut save = state.save;
    if save.peek().is_saving() || !state.phase.peek().is_ready() || !library_ready(state.library) {
        return;
    }
    let fail = move |mut save: Signal<SaveState>, message: String| {
        toast.error(message.clone());
        save.set(SaveState::Failed { message, status });
    };
    let uploads = state.uploads_in_flight.peek();
    let upload_error = if uploads.uploading > 0 {
        Some(format!(
            "有 {} 张图片正在上传，请等待完成后再保存",
            uploads.uploading
        ))
    } else if uploads.error > 0 {
        Some(format!(
            "有 {} 张图片上传失败，请移除或重试后再保存",
            uploads.error
        ))
    } else if *state.cover_uploading.peek() {
        Some("封面图正在上传，请等待完成后再保存".into())
    } else if *state.body_picker_uploading.peek() {
        Some("图片正在上传，请等待完成后再保存".into())
    } else {
        None
    };
    drop(uploads);
    if let Some(message) = upload_error {
        fail(save, message);
        return;
    }
    // Snapshot before spawning; edits made during an in-flight request stay local.
    let mut draft = state.form.peek().clone();
    draft.status = status.to_owned();
    #[cfg(target_arch = "wasm32")]
    {
        use crate::api::posts::{create_post, update_post, CreatePostResponse};
        use dioxus::prelude::spawn;
        let markdown = state
            .editor
            .peek()
            .as_ref()
            .map(|handle| handle.instance().get_markdown())
            .unwrap_or_else(|| draft.content.clone());
        let submission = match prepare(&draft, markdown.clone()) {
            Ok(submission) => submission,
            Err(message) => {
                fail(save, message.into());
                return;
            }
        };
        save.set(SaveState::Saving);
        let post_id = state.post_id;
        let was_published = state.saved.peek().is_published();
        spawn(async move {
            let PostSubmission {
                title,
                slug,
                summary,
                markdown: body,
                status: _,
                tags,
                cover_image,
            } = submission;
            let result = if let Some(id) = post_id {
                update_post(
                    id,
                    title,
                    slug,
                    summary,
                    body,
                    status.to_owned(),
                    tags,
                    cover_image,
                )
                .await
            } else {
                create_post(
                    title,
                    slug,
                    summary,
                    body,
                    status.to_owned(),
                    tags,
                    cover_image,
                )
                .await
            };
            match result {
                Ok(CreatePostResponse {
                    success: true,
                    post_id: new_id,
                    slug,
                    ..
                }) => {
                    let mut form = state.form;
                    let mut saved = state.saved;
                    let submitted_slug = draft.slug.clone();
                    let submitted_content = draft.content.clone();
                    let mut snapshot = draft;
                    snapshot.content = markdown;
                    if let Some(slug) = slug {
                        snapshot.slug = slug;
                    }
                    // Only adopt server values for fields untouched since the snapshot.
                    form.with_mut(|f| {
                        if f.slug == submitted_slug {
                            f.slug = snapshot.slug.clone();
                        }
                        if f.content == submitted_content {
                            f.content = snapshot.content.clone();
                        }
                        f.status = status.to_owned();
                    });
                    saved.set(snapshot);
                    let mut published_at = state.published_at;
                    if status == "published" && published_at.peek().is_none() {
                        published_at.set(Some(chrono::Utc::now()));
                    }
                    let mut saved_at = state.saved_at;
                    saved_at.set(Some(
                        crate::utils::time::local_now().format("%H:%M").to_string(),
                    ));
                    save.set(SaveState::Idle);
                    match (was_published, status == "published") {
                        (false, true) => toast.success("已发布"),
                        (true, false) => toast.success("已转为草稿"),
                        _ => {}
                    }
                    if let (None, Some(id)) = (post_id, new_id) {
                        let _ = dioxus::router::navigator()
                            .replace(crate::router::Route::WriteEdit { id });
                    }
                }
                Ok(CreatePostResponse { message, .. }) => fail(save, message),
                Err(error) => fail(save, failure_message("保存失败", &error)),
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    fail(save, "此页面需要 JavaScript 才能保存".into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_normalizes_optional_metadata_and_chinese_tag_separators() {
        let draft = PostDraft {
            title: "  文章标题  ".into(),
            slug: "  my-post ".into(),
            summary: " \n ".into(),
            tags: " Rust，编辑器；测试, ; ".into(),
            cover_image: " /uploads/cover.webp ".into(),
            ..PostDraft::default()
        };
        let submission = prepare(&draft, "正文".into()).unwrap();
        assert_eq!(submission.title, "文章标题");
        assert_eq!(submission.slug.as_deref(), Some("my-post"));
        assert_eq!(submission.summary, None);
        assert_eq!(submission.tags, ["Rust", "编辑器", "测试"]);
        assert_eq!(
            submission.cover_image.as_deref(),
            Some("/uploads/cover.webp")
        );
    }

    #[test]
    fn saving_rejects_missing_content_and_image_placeholders_but_accepts_blob_discussion() {
        let draft = PostDraft {
            title: "标题".into(),
            ..PostDraft::default()
        };
        assert_eq!(prepare(&draft, " \n ".into()).unwrap_err(), "内容不能为空");
        assert!(prepare(&draft, "![图](blob:pending)".into()).is_err());
        assert!(prepare(&draft, "图片 data-upload-state=error".into()).is_err());
        assert!(prepare(&draft, "代码中使用 blob: URL".into()).is_ok());
    }

    #[test]
    fn saving_rejects_blank_titles() {
        assert_eq!(
            prepare(&PostDraft::default(), "正文".into()).unwrap_err(),
            "标题不能为空"
        );
    }
}
