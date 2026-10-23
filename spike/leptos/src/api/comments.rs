//! 评论接口：前台评论区使用的读取、发表与待审核状态查询。
//!
//! 移植自主 crate `src/api/comments/{read,create,check,helpers,markdown,types}.rs`。
//! server fn 统一注册在 `/api/comments/...`；数据库操作仅在 `ssr` 构建中执行。
//! 与主 crate 的差异：无评论列表缓存、无限流；Markdown 渲染不做 syntect 高亮与
//! KaTeX，原始 HTML 一律转义（替代 lol_html 白名单清洗）。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::models::comment::PublicComment;

/// HTML 转义（与主 crate `utils::html::escape_html` 同口径），供服务端渲染与校验共用。
pub use crate::utils::comment_storage::escape_html;

// ============================================================================
// 请求与响应数据结构
// ============================================================================

/// 创建/审核/删除评论的统一响应结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentResponse {
    /// 操作是否成功。
    pub success: bool,
    /// 提示信息。
    pub message: String,
    /// 错误码，成功时为 None。
    pub error_code: Option<String>,
    /// 新评论 id。
    #[serde(default)]
    pub comment_id: Option<i64>,
    /// 评论者头像 URL。
    #[serde(default)]
    pub avatar_url: Option<String>,
    /// 评论嵌套深度。
    #[serde(default)]
    pub depth: Option<i32>,
}

/// 这些构造器只在 server fn body 内调用，WASM 端因 cfg gate 剥离了调用点，
/// 故对非 ssr 构建允许 dead_code。
#[cfg_attr(not(feature = "ssr"), allow(dead_code))]
impl CommentResponse {
    /// 构造失败响应，携带错误码（comment_id / avatar_url / depth 均为 None）。
    pub fn error(error_code: &str, message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            error_code: Some(error_code.into()),
            comment_id: None,
            avatar_url: None,
            depth: None,
        }
    }

    /// 构造成功响应（无关联数据，用于审核/删除等操作）。
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            error_code: None,
            comment_id: None,
            avatar_url: None,
            depth: None,
        }
    }

    /// 构造成功响应，携带新评论 id / 头像 / 深度（用于创建评论）。
    pub fn created(
        message: impl Into<String>,
        comment_id: i64,
        avatar_url: impl Into<String>,
        depth: i32,
    ) -> Self {
        Self {
            success: true,
            message: message.into(),
            error_code: None,
            comment_id: Some(comment_id),
            avatar_url: Some(avatar_url.into()),
            depth: Some(depth),
        }
    }
}

/// 评论树响应：包含文章下的全部已审核评论。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommentTreeResponse {
    /// 评论列表。
    pub comments: Vec<PublicComment>,
    /// 评论总数。
    pub count: i64,
}

/// 单个评论的待审核状态结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingStatusItem {
    /// 评论 id。
    pub id: i64,
    /// 当前状态：approved / pending / rejected / spam / trash / gone。
    pub status: String,
}

// ============================================================================
// 服务端辅助函数（数据转换、校验、哈希、头像、Markdown 渲染）
// ============================================================================

/// 计算字符串的 MD5 哈希，用于 Gravatar。
#[cfg(feature = "ssr")]
pub fn md5_hash(input: &str) -> String {
    use md5::Digest;
    let hash = md5::Md5::digest(input.as_bytes());
    hex::encode(hash)
}

/// 根据邮箱生成 Cravatar（Gravatar 国内镜像）头像 URL。
#[cfg(feature = "ssr")]
pub fn gravatar_url(email: &str) -> String {
    let hash = md5_hash(&email.trim().to_lowercase());
    format!("https://cravatar.cn/avatar/{}?d=mp&s=80", hash)
}

