//! Mermaid 服务端渲染（merman，Mermaid 12.1 的 Rust 实现）。
//!
//! 每个 ```` ```mermaid ```` 块渲染成浅色、深色两份内联 SVG：
//! `<div class="mermaid-light">…</div><div class="mermaid-dark">…</div>`，由 CSS 按 `html.dark`
//! 显示其一（`styles/components/mermaid.css`）。HTML 标签模式，字宽见 [`metrics`]。
//! 渲染失败时降级为 `<pre class="mermaid-error">` 显示源码，文章照常保存。
//!
//! SVG 含 `<style>`、`foreignObject` 等 sanitizer 不放行的内容，所以 Markdown 管线先输出
//! 占位符、sanitize 之后再换成 SVG（[`Diagrams`]）；SVG 由 merman 生成，作者文本已被其转义。

mod config;
mod metrics;
mod svg;

use std::borrow::Cow;
use std::panic::{catch_unwind, AssertUnwindSafe};

use merman::svg::SvgRenderOptions;
use merman::{
    Engine, MermaidConfig, OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest,
};
use sha2::{Digest, Sha256};

/// 子图框重叠时 `nodeSpacing` 的最大加宽档数（每档 10px，起点 50）。
const MAX_SPACING_STEPS: u32 = 8;

/// 渲染一张图的浅色与深色两份 SVG，返回两个 `div`。
///
/// `ordinal` 为该图在文档中的序号，与源码摘要一起构成 SVG id，保证同页多图 id 不冲突、
/// 同一输入结果完全确定（往返守卫依赖这一点）。
pub(crate) fn render_diagram(source: &str, ordinal: usize) -> Result<String, String> {
    let hash = hex::encode(&Sha256::digest(source.as_bytes())[..4]);
    let svg_id = |variant: &str| {
        merman::svg::sanitize_svg_id(&format!("mermaid-{hash}-{ordinal}-{variant}"))
    };
    let light_id = svg_id("l");
    let mut light = render_one(false, None, source, &light_id)?;
    let mut node_spacing = None;
    for step in 1..=MAX_SPACING_STEPS {
        if !svg::has_cluster_overlap(&light) {
            break;
        }
        let spacing = 50 + 10 * step;
        light = render_one(false, Some(spacing), source, &light_id)?;
        node_spacing = Some(spacing);
    }
    // 深色沿用浅色最终的间距，两份几何一致。
    let dark = render_one(true, node_spacing, source, &svg_id("d"))?;
    Ok(format!(
        r#"<div class="mermaid-light">{}</div><div class="mermaid-dark">{}</div>"#,
        svg::finish(&light),
        svg::finish(&dark)
    ))
}

