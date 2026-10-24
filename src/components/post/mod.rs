//! 文章详情组件模块
//!
//! 提供文章详情页所需的各个子组件：封面、内容、页眉、元信息、页脚、面包屑、
//! 上一篇/下一篇导航与目录。
//!
//! 站内链接都是普通 `<a href>`，由 JS 路由协调器接管点击并做视图过渡；
//! 指向 `/admin/preview` 的预览页同样走协调器，不再需要整页加载的绕路写法。

/// 面包屑导航组件。
pub mod breadcrumbs;
/// 文章内容组件。
pub mod post_content;
/// 文章封面组件。
pub mod post_cover;
/// 文章页脚组件。
pub mod post_footer;
/// 文章页眉组件。
pub mod post_header;
/// 文章元信息组件。
pub mod post_meta;
/// 上一篇/下一篇导航组件。
pub mod post_nav_links;
/// 文章目录组件。
pub mod post_toc;
