//! 404 页面模块。
//!
//! 路由器 `fallback` 与文章页（slug 不存在）都渲染本组件。
//! 一句话说明、写错的地址、返回首页 / 搜索，以及按地址里的词匹配到的「也许你在找」。

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::hooks::{use_location, use_navigate};
use serde::{Deserialize, Serialize};

use crate::components::ui::{Icon, IconName};
use crate::error::AppError;

/// 404 页面组件。
///
/// 展示 404 标记、说明、当前地址、返回首页与搜索，以及推荐文章。
/// SSR 时把响应状态码设为 404。
///
/// Leptos 版没有 Dioxus 的错误边界冒泡：文章页对不存在的 slug 直接渲染本组件，
/// 导航后路由切换即替换页面，无需在跳转前清除错误。
#[component]
pub fn NotFound(#[prop(optional)] segments: Vec<String>) -> impl IntoView {
    let _ = segments;

    // SSR 时提交 404 状态码。
    AppError::NotFound.commit_status();
    // 文章页内渲染时路径仍是原页面（如 /post/xxx），地址同样取当前路径。
    let path = use_location().pathname;
    let navigate = use_navigate();

    view! {
        <Title text="页面不存在 · Yggdrasil"/>
        <div class="not-found page-enter">
            <p class="not-found-code tabular-nums">"404"</p>
            <h1 class="not-found-title">"这个页面不存在"</h1>
            <p class="not-found-description">"地址可能写错了，或者页面已经移走。"</p>
            <p class="not-found-path">
                <code title=move || path.get()>{move || path.get()}</code>
            </p>
            <div class="not-found-actions">
                <button
                    type="button"
                    class="btn btn-primary btn-lg"
                    on:click=move |_| navigate("/", Default::default())
                >
                    "返回首页"
                </button>
                // 搜索面板不在原型范围内，链接到搜索页。
                <a class="btn btn-secondary btn-lg" href="/search">
                    <Icon name=IconName::Search size=16/>
                    "搜索"
                </a>
            </div>
            <Recommendations path=path/>
        </div>
    }
}

