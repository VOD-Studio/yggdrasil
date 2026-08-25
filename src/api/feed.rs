//! RSS 2.0 与 JSON Feed 1.1 订阅端点。
//!
//! 提供两个无中间件的公开读端点，挂载在 `static_routes` 上：
//! - `GET /feed.xml` — RSS 2.0（`application/rss+xml`）
//! - `GET /feed.json` — JSON Feed 1.1（`application/feed+json`）
//!
//! 输出最近 `FEED_ITEM_LIMIT` 篇已发布文章（含保存时已渲染的全文 `content_html`），
//! 数据经 moka 单键缓存（`CacheKey::Feed`，TTL 600s），文章写路径统一失效。
//! 渲染函数均为纯函数并接受 `now`/`base` 参数注入，便于单元测试固定输出。
//!
//! 仅在 `server` feature 启用时编译。

#![cfg(feature = "server")]

use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};

use crate::api::error::AppError;
use crate::db::pool::get_conn;
use crate::models::post::FeedItem;

/// Feed channel 标题。站点无标题配置，与首页 `HomeInfo` 硬编码保持一致。
const CHANNEL_TITLE: &str = "Yggdrasil";

/// Feed channel 描述。与首页 `HomeInfo` 副标题保持一致。
const CHANNEL_DESCRIPTION: &str = "极简、快速、现代。专注于文字本身的开源博客平台。";

/// Feed 语言。
const FEED_LANGUAGE: &str = "zh-CN";

/// Feed 输出文章条数上限。
const FEED_ITEM_LIMIT: i64 = 20;

/// RSS Content-Type（标准注册类型，带 charset 便于阅读器正确解码中文）。
const RSS_CONTENT_TYPE: &str = "application/rss+xml; charset=utf-8";

/// JSON Feed Content-Type。
const JSON_CONTENT_TYPE: &str = "application/feed+json; charset=utf-8";

/// Feed 响应缓存头，与缓存层 TTL（600s）对齐。
const FEED_CACHE_CONTROL: &str = "public, max-age=600";

/// XML 文本转义：`& < > " '` 五个字符。
///
/// 全文 HTML 直接整体转义一次（而非 CDATA）：`&` 先行替换避免二次转义，
/// 阅读器解析后得到与页面一致的 HTML 源码。
fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// 推导站点绝对 URL 基址（无尾部斜杠）。
///
/// 仅信任「站点配置 → 安全」面板中的 APP_BASE_URL；绝不使用请求 Host
/// 生成公开 Feed 链接，避免 Host header poisoning。未配置时仅回退到
/// `http://localhost`，生产环境必须配置固定的 APP_BASE_URL。
async fn site_base_url(_headers: &HeaderMap) -> String {
    let base = crate::api::settings::runtime_security_settings()
        .await
        .app_base_url;
    let base = base.trim();
    if !base.is_empty() {
        return base.trim_end_matches('/').to_string();
    }
    tracing::warn!("APP_BASE_URL 未配置，RSS/Feed 链接回退到 localhost");
    "http://localhost".to_string()
}

/// 渲染 RSS 2.0 文档。`now` 注入以便测试固定 `lastBuildDate`。
fn render_rss(base: &str, now: DateTime<Utc>, items: &[FeedItem]) -> String {
    let mut xml = String::with_capacity(4096 + items.len() * 512);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<rss version=\"2.0\"><channel>");
    xml.push_str("<title>");
    xml.push_str(CHANNEL_TITLE);
    xml.push_str("</title><link>");
    xml.push_str(base);
    xml.push_str("</link><description>");
    xml.push_str(CHANNEL_DESCRIPTION);
    xml.push_str("</description><language>");
    xml.push_str(FEED_LANGUAGE);
    xml.push_str("</language><lastBuildDate>");
    xml.push_str(&now.to_rfc2822());
    xml.push_str("</lastBuildDate>");
    for item in items {
        xml.push_str("<item><title>");
        xml.push_str(&escape_xml(&item.title));
        xml.push_str("</title><link>");
        xml.push_str(base);
        xml.push_str("/post/");
        xml.push_str(&item.slug);
        xml.push_str("</link><guid isPermaLink=\"true\">");
        xml.push_str(base);
        xml.push_str("/post/");
        xml.push_str(&item.slug);
        xml.push_str("</guid><pubDate>");
        xml.push_str(&item.published_at.to_rfc2822());
        xml.push_str("</pubDate>");
        for tag in &item.tags {
            xml.push_str("<category>");
            xml.push_str(&escape_xml(tag));
            xml.push_str("</category>");
        }
        if let Some(html) = &item.content_html {
            xml.push_str("<description>");
            xml.push_str(&escape_xml(html));
            xml.push_str("</description>");
        } else if let Some(summary) = &item.summary {
            xml.push_str("<description>");
            xml.push_str(&escape_xml(summary));
            xml.push_str("</description>");
        }
        xml.push_str("</item>");
    }
    xml.push_str("</channel></rss>");
    xml
}

