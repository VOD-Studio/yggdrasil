//! 阅读页传输模型。编辑器和 MCP 继续使用完整 Post，避免向每位读者重复发送 Markdown。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::post::{Post, PostNav, PostStatus};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublicPost {
    pub id: i32,
    pub author_id: i32,
    pub title: String,
    pub slug: String,
    pub summary: Option<String>,
    pub content_html: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<String>,
    pub cover_image: Option<String>,
    pub reading_time: u32,
    pub word_count: u32,
    pub toc_html: Option<String>,
    pub prev_post: Option<PostNav>,
    pub next_post: Option<PostNav>,
}

impl From<Post> for PublicPost {
    fn from(post: Post) -> Self {
        Self {
            id: post.id,
            author_id: post.author_id,
            title: post.title,
            slug: post.slug,
            summary: post.summary,
            content_html: post.content_html,
            published_at: post.published_at,
            created_at: post.created_at,
            updated_at: post.updated_at,
            tags: post.tags,
            cover_image: post.cover_image,
            reading_time: post.reading_time,
            word_count: post.word_count,
            toc_html: post.toc_html,
            prev_post: post.prev_post,
            next_post: post.next_post,
        }
    }
}

impl PublicPost {
    /// 复用文章展示组件；此值只用于展示，不能作为编辑器的数据源。
    pub fn into_display_post(self) -> Post {
        Post {
            id: self.id,
            author_id: self.author_id,
            title: self.title,
            slug: self.slug,
            summary: self.summary,
            content_md: String::new(),
            content_html: self.content_html,
            status: PostStatus::Published,
            published_at: self.published_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: None,
            tags: self.tags,
            cover_image: self.cover_image,
            reading_time: self.reading_time,
            word_count: self.word_count,
            toc_html: self.toc_html,
            prev_post: self.prev_post,
            next_post: self.next_post,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_payload_omits_markdown_and_preserves_rendered_content() {
        let now = Utc::now();
        let full = Post {
            id: 42,
            author_id: 7,
            title: "文章".into(),
            slug: "post".into(),
            summary: Some("摘要".into()),
            content_md: "仅编辑需要的原文".into(),
            content_html: Some("<p>正文</p>".into()),
            status: PostStatus::Published,
            published_at: Some(now),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            tags: vec!["Rust".into()],
            cover_image: None,
            reading_time: 2,
            word_count: 300,
            toc_html: Some("<nav>目录</nav>".into()),
            prev_post: Some(PostNav {
                title: "上一篇".into(),
                slug: "prev".into(),
            }),
            next_post: None,
        };
        let public = PublicPost::from(full.clone());
        let payload = serde_json::to_value(&public).unwrap();
        assert!(payload.get("content_md").is_none());
        assert_eq!(
            serde_json::to_value(&full).unwrap()["content_md"],
            full.content_md
        );
        let display = serde_json::from_value::<PublicPost>(payload)
            .unwrap()
            .into_display_post();
        assert_eq!(display.content_html, full.content_html);
        assert_eq!(display.toc_html, full.toc_html);
        assert_eq!(display.prev_post, full.prev_post);
        assert_eq!(display.author_id, full.author_id);
        assert_eq!(display.updated_at, full.updated_at);
    }
}
