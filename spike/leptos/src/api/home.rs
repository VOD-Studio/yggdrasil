//! 首页的数据接口：文章分页、标签、站点配置、最新一篇目录预览与最近笔记。
//!
//! SQL 移植自主 crate `api::posts::{list, tags, outline}`、`api::settings::site`
//! 与 `api::notes::store::list`（公开访问）。原型不接 moka 缓存，每次直接查库。
use leptos::prelude::*;

use crate::error::AppError;
use crate::models::home::{PostListResponse, PostOutline, SiteSettings, TagListResponse};
use crate::models::note::{NoteFilter, NotePage};

/// 单页允许的最大文章数。
///
/// 公开接口无需认证，不设上限时可借超大 `per_page` 迫使数据库扫描并实例化超大 Vec。
#[cfg(feature = "ssr")]
const MAX_PER_PAGE: i32 = 50;

/// 允许的最大页码，避免无意义的超大 `OFFSET` 扫描。
#[cfg(feature = "ssr")]
const MAX_PAGE: i32 = 10_000;

/// 将分页参数钳制到安全范围：页码 1–`MAX_PAGE`，每页 1–`MAX_PER_PAGE`。
#[cfg(feature = "ssr")]
fn clamp_pagination(page: i32, per_page: i32) -> (i32, i32) {
    (page.clamp(1, MAX_PAGE), per_page.clamp(1, MAX_PER_PAGE))
}

/// 获取已发布文章分页列表，按 published_at 降序。
#[server(endpoint = "home/posts")]
pub async fn list_published_posts(page: i32, per_page: i32) -> Result<PostListResponse, AppError> {
    use crate::server::db::{get_conn, query_err};

    let (page, per_page) = clamp_pagination(page, per_page);
    let client = get_conn().await?;

    let total: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM posts WHERE status = 'published' AND deleted_at IS NULL",
            &[],
        )
        .await
        .map_err(query_err)?
        .get(0);

    let offset = ((page - 1).max(0) as i64) * (per_page as i64);
    let limit = per_page as i64;
    let rows = client
        .query(
            "SELECT
                p.id, p.author_id, p.title, p.slug, p.summary, p.status,
                p.published_at, p.created_at, p.updated_at, p.cover_image,
                p.word_count, p.reading_time,
                COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') as tags
             FROM posts p
             LEFT JOIN post_tags pt ON p.id = pt.post_id
             LEFT JOIN tags t ON pt.tag_id = t.id
             WHERE p.status = 'published' AND p.deleted_at IS NULL
             GROUP BY p.id
             ORDER BY p.published_at DESC
             LIMIT $1 OFFSET $2",
            &[&limit, &offset],
        )
        .await
        .map_err(query_err)?;

    let posts = rows.iter().map(row_to_post_list_item).collect();
    Ok(PostListResponse { posts, total })
}

/// 获取全部标签及其关联的已发布文章数量，按标签名升序。
#[server(endpoint = "home/tags")]
pub async fn list_tags() -> Result<TagListResponse, AppError> {
    use crate::models::post::Tag;
    use crate::server::db::{get_conn, query_err};

    let client = get_conn().await?;
    // 聚合标签对应的已发布、未删除文章数量。
    let rows = client
        .query(
            "SELECT t.id, t.name, COUNT(p.id) as post_count
             FROM tags t
             LEFT JOIN post_tags pt ON t.id = pt.tag_id
             LEFT JOIN posts p ON pt.post_id = p.id AND p.deleted_at IS NULL AND p.status = 'published'
             GROUP BY t.id, t.name
             ORDER BY t.name",
            &[],
        )
        .await
        .map_err(query_err)?;

    let tags = rows
        .iter()
        .map(|r| Tag {
            id: r.get("id"),
            name: r.get("name"),
            post_count: r.get("post_count"),
        })
        .collect();
    Ok(TagListResponse { tags })
}

/// 站点公开配置在 settings 表中的键。
#[cfg(feature = "ssr")]
const SITE_KEYS: [&str; 5] = [
    "site_github_url",
    "site_home_title",
    "site_home_subtitle",
    "site_home_intro",
    "site_description",
];

/// 读取站点公开配置（页脚 GitHub 链接与首页标语）。缺失键回退空串。
#[server(endpoint = "home/site_settings")]
pub async fn get_site_settings() -> Result<SiteSettings, AppError> {
    use crate::server::db::{get_conn, query_err};

    let client = get_conn().await?;
    let rows = client
        .query(
            "SELECT key, value FROM settings WHERE key = ANY($1)",
            &[&SITE_KEYS.as_slice()],
        )
        .await
        .map_err(query_err)?;
    let mut settings = SiteSettings::default();
    for row in rows {
        let value: String = row.get("value");
        match row.get::<_, &str>("key") {
            "site_github_url" => settings.github_url = value,
            "site_home_title" => settings.home_title = value,
            "site_home_subtitle" => settings.home_subtitle = value,
            "site_home_intro" => settings.home_intro = value,
            "site_description" => settings.site_description = value,
            _ => {}
        }
    }
    Ok(settings)
}