/// 渲染 JSON Feed 1.1 文档（`serde_json` 直接序列化，零新依赖）。
fn render_json(base: &str, items: &[FeedItem]) -> Result<String, serde_json::Error> {
    let feed_items: Vec<Value> = items
        .iter()
        .map(|item| {
            let url = format!("{base}/post/{}", item.slug);
            let mut m = Map::new();
            m.insert("id".to_string(), json!(url.clone()));
            m.insert("url".to_string(), json!(url));
            m.insert("title".to_string(), json!(item.title));
            if let Some(html) = &item.content_html {
                m.insert("content_html".to_string(), json!(html));
            }
            if let Some(summary) = &item.summary {
                m.insert("summary".to_string(), json!(summary));
            }
            m.insert(
                "date_published".to_string(),
                json!(item.published_at.to_rfc3339()),
            );
            m.insert(
                "date_modified".to_string(),
                json!(item.updated_at.to_rfc3339()),
            );
            m.insert("tags".to_string(), json!(item.tags));
            Value::Object(m)
        })
        .collect();
    serde_json::to_string(&json!({
        "version": "https://jsonfeed.org/version/1.1",
        "title": CHANNEL_TITLE,
        "home_page_url": base,
        "feed_url": format!("{base}/feed.json"),
        "language": FEED_LANGUAGE,
        "items": feed_items,
    }))
}

/// 将数据库行转换为 Feed 条目。
///
/// 与 `row_to_post_list_item` 同款聚合标签写法；`published_at` 对
/// 空值回退 `updated_at`（published 状态理论上必有值）。
fn row_to_feed_item(row: &tokio_postgres::Row) -> FeedItem {
    let updated_at: DateTime<Utc> = row.get("updated_at");
    let mut tags: Vec<String> = row.try_get::<_, Vec<String>>("tags").unwrap_or_default();
    tags.retain(|t| !t.is_empty());
    FeedItem {
        title: row.get("title"),
        slug: row.get("slug"),
        summary: row.get("summary"),
        content_html: row.get("content_html"),
        published_at: row
            .get::<_, Option<DateTime<Utc>>>("published_at")
            .unwrap_or(updated_at),
        updated_at,
        tags,
    }
}

/// 加载 Feed 条目：缓存命中直接返回，miss 则查询 DB 并回填缓存。
async fn load_feed_items() -> Result<Vec<FeedItem>, AppError> {
    if let Some(items) = crate::cache::get_feed().await {
        return Ok(items);
    }
    let client = get_conn().await.map_err(AppError::db_conn)?;
    let rows = client
        .query(
            "SELECT p.title, p.slug, p.summary, p.content_html, p.published_at, p.updated_at,
                    COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') AS tags
             FROM posts p
             LEFT JOIN post_tags pt ON p.id = pt.post_id
             LEFT JOIN tags t ON pt.tag_id = t.id
             WHERE p.status = 'published' AND p.deleted_at IS NULL
             GROUP BY p.id
             ORDER BY p.published_at DESC
             LIMIT $1",
            &[&FEED_ITEM_LIMIT],
        )
        .await
        .map_err(AppError::query)?;
    let items: Vec<FeedItem> = rows.iter().map(row_to_feed_item).collect();
    crate::cache::set_feed(items.clone()).await;
    Ok(items)
}

/// `GET /feed.xml` — RSS 2.0 订阅源。
pub async fn rss_feed(headers: HeaderMap) -> Response {
    match load_feed_items().await {
        Ok(items) => {
            let base = site_base_url(&headers).await;
            let body = render_rss(&base, Utc::now(), &items);
            (
                [
                    (
                        header::CONTENT_TYPE,
                        HeaderValue::from_static(RSS_CONTENT_TYPE),
                    ),
                    (
                        header::CACHE_CONTROL,
                        HeaderValue::from_static(FEED_CACHE_CONTROL),
                    ),
                ],
                body,
            )
                .into_response()
        }
        Err(_) => {
            // DB 失败细节已由 AppError::db_conn/query 构造器记录完整链条。
            tracing::error!("feed 生成失败");
            (StatusCode::INTERNAL_SERVER_ERROR, "feed unavailable").into_response()
        }
    }
}