/// 解析评论的对外展示身份：`(名称, 头像 URL, 是否作者)`。
///
/// `user_id` 非空（登录用户发表的评论）时，优先使用 JOIN users 得到的
/// **实时**显示名与头像——改名/换头像后历史评论自动跟随；匿名评论
/// （`user_id` 为 NULL）回退到评论行内快照名 + 邮箱 Gravatar。
/// 返回的名称经 HTML 转义，与 `author_name` 列的存储口径一致。
#[cfg(feature = "ssr")]
pub fn resolve_author_display(
    author_name: &str,
    author_email: &str,
    user_id: Option<i32>,
    user_display_name: Option<&str>,
    user_avatar_url: Option<&str>,
) -> (String, String, bool) {
    let is_author = user_id.is_some();
    let name = user_display_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(escape_html)
        .unwrap_or_else(|| author_name.to_string());
    let avatar = user_avatar_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| gravatar_url(author_email));
    (name, avatar, is_author)
}

/// 将数据库行转换为前端展示的公开评论结构。
///
/// 调用方查询须 LEFT JOIN users 并提供 `user_id` / `user_display_name` /
/// `user_avatar` 三列（匿名评论为 NULL）。
#[cfg(feature = "ssr")]
fn row_to_public_comment(row: &tokio_postgres::Row) -> PublicComment {
    let email: String = row.get("author_email");
    let created_at_dt: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let created_at_iso = created_at_dt.to_rfc3339();
    let created_at_relative = format_relative_time(created_at_dt);

    let snapshot_name: String = row.get("author_name");
    let user_id: Option<i32> = row.get("user_id");
    let user_display_name: Option<String> = row.get("user_display_name");
    let user_avatar: Option<String> = row.get("user_avatar");
    let (author_name, avatar_url, is_author) = resolve_author_display(
        &snapshot_name,
        &email,
        user_id,
        user_display_name.as_deref(),
        user_avatar.as_deref(),
    );

    PublicComment {
        id: row.get("id"),
        parent_id: row.get("parent_id"),
        depth: row.get("depth"),
        author_name,
        author_url: row.get("author_url"),
        avatar_url,
        is_author,
        content_html: row.get("content_html"),
        created_at: created_at_relative,
        created_at_iso,
    }
}

/// 将 UTC 时间格式化为相对时间（刚刚 / N 分钟前 / N 小时前 / N 天前 / 日期）。
///
/// 分档规则与前端 `crate::utils::time::relative_label_from_millis` 完全一致。
#[cfg(feature = "ssr")]
fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let delta_millis = now.signed_duration_since(dt).num_milliseconds();
    let iso = dt.to_rfc3339();
    crate::utils::time::relative_label_from_millis(delta_millis, &iso).0
}

/// 校验评论作者昵称：非空且不超过 50 字符。
#[cfg(feature = "ssr")]
fn validate_comment_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("请输入昵称".to_string());
    }
    if trimmed.chars().count() > 50 {
        return Err("昵称长度不能超过 50 个字符".to_string());
    }
    Ok(())
}

/// 与主 crate `EMAIL_REGEX`（`^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$`）
/// 等价的手写匹配，免去 regex 依赖。
///
/// 两侧字符集都不含 `@`，故必须恰好一个 `@`；域名的 TLD 段只含字母，
/// 因此只可能匹配最后一个 `.`。
#[cfg(feature = "ssr")]
fn is_valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    let local_ok = !local.is_empty()
        && local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-'));
    if !local_ok {
        return false;
    }
    if !domain
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
    {
        return false;
    }
    let Some((host, tld)) = domain.rsplit_once('.') else {
        return false;
    };
    !host.is_empty() && tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
}

/// 校验评论作者邮箱格式。
#[cfg(feature = "ssr")]
fn validate_comment_email(email: &str) -> Result<(), String> {
    if !is_valid_email(email.trim()) {
        return Err("邮箱格式不正确".to_string());
    }
    Ok(())
}

/// 校验评论作者网址：为空时允许，非空时必须以 http:// 或 https:// 开头、
/// 不含 HTML 特殊字符与空白，且不超过 200 字符。
#[cfg(feature = "ssr")]
fn validate_comment_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err("网址必须以 http:// 或 https:// 开头".to_string());
    }
    if trimmed.len() > 200 {
        return Err("网址长度不能超过 200 个字符".to_string());
    }
    if trimmed
        .chars()
        .any(|c| matches!(c, '<' | '>' | '"' | '\'' | '&' | ' ' | '\t' | '\n' | '\r'))
    {
        return Err("网址包含非法字符".to_string());
    }
    Ok(())
}