/// 「也许你在找」：无命中或请求失败时整块不显示。
///
/// 只在浏览器里请求（Effect 不在 SSR 中执行），SSR 与水合首帧都渲染为空。
/// 地址变化时重新请求，只采用与当前地址一致的响应。
#[component]
fn Recommendations(#[prop(into)] path: Signal<String>) -> impl IntoView {
    let result = RwSignal::new(None::<PathRecommendations>);
    Effect::new(move |_| {
        let requested = path.get();
        result.set(None);
        spawn_local(async move {
            if let Ok(found) = recommend_for_path(requested.clone()).await {
                if path.try_get_untracked().as_deref() == Some(requested.as_str()) {
                    let _ = result.try_set(Some(found));
                }
            }
        });
    });

    move || {
        let Some(found) = result.get() else {
            return ().into_any();
        };
        if found.posts.is_empty() {
            return ().into_any();
        }
        let words = found.words.join("」「");
        view! {
            <section class="not-found-maybe" aria-labelledby="not-found-maybe-title">
                <h2 id="not-found-maybe-title" class="section-label">"也许你在找"</h2>
                <ul>
                    {found
                        .posts
                        .into_iter()
                        .map(|post| {
                            view! {
                                <li>
                                    <a class="not-found-row" href=format!("/post/{}", post.slug)>
                                        <span>{post.title}</span>
                                        {post.published_at.map(|at| view! {
                                            <time class="tabular-nums">
                                                {at.format("%Y.%m.%d").to_string()}
                                            </time>
                                        })}
                                    </a>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
                <p class="not-found-note">{format!("按地址里的词「{words}」匹配到的文章。")}</p>
            </section>
        }
        .into_any()
    }
}

// ============================================================================
// 404 推荐（移植自主 crate `api::posts::suggest`）
// ============================================================================

/// 一条文章建议：只含标题、摘要、发布时间与标签。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostSuggestion {
    /// URL slug。
    pub slug: String,
    /// 文章标题。
    pub title: String,
    /// 摘要，可选。
    pub summary: Option<String>,
    /// 发布时间。
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 标签名（按名称排序）。
    pub tags: Vec<String>,
}

/// 404 页「也许你在找」：从地址里拆出的词。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathRecommendations {
    /// 参与匹配的词（去掉了路由前缀、数字与过短的片段）。
    pub words: Vec<String>,
    pub posts: Vec<PostSuggestion>,
}

/// 404 推荐最多列出的文章数。
#[cfg(feature = "ssr")]
const PATH_RECOMMEND_LIMIT: i64 = 3;

/// 地址里不代表内容的段：路由前缀与常见文件名。
#[cfg(any(feature = "ssr", test))]
const ROUTE_WORDS: [&str; 10] = [
    "post", "posts", "page", "tags", "tag", "notes", "book", "about", "index", "html",
];

/// 把地址拆成用于匹配的词：按 `/ - _ .` 与空白切分，解码百分号编码，
/// 去掉路由前缀、纯数字与不足两个字符的片段，最多保留 5 个、去重。
#[cfg(any(feature = "ssr", test))]
fn path_words(path: &str) -> Vec<String> {
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let decoded = percent_decode(path);
    let mut words: Vec<String> = Vec::new();
    for word in decoded.split(|c: char| "/-_.".contains(c) || c.is_whitespace()) {
        let word = word.trim().to_lowercase();
        if word.chars().count() < 2
            || word.chars().all(|c| c.is_ascii_digit())
            || ROUTE_WORDS.contains(&word.as_str())
            || words.contains(&word)
        {
            continue;
        }
        words.push(word);
        if words.len() == 5 {
            break;
        }
    }
    words
}

/// 解码 `%XX`；不是合法 UTF-8 时退回原文。
#[cfg(any(feature = "ssr", test))]
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(byte) = text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// 转义 LIKE 模式里的通配符，`ESCAPE '\'` 下按字面匹配。
#[cfg(feature = "ssr")]
fn escape_like_pattern(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 按地址里的词匹配已发布文章的标题与 slug，命中词多的排前，最多 3 篇。
/// 地址里没有可用的词时不查询。公开接口，结果不缓存（只在 404 时调用）。
#[server(endpoint = "not_found/recommend_for_path")]
pub async fn recommend_for_path(path: String) -> Result<PathRecommendations, AppError> {
    use crate::server::db::{get_conn, query_err};

    let words = path_words(&path);
    if words.is_empty() {
        return Ok(PathRecommendations {
            words,
            posts: Vec::new(),
        });
    }
    let patterns: Vec<String> = words
        .iter()
        .map(|w| format!("%{}%", escape_like_pattern(w)))
        .collect();
    let client = get_conn().await?;
    let rows = client
        .query(
            "SELECT p.slug, p.title, p.summary, p.published_at,
                    COALESCE(array_agg(t.name ORDER BY t.name) FILTER (WHERE t.name IS NOT NULL), '{}') AS tags,
                    (SELECT COUNT(*) FROM unnest($1::text[]) w
                     WHERE p.title ILIKE w ESCAPE '\\' OR p.slug ILIKE w ESCAPE '\\') AS hits
             FROM posts p
             LEFT JOIN post_tags pt ON p.id = pt.post_id
             LEFT JOIN tags t ON pt.tag_id = t.id
             WHERE p.status = 'published' AND p.deleted_at IS NULL
               AND (p.title ILIKE ANY($1) OR p.slug ILIKE ANY($1))
             GROUP BY p.id
             ORDER BY hits DESC, p.published_at DESC NULLS LAST
             LIMIT $2",
            &[&patterns, &PATH_RECOMMEND_LIMIT],
        )
        .await
        .map_err(query_err)?;
    let posts = rows
        .iter()
        .map(|r| PostSuggestion {
            slug: r.get("slug"),
            title: r.get("title"),
            summary: r.get("summary"),
            published_at: r.get("published_at"),
            tags: r.get("tags"),
        })
        .collect();
    Ok(PathRecommendations { words, posts })
}

#[cfg(test)]
mod path_tests {
    use super::path_words;

    #[test]
    fn path_words_drop_route_prefixes_numbers_and_short_bits() {
        assert_eq!(path_words("/post/hesper-apple-1"), ["hesper", "apple"]);
        assert_eq!(path_words("/page/2"), Vec::<String>::new());
        assert_eq!(path_words("/tags/Rust?x=1"), ["rust"]);
    }
}