/// `GET /feed.json` — JSON Feed 1.1 订阅源。
pub async fn json_feed(headers: HeaderMap) -> Response {
    match load_feed_items().await {
        Ok(items) => {
            let base = site_base_url(&headers).await;
            match render_json(&base, &items) {
                Ok(body) => (
                    [
                        (
                            header::CONTENT_TYPE,
                            HeaderValue::from_static(JSON_CONTENT_TYPE),
                        ),
                        (
                            header::CACHE_CONTROL,
                            HeaderValue::from_static(FEED_CACHE_CONTROL),
                        ),
                    ],
                    body,
                )
                    .into_response(),
                Err(e) => {
                    // serde_json 序列化失败属于代码 bug（结构固定），记录具体原因。
                    tracing::error!("feed JSON 序列化失败: {e}");
                    (StatusCode::INTERNAL_SERVER_ERROR, "feed unavailable").into_response()
                }
            }
        }
        Err(_) => {
            // DB 失败细节已由 AppError::db_conn/query 构造器记录完整链条。
            tracing::error!("feed 生成失败");
            (StatusCode::INTERNAL_SERVER_ERROR, "feed unavailable").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn fixed_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn fixture_item() -> FeedItem {
        let ts = fixed_now();
        FeedItem {
            title: "A & B".to_string(),
            slug: "my-post".to_string(),
            summary: None,
            content_html: Some("<p>hi</p>".to_string()),
            published_at: ts,
            updated_at: ts,
            tags: vec!["Rust".to_string()],
        }
    }

    #[test]
    fn escape_xml_escapes_all_special_chars() {
        assert_eq!(escape_xml("&<>\"'"), "&amp;&lt;&gt;&quot;&apos;");
    }

    #[test]
    fn escape_xml_plain_text_unchanged() {
        assert_eq!(escape_xml("plain text 123"), "plain text 123");
    }

    #[test]
    fn render_rss_contains_escaped_fields() {
        let xml = render_rss("https://example.com", fixed_now(), &[fixture_item()]);
        assert!(
            xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\">")
        );
        assert!(xml.ends_with("</channel></rss>"));
        assert!(xml.contains("<title>A &amp; B</title>"));
        assert!(xml.contains("<link>https://example.com</link>"));
        assert!(xml.contains("<guid isPermaLink=\"true\">https://example.com/post/my-post</guid>"));
        assert!(xml.contains("<category>Rust</category>"));
        assert!(xml.contains("<description>&lt;p&gt;hi&lt;/p&gt;</description>"));
        assert!(xml.contains("<lastBuildDate>Fri, 2 Jan 2026 03:04:05 +0000</lastBuildDate>"));
    }

    #[test]
    fn render_rss_falls_back_to_summary_and_omits_item_description() {
        let mut item = fixture_item();
        item.content_html = None;
        item.summary = Some("摘要 & 简介".to_string());
        let xml = render_rss("https://example.com", fixed_now(), &[item]);
        assert!(xml.contains("<description>摘要 &amp; 简介</description>"));

        let mut item2 = fixture_item();
        item2.content_html = None;
        item2.summary = None;
        let xml2 = render_rss("https://example.com", fixed_now(), &[item2]);
        // 仅 channel 级 description（站点简介）保留，item 级省略。
        assert_eq!(xml2.matches("<description>").count(), 1);
    }

    #[test]
    fn render_json_roundtrips() {
        let out = render_json("https://example.com", &[fixture_item()]).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["version"], "https://jsonfeed.org/version/1.1");
        assert_eq!(v["title"], "Yggdrasil");
        assert_eq!(v["home_page_url"], "https://example.com");
        assert_eq!(v["feed_url"], "https://example.com/feed.json");
        assert_eq!(v["language"], "zh-CN");
        assert_eq!(v["items"][0]["id"], "https://example.com/post/my-post");
        assert_eq!(v["items"][0]["url"], "https://example.com/post/my-post");
        assert_eq!(v["items"][0]["title"], "A & B");
        assert_eq!(v["items"][0]["content_html"], "<p>hi</p>");
        assert_eq!(v["items"][0]["tags"][0], "Rust");
        assert_eq!(v["items"][0]["date_published"], "2026-01-02T03:04:05+00:00");
        assert!(v["items"][0].get("summary").is_none());
    }

    #[tokio::test]
    #[serial]
    async fn site_base_url_ignores_untrusted_host_when_no_settings() {
        // 未配置固定 APP_BASE_URL 时，不能使用请求 Host 生成公开链接。
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("attacker.example"));
        let r = site_base_url(&headers).await;
        assert_eq!(r, "http://localhost");
    }

    #[tokio::test]
    #[serial]
    async fn site_base_url_falls_back_to_localhost_when_nothing() {
        // 既无配置也无 Host 头 → 兜底 localhost 并告警。
        let r = site_base_url(&HeaderMap::new()).await;
        assert_eq!(r, "http://localhost");
    }
}
