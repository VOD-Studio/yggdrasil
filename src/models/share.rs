//! 分享图的展示内容与地址版本号，服务端渲染图片与前台页面 `og:image` 共用。
//!
//! 图片地址带 `?v=`：内容变化时版本号随之改变，社交平台与 CDN 不会继续用旧图。
//! 版本号按展示文字计算，两端用同一实现，结果一致。

use super::post::Post;

/// 写分享元信息所需的站点信息，由中间件放进请求扩展，前台页面服务端渲染时读取。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShareContext {
    /// APP_BASE_URL，无尾斜杠；未配置时为空，地址退回站内相对路径。
    pub base_url: String,
    /// 站点简介，页面没有自己的描述时使用。
    pub description: String,
    /// 站点分享图的站内地址（带版本号）。
    pub site_image: String,
}

/// 文章分享图上的文字。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PostShareCard {
    pub title: String,
    /// 为空时不显示摘要。
    pub summary: String,
    /// 「日期·阅读时长·首个标签」。
    pub meta: String,
}

impl PostShareCard {
    pub fn from_post(post: &Post) -> Self {
        let at = post.published_at.unwrap_or(post.created_at);
        let mut meta = vec![at.format("%Y.%m.%d").to_string()];
        if post.reading_time > 0 {
            meta.push(format!("{} 分钟", post.reading_time));
        }
        if let Some(tag) = post.tags.first() {
            meta.push(tag.clone());
        }
        Self {
            title: collapse_whitespace(&post.title),
            summary: post
                .summary
                .as_deref()
                .map(collapse_whitespace)
                .unwrap_or_default(),
            meta: meta.join(SEPARATOR),
        }
    }

    pub fn version(&self) -> String {
        version_of(&[&self.title, &self.summary, &self.meta])
    }

    /// 站内图片地址，形如 `/og/post/{slug}.png?v=…`。
    pub fn image_path(&self, slug: &str) -> String {
        format!("/og/post/{slug}.png?v={}", self.version())
    }
}

/// 分享图上的分隔点。中文字体的「·」是全宽字形，自带两侧留白，不再加空格。
pub const SEPARATOR: &str = "·";

/// 站点分享图地址；标语变化时版本号改变。
#[cfg(any(feature = "server", test))]
pub fn site_image_path(title: &str, subtitle: &str) -> String {
    format!("/og/site.png?v={}", version_of(&[title, subtitle]))
}

/// FNV-1a 64 位，取低 32 位作 8 位十六进制版本号。
fn version_of(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.bytes().chain(std::iter::once(0)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{:08x}", hash as u32)
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::post::PostStatus;
    use chrono::{TimeZone, Utc};

    fn post() -> Post {
        let at = Utc.with_ymd_and_hms(2026, 9, 16, 8, 0, 0).unwrap();
        Post {
            id: 1,
            author_id: 1,
            title: "让时间成为\n电路的一部分".into(),
            slug: "hesper".into(),
            summary: Some("  从一个字符\n如何抵达屏幕出发。 ".into()),
            content_md: String::new(),
            content_html: None,
            status: PostStatus::Published,
            published_at: Some(at),
            created_at: at,
            updated_at: at,
            deleted_at: None,
            tags: vec!["源码剖析".into(), "Rust".into()],
            cover_image: None,
            reading_time: 19,
            word_count: 4866,
            toc_html: None,
            prev_post: None,
            next_post: None,
        }
    }

    #[test]
    fn card_uses_date_reading_time_and_first_tag() {
        let card = PostShareCard::from_post(&post());
        assert_eq!(card.title, "让时间成为 电路的一部分");
        assert_eq!(card.summary, "从一个字符 如何抵达屏幕出发。");
        assert_eq!(card.meta, "2026.09.16·19 分钟·源码剖析");
    }

    #[test]
    fn card_omits_missing_reading_time_and_tags() {
        let mut post = post();
        post.reading_time = 0;
        post.tags.clear();
        post.summary = None;
        let card = PostShareCard::from_post(&post);
        assert_eq!(card.meta, "2026.09.16");
        assert!(card.summary.is_empty());
    }

    #[test]
    fn image_path_changes_with_shown_text_only() {
        let base = post();
        let path = PostShareCard::from_post(&base).image_path("hesper");
        assert!(path.starts_with("/og/post/hesper.png?v="));

        let mut edited = base.clone();
        edited.content_md = "正文不在图上".into();
        edited.word_count = 1;
        assert_eq!(PostShareCard::from_post(&edited).image_path("hesper"), path);

        edited.title = "新标题".into();
        assert_ne!(PostShareCard::from_post(&edited).image_path("hesper"), path);
    }

    #[test]
    fn version_separates_fields() {
        assert_ne!(version_of(&["ab", "c"]), version_of(&["a", "bc"]));
        assert_eq!(
            site_image_path("慢慢读源码，", "认真写长文。").len(),
            "/og/site.png?v=".len() + 8
        );
    }
}
