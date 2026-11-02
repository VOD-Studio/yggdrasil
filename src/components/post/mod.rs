//! 文章详情组件模块
//!
//! 对外提供文章详情页的封面、内容、页眉、页脚与目录。面包屑、元信息与上一篇/下一篇
//! 导航只是页眉、页脚的组成部分，仅在本模块内可见，组件图鉴随页眉、页脚一起展示。
//!
//! 站内链接都是普通 `<a href>`，由 JS 路由协调器接管点击并做视图过渡；
//! 指向 `/admin/preview` 的预览页同样走协调器，不再需要整页加载的绕路写法。

/// 面包屑导航组件。
mod breadcrumbs;
/// 文章内容组件。
pub mod post_content;
/// 文章封面组件。
pub mod post_cover;
/// 文章页脚组件。
pub mod post_footer;
/// 文章页眉组件。
pub mod post_header;
/// 文章元信息组件；`group_digits` 供首页与写作页复用。
pub mod post_meta;
/// 上一篇/下一篇导航组件。
mod post_nav_links;
/// 文章目录组件。
pub mod post_toc;
