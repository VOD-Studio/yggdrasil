//! RSS 2.0 与 JSON Feed 1.1 订阅端点。
//!
//! 提供两个公开读端点，挂载在 `feed_routes` 上（只套压缩层）：
//! - `GET /feed.xml` — RSS 2.0（`application/rss+xml`）
//! - `GET /feed.json` — JSON Feed 1.1（`application/feed+json`）
//!
//! 输出最近 `FEED_ITEM_LIMIT` 篇已发布文章（含保存时已渲染的全文 `content_html`，去掉深色图表与
//! 代码高亮标记）。20 篇全文有数 MB，渲染加压缩要两百多毫秒，所以缓存的是成品：两种格式的正文
//! 连同各启用算法的压缩结果（`CacheKey::Feed`，TTL 600s，文章写路径统一失效），命中时按
//! `Accept-Encoding` 直接返回，并支持 `If-None-Match`。
//! 渲染函数均为纯函数并接受 `now`/`base` 参数注入，便于单元测试固定输出。
//!
//! 仅在 `server` feature 启用时编译。

#![cfg(feature = "server")]

use std::borrow::Cow;
use std::sync::Arc;

use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::api::error::AppError;
use crate::api::markdown::mermaid::strip_dark_variants;
use crate::compression::{self, CompressionAlgorithms, Effort, Encoding};
use crate::db::pool::get_conn;
use crate::models::post::FeedItem;
use crate::utils::html::escape_html;

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
///
/// 文本字段经 [`escape_html`] 转义（`& < > " '`；`'` 输出数字字符引用 `&#x27;`，
/// 在 XML 中与 `&apos;` 等价）。全文 HTML 直接整体转义一次（而非 CDATA），
/// 阅读器解析后得到与页面一致的 HTML 源码。
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
        xml.push_str(&escape_html(&item.title));
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
            xml.push_str(&escape_html(tag));
            xml.push_str("</category>");
        }
        if let Some(html) = &item.content_html {
            xml.push_str("<description>");
            xml.push_str(&escape_html(html));
            xml.push_str("</description>");
        } else if let Some(summary) = &item.summary {
            xml.push_str("<description>");
            xml.push_str(&escape_html(summary));
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
        // 阅读器没有站点 CSS：图表只留浅色一份，代码去掉高亮标记。
        content_html: row.get::<_, Option<String>>("content_html").map(|html| {
            let html = match strip_dark_variants(&html) {
                Cow::Owned(stripped) => stripped,
                Cow::Borrowed(_) => html,
            };
            match strip_code_highlight(&html) {
                Cow::Owned(stripped) => stripped,
                Cow::Borrowed(_) => html,
            }
        }),
        published_at: row
            .get::<_, Option<DateTime<Utc>>>("published_at")
            .unwrap_or(updated_at),
        updated_at,
        tags,
    }
}

/// 删掉 `<pre>` 里的 `<span …>` / `</span>`，只留代码文本。高亮靠站点的 highlight.css 着色，
/// 阅读器里这些标记不起作用，却占了 Feed 约三成体积（单个代码块可达数百 KB）。
fn strip_code_highlight(html: &str) -> Cow<'_, str> {
    const CLOSE: &str = "</pre>";
    let is_tag = |rest: &str, name: &str| {
        rest.strip_prefix(name)
            .and_then(|after| after.bytes().next())
            .is_some_and(|b| b == b'>' || b.is_ascii_whitespace())
    };
    let mut out = String::new();
    let mut rest = html;
    let mut changed = false;
    while let Some(start) = rest.find("<pre") {
        if !is_tag(&rest[start..], "<pre") {
            out.push_str(&rest[..start + 4]);
            rest = &rest[start + 4..];
            continue;
        }
        let Some(len) = rest[start..].find(CLOSE) else {
            break;
        };
        out.push_str(&rest[..start]);
        let mut block = &rest[start..start + len];
        while let Some(at) = block.find('<') {
            out.push_str(&block[..at]);
            let tag = &block[at..];
            let skip = if tag.starts_with("</span>") {
                Some("</span>".len())
            } else if is_tag(tag, "<span") {
                tag.find('>').map(|end| end + 1)
            } else {
                None
            };
            match skip {
                Some(skip) => {
                    changed = true;
                    block = &tag[skip..];
                }
                None => {
                    out.push('<');
                    block = &tag[1..];
                }
            }
        }
        out.push_str(block);
        rest = &rest[start + len..];
    }
    if !changed {
        return Cow::Borrowed(html);
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// 一种格式的成品：正文、各启用算法的压缩结果与 ETag。
struct FeedDocument {
    body: Bytes,
    encoded: Vec<(Encoding, Bytes)>,
    etag: HeaderValue,
}

impl FeedDocument {
    /// 压缩取动态档（brotli 7）：成品每 10 分钟或每次文章写入后重建一次，brotli 11 压数 MB
    /// 要好几秒，只小约 8%，不值。
    fn new(body: String, algorithms: Option<CompressionAlgorithms>) -> Self {
        let encoded = algorithms
            .into_iter()
            .flat_map(CompressionAlgorithms::encodings)
            .filter_map(|encoding| {
                match compression::encode(encoding, Effort::Dynamic, body.as_bytes())? {
                    Ok(bytes) => Some((encoding, Bytes::from(bytes))),
                    Err(error) => {
                        tracing::warn!(%error, "feed 预压缩失败，该编码改由压缩层现压");
                        None
                    }
                }
            })
            .collect();
        let etag = HeaderValue::from_str(&format!("\"{}\"", hex::encode(Sha256::digest(&body))))
            .expect("SHA256 is an ASCII entity tag");
        Self {
            body: Bytes::from(body),
            encoded,
            etag,
        }
    }

    /// 协商出的编码有预压缩结果时直接返回；否则给出原文，由外层压缩层现压或回 406。
    /// 启用压缩时一律带 `Vary: Accept-Encoding`（含 304）。
    fn response(
        &self,
        content_type: &'static str,
        request: &HeaderMap,
        algorithms: Option<CompressionAlgorithms>,
    ) -> Response {
        let unchanged = request
            .get(header::IF_NONE_MATCH)
            .and_then(|h| h.to_str().ok())
            .is_some_and(|value| {
                value.split(',').any(|tag| {
                    let tag = tag.trim().strip_prefix("W/").unwrap_or(tag.trim());
                    tag == "*" || tag == self.etag.to_str().unwrap_or("")
                })
            });
        let variant = algorithms
            .and_then(|algorithms| compression::negotiate(request, algorithms))
            .and_then(|encoding| self.encoded.iter().find(|(e, _)| *e == encoding))
            .filter(|_| !unchanged);
        let mut response = Response::new(match (variant, unchanged) {
            (_, true) => Body::empty(),
            (Some((_, bytes)), false) => Body::from(bytes.clone()),
            (None, false) => Body::from(self.body.clone()),
        });
        if unchanged {
            *response.status_mut() = StatusCode::NOT_MODIFIED;
        }
        let headers = response.headers_mut();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static(FEED_CACHE_CONTROL),
        );
        headers.insert(header::ETAG, self.etag.clone());
        if let Some((encoding, _)) = variant {
            headers.insert(header::CONTENT_ENCODING, encoding.header_value());
        }
        if algorithms.is_some() {
            headers.insert(header::VARY, HeaderValue::from_static("accept-encoding"));
        }
        response
    }
}

