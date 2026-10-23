//! 文章写入业务：已鉴权入口传入作者，统一执行校验、事务和提交后缓存失效。
//!
//! 完整更新与部分更新通过内部命令区分；Cookie 与 MCP 的协议和错误映射由入口负责。

use chrono::{DateTime, Utc};
use tokio_postgres::types::ToSql;

use super::{helpers, validation};
use crate::api::error::AppError;
use crate::db::pool::get_conn;
use crate::models::post::PostStatus;

#[derive(Debug)]
pub(crate) enum WriteError {
    Invalid(String),
    Conflict(DateTime<Utc>),
    Failed(AppError),
}

impl From<AppError> for WriteError {
    fn from(value: AppError) -> Self {
        Self::Failed(value)
    }
}

/// 可选字段的写入意图；清空与不修改在业务层中有不同含义。
#[derive(Debug, Default)]
pub(crate) enum FieldChange<T> {
    #[default]
    Keep,
    Set(T),
    Clear,
}

#[derive(Debug)]
pub(crate) struct PostDraft {
    pub title: String,
    pub slug: Option<String>,
    pub summary: Option<String>,
    pub content_md: String,
    pub status: String,
    pub tags: Vec<String>,
    pub cover_image: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct PostPatch {
    pub title: Option<String>,
    pub slug: Option<String>,
    pub summary: Option<String>,
    pub content_md: Option<String>,
    pub status: Option<String>,
    pub tags: Option<Vec<String>>,
    pub cover_image: FieldChange<String>,
    pub published_at: FieldChange<DateTime<Utc>>,
    pub expected_updated_at: Option<DateTime<Utc>>,
}

pub(crate) enum UpdatePost {
    Replace(PostDraft),
    Patch(PostPatch),
}

#[derive(Debug)]
pub(crate) struct WriteOutcome {
    pub post_id: i32,
    pub slug: String,
    pub updated_at: DateTime<Utc>,
}

fn normalize_cover(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

pub(crate) async fn create(
    author_id: i32,
    draft: PostDraft,
    explicit_published_at: Option<DateTime<Utc>>,
) -> Result<WriteOutcome, WriteError> {
    validation::title(&draft.title)?;
    validation::content(&draft.content_md)?;
    let base_slug = validation::base_slug(&draft.title, draft.slug.as_deref())?;
    if let Some(s) = &draft.summary {
        validation::summary(s)?;
    }
    if let Some(c) = &draft.cover_image {
        validation::cover(c)?;
    }
    validation::tags(&draft.tags)?;
    let fields = helpers::render_post_fields(
        &draft.content_md,
        &draft.status,
        draft.cover_image.as_deref(),
    )
    .await?;
    let summary = draft
        .summary
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or(fields.auto_summary);
    let now = Utc::now();
    let published_at =
        (fields.status == PostStatus::Published).then(|| explicit_published_at.unwrap_or(now));
    let created_at = explicit_published_at.unwrap_or(now);
    let mut client = get_conn().await.map_err(AppError::db_conn)?;
    let tx = client.transaction().await.map_err(AppError::tx)?;
    let slug = crate::api::slug::ensure_unique_slug(&tx, &base_slug, None).await?;
    let row = tx.query_one(
        "INSERT INTO posts (author_id, title, slug, summary, content_md, content_html, toc_html, status, published_at, cover_image, word_count, reading_time, created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING id, updated_at",
        &[&author_id, &draft.title.trim(), &slug, &summary, &draft.content_md,
          &fields.content_html, &fields.toc_html, &fields.status.as_str(), &published_at,
          &fields.cover_image, &fields.word_count, &fields.reading_time, &created_at],
    ).await.map_err(AppError::tx)?;
    let post_id = row.get("id");
    let updated_at = row.get("updated_at");
    let tags = helpers::clean_tags(&draft.tags);
    helpers::sync_tags(&tx, post_id, &tags).await?;
    helpers::sync_asset_refs(
        &tx,
        post_id,
        &fields.content_html,
        fields.cover_image.as_deref(),
    )
    .await?;
    tx.commit().await.map_err(AppError::tx)?;
    crate::cache::invalidate_for_post_write(std::slice::from_ref(&slug), &tags).await;
    Ok(WriteOutcome {
        post_id,
        slug,
        updated_at,
    })
}

/// 完整更新：校验草稿并在事务外渲染正文，返回补丁与渲染结果。
///
/// 渲染直接使用草稿里已校验的正文与状态，避免事后从 `Option` 字段取回。
async fn replacement(draft: PostDraft) -> Result<(PostPatch, helpers::RenderedFields), WriteError> {
    validation::title(&draft.title)?;
    validation::content(&draft.content_md)?;
    let slug = validation::base_slug(&draft.title, draft.slug.as_deref())?;
    // 与 PATCH 同规则（validate_patch）：由标题生成的 slug 同样须通过格式校验。
    let trimmed_slug = slug.trim();
    if !trimmed_slug.is_empty() {
        validation::slug(trimmed_slug)?;
    }
    if let Some(s) = &draft.summary {
        validation::summary(s)?;
    }
    if let Some(c) = &draft.cover_image {
        validation::cover(c)?;
    }
    validation::tags(&draft.tags)?;
    let cover = normalize_cover(draft.cover_image.as_deref());
    let fields =
        helpers::render_post_fields(&draft.content_md, &draft.status, cover.as_deref()).await?;
    let patch = PostPatch {
        title: Some(draft.title),
        slug: Some(slug),
        summary: Some(draft.summary.unwrap_or_default()),
        content_md: Some(draft.content_md),
        status: Some(draft.status),
        tags: Some(draft.tags),
        cover_image: cover.map(FieldChange::Set).unwrap_or(FieldChange::Clear),
        ..Default::default()
    };
    Ok((patch, fields))
}

fn validate_patch(p: &PostPatch) -> Result<(), WriteError> {
    if p.title.is_none()
        && p.slug.is_none()
        && p.summary.is_none()
        && p.content_md.is_none()
        && p.status.is_none()
        && p.tags.is_none()
        && matches!(p.cover_image, FieldChange::Keep)
        && matches!(p.published_at, FieldChange::Keep)
    {
        return Err(WriteError::Invalid("至少提供一个可更新字段".into()));
    }
    if let Some(title) = &p.title {
        validation::title(title)?;
    }
    if let Some(content) = &p.content_md {
        validation::content(content)?;
    }
    if let Some(slug) = p.slug.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        validation::slug(slug)?;
    }
    if let Some(s) = &p.summary {
        validation::summary(s)?;
    }
    if let FieldChange::Set(c) = &p.cover_image {
        validation::cover(c)?;
    }
    if let Some(t) = &p.tags {
        validation::tags(t)?;
    }
    Ok(())
}

pub(crate) async fn update(
    author_id: i32,
    post_id: i32,
    command: UpdatePost,
) -> Result<WriteOutcome, WriteError> {
    let (p, prepared, align_created_at) = match command {
        // 完整更新的正文与元信息已齐全，沿用浏览器接口在事务外渲染的行为。
        UpdatePost::Replace(draft) => {
            let (p, fields) = replacement(draft).await?;
            (p, Some(fields), false)
        }
        // PATCH 的缺省字段需从锁定行读取，再决定最终状态与封面。
        UpdatePost::Patch(patch) => {
            validate_patch(&patch)?;
            (patch, None, true)
        }
    };
    let mut client = get_conn().await.map_err(AppError::db_conn)?;
    let tx = client.transaction().await.map_err(AppError::tx)?;
    let old = tx
        .query_opt(
            "SELECT slug, status, published_at, cover_image, updated_at, content_html FROM posts
         WHERE id=$1 AND author_id=$2 AND deleted_at IS NULL FOR UPDATE",
            &[&post_id, &author_id],
        )
        .await
        .map_err(AppError::query)?
        .ok_or_else(|| WriteError::Invalid("文章不存在或无权限".into()))?;
    let current_updated_at: DateTime<Utc> = old.get("updated_at");
    if p.expected_updated_at
        .is_some_and(|expected| expected != current_updated_at)
    {
        return Err(WriteError::Conflict(current_updated_at));
    }
    let old_slug: String = old.get("slug");
    let old_status: String = old.get("status");
    let old_published_at: Option<DateTime<Utc>> = old.get("published_at");
    let old_cover: Option<String> = old.get("cover_image");
    let old_tags = helpers::fetch_post_tags(&tx, post_id).await?;
    let cover_changed = !matches!(p.cover_image, FieldChange::Keep);
    let effective_cover = match &p.cover_image {
        FieldChange::Keep => old_cover,
        FieldChange::Set(value) => normalize_cover(Some(value)),
        FieldChange::Clear => None,
    };
    let rendered = match (&p.content_md, prepared) {
        (_, Some(fields)) => Some(fields),
        (Some(md), None) => Some(
            helpers::render_post_fields(
                md,
                p.status.as_deref().unwrap_or(&old_status),
                effective_cover.as_deref(),
            )
            .await?,
        ),
        (None, None) => None,
    };
    // 空摘要只在同时提供正文时重新提取；单独传空摘要沿用已有 PATCH 行为。
    let summary = match (&p.summary, &rendered) {
        (Some(s), fields) if s.trim().is_empty() => fields.as_ref().map(|f| f.auto_summary.clone()),
        (Some(s), _) => Some(s.trim().to_owned()),
        (None, Some(fields)) => Some(fields.auto_summary.clone()),
        (None, None) => None,
    };
    let slug = match p.slug.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(base) => crate::api::slug::ensure_unique_slug(&tx, base, Some(post_id)).await?,
        None => old_slug.clone(),
    };
    let status = p
        .status
        .as_deref()
        .map(|s| PostStatus::from_str(s).unwrap_or(PostStatus::Draft));
    let published_at = match p.published_at {
        FieldChange::Set(value) => Some(Some(value)),
        FieldChange::Clear => Some(None),
        FieldChange::Keep => status.as_ref().map(|s| {
            if *s == PostStatus::Published && old_status != "published" {
                Some(Utc::now())
            } else {
                old_published_at
            }
        }),
    };
    let mut sets = Vec::new();
    let mut params: Vec<Box<dyn ToSql + Sync + Send>> = Vec::new();
    macro_rules! push {
        ($col:expr, $value:expr) => {{
            params.push(Box::new($value));
            sets.push(format!("{} = ${}", $col, params.len()));
        }};
    }
    if let Some(title) = p.title {
        push!("title", title.trim().to_owned());
    }
    if let Some(md) = p.content_md {
        push!("content_md", md);
    }
    if let Some(f) = &rendered {
        push!("content_html", f.content_html.clone());
        push!("toc_html", f.toc_html.clone());
        push!("word_count", f.word_count);
        push!("reading_time", f.reading_time);
    }
    if let Some(summary) = summary {
        push!("summary", summary);
    }
    if p.slug.is_some() {
        push!("slug", slug.clone());
    }
    if let Some(status) = status {
        push!("status", status.as_str().to_owned());
    }
    if let Some(date) = published_at {
        push!("published_at", date);
        if align_created_at {
            if let Some(date) = date {
                push!("created_at", date);
            }
        }
    }
    if cover_changed {
        push!("cover_image", effective_cover.clone());
    }
    sets.push(
        "updated_at = GREATEST(clock_timestamp(), updated_at + INTERVAL '1 microsecond')".into(),
    );
    params.push(Box::new(post_id));
    let refs: Vec<&(dyn ToSql + Sync)> = params
        .iter()
        .map(|p| p.as_ref() as &(dyn ToSql + Sync))
        .collect();
    let row = tx
        .query_one(
            &format!(
                "UPDATE posts SET {} WHERE id=${} RETURNING updated_at",
                sets.join(", "),
                params.len()
            ),
            &refs,
        )
        .await
        .map_err(AppError::tx)?;
    let updated_at = row.get("updated_at");
    let tags = if let Some(tags) = p.tags {
        let tags = helpers::clean_tags(&tags);
        tx.execute("DELETE FROM post_tags WHERE post_id=$1", &[&post_id])
            .await
            .map_err(AppError::tx)?;
        helpers::sync_tags(&tx, post_id, &tags).await?;
        tags
    } else {
        old_tags.clone()
    };
    if rendered.is_some() || cover_changed {
        let html = rendered.map(|f| f.content_html).unwrap_or_else(|| {
            old.get::<_, Option<String>>("content_html")
                .unwrap_or_default()
        });
        helpers::sync_asset_refs(&tx, post_id, &html, effective_cover.as_deref()).await?;
    }
    tx.commit().await.map_err(AppError::tx)?;
    let all_tags: Vec<String> = old_tags
        .into_iter()
        .chain(tags)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    crate::cache::invalidate_for_post_write(&[old_slug, slug.clone()], &all_tags).await;
    Ok(WriteOutcome {
        post_id,
        slug,
        updated_at,
    })
}

