//! 后台和 MCP 共用的笔记规则；所有读取在 SQL 层选择已授权的版本。
use crate::api::error::AppError;
use crate::db::pool::get_conn;
use crate::models::note::*;

#[derive(Clone)]
pub enum Access {
    Public,
    Owner(i32),
    Knowledge {
        owner_id: i32,
        notebook_ids: Option<Vec<i32>>,
    },
}

impl Access {
    fn revision(&self) -> &'static str {
        match self {
            Self::Public => "published_version",
            Self::Owner(_) => "version",
            Self::Knowledge { .. } => "knowledge_version",
        }
    }
    fn owner(&self) -> Option<i32> {
        match self {
            Self::Public => None,
            Self::Owner(id) => Some(*id),
            Self::Knowledge { owner_id, .. } => Some(*owner_id),
        }
    }
    fn books(&self) -> Option<Vec<i32>> {
        match self {
            Self::Knowledge { notebook_ids, .. } => notebook_ids.clone(),
            _ => None,
        }
    }
}

fn map_note(row: &tokio_postgres::Row, access: &Access) -> Note {
    let admin = matches!(access, Access::Owner(_));
    Note {
        id: row.get("id"),
        slug: row.get("slug"),
        version: row.get("revision"),
        kind: if row.get::<_, String>("kind") == "topic" {
            NoteKind::Topic
        } else {
            NoteKind::Moment
        },
        title: row.get("title"),
        content_md: row.get("content_md"),
        content_html: row.get("content_html"),
        toc_html: row.get("toc_html"),
        summary: row.get("summary"),
        tags: row.get("tags"),
        created_at: row.get("created_at"),
        updated_at: row.get("revision_at"),
        published_at: row.get("published_at"),
        published_version: if admin {
            row.get("published_version")
        } else {
            None
        },
        knowledge_version: if admin {
            row.get("knowledge_version")
        } else {
            None
        },
        deleted_at: if admin { row.get("deleted_at") } else { None },
        notebook_ids: row.get("notebook_ids"),
    }
}

const COLUMNS: &str = "n.id, n.slug, n.created_at, n.published_at, n.published_version,
    n.knowledge_version, n.deleted_at, r.version AS revision, r.kind, r.title,
    r.content_md, r.content_html, r.toc_html, r.summary, r.tags, r.created_at AS revision_at";

// 笔记本本身的可见性也参与过滤，公开结果不泄露私密笔记本的 ID。
const BOOK_IDS: &str =
    "ARRAY(SELECT b.id FROM notebooks b JOIN notebook_notes bn ON bn.notebook_id=b.id
    WHERE bn.note_id=n.id AND ($1::int IS NOT NULL OR (b.is_public AND b.archived_at IS NULL))
      AND ($2::int[] IS NULL OR b.id=ANY($2)) ORDER BY b.id) AS notebook_ids";

