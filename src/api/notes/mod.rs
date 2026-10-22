//! 笔记的浏览器入口。认证适配后，统一调用 store 的版本与发布规则。
#![allow(clippy::unused_unit, deprecated)]
use crate::models::note::*;
use crate::models::public_note::PublicNote;
use dioxus::prelude::*;
#[cfg(feature = "server")]
pub mod attachments;
#[cfg(feature = "server")]
pub mod store;

#[server(ListNotes, "/api")]
pub async fn list_notes(filter: NoteFilter) -> Result<NotePage, ServerFnError> {
    Ok(store::list(store::Access::Public, filter).await?)
}

#[server(GetNote, "/api")]
pub async fn get_note(slug: String) -> Result<PublicNote, ServerFnError> {
    // 公开详情只返回阅读所需字段（P4.1）；编辑 Markdown 与内部版本号不出接口。
    // 后台编辑与 MCP 继续走 get_owned_note / store::get 的完整 Note 契约。
    Ok(store::get_public(&slug).await?)
}

#[server(NoteStatsSummary, "/api")]
pub async fn note_stats() -> Result<NoteStats, ServerFnError> {
    Ok(store::stats(8).await?)
}

/// 阅读页的笔记本上下文；`book` 来自 `?book=`，无效时忽略。
#[server(GetNoteContext, "/api")]
pub async fn get_note_context(
    slug: String,
    book: Option<i32>,
) -> Result<Option<NoteContext>, ServerFnError> {
    Ok(store::context(&slug, book).await?)
}

#[server(ListNotebooks, "/api")]
pub async fn list_notebooks() -> Result<Vec<Notebook>, ServerFnError> {
    Ok(store::notebooks(store::Access::Public).await?)
}

#[server(ListOwnedNotes, "/api")]
pub async fn list_owned_notes(filter: NoteFilter) -> Result<NotePage, ServerFnError> {
    let user = crate::api::auth::get_current_admin_user().await?;
    Ok(store::list(store::Access::Owner(user.id), filter).await?)
}

#[server(GetOwnedNote, "/api")]
pub async fn get_owned_note(id: i32, version: Option<i32>) -> Result<Note, ServerFnError> {
    let user = crate::api::auth::get_current_admin_user().await?;
    Ok(store::get(store::Access::Owner(user.id), Some(id), None, version).await?)
}

#[server(SaveNote, "/api")]
pub async fn save_note(draft: NoteDraft) -> Result<Note, ServerFnError> {
    let user = crate::api::auth::get_current_admin_user().await?;
    Ok(store::save(user.id, draft, None).await?)
}

#[server(ActOnNote, "/api")]
pub async fn act_on_note(
    id: i32,
    expected_version: i32,
    action: NoteAction,
) -> Result<Note, ServerFnError> {
    let user = crate::api::auth::get_current_admin_user().await?;
    Ok(store::act(user.id, id, expected_version, action).await?)
}

#[server(NoteHistory, "/api")]
pub async fn note_history(id: i32) -> Result<Vec<NoteRevision>, ServerFnError> {
    let user = crate::api::auth::get_current_admin_user().await?;
    Ok(store::history(user.id, id).await?)
}

#[server(OwnedNotebooks, "/api")]
pub async fn owned_notebooks() -> Result<Vec<Notebook>, ServerFnError> {
    let user = crate::api::auth::get_current_admin_user().await?;
    Ok(store::notebooks(store::Access::Owner(user.id)).await?)
}

#[server(SaveNotebook, "/api")]
pub async fn save_notebook(input: NotebookInput) -> Result<(), ServerFnError> {
    let user = crate::api::auth::get_current_admin_user().await?;
    Ok(store::save_notebook(user.id, input).await?)
}

#[server(ReorderNotebook, "/api")]
pub async fn reorder_notebook(id: i32, note_ids: Vec<i32>) -> Result<(), ServerFnError> {
    let user = crate::api::auth::get_current_admin_user().await?;
    Ok(store::reorder(user.id, id, note_ids).await?)
}

#[server(ArchiveNotebook, "/api")]
pub async fn archive_notebook(id: i32, archived: bool) -> Result<(), ServerFnError> {
    let user = crate::api::auth::get_current_admin_user().await?;
    Ok(store::archive_notebook(user.id, id, archived).await?)
}

#[server(NotebookOrder, "/api")]
pub async fn notebook_order(id: i32) -> Result<Vec<NotebookEntry>, ServerFnError> {
    let user = crate::api::auth::get_current_admin_user().await?;
    let client = crate::db::pool::get_conn()
        .await
        .map_err(crate::api::error::AppError::db_conn)?;
    let rows=client.query("SELECT n.id, CASE WHEN r.title='' THEN r.summary ELSE r.title END,
        n.published_version IS NOT NULL, n.version > COALESCE(n.published_version, n.version), n.deleted_at IS NOT NULL
        FROM notebook_notes bn
        JOIN notebooks b ON b.id=bn.notebook_id JOIN notes n ON n.id=bn.note_id
        JOIN note_revisions r ON r.note_id=n.id AND r.version=n.version
        WHERE b.id=$1 AND b.owner_id=$2 ORDER BY bn.position,n.id", &[&id,&user.id]).await.map_err(crate::api::error::AppError::query)?;
    Ok(rows
        .iter()
        .map(|r| NotebookEntry {
            id: r.get(0),
            title: r.get(1),
            published: r.get(2),
            ahead: r.get(3),
            trashed: r.get(4),
        })
        .collect())
}
