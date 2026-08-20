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
///
/// 展示内容包括：
/// - 封面图（如有，使用 400x300 缩略图，不启用灯箱）
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
pub fn PostCard(post: PostListItem, #[props(default = false)] featured: bool) -> Element {
    let post_slug = post.slug.clone();
    let date_str = post.formatted_date();
    let cover_src = post.cover_image.clone().unwrap_or_default();
    let has_cover = post.cover_image.is_some();
    let reading_time = post.reading_time.max(1);

    // 缩略图请求规格：Featured 使用 1200x520 高清横幅，标准卡片使用 840x360 保证 Retina 屏细腻无模糊
    let thumb_size = if featured { "1200x520" } else { "840x360" };
    let thumb_url = format!("{}?thumb={}", cover_src, thumb_size);
    let placeholder_url = format!("{}?w=30", cover_src);

    let article_class = if featured {
        "group relative mb-12 flex flex-col bg-[var(--color-paper-entry)] rounded-[2rem] border border-[var(--color-paper-border)]/60 hover:border-[var(--color-paper-accent)]/50 hover:shadow-lg transition-all duration-300 overflow-hidden"
    } else {
        "group relative mb-10 flex flex-col bg-[var(--color-paper-entry)] rounded-[2rem] border border-transparent hover:border-[var(--color-paper-border)] hover:shadow-md transition-all duration-300 overflow-hidden"
    };

    let title_class = if featured {
        "text-2xl sm:text-3xl md:text-4xl font-extrabold tracking-tight leading-tight text-[var(--color-paper-primary)] group-hover:text-[var(--color-paper-accent)] transition-colors duration-200"
    } else {
        "text-xl sm:text-2xl md:text-3xl font-extrabold tracking-tight leading-snug text-[var(--color-paper-primary)] group-hover:text-[var(--color-paper-accent)] transition-colors duration-200"
    };

    let summary_class = if featured {
        "text-base sm:text-lg text-[var(--color-paper-secondary)] leading-relaxed line-clamp-3"
    } else {
        "text-base text-[var(--color-paper-secondary)] leading-relaxed line-clamp-2"
    };

    rsx! {
        article { class: "{article_class}",
            if has_cover {
                div { class: "w-full overflow-hidden",
                    div { class: "blur-img post-card-cover-blur !rounded-none",
                        img {
                            class: "blur-img-placeholder",
                            src: "{placeholder_url}",
                            alt: "{post.title}",
                            loading: "lazy",
                        }
                        img {
                            class: "blur-img-full is-loaded",
                            src: "{thumb_url}",
                            alt: "{post.title}",
                        }
                    }
                }
            }
            div { class: "p-8 flex flex-col gap-3.5",
                if featured {
                    div { class: "flex items-center gap-2 mb-0.5",
                        span { class: "inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-semibold bg-[var(--color-paper-accent)]/15 text-[var(--color-paper-accent)] border border-[var(--color-paper-accent)]/20",
                            svg {
                                class: "w-3 h-3 fill-current",
                                view_box: "0 0 24 24",
                                path { d: "M12 2l2.4 7.4h7.6l-6.2 4.5 2.4 7.4-6.2-4.5-6.2 4.5 2.4-7.4-6.2-4.5h7.6z" }
                            }
                            "最新文章"
                        }
                    }
                }
                h2 { class: "{title_class}",
                    "{post.title}"
                }
                if let Some(summary) = post.summary.as_deref().filter(|s| !s.is_empty()) {
                    div { class: "{summary_class}",
                        "{summary}"
                    }
                }
                div { class: "mt-3 pt-3 border-t border-[var(--color-paper-border)]/30 flex flex-wrap items-center justify-between gap-3 text-sm text-[var(--color-paper-tertiary)]",
                    div { class: "flex flex-wrap items-center gap-3 font-medium",
                        span { class: "tracking-wide", "{date_str}" }
                        span { "·" }
                        span { class: "inline-flex items-center gap-1",
                            svg {
                                class: "w-3.5 h-3.5 fill-current",
                                view_box: "0 -960 960 960",
                                path { d: "m612-292 56-56-148-148v-184h-80v216l172 172ZM480-80q-83 0-156-31.5T197-197q-54-54-85.5-127T80-480q0-83 31.5-156T197-763q54-54 127-85.5T480-880q83 0 156 31.5T763-763q54-54 85.5 127T880-480q0 83-31.5 156T763-197q-54 54-127 85.5T480-80Zm0-400Zm0 320q133 0 226.5-93.5T800-480q0-133-93.5-226.5T480-800q-133 0-226.5 93.5T160-480q0 133 93.5 226.5T480-160Z" }
                            }
                            "{reading_time} 分钟阅读"
                        }
                        if post.word_count > 0 {
                            span { "·" }
                            span { "{post.word_count} 字" }
                        }
                        if !post.tags.is_empty() {
                            span { "·" }
                            for tag in post.tags.clone().into_iter() {
                                span { key: "{tag}", class: "relative z-10",
                                    TagChip {
                                         label: tag.clone(),
                                         variant: "outline",
                                         stop_propagation: true,
                                     }
                                 }
                             }
                         }
                     }
                    div { class: "hidden sm:inline-flex items-center gap-1 text-xs font-semibold text-[var(--color-paper-accent)] group-hover:translate-x-1 transition-transform duration-200",
                        "阅读全文"
                        svg {
                            class: "w-3.5 h-3.5 fill-none stroke-current",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            view_box: "0 0 24 24",
                            path { d: "M5 12h14M12 5l7 7-7 7" }
                        }
                    }
                 }
            }
            Link {
                class: "absolute inset-0 z-[2]",
                aria_label: "阅读文章：{post.title}",
                to: Route::PostDetail {
                    slug: post_slug,
                },
            }
        }
    }
}
