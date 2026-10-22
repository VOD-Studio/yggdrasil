//! 文章页眉组件
//!
//! 面包屑、衬线标题、导语、元信息与标签，末尾一条发丝线与正文分开。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::components::post::breadcrumbs::Breadcrumbs;
use crate::components::post::post_meta::PostMeta;
use crate::components::ui::StatusBadge;
use crate::models::post::{PostHeaderData, PostStatus};
use crate::router::Route;

/// 文章页眉组件。
///
/// Props：
/// - `post`：页眉展示所需的最小字段集（P4.2：不携带完整正文，调用方用
///   `PostHeaderData::from(&post)` 构造）
/// - `full_reload`：链接走整页加载（默认 false）。仅当渲染在 admin 布局内
///   （`/admin/preview`）时传 true——跨 layout 分支的客户端导航会触发
///   dioxus 0.7.10 的 suspense 卸载双重回收 bug（详见 `src/pages/admin/preview.rs`
///   模块文档）。
/// - `sample`：图鉴展示非跳转的样例链接；正式文章默认 false。
#[component]
pub fn PostHeader(
    post: PostHeaderData,
    #[props(default = false)] full_reload: bool,
    #[props(default = false)] sample: bool,
) -> Element {
    let summary = post.summary.clone().filter(|s| !s.trim().is_empty());
    rsx! {
        header { class: "post-header",
            Breadcrumbs { tag: post.tags.first().cloned(), full_reload, sample }
            h1 { class: "post-title", "data-vt-post-id": "{post.id}", "data-vt-role": "title",
                "{post.title}"
                if post.status == PostStatus::Draft {
                    StatusBadge { color_class: "badge-warning", label: "草稿", class: "post-draft" }
                }
            }
            if let Some(summary) = summary {
                p { class: "post-lede", "{summary}" }
            }
            PostMeta { post: post.clone() }
            if !post.tags.is_empty() {
                ul { class: "post-tags", aria_label: "文章标签",
                    for tag in post.tags.iter() {
                        li { key: "{tag}",
                            if sample {
                                a { class: "chip", href: "#", onclick: move |event| event.prevent_default(), "{tag}" }
                            } else {
                                Link {
                                    class: "chip",
                                    to: super::nav_target(Route::TagDetail { tag: tag.clone() }, full_reload),
                                    "{tag}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