/// 获取最新一篇已发布文章的目录预览；没有文章或文章没有小节时返回 `None`。
///
/// 排序与 `list_published_posts` 一致，前台再用 slug 核对首篇，防止两次读取之间
/// 有新文章发布造成错配。
#[server(endpoint = "home/latest_outline")]
pub async fn latest_post_outline() -> Result<Option<PostOutline>, AppError> {
    use crate::server::db::{get_conn, query_err};

    let client = get_conn().await?;
    let row = client
        .query_opt(
            "SELECT slug, toc_html FROM posts
             WHERE status = 'published' AND deleted_at IS NULL
             ORDER BY published_at DESC
             LIMIT 1",
            &[],
        )
        .await
        .map_err(query_err)?;
    Ok(row.and_then(|row| {
        let toc: Option<String> = row.get("toc_html");
        outline_from_toc(row.get("slug"), toc.as_deref().unwrap_or_default())
    }))
}

/// 公开笔记列表（每页 20 条），按版本创建时间倒序；只读已发布版本。
#[server(endpoint = "home/notes", input = leptos::server_fn::codec::Json)]
pub async fn list_notes(filter: NoteFilter) -> Result<NotePage, AppError> {
    use crate::models::note::{Note, NoteKind};
    use crate::server::db::{get_conn, query_err};

    if filter.query.chars().count() > 200 {
        return Err(AppError::BadRequest("搜索内容不能超过 200 字".into()));
    }
    let client = get_conn().await?;
    // 公开访问：不限作者、不限知识库笔记本，不看回收站，忽略管理筛选。
    let owner: Option<i32> = None;
    let books: Option<Vec<i32>> = None;
    let trash = false;
    let published: Option<bool> = None;
    let knowledge: Option<bool> = None;
    let query = escape_like_pattern(filter.query.trim());
    let kind = filter.kind.map(|k| k.as_str());
    let predicate = "FROM notes n JOIN note_revisions r ON r.note_id=n.id AND r.version=n.published_version
        WHERE ($1::int IS NULL OR n.owner_id=$1)
        AND ($2::int[] IS NULL OR EXISTS (SELECT 1 FROM notebook_notes bn WHERE bn.note_id=n.id AND bn.notebook_id=ANY($2)))
        AND (n.deleted_at IS NOT NULL)=$3
        AND ($4='' OR r.search_text ILIKE '%' || $4 || '%' ESCAPE '\\' OR EXISTS (SELECT 1 FROM unnest(r.tags) tag WHERE tag ILIKE '%' || $4 || '%' ESCAPE '\\'))
        AND ($5::text IS NULL OR r.kind=$5)
        AND ($6::int IS NULL OR EXISTS (SELECT 1 FROM notebook_notes bn JOIN notebooks b ON b.id=bn.notebook_id WHERE bn.note_id=n.id AND b.id=$6 AND ($1::int IS NOT NULL OR (b.is_public AND b.archived_at IS NULL))))
        AND ($7='' OR $7=ANY(r.tags))
        AND ($8::bool IS NULL OR (n.published_version IS NOT NULL)=$8)
        AND ($9::bool IS NULL OR (n.knowledge_version IS NOT NULL)=$9)";
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
    let total: i64 = client
        .query_one(&format!("SELECT COUNT(*) {predicate}"), params)
        .await
        .map_err(query_err)?
        .get(0);
    let page = filter.page.clamp(1, 100_000);
    // 列表仅短随记携带 HTML；长文按需读取。主题笔记读出目录只为数出「N 节」，下发前清空。
    // 笔记本本身的可见性也参与过滤，公开结果不泄露私密笔记本的 ID。
    let sql = format!(
        "SELECT n.id, n.slug, n.created_at, n.published_at, n.published_version,
            n.knowledge_version, n.deleted_at, r.version AS revision, r.kind, r.title,
            ''::text AS content_md,
            CASE WHEN r.kind='moment' AND length(r.content_md)<=2000 THEN r.content_html ELSE '' END AS content_html,
            CASE WHEN r.kind='topic' THEN r.toc_html ELSE '' END AS toc_html,
            r.summary, r.tags, r.created_at AS revision_at,
            ARRAY(SELECT b.id FROM notebooks b JOIN notebook_notes bn ON bn.notebook_id=b.id
                WHERE bn.note_id=n.id AND ($1::int IS NOT NULL OR (b.is_public AND b.archived_at IS NULL))
                  AND ($2::int[] IS NULL OR b.id=ANY($2)) ORDER BY b.id) AS notebook_ids
        {predicate}
        ORDER BY CASE WHEN $6::int IS NOT NULL THEN (SELECT position FROM notebook_notes WHERE notebook_id=$6 AND note_id=n.id) END,
        CASE WHEN $4<>'' THEN word_similarity($4, r.search_text) END DESC,
        r.created_at DESC, n.id DESC LIMIT 20 OFFSET {}",
        (page - 1) * 20
    );
    let notes = client
        .query(&sql, params)
        .await
        .map_err(query_err)?
        .iter()
        .map(|row| {
            let toc_html: String = row.get("toc_html");
            let sections = toc_sections(&toc_html, 0).total;
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
                toc_html: String::new(),
                summary: row.get("summary"),
                tags: row.get("tags"),
                created_at: row.get("created_at"),
                updated_at: row.get("revision_at"),
                published_at: row.get("published_at"),
                published_version: None,
                knowledge_version: None,
                deleted_at: None,
                notebook_ids: row.get("notebook_ids"),
                sections: u32::try_from(sections).unwrap_or(u32::MAX),
            }
        })
        .collect();
    Ok(NotePage { notes, total })
}

