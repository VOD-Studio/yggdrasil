//! 文章元信息组件
//!
//! 展示文章发布日期、阅读时长与字数统计。

use leptos::prelude::*;

use crate::models::post::PostHeaderData;

/// 文章元信息组件。
///
/// Props：
/// - `post`：页眉展示所需的最小字段集（P4.2：不携带完整正文）
///
/// 日期优先使用发布时间，否则回退到创建时间，用标题字体的旧式数字；阅读时长与字数
/// 用等宽数字。窄屏下各项整体换行。
#[component]
pub fn PostMeta(post: PostHeaderData) -> impl IntoView {
    let at = post.published_at.unwrap_or(post.created_at);
    let datetime = at.format("%Y-%m-%d").to_string();
    let date = at.format("%Y.%m.%d").to_string();
    let words = group_digits(post.word_count);
    let reading = format!("{} 分钟", post.reading_time);
    let words = format!("{words} 字");

    view! {
        <p class="post-meta" aria-label="文章元信息">
            <time class="post-meta-date" datetime=datetime>{date}</time>
            <span class="tabular-nums">{reading}</span>
            <span class="tabular-nums">{words}</span>
        </p>
    }
}

/// 千位分隔：`4866` → `4,866`。
pub(crate) fn group_digits(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::group_digits;

    #[test]
    fn groups_thousands() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(866), "866");
        assert_eq!(group_digits(4866), "4,866");
        assert_eq!(group_digits(1_234_567), "1,234,567");
    }
}