/// 校验评论内容：非空且不超过 10000 字符。
#[cfg(feature = "ssr")]
fn validate_comment_content(content: &str) -> Result<(), String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("请输入评论内容".to_string());
    }
    if trimmed.chars().count() > 10000 {
        return Err("评论内容不能超过 10000 个字符".to_string());
    }
    Ok(())
}

/// 校验蜜罐字段：正常用户蜜罐为空，机器人可能填入任意内容。
#[cfg(feature = "ssr")]
fn validate_comment_honeypot(value: &str) -> Result<(), String> {
    if value.is_empty() {
        Ok(())
    } else {
        Err("评论提交异常".to_string())
    }
}

/// 计算评论内容哈希，用于检测短时间内的重复提交。
#[cfg(feature = "ssr")]
fn compute_content_hash(post_id: i32, parent_id: Option<i64>, name: &str, content: &str) -> String {
    use sha2::{Digest, Sha256};
    let input = format!(
        "{}:{}:{}:{}",
        post_id,
        parent_id.map(|id| id.to_string()).unwrap_or_default(),
        name.trim(),
        content.trim()
    );
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// 评论链接/图片 URL 是否安全：放行 http(s)、mailto 与站内相对路径，
/// 拒绝 `javascript:`、`data:` 等其它协议（与主 crate sanitizer 的 is_safe_url 同向）。
#[cfg(feature = "ssr")]
fn is_safe_comment_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
    {
        return true;
    }
    // 无协议的相对地址：第一个 `/`、`?`、`#` 之前不得出现 `:`。
    let head_end = lower.find(['/', '?', '#']).unwrap_or(lower.len());
    !lower[..head_end].contains(':')
}

/// 将评论 Markdown 渲染为安全的 HTML。
///
/// 支持表格与删除线；标题统一渲染为 `<strong>` 以避免层级混乱；代码块转义输出。
/// 原型差异：不做 syntect 高亮与 KaTeX 渲染（公式按 TeX 源文本展示）；原始 HTML
/// 不走白名单清洗而是整体转义；链接统一加 `rel="nofollow noopener"`，危险 URL 剥离
/// `href`/整张图片，效果与主 crate `clean_comment_html` 一致。
#[cfg(feature = "ssr")]
fn render_comment_markdown(md: &str) -> String {
    use pulldown_cmark::{CowStr, Event, Options, Tag, TagEnd};

    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_MATH;
    let parser = pulldown_cmark::Parser::new_ext(md, opts);

    let mut events: Vec<Event> = Vec::new();
    let mut in_codeblock = false;
    let mut code_buffer = String::new();
    // 被剥离的图片：其 alt 文本事件照常输出为纯文本。
    let mut dropped_images: Vec<bool> = Vec::new();

    for event in parser {
        match event {
            Event::InlineMath(tex) => {
                events.push(Event::Text(CowStr::from(format!("${tex}$"))));
            }
            Event::DisplayMath(tex) => {
                events.push(Event::Text(CowStr::from(format!("$${tex}$$"))));
            }
            Event::Html(raw) | Event::InlineHtml(raw) if !in_codeblock => {
                // 原始 HTML 一律按文本转义输出。
                events.push(Event::Text(raw));
            }
            Event::Start(Tag::Heading { .. }) => {
                // 评论中不保留标题层级，统一加粗。
                events.push(Event::Start(Tag::Strong));
            }
            Event::End(TagEnd::Heading(_)) => {
                events.push(Event::End(TagEnd::Strong));
            }
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                let mut html = String::from("<a");
                if is_safe_comment_url(&dest_url) {
                    html.push_str(&format!(" href=\"{}\"", escape_html(dest_url.trim())));
                }
                if !title.is_empty() {
                    html.push_str(&format!(" title=\"{}\"", escape_html(&title)));
                }
                html.push_str(" rel=\"nofollow noopener\">");
                events.push(Event::InlineHtml(CowStr::from(html)));
            }
            Event::End(TagEnd::Link) => {
                events.push(Event::InlineHtml(CowStr::from("</a>")));
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                if is_safe_comment_url(&dest_url) {
                    dropped_images.push(false);
                    events.push(Event::Start(Tag::Image {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }));
                } else {
                    dropped_images.push(true);
                }
            }
            Event::End(TagEnd::Image) => {
                if !dropped_images.pop().unwrap_or(true) {
                    events.push(Event::End(TagEnd::Image));
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                // 语言信息仅用于高亮，原型不做高亮，故丢弃。
                in_codeblock = true;
                code_buffer.clear();
            }
            Event::Text(text) if in_codeblock => {
                code_buffer.push_str(&text);
            }
            Event::End(TagEnd::CodeBlock) => {
                let html = format!("<pre><code>{}</code></pre>", escape_html(&code_buffer));
                events.push(Event::Html(CowStr::from(html)));
                in_codeblock = false;
            }
            _ if !in_codeblock => {
                events.push(event);
            }
            _ => {}
        }
    }

    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events.into_iter());
    html
}