/// 数据库行 → 文章列表项。
#[cfg(feature = "ssr")]
fn row_to_post_list_item(row: &tokio_postgres::Row) -> crate::models::post::PostListItem {
    use crate::models::post::{PostListItem, PostStatus};

    let status_str: String = row.get("status");
    let status = PostStatus::from_str(&status_str).unwrap_or(PostStatus::Draft);
    // 聚合标签并原地过滤空字符串。
    let mut tags: Vec<String> = row.try_get::<_, Vec<String>>("tags").unwrap_or_default();
    tags.retain(|t| !t.is_empty());
    let word_count: i32 = row.get("word_count");
    let reading_time: i32 = row.get("reading_time");

    PostListItem {
        id: row.get("id"),
        author_id: row.get("author_id"),
        title: row.get("title"),
        slug: row.get("slug"),
        summary: row.get("summary"),
        status,
        published_at: row.get("published_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        deleted_at: row.try_get("deleted_at").ok(),
        tags,
        cover_image: row.get("cover_image"),
        reading_time: reading_time.max(1) as u32,
        word_count: word_count.max(0) as u32,
    }
}

#[cfg(feature = "ssr")]
fn outline_from_toc(slug: String, toc_html: &str) -> Option<PostOutline> {
    use crate::models::home::OUTLINE_PREVIEW_LIMIT;

    let sections = toc_sections(toc_html, OUTLINE_PREVIEW_LIMIT);
    (sections.total > 0).then(|| PostOutline {
        slug,
        total: u32::try_from(sections.total).unwrap_or(u32::MAX),
        headings: sections.headings,
    })
}

/// 转义 LIKE 通配符（`\` `%` `_`）。
#[cfg(feature = "ssr")]
fn escape_like_pattern(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 目录的一级小节：总数与前若干个标题（已解码为纯文本）。
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TocSections {
    total: usize,
    headings: Vec<String>,
}

/// 解析已存储目录 HTML 最外层列表的小节，最多保留 `limit` 个标题，`total` 计全部。
///
/// 目录由第一个标题的层级起算；层级先深后浅时渲染器会提前闭合根列表，
/// 深度不足 1 的链接也按一级处理。
#[cfg(feature = "ssr")]
fn toc_sections(toc_html: &str, limit: usize) -> TocSections {
    let mut sections = TocSections::default();
    let mut depth = 0usize;
    let mut rest = toc_html;
    while let Some(start) = rest.find('<') {
        let Some(len) = rest[start..].find('>') else {
            break;
        };
        let tag = &rest[start + 1..start + len];
        rest = &rest[start + len + 1..];
        match tag.split_ascii_whitespace().next().unwrap_or_default() {
            "ul" => depth += 1,
            "/ul" => depth = depth.saturating_sub(1),
            "a" => {
                let end = rest.find("</a>").unwrap_or(rest.len());
                if depth <= 1 {
                    sections.total += 1;
                    if sections.headings.len() < limit {
                        sections.headings.push(decode_text(&rest[..end]));
                    }
                }
                rest = &rest[end..];
            }
            _ => {}
        }
    }
    sections
}

/// 链接文字是转义后的纯文本：去掉残留标签，解码实体并合并空白。
#[cfg(feature = "ssr")]
fn decode_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(pos) = rest.find(['<', '&']) {
        text.push_str(&rest[..pos]);
        rest = &rest[pos..];
        if rest.starts_with('<') {
            rest = rest.find('>').map_or("", |end| &rest[end + 1..]);
            continue;
        }
        match rest.find(';').filter(|&end| end <= 10) {
            Some(end) => match decode_entity(&rest[1..end]) {
                Some(c) => {
                    text.push(c);
                    rest = &rest[end + 1..];
                }
                None => {
                    text.push('&');
                    rest = &rest[1..];
                }
            },
            None => {
                text.push('&');
                rest = &rest[1..];
            }
        }
    }
    text.push_str(rest);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(feature = "ssr")]
fn decode_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let code = name.strip_prefix('#')?;
            let value = match code.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => code.parse().ok()?,
            };
            char::from_u32(value)
        }
    }
}
