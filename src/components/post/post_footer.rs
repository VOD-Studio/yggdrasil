//! 文章页脚组件。
//! 展示文章标签与上一篇/下一篇导航。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::components::post::post_nav_links::PostNavLinks;
use crate::models::post::Post;
use crate::router::Route;

/// 文章页脚组件。
///
/// Props：
/// - `post`：文章数据模型
///
/// 展示内容包括：
/// - 文章标签云，链接到对应标签详情页
/// - 相邻文章导航（如有）
#[component]
pub fn PostFooter(post: Post) -> Element {
    rsx! {
        footer { class: "post-footer",
            if !post.tags.is_empty() {
                ul { class: "post-tags",
                    for tag in &post.tags {
                        li { key: "{tag}",
                            Link {
                                to: Route::TagDetail {
                                    tag: tag.clone(),
                                },
                                "{tag}"
                            }
                        }
                    }
                }
            }

            if post.prev_post.is_some() || post.next_post.is_some() {
                PostNavLinks { prev: post.prev_post, next: post.next_post }
            }
        }
    }
}
