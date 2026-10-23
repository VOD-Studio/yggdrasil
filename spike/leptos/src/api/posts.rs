//! 文章详情查询接口。
//!
//! 移植主 crate `api/posts/read.rs` 的 `get_post_by_slug` / `get_public_post_by_slug`
//! 查询链与 `api/posts/helpers.rs` 的 `row_to_post_full`；原版的 moka 文章缓存
//! （`crate::cache::get/set_post_by_slug`）不在原型范围内，每次直查数据库。

use leptos::prelude::*;

use crate::error::AppError;
use crate::models::post::PublicPostResponse;

/// 根据 slug 获取公开文章详情（阅读页协议，不携带原始 Markdown）。
///
/// 只返回已发布且未删除的文章，并附带基于 published_at 的上一篇/下一篇导航。
/// 文章不存在时返回 `Ok(PublicPostResponse { post: None })`，与原版语义一致，
/// 由页面渲染 404。
#[server(endpoint = "posts/by_slug")]
pub async fn get_public_post_by_slug(slug: String) -> Result<PublicPostResponse, AppError> {
    use crate::server::db::{get_conn, query_err};

    let client = get_conn().await?;

    // 使用 LATERAL JOIN 查询按 published_at 排序的相邻文章。
    let row = client
        .query_opt(
            "SELECT
                p.id, p.author_id, p.title, p.slug, p.summary, p.content_md, p.content_html, p.toc_html,
                p.status, p.published_at, p.created_at, p.updated_at, p.cover_image,
                p.word_count, p.reading_time,
                COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') as tags,
                prev.title as prev_title, prev.slug as prev_slug,
                next.title as next_title, next.slug as next_slug
             FROM posts p
             LEFT JOIN post_tags pt ON p.id = pt.post_id
             LEFT JOIN tags t ON pt.tag_id = t.id
             LEFT JOIN LATERAL (
                 SELECT title, slug FROM posts
                 WHERE published_at < p.published_at
                   AND status = 'published'
                   AND deleted_at IS NULL
                 ORDER BY published_at DESC
                 LIMIT 1
             ) prev ON true
             LEFT JOIN LATERAL (
                 SELECT title, slug FROM posts
                 WHERE published_at > p.published_at
                   AND status = 'published'
                   AND deleted_at IS NULL
                 ORDER BY published_at ASC
                 LIMIT 1
             ) next ON true
             WHERE p.slug = $1 AND p.status = 'published' AND p.deleted_at IS NULL
             GROUP BY p.id, prev.title, prev.slug, next.title, next.slug",
            &[&slug],
        )
        .await
        .map_err(query_err)?;

    let post = match row {
        Some(row) => Some(row_to_post_full(&row).await?),
        None => None,
    };

    Ok(PublicPostResponse {
        post: post.map(Into::into),
    })
}