/// 从请求头取客户端 IP：优先 `X-Real-IP`，其次 `X-Forwarded-For` 首项。
///
/// 原型不移植主 crate 的受信代理层数配置；取不到时为 None。
#[cfg(feature = "ssr")]
fn client_ip(headers: &http::HeaderMap) -> Option<String> {
    let from = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    from("x-real-ip")
        .or_else(|| from("x-forwarded-for"))
        .map(|ip| ip.chars().take(45).collect())
}

// ============================================================================
// server fn
// ============================================================================

/// 获取指定文章的已审核评论列表。
///
/// 按 id 升序返回，便于前端构建嵌套树（原型不做按文章的评论缓存）。
#[server(endpoint = "comments/list_by_post")]
pub async fn get_comments(post_id: i32) -> Result<CommentTreeResponse, AppError> {
    use crate::server::db::{get_conn, query_err};

    let client = get_conn().await?;

    let rows = client
        .query(
            "SELECT c.id, c.parent_id, c.depth, c.author_name, c.author_email, c.author_url, c.content_html, c.created_at, \
                    c.user_id, u.display_name AS user_display_name, u.avatar_url AS user_avatar \
             FROM comments c \
             LEFT JOIN users u ON c.user_id = u.id \
             WHERE c.post_id = $1 AND c.status = 'approved' AND c.deleted_at IS NULL \
               AND EXISTS (SELECT 1 FROM posts p WHERE p.id = $1 AND p.status = 'published' AND p.deleted_at IS NULL) \
             ORDER BY c.id ASC \
             LIMIT 200",
            &[&post_id],
        )
        .await
        .map_err(query_err)?;

    let comments: Vec<PublicComment> = rows.iter().map(row_to_public_comment).collect();
    let count = comments.len() as i64;
    Ok(CommentTreeResponse { comments, count })
}

