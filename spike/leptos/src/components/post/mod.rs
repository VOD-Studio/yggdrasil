//! 文章详情组件模块
//!
//! 提供文章详情页所需的各个子组件：封面、内容、页眉、元信息、页脚、面包屑、
//! 上一篇/下一篇导航与目录，以及文章页的分享元信息与增强资源注入。

/// 面包屑导航组件。
pub mod breadcrumbs;
/// 正文增强脚本与灯箱资源的按需注入。
pub mod enhance_assets;
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
/// 文章页分享元信息。
pub mod share_meta;

/// 文章详情路由 `/post/:slug` 的地址（对应 Dioxus `Route::PostDetail { slug }.to_string()`）。
pub(crate) fn post_href(slug: &str) -> String {
    format!("/post/{}", encode_path_segment(slug))
}

/// 标签路由 `/tags/:tag` 的地址（对应 Dioxus `Route::TagDetail { tag }.to_string()`）。
pub(crate) fn tag_href(tag: &str) -> String {
    format!("/tags/{}", encode_path_segment(tag))
}

/// 按 Dioxus 路由的 `PATH_ASCII_SET` 百分号编码路径段：控制字符、空格、
/// `"#<>?^`{}` 与全部非 ASCII 字节，其余（含 `/`）原样保留，生成的地址与原版逐字节一致。
fn encode_path_segment(segment: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(segment.len());
    for &byte in segment.as_bytes() {
        let escape = byte < 0x20
            || byte >= 0x7F
            || matches!(
                byte,
                b' ' | b'"' | b'#' | b'<' | b'>' | b'?' | b'^' | b'`' | b'{' | b'}'
            );
        if escape {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0F) as usize] as char);
        } else {
            out.push(byte as char);
        }
    }
    out
}

/// 整页加载链接的 `rel`：Leptos 路由不拦截 `rel="external"` 的 `<a>`
/// （对应 Dioxus `NavigationTarget::External`）；客户端路由不加该属性。
pub(crate) fn full_reload_rel(full_reload: bool) -> Option<&'static str> {
    full_reload.then_some("external")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_like_dioxus_path_segments() {
        assert_eq!(post_href("hello-world"), "/post/hello-world");
        assert_eq!(tag_href("源码"), "/tags/%E6%BA%90%E7%A0%81");
        assert_eq!(tag_href("a b?#"), "/tags/a%20b%3F%23");
    }
}
