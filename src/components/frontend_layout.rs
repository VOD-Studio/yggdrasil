//! 前台布局组件
//!
//! 包裹所有前台路由，提供统一的 Header、Footer 与主内容区容器，
//! 并为不同路由在 Suspense 中展示对应的骨架屏。
//!
//! 布局在主包里只构建一次（Leptos 组件体只运行一次），随路由变化的部分
//! （导航激活项、主区宽度、站点分享信息）跟随当前显示的页面
//! （[`use_rendered_route`]），骨架屏按地址栏指向的目标页选择。

use leptos::children::{ChildrenFn, ViewFn};
use leptos::error::Errors;
use leptos::prelude::*;
use leptos_router::components::Outlet;
use leptos_router::hooks::use_location;

use crate::components::footer::Footer;
use crate::components::header::{Header, SearchIconLink};
use crate::components::nav::build_nav_items;
use crate::components::palette::use_search_palette_provider;
use crate::components::search_palette::SearchPalette;
use crate::components::share_meta::SiteShareMeta;
use crate::components::skeletons::archive_skeleton::{ArchiveSkeleton, TagDetailSkeleton};
use crate::components::skeletons::changelog_skeleton::ChangelogSkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::friends_skeleton::FriendsSkeleton;
use crate::components::skeletons::home_skeleton::HomeSkeleton;
use crate::components::skeletons::notes_route_skeleton::NotesRouteSkeleton;
use crate::components::skeletons::post_detail_skeleton::PostDetailSkeleton;
use crate::components::skeletons::search_skeleton::SearchSkeleton;
use crate::components::skeletons::writing_guide_skeleton::WritingGuideSkeleton;
use crate::navigation::use_rendered_route;
use crate::pages::not_found::{NotFound, PageLoadFailed};
use crate::routes::{is_post_path, is_showcase_path};
use crate::theme::ThemeToggle;
use crate::utils::error::is_not_found_text;

/// 路径对应的骨架屏种类。
///
/// 纯数据分类，与渲染分离以便单测；渲染见 [`route_skeleton`]。
#[derive(Debug, Clone, PartialEq)]
enum SkeletonKind {
    /// 笔记索引、笔记本与单篇笔记（`/notes` 及其子路径）。
    Notes,
    /// 首页分页（`/page/<n>`），页码至少为 1。
    HomePage(i32),
    /// 归档。
    Archives,
    /// 单个标签（`/tags/<tag>`），携带解码后的标签名。
    TagDetail(String),
    /// 友链。
    Friends,
    /// 搜索。
    Search,
    /// 文章详情。
    PostDetail,
    /// 更新日志。
    Changelog,
    /// 写作指南。
    WritingGuide,
    /// 组件图鉴及其详情。
    Showcase,
    /// 其余路径：首页骨架。
    Fallback,
}

/// 按路径（`use_location().pathname`，不含查询串）选择骨架屏种类。
fn skeleton_kind(path: &str) -> SkeletonKind {
    let path = match path.trim_end_matches('/') {
        "" => "/",
        trimmed => trimmed,
    };
    if path == "/notes" || path.starts_with("/notes/") {
        return SkeletonKind::Notes;
    }
    if let Some(page) = path
        .strip_prefix("/page/")
        .and_then(|page| page.parse::<i32>().ok())
    {
        return SkeletonKind::HomePage(page.max(1));
    }
    if let Some(tag) = path.strip_prefix("/tags/").filter(|tag| !tag.contains('/')) {
        return SkeletonKind::TagDetail(
            percent_encoding::percent_decode_str(tag)
                .decode_utf8_lossy()
                .into_owned(),
        );
    }
    if is_post_path(path) {
        return SkeletonKind::PostDetail;
    }
    if is_showcase_path(path) {
        return SkeletonKind::Showcase;
    }
    match path {
        "/archives" => SkeletonKind::Archives,
        "/friends" => SkeletonKind::Friends,
        "/search" => SkeletonKind::Search,
        "/changelog" => SkeletonKind::Changelog,
        "/about/writing" => SkeletonKind::WritingGuide,
        _ => SkeletonKind::Fallback,
    }
}