pub async fn list(access: Access, filter: NoteFilter) -> Result<NotePage, AppError> {
    if filter.query.chars().count() > 200 {
        return Err(AppError::BadRequest("搜索内容不能超过 200 字".into()));
    }
    let client = get_conn().await.map_err(AppError::db_conn)?;
    let owner = access.owner();
    let books = access.books();
    let trash = matches!(access, Access::Owner(_)) && filter.trash;
    let (published, knowledge) = if matches!(access, Access::Owner(_)) {
        (filter.published, filter.knowledge)
    } else {
        (None, None)
    };
    let query = crate::utils::server::escape_like_pattern(filter.query.trim());
    let kind = filter.kind.map(|k| k.as_str());
    let predicate = format!("FROM notes n JOIN note_revisions r ON r.note_id=n.id AND r.version=n.{}
        WHERE ($1::int IS NULL OR n.owner_id=$1)
        AND ($2::int[] IS NULL OR EXISTS (SELECT 1 FROM notebook_notes bn WHERE bn.note_id=n.id AND bn.notebook_id=ANY($2)))
        AND (n.deleted_at IS NOT NULL)=$3
        AND ($4='' OR r.search_text ILIKE '%' || $4 || '%' ESCAPE '\\' OR EXISTS (SELECT 1 FROM unnest(r.tags) tag WHERE tag ILIKE '%' || $4 || '%' ESCAPE '\\'))
        AND ($5::text IS NULL OR r.kind=$5)
        AND ($6::int IS NULL OR EXISTS (SELECT 1 FROM notebook_notes bn JOIN notebooks b ON b.id=bn.notebook_id WHERE bn.note_id=n.id AND b.id=$6 AND ($1::int IS NOT NULL OR (b.is_public AND b.archived_at IS NULL))))
        AND ($7='' OR $7=ANY(r.tags))
        AND ($8::bool IS NULL OR (n.published_version IS NOT NULL)=$8)
        AND ($9::bool IS NULL OR (n.knowledge_version IS NOT NULL)=$9)", access.revision());
    let params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[
        &owner,
        &books,
        &trash,
        &query,
        &kind,
        &filter.notebook_id,
        &filter.tag,
        &published,
        &knowledge,
    ];
    let total = client
        .query_one(&format!("SELECT COUNT(*) {predicate}"), params)
        .await
        .map_err(AppError::query)?
        .get(0);
    let page = filter.page.clamp(1, 100_000);
    // 列表仅短随记携带 HTML；长文按需读取，避免每页传输完整知识库。
    let columns = COLUMNS.replace("r.content_md, r.content_html, r.toc_html", "''::text AS content_md, CASE WHEN r.kind='moment' AND length(r.content_md)<=2000 THEN r.content_html ELSE '' END AS content_html, ''::text AS toc_html");
    let sql = format!("SELECT {columns}, {BOOK_IDS} {predicate}
        ORDER BY CASE WHEN $6::int IS NOT NULL THEN (SELECT position FROM notebook_notes WHERE notebook_id=$6 AND note_id=n.id) END,
        CASE WHEN $4<>'' THEN word_similarity($4, r.search_text) END DESC,
        r.created_at DESC, n.id DESC LIMIT 20 OFFSET {}", (page-1)*20);
    let notes = client
        .query(&sql, params)
        .await
        .map_err(AppError::query)?
        .iter()
        .map(|r| map_note(r, &access))
        .collect();
    Ok(NotePage { notes, total })
}

pub async fn get(
    access: Access,
    id: Option<i32>,
    slug: Option<String>,
    version: Option<i32>,
) -> Result<Note, AppError> {
    let client = get_conn().await.map_err(AppError::db_conn)?;
    let owner = access.owner();
    let books = access.books();
    let requested = if matches!(access, Access::Owner(_)) {
        version
    } else {
        None
    };
    let sql = format!("SELECT {COLUMNS}, {BOOK_IDS} FROM notes n
        JOIN note_revisions r ON r.note_id=n.id AND r.version=COALESCE($5, n.{})
        WHERE ($1::int IS NULL OR n.owner_id=$1)
        AND ($2::int[] IS NULL OR EXISTS (SELECT 1 FROM notebook_notes bn WHERE bn.note_id=n.id AND bn.notebook_id=ANY($2)))
        AND (($3::int IS NOT NULL AND n.id=$3) OR ($4::text IS NOT NULL AND n.slug=$4))
        AND (n.deleted_at IS NULL OR $6)", access.revision());
    let row = client
        .query_opt(
            &sql,
            &[
                &owner,
                &books,
                &id,
                &slug,
                &requested,
                &matches!(access, Access::Owner(_)),
            ],
        )
        .await
        .map_err(AppError::query)?
        .ok_or(AppError::NotFound("笔记不存在或无权访问"))?;
    Ok(map_note(&row, &access))
}

fn validate(draft: &mut NoteDraft) -> Result<(), AppError> {
    draft.title = draft.title.trim().to_string();
    if draft.title.chars().count() > 200 || draft.content_md.len() > 200_000 {
        return Err(AppError::BadRequest(
            "标题最多 200 字，正文最多 200 KB".into(),
        ));
    }
    draft.tags = crate::api::posts::helpers::clean_tags(&draft.tags);
    if draft.tags.len() > 16 || draft.tags.iter().any(|t| t.chars().count() > 40) {
        return Err(AppError::BadRequest(
            "最多 16 个标签，每个标签最多 40 字".into(),
        ));
    }
    draft.notebook_ids.sort_unstable();
    draft.notebook_ids.dedup();
    if draft.notebook_ids.len() > 30 {
        return Err(AppError::BadRequest("最多收录到 30 个笔记本".into()));
    }
    if draft.id.is_some() && draft.expected_version.is_none() {
        return Err(AppError::BadRequest("更新笔记必须提供当前版本".into()));
    }
    Ok(())
}

pub async fn save(
    owner_id: i32,
    mut draft: NoteDraft,
    allowed_books: Option<&[i32]>,
) -> Result<Note, AppError> {
    validate(&mut draft)?;
    if let Some(allowed) = allowed_books {
        if draft.notebook_ids.is_empty() || draft.notebook_ids.iter().any(|b| !allowed.contains(b))
        {
            return Err(AppError::Forbidden("笔记必须位于令牌授权的笔记本中"));
        }
    }
    let fields =
        crate::api::posts::helpers::render_post_fields(&draft.content_md, "draft", None).await?;
    let mut client = get_conn().await.map_err(AppError::db_conn)?;
    let tx = client.transaction().await.map_err(AppError::tx)?;
    let count: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM notebooks WHERE owner_id=$1 AND id=ANY($2)",
            &[&owner_id, &draft.notebook_ids],
        )
        .await
        .map_err(AppError::query)?
        .get(0);
    if count != draft.notebook_ids.len() as i64 {
        return Err(AppError::Forbidden("笔记本不存在或无权访问"));
    }
    let (id, version) = if let Some(id) = draft.id {
        let row = tx.query_opt("SELECT version FROM notes WHERE id=$1 AND owner_id=$2 AND deleted_at IS NULL FOR UPDATE", &[&id, &owner_id]).await.map_err(AppError::query)?.ok_or(AppError::NotFound("笔记不存在或已移入回收站"))?;
        let current: i32 = row.get(0);
        if Some(current) != draft.expected_version {
            return Err(AppError::BadRequest(
                "笔记已在别处修改，请重新加载后再保存；当前编辑内容仍保留在页面".into(),
            ));
        }
        if let Some(allowed) = allowed_books {
            let found: bool = tx.query_one("SELECT EXISTS(SELECT 1 FROM notebook_notes WHERE note_id=$1 AND notebook_id=ANY($2))", &[&id, &allowed]).await.map_err(AppError::query)?.get(0);
            if !found {
                return Err(AppError::Forbidden("笔记不在令牌授权范围内"));
            }
        }
        (id, current + 1)
    } else {
        let slug = uuid::Uuid::new_v4().simple().to_string();
        let id = tx
            .query_one(
                "INSERT INTO notes(owner_id, slug) VALUES ($1,$2) RETURNING id",
                &[&owner_id, &slug],
            )
            .await
            .map_err(AppError::tx)?
            .get(0);
        (id, 1)
    };
    tx.execute("INSERT INTO note_revisions(note_id, version, kind, title, content_md, content_html, toc_html, summary, tags) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        &[&id, &version, &draft.kind.as_str(), &draft.title, &draft.content_md, &fields.content_html, &fields.toc_html.unwrap_or_default(), &fields.auto_summary, &draft.tags]).await.map_err(AppError::tx)?;
    tx.execute(
        "UPDATE notes SET version=$2, updated_at=NOW() WHERE id=$1",
        &[&id, &version],
    )
    .await
    .map_err(AppError::tx)?;
    let allowed = allowed_books.map(|b| b.to_vec());
    tx.execute("DELETE FROM notebook_notes WHERE note_id=$1 AND NOT(notebook_id=ANY($2)) AND ($3::int[] IS NULL OR notebook_id=ANY($3))", &[&id, &draft.notebook_ids, &allowed]).await.map_err(AppError::tx)?;
    for book in &draft.notebook_ids {
        tx.execute("INSERT INTO notebook_notes(notebook_id, note_id, position) SELECT $1,$2,COALESCE(MAX(position),-1)+1 FROM notebook_notes WHERE notebook_id=$1 ON CONFLICT DO NOTHING", &[book, &id]).await.map_err(AppError::tx)?;
    }
    super::attachments::sync_refs(&tx, owner_id, id, version, &fields.content_html).await?;
    tx.commit().await.map_err(AppError::tx)?;
    // 笔记本归属可影响公开目录，即使正文仍是工作稿也应失效目录缓存。
    invalidate();
    get(Access::Owner(owner_id), Some(id), None, None).await
}

pub async fn act(
    owner_id: i32,
    id: i32,
    expected: i32,
    action: NoteAction,
) -> Result<Note, AppError> {
    let mut client = get_conn().await.map_err(AppError::db_conn)?;
    let tx = client.transaction().await.map_err(AppError::tx)?;
    let row = tx.query_opt("SELECT n.version, n.deleted_at, r.kind, r.title, r.content_md FROM notes n JOIN note_revisions r ON r.note_id=n.id AND r.version=n.version WHERE n.id=$1 AND n.owner_id=$2 FOR UPDATE OF n", &[&id, &owner_id]).await.map_err(AppError::query)?.ok_or(AppError::NotFound("笔记不存在"))?;
    if row.get::<_, i32>("version") != expected {
        return Err(AppError::BadRequest("笔记版本已变化，请重新加载".into()));
    }
    if row
        .get::<_, Option<chrono::DateTime<chrono::Utc>>>("deleted_at")
        .is_some()
        && action != NoteAction::Restore
    {
        return Err(AppError::BadRequest("请先从回收站恢复笔记".into()));
    }
    if matches!(action, NoteAction::Publish | NoteAction::IncludeKnowledge)
        && (row.get::<_, String>("content_md").trim().is_empty()
            || (row.get::<_, String>("kind") == "topic"
                && row.get::<_, String>("title").trim().is_empty()))
    {
        return Err(AppError::BadRequest(
            "请先填写正文；主题笔记还需要标题".into(),
        ));
    }
    let update = match action {
        NoteAction::Publish => {
            "published_version=version, published_at=COALESCE(published_at,NOW())"
        }
        NoteAction::Unpublish => "published_version=NULL, published_at=NULL",
        NoteAction::IncludeKnowledge => "knowledge_version=version",
        NoteAction::ExcludeKnowledge => "knowledge_version=NULL",
        NoteAction::Trash => {
            "deleted_at=NOW(), published_version=NULL, published_at=NULL, knowledge_version=NULL"
        }
        NoteAction::Restore => "deleted_at=NULL",
    };
    tx.execute(
        &format!("UPDATE notes SET {update}, updated_at=NOW() WHERE id=$1"),
        &[&id],
    )
    .await
    .map_err(AppError::tx)?;
    tx.commit().await.map_err(AppError::tx)?;
    invalidate();
    get(Access::Owner(owner_id), Some(id), None, None).await
}

pub async fn history(owner_id: i32, id: i32) -> Result<Vec<NoteRevision>, AppError> {
    let client = get_conn().await.map_err(AppError::db_conn)?;
    let rows = client.query("SELECT r.version,r.title,r.created_at FROM note_revisions r JOIN notes n ON n.id=r.note_id WHERE n.id=$1 AND n.owner_id=$2 ORDER BY r.version DESC LIMIT 100", &[&id, &owner_id]).await.map_err(AppError::query)?;
    Ok(rows
        .iter()
        .map(|r| NoteRevision {
            version: r.get(0),
            title: r.get(1),
            created_at: r.get(2),
        })
        .collect())
}

pub async fn notebooks(access: Access) -> Result<Vec<Notebook>, AppError> {
    let client = get_conn().await.map_err(AppError::db_conn)?;
    let owner = access.owner();
    let books = access.books();
    let include_archived = matches!(access, Access::Owner(_));
    let sql = format!("SELECT b.*, (SELECT COUNT(*) FROM notebook_notes bn JOIN notes n ON n.id=bn.note_id WHERE bn.notebook_id=b.id AND n.deleted_at IS NULL AND n.{} IS NOT NULL) AS note_count
        FROM notebooks b WHERE ($1::int IS NULL AND b.is_public OR b.owner_id=$1)
        AND ($2::int[] IS NULL OR b.id=ANY($2)) AND ($3 OR b.archived_at IS NULL)
        ORDER BY b.updated_at DESC,b.id DESC", access.revision());
    let rows = client
        .query(&sql, &[&owner, &books, &include_archived])
        .await
        .map_err(AppError::query)?;
    Ok(rows
        .iter()
        .map(|r| Notebook {
            id: r.get("id"),
            title: r.get("title"),
            description: r.get("description"),
            is_public: r.get("is_public"),
            archived_at: r.get("archived_at"),
            note_count: r.get("note_count"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

pub async fn save_notebook(owner_id: i32, input: NotebookInput) -> Result<(), AppError> {
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 100 || input.description.chars().count() > 1000 {
        return Err(AppError::BadRequest(
            "笔记本标题为 1–100 字，介绍最多 1000 字".into(),
        ));
    }
    let client = get_conn().await.map_err(AppError::db_conn)?;
    if let Some(id) = input.id {
        let changed = client.execute("UPDATE notebooks SET title=$3,description=$4,is_public=$5,updated_at=NOW() WHERE id=$1 AND owner_id=$2 AND (archived_at IS NULL OR NOT $5)", &[&id,&owner_id,&title,&input.description,&input.is_public]).await.map_err(AppError::query)?;
        if changed == 0 {
            return Err(AppError::BadRequest(
                "笔记本不存在，或已归档；请先恢复再公开".into(),
            ));
        }
    } else {
        client
            .execute(
                "INSERT INTO notebooks(owner_id,title,description,is_public) VALUES ($1,$2,$3,$4)",
                &[&owner_id, &title, &input.description, &input.is_public],
            )
            .await
            .map_err(AppError::query)?;
    }
    invalidate();
    Ok(())
}

pub async fn archive_notebook(owner_id: i32, id: i32, archived: bool) -> Result<(), AppError> {
    let client = get_conn().await.map_err(AppError::db_conn)?;
    let changed = client.execute(
        "UPDATE notebooks SET archived_at=CASE WHEN $3 THEN COALESCE(archived_at,NOW()) ELSE NULL END,
         is_public=CASE WHEN $3 THEN FALSE ELSE is_public END, updated_at=NOW() WHERE id=$1 AND owner_id=$2",
        &[&id,&owner_id,&archived],
    ).await.map_err(AppError::query)?;
    if changed == 0 {
        return Err(AppError::NotFound("笔记本不存在"));
    }
    invalidate();
    Ok(())
}

pub async fn reorder(owner_id: i32, book: i32, ids: Vec<i32>) -> Result<(), AppError> {
    let mut client = get_conn().await.map_err(AppError::db_conn)?;
    let tx = client.transaction().await.map_err(AppError::tx)?;
    tx.query_opt(
        "SELECT id FROM notebooks WHERE id=$1 AND owner_id=$2 FOR UPDATE",
        &[&book, &owner_id],
    )
    .await
    .map_err(AppError::query)?
    .ok_or(AppError::NotFound("笔记本不存在"))?;
    let existing: Vec<i32> = tx
        .query(
            "SELECT note_id FROM notebook_notes WHERE notebook_id=$1 ORDER BY position,note_id",
            &[&book],
        )
        .await
        .map_err(AppError::query)?
        .iter()
        .map(|r| r.get(0))
        .collect();
    let mut a = existing;
    a.sort_unstable();
    let mut b = ids.clone();
    b.sort_unstable();
    if a != b {
        return Err(AppError::BadRequest(
            "笔记本内容已变化，请刷新后排序".into(),
        ));
    }
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE notebook_notes SET position=$3 WHERE notebook_id=$1 AND note_id=$2",
            &[&book, id, &(position as i32)],
        )
        .await
        .map_err(AppError::tx)?;
    }
    tx.commit().await.map_err(AppError::tx)?;
    invalidate();
    Ok(())
}

fn invalidate() {
    crate::ssr_cache::bump_global_generation();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires disposable DATABASE_URL database ygg_notes_test"]
    fn notes_database_workflow() {
        crate::db::TEST_DATABASE_RUNTIME.block_on(async {
            let _guard = crate::db::TEST_DATABASE_LOCK.lock().await;
            let mut client = get_conn().await.unwrap();
            let database: String = client
                .query_one("SELECT current_database()", &[])
                .await
                .unwrap()
                .get(0);
            assert_eq!(
                database, "ygg_notes_test",
                "requires isolated test database"
            );
            crate::db::migrate::run_on_conn(&mut client).await.unwrap();
            client
                .batch_execute(
                    "TRUNCATE users CASCADE;
                INSERT INTO users(id,username,email,password_hash,role) VALUES
                (1,'note-owner','note@test.invalid','unused','admin'),
                (2,'note-other','other@test.invalid','unused','blocked');",
                )
                .await
                .unwrap();
            save_notebook(
                1,
                NotebookInput {
                    title: "公开主题".into(),
                    is_public: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            save_notebook(
                1,
                NotebookInput {
                    title: "私密主题".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            let books = notebooks(Access::Owner(1)).await.unwrap();
            let public = books.iter().find(|b| b.is_public).unwrap().id;
            let private = books.iter().find(|b| !b.is_public).unwrap().id;
            let note = save(
                1,
                NoteDraft {
                    title: "所有权".into(),
                    kind: NoteKind::Topic,
                    content_md: "第一版 Rust 100%_".into(),
                    notebook_ids: vec![public, private],
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
            assert!(get(Access::Public, Some(note.id), None, None)
                .await
                .is_err());
            assert!(get(Access::Owner(2), Some(note.id), None, None)
                .await
                .is_err());
            assert_eq!(
                list(Access::Public, NoteFilter::default())
                    .await
                    .unwrap()
                    .total,
                0
            );
            assert_eq!(notebooks(Access::Public).await.unwrap()[0].note_count, 0);
            act(1, note.id, 1, NoteAction::Publish).await.unwrap();
            act(1, note.id, 1, NoteAction::IncludeKnowledge)
                .await
                .unwrap();
            let mut draft = note.draft();
            draft.content_md = "第二版，尚未发布".into();
            let updated = save(1, draft.clone(), None).await.unwrap();
            assert_eq!(updated.version, 2);
            assert!(
                save(1, draft, None).await.is_err(),
                "stale writes must fail"
            );
            let published = get(Access::Public, Some(note.id), None, Some(2))
                .await
                .unwrap();
            assert_eq!(
                published.version, 1,
                "public cannot select a draft revision"
            );
            assert_eq!(published.notebook_ids, vec![public]);
            assert!(published.knowledge_version.is_none());
            let knowledge = Access::Knowledge {
                owner_id: 1,
                notebook_ids: Some(vec![private]),
            };
            assert_eq!(
                get(knowledge.clone(), Some(note.id), None, None)
                    .await
                    .unwrap()
                    .version,
                1
            );
            assert!(get(
                Access::Knowledge {
                    owner_id: 1,
                    notebook_ids: Some(vec![])
                },
                Some(note.id),
                None,
                None
            )
            .await
            .is_err());
            assert_eq!(
                list(
                    Access::Public,
                    NoteFilter {
                        query: "100%_".into(),
                        ..Default::default()
                    }
                )
                .await
                .unwrap()
                .total,
                1
            );
            assert_eq!(
                list(
                    Access::Public,
                    NoteFilter {
                        query: "第二版".into(),
                        ..Default::default()
                    }
                )
                .await
                .unwrap()
                .total,
                0
            );
            assert_eq!(
                list(
                    Access::Public,
                    NoteFilter {
                        notebook_id: Some(private),
                        ..Default::default()
                    }
                )
                .await
                .unwrap()
                .total,
                0
            );
            act(1, note.id, 2, NoteAction::Unpublish).await.unwrap();
            assert!(get(Access::Public, Some(note.id), None, None)
                .await
                .is_err());
            assert!(get(knowledge.clone(), Some(note.id), None, None)
                .await
                .is_ok());
            act(1, note.id, 2, NoteAction::Trash).await.unwrap();
            assert!(get(knowledge, Some(note.id), None, None).await.is_err());
            assert_eq!(
                list(
                    Access::Owner(1),
                    NoteFilter {
                        trash: true,
                        ..Default::default()
                    }
                )
                .await
                .unwrap()
                .total,
                1
            );
            act(1, note.id, 2, NoteAction::Restore).await.unwrap();
            assert!(
                get(Access::Public, Some(note.id), None, None)
                    .await
                    .is_err(),
                "restore never republishes"
            );
            assert_eq!(history(1, note.id).await.unwrap().len(), 2);
            let mut old = get(Access::Owner(1), Some(note.id), None, Some(1))
                .await
                .unwrap()
                .draft();
            old.expected_version = Some(2);
            assert_eq!(
                save(1, old, None).await.unwrap().version,
                3,
                "restoration creates a new version"
            );
            assert!(act(2, note.id, 3, NoteAction::Publish).await.is_err());
            reorder(1, public, vec![note.id]).await.unwrap();
            assert!(reorder(2, public, vec![note.id]).await.is_err());

            // 私密图片不能靠猜 URL 读取；发布和撤回立即改变访问权。
            let image_id=uuid::Uuid::new_v4();
            client.execute("INSERT INTO note_attachments(id,owner_id,mime,data) VALUES($1,1,'image/png',$2)", &[&image_id, &vec![1u8,2,3]]).await.unwrap();
            let media_status=|| super::super::attachments::read(axum::http::HeaderMap::new(),axum::extract::Path(image_id.to_string()));
            assert_eq!(media_status().await.status(),axum::http::StatusCode::NOT_FOUND);
            let reference_only=save(1,NoteDraft {content_md:format!("`/note-media/{image_id}`\n\n![外站](https://example.invalid/note-media/{image_id})"),..Default::default()},None).await.unwrap();
            act(1,reference_only.id,1,NoteAction::Publish).await.unwrap();
            assert_eq!(media_status().await.status(),axum::http::StatusCode::NOT_FOUND,"code examples and remote URLs cannot publish local media");
            let draft=NoteDraft {content_md:format!("![私密图片](/note-media/{image_id})"),..Default::default()};
            assert!(save(2,draft.clone(),None).await.is_err(),"cannot reuse someone else's private attachment");
            let image_note=save(1,draft,None).await.unwrap();
            act(1,image_note.id,1,NoteAction::IncludeKnowledge).await.unwrap();
            assert_eq!(media_status().await.status(),axum::http::StatusCode::NOT_FOUND,"knowledge enrollment is not public access");
            act(1,image_note.id,1,NoteAction::Publish).await.unwrap();
            let response=media_status().await;
            assert_eq!(response.status(),axum::http::StatusCode::OK);
            assert_eq!(response.headers()["cache-control"],"private, no-store");
            let mut draft=image_note.draft();draft.content_md="草稿移除了图片，公开版仍保留".into();
            save(1,draft,None).await.unwrap();
            assert_eq!(media_status().await.status(),axum::http::StatusCode::OK);
            act(1,image_note.id,2,NoteAction::Unpublish).await.unwrap();
            assert_eq!(media_status().await.status(),axum::http::StatusCode::NOT_FOUND);

            // 公开素材即使只被笔记历史版引用，也不能变成可清理孤儿。
            let asset_id=uuid::Uuid::new_v4();
            let path=format!("2026/09/20/{asset_id}.webp");
            client.execute("INSERT INTO assets(id,path,filename,mime,size_bytes,width,height) VALUES($1,$2,$3,'image/webp',3,1,1)",&[&asset_id,&path,&asset_id.to_string()]).await.unwrap();
            let asset_note=save(1,NoteDraft {content_md:format!("![素材](/uploads/{path})"),..Default::default()},None).await.unwrap();
            let mut draft=asset_note.draft();draft.content_md="新草稿不再使用图片".into();
            save(1,draft,None).await.unwrap();
            act(1,asset_note.id,2,NoteAction::Trash).await.unwrap();
            let assets=crate::api::assets::list::list_assets_impl(crate::models::asset::AssetFilter::Used,asset_id.to_string(),crate::models::asset::AssetSort::CreatedDesc,1).await.unwrap();
            assert_eq!(assets.total,1);
            assert_eq!(assets.assets[0].ref_count,1);
            assert!(matches!(&assets.assets[0].refs[0],crate::models::asset::AssetRef::Note {note_id,..} if *note_id==asset_note.id));
            assert!(!crate::api::assets::delete::delete_asset_impl(asset_id.to_string()).await.unwrap().success);

            let filtered=save(1,NoteDraft {title:"状态筛选验收".into(),content_md:"正文".into(),notebook_ids:vec![public],..Default::default()},None).await.unwrap();
            let count=|scope,published,knowledge|list(scope,NoteFilter {query:"状态筛选验收".into(),published,knowledge,..Default::default()});
            assert_eq!(count(Access::Owner(1),Some(false),Some(false)).await.unwrap().total,1);
            assert_eq!(count(Access::Owner(1),Some(true),None).await.unwrap().total,0);
            act(1,filtered.id,1,NoteAction::Publish).await.unwrap();
            assert_eq!(count(Access::Owner(1),Some(true),Some(false)).await.unwrap().total,1);
            act(1,filtered.id,1,NoteAction::IncludeKnowledge).await.unwrap();
            assert_eq!(count(Access::Owner(1),Some(true),Some(true)).await.unwrap().total,1);
            assert_eq!(count(Access::Owner(1),None,Some(false)).await.unwrap().total,0);
            assert_eq!(count(Access::Public,Some(false),Some(false)).await.unwrap().total,1,"public endpoint ignores private state probes");
            assert!(archive_notebook(2,public,true).await.is_err());
            archive_notebook(1,public,true).await.unwrap();
            assert!(notebooks(Access::Public).await.unwrap().iter().all(|b|b.id!=public));
            let archived=notebooks(Access::Owner(1)).await.unwrap().into_iter().find(|b|b.id==public).unwrap();
            assert!(archived.archived_at.is_some());assert!(!archived.is_public);
            assert!(get(Access::Public,Some(filtered.id),None,None).await.is_ok(),"archiving directory does not retract published notes");
            let knowledge=Access::Knowledge {owner_id:1,notebook_ids:Some(vec![public])};
            assert!(get(knowledge.clone(),Some(filtered.id),None,None).await.is_ok(),"archiving does not silently revoke grants");
            assert!(notebooks(knowledge).await.unwrap().is_empty());
            assert!(save_notebook(1,NotebookInput {id:Some(public),title:"归档目录".into(),is_public:true,..Default::default()}).await.is_err());
            archive_notebook(1,public,false).await.unwrap();
            let restored=notebooks(Access::Owner(1)).await.unwrap().into_iter().find(|b|b.id==public).unwrap();
            assert!(restored.archived_at.is_none());assert!(!restored.is_public,"restore requires explicit re-publication");
            assert_eq!(get(Access::Owner(1),Some(filtered.id),None,None).await.unwrap().notebook_ids,vec![public]);
        });
    }
}