/// 渲染好的两份 Feed 文档，整体放进 `CacheKey::Feed`。
pub struct RenderedFeed {
    /// 渲染时用的站点基址；面板改了 APP_BASE_URL 后据此判定成品过期。
    base: String,
    rss: FeedDocument,
    json: FeedDocument,
}

/// 渲染并压缩两种格式。`lastBuildDate` 取最近一篇的更新时间而不是当前时间：内容没变时
/// 重建出的正文与 ETag 也不变，阅读器的条件请求能拿到 304。
fn build_feed(
    base: String,
    items: &[FeedItem],
    algorithms: Option<CompressionAlgorithms>,
) -> Result<RenderedFeed, serde_json::Error> {
    let built = items
        .iter()
        .map(|item| item.updated_at)
        .max()
        .unwrap_or_else(Utc::now);
    let rss = FeedDocument::new(render_rss(&base, built, items), algorithms);
    let json = FeedDocument::new(render_json(&base, items)?, algorithms);
    Ok(RenderedFeed { base, rss, json })
}

/// 同一时刻只重建一份成品：缓存失效后并发到达的请求排队等第一个建完。
static BUILD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 取 Feed 成品：缓存命中直接返回，未命中则查库、在阻塞线程池渲染压缩并回填缓存。
async fn load_feed(headers: &HeaderMap) -> Result<Arc<RenderedFeed>, AppError> {
    let base = site_base_url(headers).await;
    let cached = |feed: Option<Arc<RenderedFeed>>| feed.filter(|feed| feed.base == base);
    if let Some(feed) = cached(crate::cache::get_feed().await) {
        return Ok(feed);
    }
    let _guard = BUILD_LOCK.lock().await;
    if let Some(feed) = cached(crate::cache::get_feed().await) {
        return Ok(feed);
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
    drop(client);
    let algorithms = compression::algorithms();
    let feed = tokio::task::spawn_blocking(move || {
        let items: Vec<FeedItem> = rows.iter().map(row_to_feed_item).collect();
        build_feed(base, &items, algorithms)
    })
    .await
    .map_err(|error| {
        tracing::error!(%error, "feed 渲染任务异常退出");
        AppError::Internal("feed 生成失败")
    })?
    .map_err(|error| {
        // serde_json 序列化失败属于代码 bug（结构固定），记录具体原因。
        tracing::error!(%error, "feed JSON 序列化失败");
        AppError::Internal("feed 生成失败")
    })?;
    let feed = Arc::new(feed);
    crate::cache::set_feed(feed.clone()).await;
    Ok(feed)
}

async fn serve(headers: HeaderMap, json: bool) -> Response {
    match load_feed(&headers).await {
        Ok(feed) => {
            let (document, content_type) = if json {
                (&feed.json, JSON_CONTENT_TYPE)
            } else {
                (&feed.rss, RSS_CONTENT_TYPE)
            };
            document.response(content_type, &headers, compression::algorithms())
        }
        Err(_) => {
            // 失败细节已由 AppError 构造器或上面的日志记录完整链条。
            tracing::error!("feed 生成失败");
            (StatusCode::INTERNAL_SERVER_ERROR, "feed unavailable").into_response()
        }
    }
}

/// `GET /feed.xml` — RSS 2.0 订阅源。
pub async fn rss_feed(headers: HeaderMap) -> Response {
    serve(headers, false).await
}

/// `GET /feed.json` — JSON Feed 1.1 订阅源。
pub async fn json_feed(headers: HeaderMap) -> Response {
    serve(headers, true).await
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
    fn render_rss_escapes_apostrophe_as_numeric_reference() {
        let mut item = fixture_item();
        item.title = "It's <ok>".to_string();
        let xml = render_rss("https://example.com", fixed_now(), &[item]);
        assert!(xml.contains("<title>It&#x27;s &lt;ok&gt;</title>"));
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

    #[test]
    fn strip_code_highlight_keeps_code_text_and_other_markup() {
        let html = concat!(
            r#"<p>前文 <span class="katex">x</span></p>"#,
            r#"<pre><code class="language-rust"><span class="source rust">"#,
            r#"<span class="storage type">fn</span> a&lt;T&gt;() {}"#,
            "\n</span></code></pre>",
            r#"<p><span>后文</span></p><pre>plain</pre>"#,
        );
        assert_eq!(
            strip_code_highlight(html),
            concat!(
                r#"<p>前文 <span class="katex">x</span></p>"#,
                r#"<pre><code class="language-rust">fn a&lt;T&gt;() {}"#,
                "\n</code></pre>",
                r#"<p><span>后文</span></p><pre>plain</pre>"#,
            )
        );
        // 没有可删的标记时不重新分配；`<prefix>` 之类不是代码块。
        for untouched in [
            "<p><span>无代码</span></p>",
            "<pre>plain</pre>",
            "<prefix><span>x</span></prefix>",
            "<pre><span>未闭合",
        ] {
            assert!(
                matches!(strip_code_highlight(untouched), Cow::Borrowed(_)),
                "{untouched}"
            );
        }
    }

    fn accept(encoding: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static(encoding));
        headers
    }

    #[tokio::test]
    async fn built_feed_serves_precompressed_variants_and_304() {
        let mut item = fixture_item();
        item.content_html = Some("<p>正文</p>".repeat(200));
        let algorithms = compression::parse_compression_algorithms("br, gzip");
        assert!(algorithms.is_some());
        let feed = build_feed("https://example.com".into(), &[item.clone()], algorithms).unwrap();
        assert!(std::str::from_utf8(&feed.rss.body)
            .unwrap()
            .contains("<lastBuildDate>Fri, 2 Jan 2026 03:04:05 +0000</lastBuildDate>"));

        for (document, content_type) in [
            (&feed.rss, RSS_CONTENT_TYPE),
            (&feed.json, JSON_CONTENT_TYPE),
        ] {
            for (name, encoding) in [("br", Encoding::Brotli), ("gzip", Encoding::Gzip)] {
                let response = document.response(content_type, &accept(name), algorithms);
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(response.headers()[header::CONTENT_TYPE], content_type);
                assert_eq!(response.headers()[header::CONTENT_ENCODING], name);
                assert_eq!(response.headers()[header::VARY], "accept-encoding");
                assert_eq!(
                    response.headers()[header::CACHE_CONTROL],
                    FEED_CACHE_CONTROL
                );
                let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                assert!(body.len() < document.body.len());
                assert_eq!(compression::decode(encoding, &body), document.body);
            }
            // 没有可用变体（identity、未启用压缩）时原文交给外层。
            let identity = document.response(content_type, &accept("identity"), algorithms);
            assert!(!identity.headers().contains_key(header::CONTENT_ENCODING));
            assert_eq!(identity.headers()[header::VARY], "accept-encoding");
            let off = document.response(content_type, &accept("br"), None);
            assert!(!off.headers().contains_key(header::CONTENT_ENCODING));
            assert!(!off.headers().contains_key(header::VARY));

            let mut conditional = accept("br");
            conditional.insert(header::IF_NONE_MATCH, document.etag.clone());
            let response = document.response(content_type, &conditional, algorithms);
            assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
            assert!(!response.headers().contains_key(header::CONTENT_ENCODING));
            assert_eq!(response.headers()[header::ETAG], document.etag);
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            assert!(body.is_empty());
        }

        // 内容不变则重建后的 ETag 不变；内容变了 ETag 跟着变。
        let again = build_feed("https://example.com".into(), &[item.clone()], None).unwrap();
        assert_eq!(again.rss.etag, feed.rss.etag);
        assert_eq!(again.json.etag, feed.json.etag);
        assert!(again.rss.encoded.is_empty());
        item.title = "改了标题".into();
        let changed = build_feed("https://example.com".into(), &[item], None).unwrap();
        assert_ne!(changed.rss.etag, feed.rss.etag);
        assert_ne!(changed.json.etag, feed.json.etag);
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
