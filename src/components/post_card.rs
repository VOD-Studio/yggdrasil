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
pub fn PostCard(post: PostListItem) -> Element {
    let post_slug = post.slug.clone();
    let date_str = post.formatted_date();
    let cover_src = post.cover_image.clone().unwrap_or_default();
    let has_cover = post.cover_image.is_some();

    rsx! {
        article { class: "group relative mb-12 flex flex-col bg-[var(--color-paper-entry)] rounded-[2rem] border border-transparent hover:border-[var(--color-paper-border)] hover:shadow-sm overflow-hidden transition-all duration-300",
            if has_cover {
                div { class: "w-full overflow-hidden",
                    div { class: "blur-img post-card-cover-blur !rounded-none",
                        img {
                            class: "blur-img-placeholder",
                            src: "{cover_src}?w=20",
                            alt: "{post.title}",
                            loading: "lazy",
                        }
                        img {
                            class: "blur-img-full is-loaded",
                            src: "{cover_src}?thumb=420x180",
                            alt: "{post.title}",
                        }
                    }
                }
            }
            div { class: "p-8 flex flex-col gap-3",
                h2 { class: "text-2xl md:text-3xl font-extrabold tracking-tight leading-tight text-[var(--color-paper-primary)] group-hover:text-[var(--color-paper-accent)] transition-colors duration-200",
                    "{post.title}"
                }
                div { class: "mt-1 text-base text-[var(--color-paper-secondary)] leading-relaxed line-clamp-2",
                    "{post.summary.as_deref().unwrap_or_default()}"
                }
                div { class: "mt-4 flex flex-wrap items-center gap-3 text-sm font-medium text-[var(--color-paper-tertiary)]",
                    span { class: "tracking-wide", "{date_str}" }
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
            }
            // 覆盖层链接：铺满卡片，承担整卡跳转。
            // z-[2] 高于封面完整图 (.blur-img-full z-index:1, input.css:611)，
            // 使鼠标 hover 封面时命中本 <a> 而非 <img>：光标变 pointer 且点击可跳转；
            // 仍低于标签 (z-10)，标签独立点击（stop_propagation）不受影响。
            Link {
                class: "absolute inset-0 z-[2]",
                aria_label: "post link to {post.title}",
                to: Route::PostDetail {
                    slug: post_slug,
                },
            }
        }
    }
}