/// 根据当前前台路径选择对应的骨架屏组件。
fn route_skeleton(path: &str) -> AnyView {
    match skeleton_kind(path) {
        SkeletonKind::Notes => {
            let path = path.to_string();
            view! { <DelayedSkeleton><NotesRouteSkeleton path=path.clone()/></DelayedSkeleton> }
                .into_any()
        }
        SkeletonKind::HomePage(page) => {
            view! { <DelayedSkeleton><HomeSkeleton current_page=page/></DelayedSkeleton> }
                .into_any()
        }
        SkeletonKind::Archives => {
            view! { <DelayedSkeleton><ArchiveSkeleton/></DelayedSkeleton> }.into_any()
        }
        SkeletonKind::TagDetail(tag) => view! { <TagDetailSkeleton tag=tag/> }.into_any(),
        SkeletonKind::Friends => view! {
            <DelayedSkeleton>
                <div class="friends">
                    <FriendsSkeleton/>
                </div>
            </DelayedSkeleton>
        }
        .into_any(),
        SkeletonKind::Search => {
            view! { <DelayedSkeleton><SearchSkeleton/></DelayedSkeleton> }.into_any()
        }
        SkeletonKind::PostDetail => {
            view! { <DelayedSkeleton><PostDetailSkeleton/></DelayedSkeleton> }.into_any()
        }
        SkeletonKind::Changelog => {
            view! { <DelayedSkeleton><ChangelogSkeleton/></DelayedSkeleton> }.into_any()
        }
        SkeletonKind::WritingGuide => {
            view! { <DelayedSkeleton><WritingGuideSkeleton/></DelayedSkeleton> }.into_any()
        }
        SkeletonKind::Showcase => {
            view! { <div class="py-20" role="status">"正在翻开组件图鉴…"</div> }.into_any()
        }
        SkeletonKind::Fallback => {
            view! { <DelayedSkeleton><HomeSkeleton/></DelayedSkeleton> }.into_any()
        }
    }
}