/// 创建一条新评论。
///
/// 对作者昵称、邮箱、网址与内容进行基础校验；
/// 若目标文章未发布或父评论未通过审核，则拒绝提交；
/// 匿名评论置为 pending，登录用户（有效会话）直发 approved，成功后清空 SSR 缓存。
#[server(endpoint = "comments/create", input = leptos::server_fn::codec::Json)]
pub async fn create_comment(
    post_id: i32,
    parent_id: Option<i64>,
    author_name: String,
    author_email: String,
    author_url: Option<String>,
    content_md: String,
    honeypot: String,
) -> Result<CommentResponse, AppError> {
    use crate::server::db::{get_conn, query_err};

    let headers: http::HeaderMap = leptos_axum::extract()
        .await
        .map_err(|e| AppError::internal("extract headers", e))?;

    // 登录用户识别：有效会话 → 以账号身份发表（直发 approved）；
    // 匿名/会话失效 → 原审核流程。
    let session_user = crate::api::auth::current_session_user().await?;

    if session_user.is_none() {
        // 蜜罐字段二次校验：禁用 JS 的机器人可能绕过前端拦截，这里作为服务端防线。
        if let Err(e) = validate_comment_honeypot(&honeypot) {
            return Ok(CommentResponse::error("spam_detected", e));
        }
        // 依次校验昵称、邮箱与网址（登录用户的身份字段取自账号，无需校验）。
        if let Err(e) = validate_comment_name(&author_name) {
            return Ok(CommentResponse::error("invalid_input", e));
        }
        if let Err(e) = validate_comment_email(&author_email) {
            return Ok(CommentResponse::error("invalid_input", e));
        }
        if let Some(url) = &author_url {
            if let Err(e) = validate_comment_url(url) {
                return Ok(CommentResponse::error("invalid_input", e));
            }
        }
    }
    if let Err(e) = validate_comment_content(&content_md) {
        return Ok(CommentResponse::error("invalid_input", e));
    }

    let mut client = get_conn().await?;

    // 确认目标文章存在且处于已发布状态。
    let post_row = client
        .query_opt(
            "SELECT status, deleted_at FROM posts WHERE id = $1",
            &[&post_id],
        )
        .await
        .map_err(query_err)?;

    match post_row {
        None => {
            return Ok(CommentResponse::error("post_not_found", "文章不存在"));
        }
        Some(row) => {
            let status: String = row.get("status");
            let deleted_at: Option<chrono::DateTime<chrono::Utc>> = row.get("deleted_at");
            if status != "published" || deleted_at.is_some() {
                return Ok(CommentResponse::error("post_not_found", "文章不存在"));
            }
        }
    }

    // 若存在父评论，校验其归属文章与审核状态，并计算当前评论的嵌套深度。
    let mut depth: i32 = 0;
    if let Some(pid) = parent_id {
        let parent_row = client
            .query_opt(
                "SELECT post_id, status, depth FROM comments WHERE id = $1 AND deleted_at IS NULL",
                &[&pid],
            )
            .await
            .map_err(query_err)?;

        match parent_row {
            None => {
                return Ok(CommentResponse::error("parent_not_found", "父评论不存在"));
            }
            Some(row) => {
                let parent_post_id: i32 = row.get("post_id");
                let parent_status: String = row.get("status");
                let parent_depth: i32 = row.get("depth");

                if parent_post_id != post_id {
                    return Ok(CommentResponse::error("parent_not_found", "父评论不存在"));
                }
                if parent_status != "approved" {
                    return Ok(CommentResponse::error(
                        "parent_not_approved",
                        "父评论未通过审核",
                    ));
                }

                depth = parent_depth + 1;
                if depth > 20 {
                    return Ok(CommentResponse::error("too_deep", "评论嵌套层级过深"));
                }
            }
        }
    }

    // 身份字段定稿：登录用户取账号值（显示名回退用户名、无个人网址），
    // 匿名取表单值。查重哈希的作者键对用户用稳定 "user:<id>"（改名不影响查重）。
    let (final_name, final_email, final_url, user_id) = match &session_user {
        Some(u) => (
            u.display_name.clone().unwrap_or_else(|| u.username.clone()),
            u.email.clone(),
            None,
            Some(u.id),
        ),
        None => (
            author_name.clone(),
            author_email.clone(),
            author_url.clone(),
            None,
        ),
    };
    let author_key = match user_id {
        Some(id) => format!("user:{id}"),
        None => final_name.clone(),
    };

    // 基于文章、父评论、作者与内容计算哈希，防止短时间重复提交。
    let content_hash = compute_content_hash(post_id, parent_id, &author_key, &content_md);

    // Markdown 渲染移到阻塞线程池执行，避免阻塞 async runtime。
    let md_for_render = content_md.clone();
    let content_html = tokio::task::spawn_blocking(move || render_comment_markdown(&md_for_render))
        .await
        .map_err(|e| AppError::internal("Markdown 渲染任务失败", e))?;
    let author_name_safe = escape_html(final_name.trim());
    let author_url_safe = final_url
        .as_ref()
        .map(|u| escape_html(u.trim()))
        .filter(|u| !u.is_empty());
    let ip_address = client_ip(&headers);
    let user_agent: Option<String> = headers
        .get(http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(500).collect());

    // 查重与插入在同一事务内，并用 advisory lock 串行化相同内容的并发提交：
    // key 取 content_hash 前 16 个 hex 字符（8 字节）解析为 i64。
    let lock_key: i64 = i64::from_str_radix(&content_hash[..16], 16).unwrap_or(0);

    let tx = client.transaction().await.map_err(query_err)?;
    // 事务级 advisory 锁：随事务结束自动释放，无需显式 unlock。
    tx.execute("SELECT pg_advisory_xact_lock($1)", &[&lock_key])
        .await
        .map_err(query_err)?;

    let dup: Option<i64> = tx
        .query_opt(
            "SELECT id FROM comments WHERE post_id = $1 AND content_hash = $2 AND created_at > NOW() - INTERVAL '5 minutes'",
            &[&post_id, &content_hash],
        )
        .await
        .map_err(query_err)?
        .map(|r| r.get(0));

    if dup.is_some() {
        // 重复：回滚（释放 advisory 锁）后返回。
        tx.rollback().await.ok();
        return Ok(CommentResponse::error("duplicate", "请勿重复提交"));
    }

    // 登录用户直发 approved（记录 approved_at）；匿名进入 pending 等待审核。
    let initial_status: &str = if session_user.is_some() {
        "approved"
    } else {
        "pending"
    };
    let approved_at: Option<chrono::DateTime<chrono::Utc>> =
        session_user.is_some().then(chrono::Utc::now);
    let email_trimmed = final_email.trim();

    let row = tx
        .query_one(
            "INSERT INTO comments \
             (post_id, parent_id, depth, author_name, author_email, author_url, \
              content_md, content_html, content_hash, status, ip_address, user_agent, \
              user_id, approved_at) \
              VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
              RETURNING id",
            &[
                &post_id,
                &parent_id,
                &depth,
                &author_name_safe,
                &email_trimmed,
                &author_url_safe,
                &content_md,
                &content_html,
                &content_hash,
                &initial_status,
                &ip_address,
                &user_agent,
                &user_id,
                &approved_at,
            ],
        )
        .await
        .map_err(query_err)?;

    tx.commit().await.map_err(query_err)?;

    let comment_id: i64 = row.get(0);

    // 响应头像：登录用户用账号头像（未设置回退邮箱 Gravatar），匿名用邮箱 Gravatar。
    let avatar_url = match &session_user {
        Some(u) => u
            .avatar_url
            .clone()
            .unwrap_or_else(|| gravatar_url(&u.email)),
        None => gravatar_url(&final_email),
    };

    // 新评论影响评论列表与待审核计数（主 crate 精确失效对应缓存；原型清空整页缓存）。
    crate::server::ssr_cache::invalidate_all();

    let message = if session_user.is_some() {
        "评论已发布"
    } else {
        "评论已提交，等待审核"
    };

    Ok(CommentResponse::created(
        message, comment_id, avatar_url, depth,
    ))
}

