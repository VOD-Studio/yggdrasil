//! 文章组件的本地样例。所有展示复用正式组件，演示操作仅改本地状态。

use dioxus::prelude::*;

use crate::components::post::breadcrumbs::Breadcrumbs;
use crate::components::post::post_content::PostContent;
use crate::components::post::post_footer::PostFooter;
use crate::components::post::post_header::PostHeader;
use crate::components::post::post_meta::PostMeta;
use crate::components::post::post_nav_links::PostNavLinks;
use crate::components::post::post_toc::PostToc;
use crate::models::post::{PostNav, PostStatus};

pub(super) fn preview(slug: &str, detail: bool) -> Option<Element> {
    match slug {
        "breadcrumbs" => Some(rsx! { BreadcrumbsPreview { detail } }),
        "post-header" => Some(rsx! { PostHeaderPreview { detail } }),
        "post-meta" => Some(rsx! { PostMetaPreview {} }),
        "post-content" => Some(rsx! { PostContentPreview { detail } }),
        "post-footer" => Some(rsx! { PostFooterPreview { detail } }),
        "post-nav-links" => Some(rsx! { PostNavLinksPreview { detail } }),
        "post-toc" => Some(rsx! { PostTocPreview { detail } }),
        _ => None,
    }
}

#[component]
fn PostMetaPreview() -> Element {
    let post = super::sample_post();

    rsx! {
        article { class: "showcase-post-meta-demo", "data-showcase-preview": "post-meta",
            p { class: "showcase-post-meta-kicker", "设计手记" }
            h3 { class: "post-title", "{post.title}" }
            p { class: "post-description", "{post.summary.as_deref().unwrap_or_default()}" }
            PostMeta { post: crate::models::post::PostHeaderData::from(&post) }
        }
    }
}

