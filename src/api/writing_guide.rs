//! 公开写作指南。源码与下载共用 public 中的文件，不读取令牌或用户数据。
#![allow(clippy::unused_unit, deprecated)]

use dioxus::prelude::*;

pub const SKILL_SOURCE: &str = include_str!("../../public/skills/yggdrasil-writing/SKILL.md");
pub const SKILL_URL: &str = "/skills/yggdrasil-writing/SKILL.md";

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct WritingGuideData {
    pub html: String,
    pub toc_html: String,
    pub configs: [String; 3],
    pub install_prompt: String,
}

/// Frontmatter 留在下载及复制的原文中；阅读版从 Markdown 正文开始。
#[cfg(feature = "server")]
fn render_guide() -> crate::api::markdown::RenderedContent {
    let body = SKILL_SOURCE
        .strip_prefix("---\n")
        .and_then(|text| text.split_once("\n---\n"))
        .map(|(_, body)| body.trim_start())
        .unwrap_or(SKILL_SOURCE);
    // 页面已有 h1，文档标题由面板的 h2 承担。
    let body = body.strip_prefix("# Yggdrasil 文章写作\n").unwrap_or(body);
    crate::api::markdown::render_markdown_enhanced(body)
}

#[server]
pub async fn get_writing_guide() -> Result<WritingGuideData, ServerFnError> {
    static RENDERED: std::sync::LazyLock<crate::api::markdown::RenderedContent> =
        std::sync::LazyLock::new(render_guide);
    let base_url = crate::api::settings::runtime_security_settings()
        .await
        .app_base_url;
    let base_url = if base_url.trim().is_empty() {
        crate::mcp::config::base_url_from_env()
    } else {
        base_url
    };
    let configs = crate::mcp::config::generate_client_configs(&base_url, "YOUR_YGGDRASIL_TOKEN");
    Ok(WritingGuideData {
        html: RENDERED.html.clone(),
        toc_html: RENDERED.toc_html.clone(),
        configs: [configs.codex_toml, configs.omp_json, configs.opencode_json],
        install_prompt: format!(
            "请从 {}{SKILL_URL} 下载完整的 SKILL.md，将 yggdrasil-writing 安装为用户全局 skill，保存到 ~/.agents/skills/yggdrasil-writing/SKILL.md（不要安装到项目目录），并确认当前 Agent 能发现该技能。",
            base_url.trim_end_matches('/')
        ),
    })
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn guide_renders_documentation_without_activating_example_blocks() {
        let rendered = render_guide();
        assert!(!rendered.html.contains("description:"));
        assert!(!rendered.html.contains("<h1"));
        assert!(rendered.html.contains("<h2"));
        assert!(rendered.html.contains("<table"));
        assert!(rendered.html.contains("language-markdown"));
        assert!(!rendered.html.contains("data-runnable=\"true\""));
        assert!(!rendered.html.contains("class=\"language-mermaid\""));
        assert!(rendered.toc_html.contains("可运行代码块"));
        assert!(SKILL_SOURCE.starts_with("---\nname: yggdrasil-writing\n"));
    }
}
