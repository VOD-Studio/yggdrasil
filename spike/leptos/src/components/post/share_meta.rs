//! 文章页分享元信息：标题、摘要、文章分享图与发布时间、标签。
//!
//! 移植主 crate `components/share_meta.rs` 的 `PostShareMeta`；站点信息与公共
//! Open Graph 标签复用 [`crate::components::share_meta`] 的 `use_share_context` /
//! `ShareTags`。分享图文字与版本号（主 crate `models/share.rs` 的 `PostShareCard`）
//! 在此私有移植，保证 `og:image` 地址与主站服务端生成的图片地址一致。

use leptos::prelude::*;
use leptos_meta::Meta;

use crate::components::share_meta::{use_share_context, ShareTags};
use crate::models::post::Post;

/// 文章页：标题、摘要、文章分享图与发布时间、标签。
#[component]
pub fn PostShareMeta(post: Post) -> impl IntoView {
    let ctx = use_share_context();
    let card = PostShareCard::from_post(&post);
    let description = if card.summary.is_empty() {
        ctx.description.clone()
    } else {
        card.summary.clone()
    };
    let published = post.published_at.unwrap_or(post.created_at).to_rfc3339();
    let url = format!("{}/post/{}", ctx.base_url, post.slug);
    let image = format!("{}{}", ctx.base_url, card.image_path(&post.slug));
    let tags = post
        .tags
        .iter()
        .map(|tag| view! { <Meta property="article:tag" content=tag.clone()/> })
        .collect_view();

    view! {
        <ShareTags
            kind="article"
            title=card.title.clone()
            description=description
            url=url
            image=image
            image_alt=card.title.clone()
        />
        <Meta property="article:published_time" content=published/>
        {tags}
    }
}

/// 文章分享图上的文字。
struct PostShareCard {
    title: String,
    /// 为空时不显示摘要。
    summary: String,
    /// 「日期·阅读时长·首个标签」。
    meta: String,
}

/// 分享图上的分隔点。中文字体的「·」是全宽字形，自带两侧留白，不再加空格。
const SEPARATOR: &str = "·";

impl PostShareCard {
    fn from_post(post: &Post) -> Self {
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

    fn version(&self) -> String {
        version_of(&[&self.title, &self.summary, &self.meta])
    }

    /// 站内图片地址，形如 `/og/post/{slug}.png?v=…`。
    fn image_path(&self, slug: &str) -> String {
        format!("/og/post/{slug}.png?v={}", self.version())
    }
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
