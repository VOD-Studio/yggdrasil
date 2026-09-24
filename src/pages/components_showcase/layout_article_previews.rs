//! 布局与文章组件的本地样例。所有展示复用正式组件，演示操作仅改本地状态。

use dioxus::prelude::*;

use crate::components::admin_layout::{AdminShell, AdminSidebar};
use crate::components::footer::FooterView;
use crate::components::frontend_layout::FrontendShell;
use crate::components::header::Header;
use crate::components::nav::build_nav_items;
use crate::components::post::post_content::PostContent;
use crate::components::post::post_footer::PostFooter;
use crate::components::post::post_toc::PostToc;
use crate::models::post::PostNav;
use crate::router::Route;

pub(super) fn preview(slug: &str, detail: bool) -> Option<Element> {
    match slug {
        "admin-layout" => Some(rsx! { AdminLayoutPreview { detail } }),
        "header" => Some(rsx! { HeaderPreview { detail } }),
        "footer" => Some(rsx! { FooterPreview { detail } }),
        "frontend-layout" => Some(rsx! { FrontendLayoutPreview { detail } }),
        "post-content" => Some(rsx! { PostContentPreview { detail } }),
        "post-footer" => Some(rsx! { PostFooterPreview { detail } }),
        "post-toc" => Some(rsx! { PostTocPreview { detail } }),
        _ => None,
    }
}

#[component]
fn AdminLayoutPreview(detail: bool) -> Element {
    let mut active = use_signal(|| "全部文章");
    let mut status = use_signal(|| "侧栏操作只更新这份本地演示。");

    let active_name = active();
    let (stat_1_label, stat_1_val, stat_2_label, stat_2_val) = match active_name {
        "仪表盘" => ("总访客", "1,280", "今日阅读", "342"),
        "写文章" => ("当前字数", "1,420", "自动保存", "2分钟前"),
        "笔记" | "笔记本" => ("公开笔记", "24", "笔记本数", "05"),
        "回收站" => ("待清理", "02", "保留期限", "30天"),
        "评论管理" => ("待审核", "01", "累计评论", "156"),
        "素材" => ("媒体文件", "48", "已占用", "12.8MB"),
        "友链" => ("正常链接", "18", "待确认", "02"),
        "个人信息" => ("账号角色", "管理员", "安全状态", "双重认证"),
        "设置" | "系统" => ("系统状态", "正常", "运行时间", "14天"),
        "日志" => ("今日日志", "84条", "错误告警", "0"),
        _ => ("草稿", "03", "已发布", "12"),
    };

    rsx! {
        div { class: "showcase-admin-window", "data-showcase-preview": "admin-layout",
            AdminShell {
                preview: true,
                sidebar: rsx! { AdminSidebar {
                    route: Route::Admin {},
                    user_name: Some("图鉴访客".to_string()),
                    avatar_url: None,
                    demo_active: Some(active().to_string()),
                    preview: true,
                    on_demo_navigate: move |label| {
                        active.set(label);
                        status.set("已切换本地示例栏目。");
                    },
                    on_logout: move |_| status.set("这里只演示退出按钮，不会退出当前账号。"),
                } },
                content: rsx! {
                    article { class: "showcase-admin-content flex flex-col h-full",
                        div { class: "flex items-center justify-between",
                            p { class: "text-xs font-mono text-paper-secondary tracking-wider", "ADMIN / LOCAL DEMO" }
                            if detail {
                                span { class: "text-xs px-2 py-0.5 rounded-full bg-[var(--color-paper-entry)] text-paper-secondary border border-[var(--color-paper-border)]", "本地演示" }
                            }
                        }
                        h2 { class: "text-xl font-semibold mt-1.5 flex items-center gap-2", "{active()}" }
                        p { class: "text-sm text-paper-secondary mt-1", "{status()}" }

                        div { class: "mt-4 grid grid-cols-2 gap-3",
                            div { class: "rounded-2xl bg-paper-entry p-3.5 border border-[var(--color-paper-border)]/50",
                                small { class: "text-paper-secondary text-xs", "{stat_1_label}" }
                                strong { class: "block text-lg mt-0.5 font-bold tracking-tight", "{stat_1_val}" }
                            }
                            div { class: "rounded-2xl bg-paper-entry p-3.5 border border-[var(--color-paper-border)]/50",
                                small { class: "text-paper-secondary text-xs", "{stat_2_label}" }
                                strong { class: "block text-lg mt-0.5 font-bold tracking-tight", "{stat_2_val}" }
                            }
                        }

                        if detail {
                            div { class: "mt-5 flex-1 min-h-0 flex flex-col",
                                p { class: "text-xs font-medium text-paper-secondary mb-2 tracking-wide", "示例内容列表" }
                                div { class: "rounded-xl border border-[var(--color-paper-border)] divide-y divide-[var(--color-paper-border)] bg-paper-entry/40 overflow-hidden text-xs",
                                    div { class: "px-3.5 py-2.5 flex items-center justify-between hover:bg-paper-entry/80 transition-colors",
                                        div { class: "flex items-center gap-2 min-w-0",
                                            span { class: "w-1.5 h-1.5 rounded-full bg-emerald-500 flex-shrink-0" }
                                            span { class: "truncate font-medium", "Dioxus 0.7 响应式设计实践" }
                                        }
                                        span { class: "text-paper-secondary flex-shrink-0 ml-2 font-mono text-[11px]", "09-22" }
                                    }
                                    div { class: "px-3.5 py-2.5 flex items-center justify-between hover:bg-paper-entry/80 transition-colors",
                                        div { class: "flex items-center gap-2 min-w-0",
                                            span { class: "w-1.5 h-1.5 rounded-full bg-amber-500 flex-shrink-0" }
                                            span { class: "truncate font-medium", "Rust 全栈博客架构与布局演进" }
                                        }
                                        span { class: "text-paper-secondary flex-shrink-0 ml-2 font-mono text-[11px]", "09-18" }
                                    }
                                    div { class: "px-3.5 py-2.5 flex items-center justify-between hover:bg-paper-entry/80 transition-colors",
                                        div { class: "flex items-center gap-2 min-w-0",
                                            span { class: "w-1.5 h-1.5 rounded-full bg-emerald-500 flex-shrink-0" }
                                            span { class: "truncate font-medium", "Tailwind CSS v4 样式系统接入记录" }
                                        }
                                        span { class: "text-paper-secondary flex-shrink-0 ml-2 font-mono text-[11px]", "09-15" }
                                    }
                                }
                                p { class: "text-xs text-paper-secondary mt-auto pt-3", "可展开内容管理或工具菜单，点击任意栏目查看联动状态。" }
                            }
                        }
                    }
                },
            }
        }
    }
}

