//! 富文本编辑器往返守卫：比较原文与编辑器往返后的 Markdown 渲染结果。

use dioxus::prelude::*;

#[cfg(feature = "server")]
use super::helpers::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::error::AppError;

/// 富文本编辑器载入正文后比较原文与往返 Markdown：两者渲染出的 HTML 完全相同才返回 true。
#[server(CheckMarkdownRoundtrip, "/api")]
pub async fn check_markdown_roundtrip(
    original: String,
    roundtrip: String,
) -> Result<bool, ServerFnError> {
    let _user = get_current_admin_user().await?;
    #[cfg(feature = "server")]
    {
        let same = tokio::task::spawn_blocking(move || renders_identically(&original, &roundtrip))
            .await
            .map_err(|_| AppError::Internal("Markdown 渲染任务失败"))?;
        Ok(same)
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(false)
    }
}

#[cfg(feature = "server")]
fn renders_identically(original: &str, roundtrip: &str) -> bool {
    use crate::api::markdown::render_markdown_enhanced as render;
    render(original).html == render(roundtrip).html
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::renders_identically;

    #[test]
    fn alert_marker_escaped_by_editor_is_lossy() {
        assert!(!renders_identically("> [!NOTE]\n> x", "> \\[!NOTE\\]\n> x"));
    }

    #[test]
    fn table_column_padding_is_not_lossy() {
        assert!(renders_identically(
            "| a | b |\n| --- | --- |\n| 1 | 2 |",
            "| a   | b   |\n| --- | --- |\n| 1   | 2   |"
        ));
    }

    #[test]
    fn escaped_intraword_delimiters_are_not_lossy() {
        assert!(renders_identically("H~2~O 与 x^2^", "H\\~2\\~O 与 x^2\\^"));
    }
}
