//! 文章组件的本地样例。所有展示复用正式组件，演示操作仅改本地状态。

use leptos::prelude::*;

use crate::components::post::post_content::PostContent;
use crate::components::post::post_footer::PostFooter;
use crate::components::post::post_header::PostHeader;
use crate::components::post::post_toc::PostToc;
use crate::models::post::{PostFooterData, PostHeaderData, PostNav, PostStatus};

pub(super) fn preview(slug: &str, detail: bool) -> Option<AnyView> {
    match slug {
        "post-header" => Some(view! { <PostHeaderPreview detail/> }.into_any()),
        "post-content" => Some(view! { <PostContentPreview detail/> }.into_any()),
        "post-footer" => Some(view! { <PostFooterPreview detail/> }.into_any()),
        "post-toc" => Some(view! { <PostTocPreview detail/> }.into_any()),
        _ => None,
    }
}

#[component]
fn PostHeaderPreview(detail: bool) -> impl IntoView {
    let mut post = super::sample_post();
    post.summary = Some(
        "在时间的缝隙里，种下一些文字。记录设计、代码与日常，让零散的念头慢慢长成一片风景。"
            .to_string(),
    );
    let mut draft = post.clone();
    draft.title = "还在生长的想法".to_string();
    draft.summary = Some("这一页，留给尚未完成的念头。".to_string());
    draft.status = PostStatus::Draft;
    draft.published_at = None;
    let mut without_summary = post.clone();
    without_summary.title = "在时间的缝隙里种下一些文字：从零开始构建自己的数字花园".to_string();
    without_summary.summary = None;
    without_summary.tags = Vec::new();

    let examples = detail.then(|| {
        view! {
            <div class="showcase-post-header-examples">
                <section class="showcase-post-header-example">
                    <h3>"草稿" <span>"标题旁显示草稿标记"</span></h3>
                    <article
                        class="showcase-post-header-sheet"
                        data-showcase-sample="draft"
                        inert="true"
                    >
                        <PostHeader post=PostHeaderData::from(&draft)/>
                    </article>
                </section>
                <section class="showcase-post-header-example">
                    <h3>"长标题 · 无摘要 · 无标签" <span>"面包屑只剩「文章」"</span></h3>
                    <article
                        class="showcase-post-header-sheet"
                        data-showcase-sample="no-summary"
                        inert="true"
                    >
                        <PostHeader post=PostHeaderData::from(&without_summary)/>
                    </article>
                </section>
            </div>
        }
    });

    view! {
        <div class="showcase-post-header-demo" data-showcase-preview="post-header">
            <article
                class="showcase-post-header-sheet"
                data-showcase-sample="published"
                inert="true"
            >
                <PostHeader post=PostHeaderData::from(&post)/>
            </article>
            {examples}
        </div>
    }
}

fn article_html(prefix: &str, alternate: bool) -> String {
    let opening = if alternate {
        "雨停之后，树叶留下细小的光。"
    } else {
        "一片叶子，也记得来时的风。"
    };
    format!(
        r#"<h2 id="{prefix}-opening">从一片叶子开始</h2>
<p>{opening} 这段固定文章展示正文排版和图片，不连接任何服务。</p>
<h3 id="{prefix}-details">沿途所见</h3>
<ul><li>清晨的光</li><li>缓慢生长的枝条</li></ul>
<blockquote><p>让想法，像树一样生长。</p></blockquote>
<pre><code class="language-rust">let leaf = "Yggdrasil";
println!("{{leaf}}");</code></pre>
<h2 id="{prefix}-image">留下一张照片</h2>
<p><img src="/images/samples/clay-circle.webp" alt="暖白纸面上的一轮陶土色圆" loading="lazy"></p>"#
    )
}

fn toc_html(prefix: &str) -> String {
    format!(
        r##"<ul><li><a href="#{prefix}-opening">从一片叶子开始</a><ul><li><a href="#{prefix}-details">沿途所见</a></li></ul></li><li><a href="#{prefix}-image">留下一张照片</a></li></ul>"##
    )
}

