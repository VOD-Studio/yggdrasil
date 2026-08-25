//! 文章模块内部辅助函数。
//!
//! 提供数据库行到 `Post` 模型的转换、标签同步与标签清洗等工具函数。
//! 仅在 `feature = "server"` 启用的服务端构建中可用。

#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::models::post::{Post, PostListItem, PostStatus};
#[cfg(feature = "server")]
use crate::utils::text::{auto_summary, count_words, reading_time};

/// 复用认证模块的当前 admin 用户获取逻辑。
#[cfg(feature = "server")]
pub(super) use crate::api::auth::get_current_admin_user;

/// 将数据库行转换为轻量列表项 DTO。
///
/// 不包含 `content_md`/`content_html`；字数与阅读时长直接读取已持久化的列。
/// 同步函数，不依赖数据库连接。
#[cfg(feature = "server")]
pub(super) fn row_to_post_list_item(row: &tokio_postgres::Row) -> PostListItem {
    let id: i32 = row.get("id");
    let status_str: String = row.get("status");
    let status = PostStatus::from_str(&status_str).unwrap_or(PostStatus::Draft);

    // 聚合标签并原地过滤空字符串（retain 避免 into_iter+filter+collect 的二次 Vec 分配）。
    let mut tags: Vec<String> = row.try_get::<_, Vec<String>>("tags").unwrap_or_default();
    tags.retain(|t| !t.is_empty());

    let word_count: i32 = row.get("word_count");
    let reading_time: i32 = row.get("reading_time");

    PostListItem {
        id,
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

/// 将数据库行转换为完整文章详情。
///
/// 相比列表项额外包含上一篇/下一篇导航，
/// 并在 content_html 为空时重新渲染 Markdown 以兼容旧数据。
#[cfg(feature = "server")]
pub(super) async fn row_to_post_full(row: &tokio_postgres::Row) -> Result<Post, AppError> {
    let id: i32 = row.get("id");
    let role_str: String = row.get("status");
    let status = PostStatus::from_str(&role_str).unwrap_or(PostStatus::Draft);

    // 聚合标签并原地过滤空字符串（retain 避免 into_iter+filter+collect 的二次 Vec 分配）。
    let mut tags: Vec<String> = row.try_get::<_, Vec<String>>("tags").unwrap_or_default();
    tags.retain(|t| !t.is_empty());

    // 解析上一篇文章导航。
    let prev_post = if let Ok(prev_title) = row.try_get::<_, String>("prev_title") {
        if let Ok(prev_slug) = row.try_get::<_, String>("prev_slug") {
            Some(crate::models::post::PostNav {
                title: prev_title,
                slug: prev_slug,
            })
        } else {
            None
        }
    } else {
        None
    };

    // 解析下一篇文章导航。
    let next_post = if let Ok(next_title) = row.try_get::<_, String>("next_title") {
        if let Ok(next_slug) = row.try_get::<_, String>("next_slug") {
            Some(crate::models::post::PostNav {
                title: next_title,
                slug: next_slug,
            })
        } else {
            None
        }
    } else {
        None
    };

    // 读取正文与已持久化的字数/阅读时长；若列尚未回填（旧数据为 0），则现场计算。
    let content_md: String = row.get("content_md");
    let stored_word_count: i32 = row.get("word_count");
    let stored_reading_time: i32 = row.get("reading_time");
    let (word_count, reading_time) = if stored_word_count > 0 && stored_reading_time > 0 {
        (stored_word_count as u32, stored_reading_time as u32)
    } else {
        let wc = count_words(&content_md);
        (wc, reading_time(wc))
    };

    let content_html: Option<String> = row.get("content_html");
    let toc_html_row: Option<String> = row.get("toc_html");

    // 若数据库中未渲染 HTML（旧数据兼容），则现场渲染 Markdown。
    let (content_html, toc_html) = if let Some(html) = content_html {
        (html, toc_html_row)
    } else {
        // 旧数据 fallback 仍可能触发完整 Markdown/KaTeX/高亮渲染，
        // 必须移出 Tokio worker，避免单条旧文章阻塞其它请求。
        let md_for_render = content_md.clone();
        let rendered = tokio::task::spawn_blocking(move || {
            crate::api::markdown::render_markdown_enhanced(&md_for_render)
        })
        .await
        .map_err(|_| AppError::Internal("旧文章 Markdown 渲染任务失败"))?;
        (
            rendered.html,
            if rendered.toc_html.is_empty() {
                None
            } else {
                Some(rendered.toc_html)
            },
        )
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

/// 在事务中同步文章的标签关联。
///
/// 对传入的每个标签：若不存在则插入 tags 表，否则查询已有 id，
/// 然后在 post_tags 表中建立关联。不会删除旧关联，调用方需先清理。
#[cfg(feature = "server")]
pub(crate) async fn sync_tags(
    tx: &deadpool_postgres::Transaction<'_>,
    post_id: i32,
    tags: &[String],
) -> Result<(), AppError> {
    for tag_name in tags {
        let tag_id: i32 = {
            // 先尝试插入，若已存在则返回空。
            let row = tx
                .query_opt(
                    "INSERT INTO tags (name) VALUES ($1) ON CONFLICT (name) DO NOTHING RETURNING id",
                    &[&tag_name.as_str()],
                )
                .await
                .map_err(AppError::tx)?;

            match row {
                Some(r) => r.get(0),
                None => {
                    // 插入冲突时回查标签 id。
                    let row = tx
                        .query_opt("SELECT id FROM tags WHERE name = $1", &[&tag_name.as_str()])
                        .await
                        .map_err(AppError::query)?;
                    row.map(|r| r.get(0))
                        .ok_or(AppError::NotFound("标签不存在"))?
                }
            }
        };

        tx.execute(
            "INSERT INTO post_tags (post_id, tag_id) VALUES ($1, $2)",
            &[&post_id, &tag_id],
        )
        .await
        .map_err(AppError::tx)?;
    }

    Ok(())
}

/// 清洗标签列表：去头尾空白、过滤空字符串并去重（保留原始顺序）。
#[cfg(feature = "server")]
pub(crate) fn clean_tags(tags: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    tags.iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .filter(|t| seen.insert(t.to_lowercase()))
        .collect()
}
#[cfg(feature = "server")]
/// 取指定文章的全部标签名（按 post_tags 关联）。R5：此前 9+ 处重复同一条 SQL。
pub(crate) async fn fetch_post_tags(
    client: &impl deadpool_postgres::GenericClient,
    post_id: i32,
) -> Result<Vec<String>, AppError> {
    let rows = client
        .query(
            "SELECT t.name FROM tags t JOIN post_tags pt ON t.id = pt.tag_id WHERE pt.post_id = $1",
            &[&post_id],
        )
        .await
        .map_err(AppError::query)?;
    Ok(rows.iter().map(|r| r.get(0)).collect())
}
#[cfg(feature = "server")]
/// 批量取多篇文章的标签名并集（去重）。用于批量删除/清空回收站的缓存失效。
pub(crate) async fn fetch_post_tags_batch(
    client: &impl deadpool_postgres::GenericClient,
    post_ids: &[i32],
) -> Result<Vec<String>, AppError> {
    let rows = client
        .query(
            "SELECT DISTINCT t.name FROM tags t JOIN post_tags pt ON t.id = pt.tag_id WHERE pt.post_id = ANY($1)",
            &[&post_ids],
        )
        .await
        .map_err(AppError::query)?;
    Ok(rows.iter().map(|r| r.get(0)).collect())
}

/// 匹配 HTML/Markdown 中出现的本地上传图片路径，捕获组为相对路径
/// （如 `2026/07/24/153000.<uuid>.webp`，不含 /uploads/ 前缀与 query）。
/// 覆盖 blur-img 双层结构的 src 与 data-src。
#[cfg(feature = "server")]
static ASSET_PATH_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r#"/uploads/(\d{4}/\d{2}/\d{2}/[^\"'\s?#\)]+)"#)
        .expect("ASSET_PATH_RE 正则模式应在编译期通过校验")
});

/// 从文章 HTML 与封面 URL 中提取全部引用的本地上传图片相对路径（去重）。
///
/// 外链图（非 /uploads/ 路径）与无法识别的路径自然被忽略。
/// pub(crate)：重建素材索引（api::assets::rebuild）复用同一提取逻辑。
#[cfg(feature = "server")]
pub(crate) fn extract_asset_paths(content_html: &str, cover_image: Option<&str>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut paths: Vec<String> = ASSET_PATH_RE
        .captures_iter(content_html)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .filter(|p| seen.insert(p.clone()))
        .collect();
    if let Some(cover) = cover_image {
        if let Some(rel) = cover
            .strip_prefix("/uploads/")
            .map(|p| p.split('?').next().unwrap_or(p))
        {
            if seen.insert(rel.to_string()) {
                paths.push(rel.to_string());
            }
        }
    }
    paths
}

/// 在事务中同步文章的素材引用关联（asset_refs）。
///
/// 语义镜像 [`sync_tags`]：调用方需在事务内先删除旧关联（本函数自带 DELETE），
/// 再按 content_html + cover_image 中出现的 /uploads/ 路径重建。
/// 未登记到 assets 表的路径（如回填前的旧图）静默跳过，由重建索引兜底。
#[cfg(feature = "server")]
pub(crate) async fn sync_asset_refs(
    tx: &deadpool_postgres::Transaction<'_>,
    post_id: i32,
    content_html: &str,
    cover_image: Option<&str>,
) -> Result<(), AppError> {
    tx.execute("DELETE FROM asset_refs WHERE post_id = $1", &[&post_id])
        .await
        .map_err(AppError::tx)?;

    let paths = extract_asset_paths(content_html, cover_image);
    if !paths.is_empty() {
        tx.execute(
            "INSERT INTO asset_refs (asset_id, post_id) \
             SELECT id, $1 FROM assets WHERE path = ANY($2) \
             ON CONFLICT DO NOTHING",
            &[&post_id, &paths],
        )
        .await
        .map_err(AppError::tx)?;
    }
    Ok(())
}

/// Markdown 渲染 + 度量派生的完整结果。
///
/// `auto_summary` 为正文自动摘要（非用户填写值），调用方负责与用户 summary 合并。
/// `word_count` / `reading_time` 已转为 `i32` 以直接绑定 SQL 参数。
#[cfg(feature = "server")]
pub(crate) struct RenderedFields {
    pub content_html: String,
    pub toc_html: Option<String>,
    pub auto_summary: String,
    pub status: PostStatus,
    pub cover_image: Option<String>,
    pub word_count: i32,
    pub reading_time: i32,
}

/// 渲染 Markdown 并派生全部度量字段（R4）。
///
/// 收敛 create / update / MCP 写操作中重复的「spawn_blocking 渲染 → 7 步派生」：
/// content_html、toc_html（空则 None）、auto_summary、status（from_str 回退 Draft）、
/// cover_image（trim 后非空则保留）、word_count、reading_time。
#[cfg(feature = "server")]
pub(crate) async fn render_post_fields(
    content_md: &str,
    status_str: &str,
    cover_image: Option<&str>,
) -> Result<RenderedFields, AppError> {
    let md_for_render = content_md.to_string();
    let rendered = tokio::task::spawn_blocking(move || {
        crate::api::markdown::render_markdown_enhanced(&md_for_render)
    })
    .await
    .map_err(|_| AppError::Internal("Markdown 渲染任务失败"))?;

    let toc_html = if rendered.toc_html.is_empty() {
        None
    } else {
        Some(rendered.toc_html)
    };

    let auto_summary = auto_summary(content_md);
    let status = PostStatus::from_str(status_str).unwrap_or(PostStatus::Draft);
    let cover_image = cover_image
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string);

    let word_count = count_words(content_md);
    let reading_time = reading_time(word_count);

    Ok(RenderedFields {
        content_html: rendered.html,
        toc_html,
        auto_summary,
        status,
        cover_image,
        word_count: word_count as i32,
        reading_time: reading_time as i32,
    })
}

/// 渲染 Markdown 并仅派生重建所需的最小字段（R4）。
///
/// rebuild 仅需 content_html / toc_html / word_count / reading_time，
/// 不计算 summary / status / cover_image，避免无谓开销。
#[cfg(feature = "server")]
pub(crate) async fn render_post_fields_minimal(
    content_md: &str,
) -> Result<(String, Option<String>, i32, i32), AppError> {
    let md_for_render = content_md.to_string();
    let rendered = tokio::task::spawn_blocking(move || {
        crate::api::markdown::render_markdown_enhanced(&md_for_render)
    })
    .await
    .map_err(|_| AppError::Internal("Markdown 渲染任务失败"))?;

    let toc_html = if rendered.toc_html.is_empty() {
        None
    } else {
        Some(rendered.toc_html)
    };

    let word_count = count_words(content_md);
    let reading_time = reading_time(word_count);

    Ok((
        rendered.html,
        toc_html,
        word_count as i32,
        reading_time as i32,
    ))
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::{clean_tags, extract_asset_paths};

    #[test]
    fn clean_tags_trims_whitespace() {
        let input = vec!["  rust ".to_string(), "\t\nwasm\t".to_string()];
        assert_eq!(
            clean_tags(&input),
            vec!["rust".to_string(), "wasm".to_string()]
        );
    }

    #[test]
    fn clean_tags_filters_empty_strings() {
        let input = vec![
            "".to_string(),
            "  ".to_string(),
            "\t".to_string(),
            "valid".to_string(),
        ];
        assert_eq!(clean_tags(&input), vec!["valid".to_string()]);
    }

    #[test]
    fn clean_tags_removes_duplicates_case_insensitive() {
        let input = vec![
            "rust".to_string(),
            "  rust  ".to_string(),
            "Rust".to_string(),
            "wasm".to_string(),
        ];
        assert_eq!(
            clean_tags(&input),
            vec!["rust".to_string(), "wasm".to_string()]
        );
    }

    #[test]
    fn clean_tags_keeps_already_clean_input() {
        let input = vec!["rust".to_string(), "wasm".to_string(), "dioxus".to_string()];
        assert_eq!(
            clean_tags(&input),
            vec!["rust".to_string(), "wasm".to_string(), "dioxus".to_string()]
        );
    }

    // —— extract_asset_paths ——

    #[test]
    fn extract_asset_paths_from_blur_img_html() {
        // blur-img 双层结构：src 带 ?w=20，data-src 带 ?w=800，同一张图只应提取一次。
        let html = r#"<span class="blur-img"><img class="blur-img-placeholder" src="/uploads/2026/07/24/a.webp?w=20"><img class="blur-img-full" data-src="/uploads/2026/07/24/a.webp?w=800"></span>"#;
        assert_eq!(
            extract_asset_paths(html, None),
            vec!["2026/07/24/a.webp".to_string()]
        );
    }

    #[test]
    fn extract_asset_paths_multiple_and_cover() {
        let html = r#"<p><img src="/uploads/2026/07/24/a.webp"></p><p><img src="/uploads/2026/06/01/b.png?w=800"></p><p><img src="https://cdn.example.com/x.webp"></p>"#;
        let paths = extract_asset_paths(html, Some("/uploads/2026/07/24/cover.jpg?w=600"));
        assert_eq!(paths.len(), 3);
        assert!(paths.contains(&"2026/07/24/a.webp".to_string()));
        assert!(paths.contains(&"2026/06/01/b.png".to_string()));
        assert!(paths.contains(&"2026/07/24/cover.jpg".to_string()));
    }

    #[test]
    fn extract_asset_paths_cover_dedup_and_external() {
        // 封面与正文同图时去重；外链封面不产生路径。
        let html = r#"<img src="/uploads/2026/07/24/a.webp">"#;
        assert_eq!(
            extract_asset_paths(html, Some("/uploads/2026/07/24/a.webp")),
            vec!["2026/07/24/a.webp".to_string()]
        );
        assert!(extract_asset_paths(html, Some("https://example.com/c.webp")).len() == 1);
        assert!(extract_asset_paths(html, None).len() == 1);
    }

    #[test]
    fn extract_asset_paths_empty() {
        assert!(extract_asset_paths("<p>no image</p>", None).is_empty());
    }
}
