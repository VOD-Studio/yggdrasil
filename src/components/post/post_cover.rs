//! 文章封面组件
//!
//! 在文章详情页渲染封面大图，使用 blur-up 双层结构与 lightbox.js 灯箱。
//! 封面为单张（data-single），不参与正文图集切换。

use dioxus::prelude::*;

/// 文章封面组件。
///
/// Props：
/// - `src`：封面原图 URL
/// - `post_id`：参与文章共享过渡的文章 ID（可选）
///
/// 渲染 `.blur-img` 结构，与正文图片一致；`data-single="true"` 标记为单张，
/// 由 `lightbox.js` 接管点击放大（原地缩放飞出 + 原图展示）。
/// 服务端读取真实尺寸写入 `--ar`，确保占位期间维持正确宽高比。
#[component]
pub fn PostCover(src: String, #[props(default)] post_id: Option<i32>) -> Element {
    // SSR 时读真实尺寸算 --ar；WASM 端不读（HTML 已在 SSR 写入）。
    let ar_style = {
        #[cfg_attr(not(feature = "server"), allow(unused_mut))]
        let mut s = String::new();
        #[cfg(feature = "server")]
        {
            if let Some(rel) = src
                .strip_prefix("/uploads/")
                .map(|p| p.split('?').next().unwrap_or(p))
            {
                if let Some((w, h)) = crate::api::image::get_image_dimensions(rel) {
                    // CSS aspect-ratio 用斜杠分隔（width / height）
                    s = format!("--ar:{} / {};", w, h);
                }
            }
        }
        s
    };

    // 占位图 ?w=20，展示图 ?w=1200；灯箱原图由 lightbox.js 去 query 得到。
    let placeholder_src = if src.contains('?') {
        format!("{}&w=20", src.split('?').next().unwrap_or(&src))
    } else {
        format!("{}?w=20", src)
    };
    let full_src = if src.contains('?') {
        format!("{}&w=1200", src.split('?').next().unwrap_or(&src))
    } else {
        format!("{}?w=1200", src)
    };

    rsx! {
        figure { class: "entry-cover",
            "data-vt-post-id": post_id.map(|id| id.to_string()),
            "data-vt-role": post_id.map(|_| "cover"),
            span {
                class: "blur-img entry-cover-blur lightbox-single",
                style: "{ar_style}",
                img {
                    class: "blur-img-placeholder",
                    src: "{placeholder_src}",
                    alt: "封面图片",
                }
                img {
                    class: "blur-img-full",
                    "data-src": "{full_src}",
                    alt: "封面图片",
                }
            }
        }
    }
}