#[component]
fn PostContentPreview(detail: bool) -> impl IntoView {
    let alternate = RwSignal::new(false);
    let (scope_id, heading_prefix) = if detail {
        (
            "showcase-post-content-detail",
            "showcase-post-content-detail",
        )
    } else {
        ("showcase-post-content-card", "showcase-post-content-card")
    };
    let toggle = detail.then(|| {
        view! {
            <button
                class="mb-4 text-sm text-clay-text"
                type="button"
                on:click=move |_| alternate.update(|value| *value = !*value)
            >
                "切换文章样例"
            </button>
        }
    });

    view! {
        <div class="showcase-article-window" data-showcase-preview="post-content">
            {toggle}
            // 切换样例时整段重建，等价于 BASE 里按 variant 做 key 的重新挂载。
            {move || {
                let variant = alternate.get();
                view! {
                    <PostContent
                        content_html=article_html(heading_prefix, variant)
                        scope_id=scope_id
                    />
                }
            }}
        </div>
    }
}

#[component]
fn PostFooterPreview(detail: bool) -> impl IntoView {
    let prev = PostNav {
        title: "沿着风的方向".to_string(),
        slug: "sample-previous".to_string(),
    };
    let next = PostNav {
        title: "下一圈年轮".to_string(),
        slug: "sample-next".to_string(),
    };
    let footer = |prev_post: Option<PostNav>, next_post: Option<PostNav>| PostFooterData {
        prev_post,
        next_post,
    };
    let examples = detail.then(|| {
        view! {
            <div class="showcase-post-nav-examples">
                <section class="showcase-post-nav-example" data-showcase-sample="next-only">
                    <h3>"第一篇文章" <span>"只有下一篇"</span></h3>
                    <PostFooter post=footer(None, Some(next.clone())) sample=true/>
                </section>
                <section class="showcase-post-nav-example" data-showcase-sample="prev-only">
                    <h3>"最后一篇文章" <span>"只有上一篇"</span></h3>
                    <PostFooter post=footer(Some(prev.clone()), None) sample=true/>
                </section>
                <section class="showcase-post-nav-empty" data-showcase-sample="empty">
                    <PostFooter post=footer(None, None) sample=true/>
                    <p>"没有相邻文章时，整个页脚不渲染。"</p>
                </section>
            </div>
            <p class="showcase-post-footer-note">"试试悬停或用 Tab 聚焦卡片。样例链接停留在本页。"</p>
        }
    });

    view! {
        <div class="showcase-article-window" data-showcase-preview="post-footer">
            <div data-showcase-sample="both">
                <PostFooter post=footer(Some(prev.clone()), Some(next.clone())) sample=true/>
            </div>
            {examples}
        </div>
    }
}

#[component]
fn PostTocPreview(detail: bool) -> impl IntoView {
    let (scroll_id, nav_id, content_id) = if detail {
        (
            "showcase-post-toc-scroll-detail",
            "showcase-post-toc-nav-detail",
            "showcase-post-toc-content-detail",
        )
    } else {
        (
            "showcase-post-toc-scroll-card",
            "showcase-post-toc-nav-card",
            "showcase-post-toc-content-card",
        )
    };

    view! {
        <div
            id=scroll_id
            class="showcase-toc-window showcase-article-window"
            data-showcase-preview="post-toc"
            data-local-anchor-scroll="true"
        >
            <PostToc
                toc_html=toc_html(content_id)
                title="文章目录"
                nav_id=nav_id
                content_id=content_id
                scroll_id=scroll_id
            />
            <PostContent content_html=article_html(content_id, false) scope_id=content_id/>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_and_toc_share_scoped_headings() {
        let html = article_html("sample", false);
        let toc = toc_html("sample");
        for id in ["sample-opening", "sample-details", "sample-image"] {
            assert!(html.contains(&format!("id=\"{id}\"")));
            assert!(toc.contains(&format!("href=\"#{id}\"")));
        }
        assert!(!html.contains("data-runnable"));
    }

    #[test]
    fn alternate_article_changes_real_content() {
        assert_ne!(article_html("sample", false), article_html("sample", true));
    }
}