fn render_one(
    dark: bool,
    node_spacing: Option<u32>,
    source: &str,
    id: &str,
) -> Result<String, String> {
    let config = MermaidConfig::from_value(config::site_config(dark, node_spacing));
    let request = SvgRequest {
        environment: metrics::environment(),
        options: SvgRenderOptions {
            diagram_id: Some(id.to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    // 第三方解析器面对作者输入若 panic，不能让文章保存失败。
    let result = catch_unwind(AssertUnwindSafe(|| {
        Renderer::new()
            .with_engine(Engine::new().with_site_config(config))
            .render(RenderRequest::svg(source, OperationControl::new(), request))
    }));
    match result {
        Ok(Ok(RenderOutput::Svg(Some(svg)))) => Ok(svg.svg().to_string()),
        Ok(Ok(_)) => Err("未识别的 Mermaid 图表类型".to_string()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("图表渲染器内部错误".to_string()),
    }
}

/// 一次 Markdown 渲染中的全部图表：先写占位符，sanitize 之后再换成 SVG。
///
/// 占位符含随机 nonce，只在本次渲染内有效，作者在正文里写同样的文字不会被替换。
pub(super) struct Diagrams {
    nonce: String,
    seen: usize,
    rendered: Vec<String>,
}

impl Diagrams {
    pub(super) fn new() -> Self {
        Self {
            nonce: uuid::Uuid::new_v4().simple().to_string(),
            seen: 0,
            rendered: Vec::new(),
        }
    }

    fn marker(&self, index: usize) -> String {
        format!("ygg-mermaid-{}-{index}-end", self.nonce)
    }

    /// 渲染一个 mermaid 代码块；成功写占位 `<figure>`，失败降级为转义后的源码。
    pub(super) fn push(&mut self, html: &mut String, source: &str) {
        let result = render_diagram(source, self.seen);
        self.seen += 1;
        match result {
            Ok(inner) => {
                html.push_str(r#"<figure class="mermaid-diagram">"#);
                html.push_str(&self.marker(self.rendered.len()));
                html.push_str("</figure>");
                self.rendered.push(inner);
            }
            Err(e) => {
                tracing::warn!(error = %e, "Mermaid 图表渲染失败");
                html.push_str(r#"<pre class="mermaid-error"><code>"#);
                html.push_str(&crate::utils::html::escape_html(source));
                html.push_str("</code></pre>");
            }
        }
    }

    /// 把已 sanitize 的 HTML 中的占位符换成 SVG。
    pub(super) fn substitute(self, mut html: String) -> String {
        for (index, inner) in self.rendered.iter().enumerate() {
            let marker = self.marker(index);
            if let Some(at) = html.find(&marker) {
                html.replace_range(at..at + marker.len(), inner);
            } else {
                tracing::error!(index, "Mermaid 占位符在清理后丢失");
            }
        }
        html
    }
}

/// 删掉深色版本，只留浅色 SVG（Feed 阅读器没有站点 CSS，不处理会同时显示两份）。
pub(crate) fn strip_dark_variants(html: &str) -> Cow<'_, str> {
    const OPEN: &str = r#"<div class="mermaid-dark">"#;
    // 图表末尾特有的结束序列；只删到 `</svg></div>`，保留 `</figure>`。
    const CLOSE: &str = "</svg></div></figure>";
    const KEEP: usize = "</figure>".len();
    if !html.contains(OPEN) {
        return Cow::Borrowed(html);
    }
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find(OPEN) {
        let Some(close) = rest[start..].find(CLOSE) else {
            break;
        };
        out.push_str(&rest[..start]);
        rest = &rest[start + close + CLOSE.len() - KEEP..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::api::markdown::render_markdown_enhanced;
    use merman::svg::TextStyle;

    const OVERLAP_FIXTURE: &str = r#"flowchart TD
    subgraph CargoChef [Cargo Chef 3 阶段缓存物理分层]
        Planner[Stage 1: Planner<br/>生成 recipe.json 依赖清单] --> Cooker
        Cooker[Stage 2: Cooker 阶段<br/>仅根据 recipe.json 编译第三方依赖 .rlib] --> Builder
        Builder[Stage 3: Builder 阶段<br/>注入应用源码 src/ 进行应用链接]
    end

    subgraph CacheMechanism ["BuildKit + GHA Cache (mode=max)"]
        CookerCache[(GHA Cache Server<br/>scope=amd64 / arm64<br/>缓存完整 .rlib 中间产物)]
        Cooker <-->|源码未改动依赖时秒级命中| CookerCache
    end

    style CargoChef fill:#0f172a,stroke:#334155,color:#fff
    style CacheMechanism fill:#1e1b4b,stroke:#6366f1,color:#fff
"#;

    const SIMPLE: &str = "```mermaid\nflowchart LR\n  A[开始] --> B[结束]\n```";

    fn regular(size: f64) -> TextStyle {
        TextStyle {
            font_size: size,
            ..Default::default()
        }
    }

    #[test]
    fn text_width_takes_widest_font_and_falls_back() {
        let a = metrics::text_width_px("A", &regular(16.0));
        // SF Pro Text 的 'A' 最宽：0.673828125 em 向上取整到 0.674。
        assert!((a - 0.674 * 16.0).abs() < 1e-9, "{a}");
        // 西文字体都没有「中」，回退到 Noto Sans CJK 的 1 em。
        assert!((metrics::text_width_px("中", &regular(16.0)) - 16.0).abs() < 1e-9);
        // 基本多文种平面以外按 1 em 计。
        assert!((metrics::text_width_px("😀", &regular(16.0)) - 16.0).abs() < 1e-9);
        // 零宽字符不占宽度。
        assert_eq!(metrics::text_width_px("A\u{200B}", &regular(16.0)), a);
        let bold = TextStyle {
            font_weight: Some("700".into()),
            ..regular(16.0)
        };
        assert!(metrics::text_width_px("A", &bold) > a);
    }

    fn split_variants(inner: &str) -> (&str, &str) {
        let dark_at = inner.find(r#"<div class="mermaid-dark">"#).unwrap();
        (&inner[..dark_at], &inner[dark_at..])
    }

    fn view_box(svg: &str) -> &str {
        let at = svg.find("viewBox=\"").unwrap() + "viewBox=\"".len();
        svg[at..].split('"').next().unwrap()
    }

    #[test]
    fn pipeline_inlines_both_variants_after_sanitize() {
        let html = render_markdown_enhanced(SIMPLE).html;
        assert!(
            html.contains(r#"<figure class="mermaid-diagram">"#),
            "{html}"
        );
        assert!(html.contains(r#"class="mermaid-light""#));
        assert!(html.contains(r#"class="mermaid-dark""#));
        assert_eq!(html.matches("<svg").count(), 2);
        assert!(
            html.matches("<style").count() >= 2,
            "SVG 样式被 sanitizer 剥掉"
        );
        assert!(!html.contains("ygg-mermaid-"));
        assert_eq!(
            html,
            render_markdown_enhanced(SIMPLE).html,
            "渲染结果须确定"
        );
    }

    #[test]
    fn invalid_diagram_degrades_to_source() {
        let html = render_markdown_enhanced("```mermaid\n这不是图表 <b>\n```").html;
        // 与普通代码块一样保留围栏内容末尾的换行；源码经转义。
        assert!(
            html.contains("<pre class=\"mermaid-error\"><code>这不是图表 &lt;b&gt;\n</code></pre>"),
            "{html}"
        );
        assert!(!html.contains("mermaid-diagram"));
    }

    #[test]
    fn overlapping_clusters_get_wider_spacing_and_lifted_titles() {
        let inner = render_diagram(OVERLAP_FIXTURE, 0).unwrap();
        let (light, dark) = split_variants(&inner);
        assert!(!svg::has_cluster_overlap(light));
        assert!(!svg::has_cluster_overlap(dark));
        assert_eq!(view_box(light), view_box(dark));
        for variant in [light, dark] {
            for group in variant.split(r#"<g class="clusters">"#).skip(1) {
                if let Some(at) = group.find(r#"<g class="cluster-label""#) {
                    assert!(
                        !group[at..].contains(r#"<g class="cluster" id="#),
                        "子图标题之后不应再有子图底框"
                    );
                }
            }
        }
    }

    #[test]
    fn strip_dark_variants_keeps_light_svg() {
        let html = render_markdown_enhanced(SIMPLE).html;
        let stripped = strip_dark_variants(&html);
        assert!(!stripped.contains("mermaid-dark"));
        assert!(!stripped.contains(r#"-d""#));
        let light_start = html.find(r#"<div class="mermaid-light">"#).unwrap();
        let light_end = html.find(r#"<div class="mermaid-dark">"#).unwrap();
        assert!(stripped.contains(&html[light_start..light_end]));
        assert!(stripped.ends_with("</figure>"), "{stripped}");
        assert!(matches!(
            strip_dark_variants("<p>无图</p>"),
            Cow::Borrowed(_)
        ));
    }
}
