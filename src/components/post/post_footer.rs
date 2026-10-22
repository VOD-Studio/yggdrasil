//! 文章页脚组件：正文后的发丝线与上一篇 / 下一篇。

use dioxus::prelude::*;

use crate::components::post::post_nav_links::PostNavLinks;
use crate::models::post::PostFooterData;

/// 文章页脚组件。
///
/// Props：
/// - `post`：页脚展示所需的最小字段集（P4.2：不携带完整正文，调用方用
///   `PostFooterData::from(&post)` 构造）
/// - `full_reload`：链接走整页加载而非客户端路由（默认 false）。仅当本组件
///   渲染在 admin 布局内（`/admin/preview`）时传 true——跨 layout 分支的
///   客户端导航会触发 dioxus 0.7.10 的 suspense 卸载双重回收 bug（详见
///   `src/pages/admin/preview.rs` 模块文档），整页加载可完全规避。
/// - `sample`：图鉴展示非跳转的样例链接；正式文章默认 false。
///
/// 标签已移到页眉；没有相邻文章时不渲染。
#[component]
pub fn PostFooter(
    post: PostFooterData,
    #[props(default = false)] full_reload: bool,
    #[props(default = false)] sample: bool,
) -> Element {
    if post.prev_post.is_none() && post.next_post.is_none() {
        return rsx! {};
    }
    rsx! {
        footer { class: "post-footer",
            PostNavLinks { prev: post.prev_post, next: post.next_post, full_reload, sample }
        }
    }
}