/// 将数据库行转换为完整文章详情。
///
/// 相比列表项额外包含上一篇/下一篇导航，
/// 并在 content_html 为空时重新渲染 Markdown 以兼容旧数据。
#[cfg(feature = "ssr")]
async fn row_to_post_full(
    row: &tokio_postgres::Row,
) -> Result<crate::models::post::Post, AppError> {
    use crate::models::post::{Post, PostNav, PostStatus};

    let id: i32 = row.get("id");
    let role_str: String = row.get("status");
    let status = PostStatus::from_str(&role_str).unwrap_or(PostStatus::Draft);

    // 聚合标签并原地过滤空字符串（retain 避免 into_iter+filter+collect 的二次 Vec 分配）。
    let mut tags: Vec<String> = row.try_get::<_, Vec<String>>("tags").unwrap_or_default();
    tags.retain(|t| !t.is_empty());

    // 解析上一篇/下一篇文章导航（LATERAL 未命中时两列均为 NULL，try_get 失败即 None）。
    let prev_post = match (
        row.try_get::<_, String>("prev_title"),
        row.try_get::<_, String>("prev_slug"),
    ) {
        (Ok(title), Ok(slug)) => Some(PostNav { title, slug }),
        _ => None,
    };
    let next_post = match (
        row.try_get::<_, String>("next_title"),
        row.try_get::<_, String>("next_slug"),
    ) {
        (Ok(title), Ok(slug)) => Some(PostNav { title, slug }),
        _ => None,
    };

    // 读取正文与已持久化的字数/阅读时长；若列尚未回填（旧数据为 0），则现场计算。
    let content_md: String = row.get("content_md");
    let stored_word_count: i32 = row.get("word_count");
    let stored_reading_time: i32 = row.get("reading_time");
    let (word_count, reading_time) = if stored_word_count > 0 && stored_reading_time > 0 {
        (stored_word_count as u32, stored_reading_time as u32)
    } else {
        let wc = count_words(&content_md);
        (wc, reading_time_of(wc))
    };

    let content_html: Option<String> = row.get("content_html");
    let toc_html: Option<String> = row.get("toc_html");

    // 若数据库中未渲染 HTML（旧数据兼容），则现场渲染 Markdown。
    let (content_html, toc_html) = match content_html {
        Some(html) => (html, toc_html),
        None => {
            // 旧数据 fallback 移出 Tokio worker，避免单条旧文章阻塞其它请求。
            let md_for_render = content_md.clone();
            let html = tokio::task::spawn_blocking(move || render_legacy_markdown(&md_for_render))
                .await
                .map_err(|e| AppError::internal("旧文章 Markdown 渲染任务失败", e))?;
            (html, None)
        }
    };

    Ok(Post {
        id,
        author_id: row.get("author_id"),
        title: row.get("title"),
        slug: row.get("slug"),
        summary: row.get("summary"),
        content_md,
        content_html: Some(content_html),
        status,
        published_at: row.get("published_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        deleted_at: row.try_get("deleted_at").ok(),
        tags,
        cover_image: row.get("cover_image"),
        reading_time,
        word_count,
        toc_html,
        prev_post,
        next_post,
    })
}

/// 旧数据兜底渲染：主 crate 的增强渲染管线（KaTeX/高亮/目录）不在原型范围内，
/// 这里用 pulldown-cmark 输出基础 HTML，不生成目录。
#[cfg(feature = "ssr")]
fn render_legacy_markdown(md: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let mut out = String::with_capacity(md.len() * 3 / 2);
    html::push_html(&mut out, Parser::new_ext(md, options));
    out
}

/// 统计 Markdown 文本的有效字数（对应主 crate `utils::text::count_words`）。
///
/// 中文字符每个计 1；英文字母按连续字母串计 1 个词。代码块、行内代码与图片
/// 不计入；空文本返回 1，避免出现 0 字的显示问题。
#[cfg(feature = "ssr")]
fn count_words(md: &str) -> u32 {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    // 原版用正则剥离 Markdown 标记；这里取解析后的文本事件，跳过代码块与图片。
    let mut plain = String::with_capacity(md.len());
    let mut skip_depth = 0u32;
    for event in Parser::new(md) {
        match event {
            Event::Start(Tag::CodeBlock(_)) | Event::Start(Tag::Image { .. }) => skip_depth += 1,
            Event::End(TagEnd::CodeBlock) | Event::End(TagEnd::Image) => {
                skip_depth = skip_depth.saturating_sub(1)
            }
            Event::Text(text) if skip_depth == 0 => {
                plain.push_str(&text);
                plain.push(' ');
            }
            Event::SoftBreak | Event::HardBreak => plain.push(' '),
            _ => {}
        }
    }

    let mut count = 0u32;
    let mut in_word = false;
    for c in plain.chars() {
        // CJK 统一表意文字范围（基本区）
        if (0x4E00..=0x9FFF).contains(&(c as u32)) {
            count += 1;
            in_word = false;
        } else if c.is_alphabetic() {
            if !in_word {
                count += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }
    count.max(1)
}

/// 由字数估算阅读时长（分钟）：按每分钟 200 字计算，至少返回 1 分钟。
#[cfg(feature = "ssr")]
fn reading_time_of(word_count: u32) -> u32 {
    (word_count / 200).max(1)
}
