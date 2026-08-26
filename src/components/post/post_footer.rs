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
/// - `full_reload`：链接走整页加载而非客户端路由（默认 false）。仅当本组件
///   渲染在 admin 布局内（`/admin/preview`）时传 true——跨 layout 分支的
///   客户端导航会触发 dioxus 0.7.10 的 suspense 卸载双重回收 bug（详见
///   `src/pages/admin/preview.rs` 模块文档），整页加载可完全规避。
///
/// 展示内容包括：
/// - 文章标签云，链接到对应标签详情页
/// - 相邻文章导航（如有）
#[component]
pub fn PostFooter(post: Post, #[props(default = false)] full_reload: bool) -> Element {
    let tag_to = |tag: &str| super::nav_target(
        Route::TagDetail {
            tag: tag.to_string(),
        },
        full_reload,
    );
    rsx! {
        footer { class: "post-footer",
            if !post.tags.is_empty() {
                ul { class: "post-tags",
                    for tag in &post.tags {
                        li { key: "{tag}",
                            Link {
                                to: tag_to(tag),
                                "{tag}"
                            }
                        }
                    }
                }
            }

            if post.prev_post.is_some() || post.next_post.is_some() {
                PostNavLinks { prev: post.prev_post, next: post.next_post, full_reload }
            }
        }
    }
}
