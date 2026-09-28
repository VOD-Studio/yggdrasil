//! 文章元信息组件
//!
//! 展示文章发布日期、阅读时长与字数统计。

use dioxus::prelude::*;

use crate::models::post::Post;

/// 文章元信息组件。
///
/// Props：
/// - `post`：文章数据模型
///
/// 日期优先使用发布时间，否则回退到创建时间；每项在窄屏下整体换行。
#[component]
pub fn PostMeta(post: Post) -> Element {
    let date = post.formatted_date();

    rsx! {
        div { class: "post-meta", role: "group", aria_label: "文章元信息",
            span { class: "post-meta-item",
                svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round", "aria-hidden": "true",
                    rect { x: "3", y: "5", width: "18", height: "16", rx: "2" }
                    path { d: "M16 3v4M8 3v4M3 11h18" }
                }
                time { datetime: date.clone(), "{date}" }
            }
            span { class: "post-meta-item",
                svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round", "aria-hidden": "true",
                    circle { cx: "12", cy: "12", r: "9" }
                    path { d: "M12 7v5l3 2" }
                }
                span { strong { "{post.reading_time}" } " 分钟阅读" }
            }
            span { class: "post-meta-item",
                svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round", "aria-hidden": "true",
                    path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8ZM14 2v6h6M8 13h8M8 17h5" }
                }
                span { strong { "{post.word_count}" } " 字" }
            }
        }
    }
}