fn sample_route(label: &str) -> Route {
    match label {
        "笔记" => Route::Notes {},
        "归档" => Route::Archives {},
        "友链" => Route::Friends {},
        "关于" => Route::About {},
        "搜索" => Route::Search {},
        _ => Route::Home {},
    }
}

#[component]
fn HeaderPreview(detail: bool) -> Element {
    let mut active = use_signal(|| "笔记");
    let menu_id = if detail {
        "mobile-nav-menu-showcase-header-detail"
    } else {
        "mobile-nav-menu-showcase-header-card"
    };
    rsx! {
        div { class: "showcase-layout-window", "data-showcase-preview": "header",
            Header {
                nav_items: build_nav_items(sample_route(active())),
                right_content: rsx! {
                    button {
                        class: "p-2 rounded-full text-paper-secondary hover:text-paper-accent",
                        r#type: "button",
                        aria_label: "演示搜索操作",
                        onclick: move |_| active.set("搜索"),
                        "⌕"
                    }
                },
                max_width: "max-w-6xl",
                menu_id,
                page_shell: false,
                on_demo_navigate: move |label| active.set(label),
            }
            if detail { p { class: "px-6 py-3 text-sm text-paper-secondary", "当前演示栏目：{active()}。窄屏下可展开导航菜单。" } }
        }
    }
}

#[component]
fn FooterPreview(detail: bool) -> Element {
    let scroll_id = if detail {
        "showcase-footer-scroll-detail"
    } else {
        "showcase-footer-scroll-card"
    };
    rsx! {
        div { class: "showcase-layout-window", "data-showcase-preview": "footer",
            div { id: scroll_id, class: "showcase-footer-scroll",
                p { "页脚会陪你到页面尽头。" }
                if detail {
                    p { "向下滚动这一小块内容，再点击页脚右侧按钮，就会回到演示顶部。" }
                    div { style: "height: 180px" }
                    p { "这里是演示内容的末尾。" }
                }
            }
            FooterView {
                github_url: Some("https://github.com".to_string()),
                top_visible: true,
                page_shell: false,
                inline_top: true,
                on_top: move |_| scroll_demo_to_top(scroll_id),
            }
        }
    }
}

#[component]
fn FrontendLayoutPreview(detail: bool) -> Element {
    let mut active = use_signal(|| "首页");
    let menu_id = if detail {
        "mobile-nav-menu-showcase-layout-detail"
    } else {
        "mobile-nav-menu-showcase-layout-card"
    };
    let scroll_id = if detail {
        "showcase-frontend-scroll-detail"
    } else {
        "showcase-frontend-scroll-card"
    };
    rsx! {
        div { id: scroll_id, class: "showcase-layout-window showcase-frontend-scroll", "data-showcase-preview": "frontend-layout",
            FrontendShell {
                min_height: "min-h-[250px]",
                max_width: "max-w-4xl",
                header: rsx! { Header {
                    nav_items: build_nav_items(sample_route(active())),
                    right_content: rsx! { button { r#type: "button", aria_label: "演示搜索操作", onclick: move |_| active.set("搜索"), "⌕" } },
                    max_width: "max-w-4xl",
                    menu_id,
                    page_shell: false,
                    on_demo_navigate: move |label| active.set(label),
                } },
                main_content: rsx! {
                    article {
                        h2 { class: "text-xl font-semibold", "一页内容，安心阅读" }
                        p { class: "mt-3 text-paper-secondary", "前台布局将导航、正文和页脚排成清晰的阅读路线。当前栏目：{active()}。" }
                        if detail { p { class: "mt-3 text-paper-secondary", "菜单和搜索仅更新这份固定样例，不会离开组件图鉴。" } }
                    }
                },
                footer: rsx! { FooterView {
                    github_url: Some("https://github.com".to_string()),
                    top_visible: true,
                    page_shell: false,
                    inline_top: true,
                    on_top: move |_| scroll_demo_to_top(scroll_id),
                } },
            }
        }
    }
}

fn scroll_demo_to_top(id: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id))
    {
        element.set_scroll_top(0);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = id;
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
<p><img src="/images/empty-state/dog-camera.webp" alt="相机里的两只小狗" loading="lazy"></p>"#
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
                    class: "mb-4 text-sm text-paper-accent",
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
    let _ = detail;
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
            PostFooter { post, sample: true }
            p { class: "mt-3 text-xs text-paper-secondary", "样例标签和相邻文章不跳转。" }
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