#[component]
fn PostHeaderPreview(detail: bool) -> Element {
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

    rsx! {
        div { class: "showcase-post-header-demo", "data-showcase-preview": "post-header",
            article { class: "showcase-post-header-sheet", "data-showcase-sample": "published", inert: "true",
                PostHeader { post: crate::models::post::PostHeaderData::from(&post) }
            }
            if detail {
                div { class: "showcase-post-header-examples",
                    section { class: "showcase-post-header-example",
                        h3 { "草稿" span { "标题旁显示草稿标记" } }
                        article { class: "showcase-post-header-sheet", "data-showcase-sample": "draft", inert: "true",
                            PostHeader { post: crate::models::post::PostHeaderData::from(&draft) }
                        }
                    }
                    section { class: "showcase-post-header-example",
                        h3 { "长标题 · 无摘要" span { "自然换行" } }
                        article { class: "showcase-post-header-sheet", "data-showcase-sample": "no-summary", inert: "true",
                            PostHeader { post: crate::models::post::PostHeaderData::from(&without_summary) }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BreadcrumbsPreview(detail: bool) -> Element {
    rsx! {
        div { class: "showcase-breadcrumb-demo", "data-showcase-preview": "breadcrumbs",
            article { class: "showcase-breadcrumb-context",
                div { class: "showcase-breadcrumb-path",
                    Breadcrumbs { tag: Some("源码剖析".to_string()), sample: true }
                }
                div { class: "showcase-breadcrumb-copy",
                    h3 { "让时间成为电路的一部分" }
                    p { class: "showcase-breadcrumb-summary", "「文章」回到首页，栏目取文章的第一个标签。" }
                }
            }
            if detail {
                div { class: "showcase-breadcrumb-examples",
                    section { class: "showcase-breadcrumb-example",
                        h3 { "没有标签" span { "只显示「文章」" } }
                        div { class: "showcase-breadcrumb-path",
                            Breadcrumbs { sample: true }
                        }
                    }
                }
                p { class: "showcase-breadcrumb-note", "样例链接停留在本页。" }
            }
        }
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
fn PostContentPreview(detail: bool) -> Element {
    let mut alternate = use_signal(|| false);
    let (scope_id, heading_prefix) = if detail {
        (
            "showcase-post-content-detail",
            "showcase-post-content-detail",
        )
    } else {
        ("showcase-post-content-card", "showcase-post-content-card")
    };
    rsx! {
        div { class: "showcase-article-window", "data-showcase-preview": "post-content",
            if detail {
                button {
                    class: "mb-4 text-sm text-clay-text",
                    r#type: "button",
                    onclick: move |_| alternate.toggle(),
                    "切换文章样例"
                }
            }
            for variant in std::iter::once(alternate()) {
                PostContent {
                    key: "sample-{variant}",
                    content_html: article_html(heading_prefix, variant),
                    scope_id: Some(scope_id),
                }
            }
        }
    }
}

#[component]
fn PostFooterPreview(detail: bool) -> Element {
    let mut post = super::sample_post();
    post.tags = vec!["设计".to_string(), "阅读".to_string(), "Rust".to_string()];
    post.prev_post = Some(PostNav {
        title: "沿着风的方向".to_string(),
        slug: "sample-previous".to_string(),
    });
    post.next_post = Some(PostNav {
        title: "下一圈年轮".to_string(),
        slug: "sample-next".to_string(),
    });
    rsx! {
        div { class: "showcase-article-window", "data-showcase-preview": "post-footer",
            PostFooter { post: crate::models::post::PostFooterData::from(&post), sample: true }
            if detail {
                p { class: "showcase-post-footer-note", "样例标签和相邻文章不跳转。" }
            }
        }
    }
}

#[component]
fn PostNavLinksPreview(detail: bool) -> Element {
    let prev = PostNav {
        title: "一片叶子，落在代码之间".to_string(),
        slug: "sample-previous".to_string(),
    };
    let next = PostNav {
        title: "下一圈年轮：让想法慢慢长成一座数字花园".to_string(),
        slug: "sample-next".to_string(),
    };

    rsx! {
        div { class: "showcase-post-nav-demo", "data-showcase-preview": "post-nav-links",
            section { class: "showcase-post-nav-sheet", "data-showcase-sample": "both",
                h3 { class: "showcase-post-nav-heading", "继续阅读" span { "沿着文字，走向下一页" } }
                PostNavLinks { prev: Some(prev.clone()), next: Some(next.clone()), sample: true }
            }
            if detail {
                div { class: "showcase-post-nav-examples",
                    section { class: "showcase-post-nav-example", "data-showcase-sample": "next-only",
                        h3 { "第一篇文章" span { "只有下一篇" } }
                        PostNavLinks { prev: None, next: Some(next), sample: true }
                    }
                    section { class: "showcase-post-nav-example", "data-showcase-sample": "prev-only",
                        h3 { "最后一篇文章" span { "只有上一篇" } }
                        PostNavLinks { prev: Some(prev), next: None, sample: true }
                    }
                    section { class: "showcase-post-nav-empty", "data-showcase-sample": "empty",
                        PostNavLinks { prev: None, next: None, sample: true }
                        p { "没有相邻文章时，不展示导航入口。" }
                    }
                }
                p { class: "showcase-post-nav-note", "试试悬停或用 Tab 聚焦卡片。样例链接停留在本页。" }
            }
        }
    }
}

#[component]
fn PostTocPreview(detail: bool) -> Element {
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
    rsx! {
        div {
            id: scroll_id,
            class: "showcase-toc-window showcase-article-window",
            "data-showcase-preview": "post-toc",
            "data-local-anchor-scroll": "true",
            PostToc {
                toc_html: toc_html(content_id),
                title: "文章目录",
                nav_id: Some(nav_id),
                content_id: Some(content_id),
                scroll_id: Some(scroll_id),
            }
            PostContent { content_html: article_html(content_id, false), scope_id: Some(content_id) }
        }
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
