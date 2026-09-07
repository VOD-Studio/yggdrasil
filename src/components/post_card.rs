//! 文章卡片组件
//!
//! 在首页、标签详情等列表中展示单篇文章的标题、摘要、封面、日期与标签。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::components::ui::TagChip;
use crate::models::post::PostListItem;
use crate::router::Route;

/// 文章卡片组件。
///
/// Props：
/// - `post`：文章数据模型
/// - `compact`：首页紧凑文章流；其它列表默认保留普通卡片
///
/// 展示内容包括：
/// - 封面图（如有，按版式请求缩略图，不启用灯箱）
/// - 文章标题
/// - 摘要（最多两行）
/// - 发布日期与标签
///
/// 交互模型（采用覆盖层链接，避免 `<a>` 嵌套 `<a>` 的非法 HTML）：
/// - 整张卡片可点击跳转到文章详情：通过末尾一个绝对定位、铺满卡片的覆盖层 `Link` 实现。
/// - 标签是独立的 `Link`，通过 `relative z-10` 叠在覆盖层之上，并 `stop_propagation`，
///   点击标签进入标签详情页而不触发卡片跳转。
/// - 封面用裸 `.blur-img`（纯展示，无灯箱），点击走卡片跳转，避免交互歧义。
#[component]
pub fn PostCard(post: PostListItem, #[props(default = false)] compact: bool) -> Element {
    let post_slug = post.slug.clone();
    let date_str = post.formatted_date();
    let reading_time = post.reading_time.max(1);
    let tag_variant = if compact { "text" } else { "outline" };
    let thumb_size = if compact { "480x360" } else { "840x360" };
    let (article_class, title_class, summary_class) = if compact {
        (
            "post-card-editorial post-card-compact group relative",
            "post-card-title",
            "post-card-summary line-clamp-2",
        )
    } else {
        (
            "group relative mb-10 flex flex-col bg-paper-entry rounded-card hover:shadow-md transition-shadow overflow-hidden",
            "text-xl sm:text-2xl md:text-3xl font-extrabold tracking-tight leading-snug text-paper-primary group-hover:text-paper-accent transition-colors",
            "text-base text-paper-secondary leading-relaxed line-clamp-2",
        )
    };

    rsx! {
        article { class: "{article_class}",
            if let Some(cover) = post.cover_image.as_deref() {
                div { class: "post-card-cover overflow-hidden",
                    div { class: "blur-img post-card-cover-blur !rounded-none",
                        img {
                            class: "blur-img-placeholder",
                            src: "{cover}?w=30",
                            alt: "",
                            loading: "lazy",
                        }
                        img {
                            class: "blur-img-full is-loaded",
                            src: "{cover}?thumb={thumb_size}",
                            alt: "{post.title}",
                            loading: "lazy",
                            decoding: "async",
                        }
                    }
                }
            }
            div { class: "post-card-body p-8 flex flex-col gap-3.5 min-w-0",
                div { class: "post-card-meta flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-paper-secondary",
                    time { datetime: "{date_str}", "{date_str}" }
                    span { "{reading_time} 分钟阅读" }
                    if post.word_count > 0 {
                        span { "{post.word_count} 字" }
                    }
                }
                h2 { class: "{title_class}",
                    "{post.title}"
                }
                if let Some(summary) = post.summary.as_deref().filter(|s| !s.is_empty()) {
                    p { class: "{summary_class}",
                        "{summary}"
                    }
                }
                if !post.tags.is_empty() {
                    div { class: "post-card-footer post-card-tags flex flex-wrap gap-x-3 gap-y-2 text-xs text-paper-secondary",
                        for tag in post.tags.iter() {
                            span { key: "{tag}", class: "relative z-10",
                                TagChip {
                                    label: tag.clone(),
                                    to: Route::TagDetail { tag: tag.clone() },
                                    variant: tag_variant,
                                    stop_propagation: true,
                                }
                            }
                        }
                    }
                }
            }
            Link {
                class: "absolute inset-0 z-[2] rounded-[inherit] focus-visible:outline-2 focus-visible:outline-offset-[-3px] focus-visible:outline-paper-accent",
                aria_label: "阅读文章：{post.title}",
                to: Route::PostDetail {
                    slug: post_slug,
                },
            }
        }
    }
}
