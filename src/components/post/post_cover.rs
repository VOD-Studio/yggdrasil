//! 文章封面组件
//!
//! 在文章详情页渲染封面大图，使用 blur-up 双层结构与 lightbox.js 灯箱。
//! 封面为单张（data-single），不参与正文图集切换。

use leptos::prelude::*;

/// 文章封面组件。
///
/// Props：
/// - `src`：封面原图 URL
/// - `post_id`：参与文章共享过渡的文章 ID（可选）
/// - `eager`：首屏封面由浏览器按显示宽度优先加载
///
/// 渲染 `.blur-img` 结构，与正文图片一致；`data-single="true"` 标记为单张，
/// 由 `lightbox.js` 接管点击放大（原地缩放飞出 + 原图展示）。
/// 服务端读取真实尺寸写入 `--ar`，确保占位期间维持正确宽高比。
#[component]
pub fn PostCover(
    #[prop(into)] src: String,
    #[prop(optional, into)] post_id: MaybeProp<i32>,
    #[prop(optional)] eager: bool,
) -> impl IntoView {
    let upload = src.strip_prefix("/uploads/");
    // SSR 时读真实尺寸算 --ar；WASM 端不读（HTML 已在 SSR 写入）。
    let ar_style = {
        #[cfg_attr(not(feature = "server"), allow(unused_mut))]
        let mut s = String::new();
        #[cfg(feature = "server")]
        {
            if let Some(rel) = upload.map(|p| p.split(['?', '#']).next().unwrap_or(p)) {
                if let Some((w, h)) = crate::api::image::get_image_dimensions(rel) {
                    // CSS aspect-ratio 用斜杠分隔（width / height）
                    s = format!("--ar:{} / {};", w, h);
                }
            }
        }
        s
    };

    // 只有站内上传端点支持按宽度缩放，外链和静态图片沿用原地址。
    let placeholder_src = upload.map(|_| width_url(&src, 20));
    let srcset = (eager && upload.is_some()).then(|| {
        [480, 800, 1200]
            .map(|width| format!("{} {width}w", width_url(&src, width)))
            .join(", ")
    });
    let sizes = srcset
        .as_ref()
        .map(|_| "(max-width: 728px) calc(100vw - 3rem), 680px");
    let full_src = if upload.is_some() {
        width_url(&src, 1200)
    } else {
        src
    };
    let post_id = post_id.get_untracked();

    view! {
        <figure
            class="entry-cover"
            data-vt-post-id=post_id.map(|id| id.to_string())
            data-vt-role=post_id.map(|_| "cover")
        >
            <span class="blur-img entry-cover-blur lightbox-single" style=ar_style>
                {placeholder_src.map(|src| view! { <img class="blur-img-placeholder" src=src alt=""/> })}
                <img
                    class="blur-img-full"
                    alt="封面图片"
                    fetchpriority=eager.then_some("high")
                    sizes=sizes
                    srcset=srcset
                    src=eager.then(|| full_src.clone())
                    data-src=full_src
                />
            </span>
        </figure>
    }
}

/// 替换已有的宽度参数，保留其余图片变换和版本参数。片段无需发送给图片服务。
fn width_url(src: &str, width: u16) -> String {
    let url = src.split('#').next().unwrap_or(src);
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let mut variant = format!("{path}?w={width}");
    for field in query
        .split('&')
        .filter(|field| !field.is_empty() && field.split('=').next() != Some("w"))
    {
        variant.push('&');
        variant.push_str(field);
    }
    variant
}

#[cfg(test)]
mod tests {
    use super::width_url;

    #[test]
    fn cover_width_preserves_other_image_parameters() {
        assert_eq!(
            width_url("/uploads/a.webp?w=900&format=webp&v=42#p", 480),
            "/uploads/a.webp?w=480&format=webp&v=42"
        );
        assert_eq!(width_url("/uploads/b.jpg", 20), "/uploads/b.jpg?w=20");
    }
}