pub(crate) enum PostAction {
    Publish,
    /// 转回草稿：保留首次发布时间，重新发布时沿用。
    Unpublish,
    Trash,
    Delete,
    Purge,
}

/// 管理员可传 None 操作全站文章；令牌入口传 Some(author_id) 限制到原作者。
pub(crate) async fn act(
    post_id: i32,
    author_id: Option<i32>,
    action: PostAction,
) -> Result<WriteOutcome, WriteError> {
    let mut client = get_conn().await.map_err(AppError::db_conn)?;
    let tx = client.transaction().await.map_err(AppError::tx)?;
    let condition = match action {
        PostAction::Publish | PostAction::Unpublish | PostAction::Trash => "AND deleted_at IS NULL",
        PostAction::Purge => "AND deleted_at IS NOT NULL",
        PostAction::Delete => "",
    };
    let row = tx.query_opt(&format!("SELECT slug, updated_at FROM posts WHERE id=$1 AND ($2::int IS NULL OR author_id=$2) {condition} FOR UPDATE"), &[&post_id, &author_id])
        .await.map_err(AppError::query)?.ok_or_else(|| WriteError::Invalid("文章不存在或无权限".into()))?;
    let slug = row.get("slug");
    let tags = helpers::fetch_post_tags(&tx, post_id).await?;
    let sql = match action {
        PostAction::Publish => "UPDATE posts SET status='published', published_at=COALESCE(published_at, NOW()), updated_at=GREATEST(clock_timestamp(), updated_at + INTERVAL '1 microsecond') WHERE id=$1 RETURNING updated_at",
        PostAction::Unpublish => "UPDATE posts SET status='draft', updated_at=GREATEST(clock_timestamp(), updated_at + INTERVAL '1 microsecond') WHERE id=$1 RETURNING updated_at",
        PostAction::Trash => "UPDATE posts SET deleted_at=NOW(), updated_at=GREATEST(clock_timestamp(), updated_at + INTERVAL '1 microsecond') WHERE id=$1 RETURNING updated_at",
        PostAction::Delete | PostAction::Purge => "DELETE FROM posts WHERE id=$1 RETURNING updated_at",
    };
    let updated = tx.query_one(sql, &[&post_id]).await.map_err(AppError::tx)?;
    let updated_at = updated.get("updated_at");
    tx.commit().await.map_err(AppError::tx)?;
    crate::cache::invalidate_for_post_write(std::slice::from_ref(&slug), &tags).await;
    Ok(WriteOutcome {
        post_id,
        slug,
        updated_at,
    })
}
