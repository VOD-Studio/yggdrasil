//! 文章页脚组件：正文后的发丝线与上一篇 / 下一篇。

use leptos::prelude::*;

use crate::components::post::post_nav_links::PostNavLinks;
use crate::models::post::PostFooterData;

/// 文章页脚组件。
///
/// Props：
/// - `post`：页脚展示所需的最小字段集（P4.2：不携带完整正文，调用方用
///   `PostFooterData::from(&post)` 构造）
/// - `sample`：图鉴展示非跳转的样例链接；正式文章默认 false。
///
/// 标签已移到页眉；没有相邻文章时不渲染。
#[component]
pub fn PostFooter(post: PostFooterData, #[prop(optional)] sample: bool) -> impl IntoView {
    if post.prev_post.is_none() && post.next_post.is_none() {
        return ().into_any();
    }
    view! {
        <footer class="post-footer">
            <PostNavLinks prev=post.prev_post next=post.next_post sample/>
        </footer>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::post::PostNav;
    use leptos::reactive::owner::Owner;

    #[test]
    fn renders_footer_only_with_neighbours() {
        let empty = Owner::new().with(|| {
            view! { <PostFooter post=PostFooterData { prev_post: None, next_post: None }/> }
                .to_html()
        });
        assert!(!empty.contains("post-footer"), "{empty}");
        let filled = Owner::new().with(|| {
            view! {
                <PostFooter post=PostFooterData {
                    prev_post: Some(PostNav { title: "旧文".into(), slug: "old".into() }),
                    next_post: None,
                }/>
            }
            .to_html()
        });
        assert!(filled.contains("post-footer"), "{filled}");
        assert!(filled.contains("旧文"), "{filled}");
    }
}