/// 路由内容渲染出错时的兜底：错误文本表示“不存在”则显示 404，其余显示“页面加载失败”。
fn route_error_fallback(errors: ArcRwSignal<Errors>) -> impl IntoView {
    move || {
        let text = errors.with(|errors| {
            errors
                .iter()
                .map(|(_, error)| error.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        });
        if is_not_found_text(&text) {
            view! { <NotFound/> }.into_any()
        } else {
            view! { <PageLoadFailed/> }.into_any()
        }
    }
}

/// 前台整体布局组件。
///
/// 负责渲染 Header（含前台导航与主题切换）、主内容区与 Footer，
/// 并在路由内容加载过程中显示与路由匹配的骨架屏。
///
/// Props：
/// - `children`：显式传入的页面内容（未匹配路由的 404 兜底用）；缺省渲染嵌套路由的 `<Outlet/>`
#[component]
pub fn FrontendLayout(#[prop(optional)] children: Option<ChildrenFn>) -> impl IntoView {
    // 骨架屏占位的是正在加载的目标页，按地址栏选。
    let target_path = use_location().pathname;
    // 其余随路由变化的部分跟随已显示的页面：懒加载路由切换期间旧页面仍在，
    // 外壳提前换成新页面的宽度或导航会把旧页面套进错误的版式。
    let route = use_rendered_route();
    let pathname = Memo::new(move |_| route.read().pathname.clone());
    let nav_items = Memo::new(move |_| build_nav_items(&pathname.read()));
    let palette = use_search_palette_provider();
    // 页头与页脚在所有前台页面统一为 880px 索引宽度，栏目切换时外壳不跳动。
    // 主区按页面放宽：文章页 1240px（680px 正文 + 右侧目录），图鉴 1152px，其余与页头同宽。
    let max_width = "max-w-[880px]";
    // 文章页由 PostDetail 写自己的分享元信息，其余页面用站点的。
    let is_post = Memo::new(move |_| is_post_path(&pathname.read()));
    let main_width = Signal::derive(move || {
        pathname.with(|path| {
            if is_post_path(path) {
                "max-w-[1240px]"
            } else if is_showcase_path(path) {
                "max-w-6xl"
            } else {
                max_width
            }
        })
    });
    // 分享元信息里的页面地址：路径加查询串。
    let share_path = Memo::new(move |_| route.read().key());

    view! {
        <FrontendShell
            max_width=main_width
            header=move || {
                view! {
                    <Header
                        max_width=max_width
                        nav_items=nav_items
                        right_content=move || {
                            view! {
                                <SearchIconLink on_click=move |_: leptos::ev::MouseEvent| {
                                    palette.open()
                                }/>
                                <ThemeToggle icon_only=true/>
                            }
                        }
                    />
                }
            }
            footer=move || view! { <Footer max_width=max_width/> }
            main_content=move || {
                let children = children.clone();
                view! {
                    <Show when=move || !is_post.get()>
                        <SiteShareMeta path=share_path/>
                    </Show>
                    // 页面自己的 Suspense 先接住数据读取；这里兜底未被包住的资源读取。
                    // 错误边界在内层：路由内容出错时仍保留页头页脚与骨架屏逻辑。
                    <Suspense fallback=move || route_skeleton(&target_path.get_untracked())>
                        <ErrorBoundary fallback=route_error_fallback>
                            {match children {
                                Some(children) => children().into_any(),
                                None => view! { <Outlet/> }.into_any(),
                            }}
                        </ErrorBoundary>
                    </Suspense>
                }
            }
        />
        // 放在 main 外：栏目过渡只捕获 main，面板若在其中会被截进正文快照，
        // 遮罩只盖住正文区并跟着正文一起滑入。
        <SearchPalette/>
    }
}

/// 前台页面与图鉴样例共用的布局关系；路由内容由外层提供。
///
/// Props：
/// - `header` / `main_content` / `footer`：三块内容
/// - `max_width`：主区宽度类（可随路径变化）
/// - `min_height`：最外层容器的最小高度类
#[component]
pub(crate) fn FrontendShell(
    #[prop(into)] header: ViewFn,
    #[prop(into)] main_content: ViewFn,
    #[prop(into)] footer: ViewFn,
    #[prop(into, default = "max-w-[880px]".into())] max_width: Signal<&'static str>,
    #[prop(default = "min-h-screen")] min_height: &'static str,
) -> impl IntoView {
    view! {
        <div class=format!("{min_height} flex flex-col bg-surface-0")>
            {header.run()}
            <main class=move || {
                format!(
                    "flex-1 w-full {} mx-auto px-6 py-6 md:py-12 overflow-x-clip",
                    max_width.get(),
                )
            }>{main_content.run()}</main>
            {footer.run()}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_kind_matches_base_route_mapping() {
        assert_eq!(skeleton_kind("/notes"), SkeletonKind::Notes);
        assert_eq!(skeleton_kind("/notes/book/3"), SkeletonKind::Notes);
        assert_eq!(skeleton_kind("/notes/some-note"), SkeletonKind::Notes);
        assert_eq!(skeleton_kind("/page/3"), SkeletonKind::HomePage(3));
        assert_eq!(skeleton_kind("/page/0"), SkeletonKind::HomePage(1));
        assert_eq!(skeleton_kind("/page/-4"), SkeletonKind::HomePage(1));
        assert_eq!(skeleton_kind("/archives"), SkeletonKind::Archives);
        assert_eq!(skeleton_kind("/archives/"), SkeletonKind::Archives);
        assert_eq!(skeleton_kind("/friends"), SkeletonKind::Friends);
        assert_eq!(skeleton_kind("/search"), SkeletonKind::Search);
        assert_eq!(skeleton_kind("/post/hello"), SkeletonKind::PostDetail);
        assert_eq!(skeleton_kind("/changelog"), SkeletonKind::Changelog);
        assert_eq!(skeleton_kind("/about/writing"), SkeletonKind::WritingGuide);
        assert_eq!(skeleton_kind("/about/components"), SkeletonKind::Showcase);
        assert_eq!(
            skeleton_kind("/about/components/button"),
            SkeletonKind::Showcase
        );
    }

    #[test]
    fn skeleton_kind_decodes_tag_name() {
        assert_eq!(
            skeleton_kind("/tags/Rust%20%E7%BC%96%E7%A8%8B"),
            SkeletonKind::TagDetail("Rust 编程".to_string())
        );
        // `/tags` 本身不是标签页（服务端重定向到 /archives）。
        assert_eq!(skeleton_kind("/tags"), SkeletonKind::Fallback);
    }

    #[test]
    fn skeleton_kind_falls_back_to_home() {
        assert_eq!(skeleton_kind("/"), SkeletonKind::Fallback);
        assert_eq!(skeleton_kind("/about"), SkeletonKind::Fallback);
        assert_eq!(skeleton_kind("/page/abc"), SkeletonKind::Fallback);
        assert_eq!(skeleton_kind("/no-such-page"), SkeletonKind::Fallback);
    }

    #[test]
    fn shell_renders_regions_in_order_with_width_class() {
        let html = Owner::new().with(|| {
            view! {
                <FrontendShell
                    header=|| view! { <header id="h"></header> }
                    main_content=|| view! { <p id="m"></p> }
                    footer=|| view! { <footer id="f"></footer> }
                    max_width="max-w-6xl"
                    min_height="min-h-0"
                />
            }
            .to_html()
        });
        assert!(html.contains("class=\"min-h-0 flex flex-col bg-surface-0\""));
        assert!(html.contains(
            "class=\"flex-1 w-full max-w-6xl mx-auto px-6 py-6 md:py-12 overflow-x-clip\""
        ));
        let h = html.find("id=\"h\"").unwrap();
        let main = html.find("<main").unwrap();
        let m = html.find("id=\"m\"").unwrap();
        let f = html.find("id=\"f\"").unwrap();
        assert!(h < main && main < m && m < f);
    }

    #[test]
    fn shell_defaults_to_index_width() {
        let html = Owner::new().with(|| {
            view! {
                <FrontendShell
                    header=|| ()
                    main_content=|| ()
                    footer=|| ()
                />
            }
            .to_html()
        });
        assert!(html.contains("class=\"min-h-screen flex flex-col bg-surface-0\""));
        assert!(html.contains("flex-1 w-full max-w-[880px] mx-auto"));
    }
}