/// 查询一组评论的当前审核状态。
///
/// 用于前端轮询刚刚提交的评论是否已通过审核；某 id 不存在时返回状态 `"gone"`。
#[server(endpoint = "comments/check_pending", input = leptos::server_fn::codec::Json)]
pub async fn check_pending_status(ids: Vec<i64>) -> Result<Vec<PendingStatusItem>, AppError> {
    use crate::server::db::{get_conn, query_err};

    // 空列表直接返回空结果，避免无意义的数据库查询。
    if ids.is_empty() {
        return Ok(vec![]);
    }
    // 上限 100 条，防止超大 IN 列表拖慢查询或被滥用枚举评论状态。
    if ids.len() > 100 {
        return Ok(vec![]);
    }

    let client = get_conn().await?;

    let rows = client
        .query(
            "SELECT id, status FROM comments WHERE id = ANY($1)",
            &[&ids],
        )
        .await
        .map_err(query_err)?;

    // 将查询结果收集为 HashMap，便于后续按传入顺序补齐缺失项。
    let found: std::collections::HashMap<i64, String> = rows
        .iter()
        .map(|r| (r.get::<_, i64>(0), r.get::<_, String>(1)))
        .collect();

    let result: Vec<PendingStatusItem> = ids
        .into_iter()
        .map(|id| {
            let status = found
                .get(&id)
                .cloned()
                .unwrap_or_else(|| "gone".to_string());
            PendingStatusItem { id, status }
        })
        .collect();

    Ok(result)
}
