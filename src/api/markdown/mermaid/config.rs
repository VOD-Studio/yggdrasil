//! Mermaid 站点配置与配色。
//!
//! 用 `theme: "base"`：base 主题不硬编码颜色，themeVariables 能完全控制调色板。
//! 颜色烤进 SVG 内联样式，不能用 CSS 变量，取值对应 `styles/theme.css` 的 token；
//! 浅色、深色各渲染一份，由 CSS 按 `html.dark` 显示其一。
//! 设计：节点是浮在代码块底色上的卡片（surface 色阶 + 发丝线边框），文字用 fg，连线用 fg-3，
//! 注释用陶土浅底；强调色留给作者用 classDef 手动指定。

use serde_json::{json, Value};

/// 一套主题的色调。
struct Tones {
    /// 图背景，等于代码块底色 surface-1。
    background: &'static str,
    node: &'static str,
    node_alt: &'static str,
    node_soft: &'static str,
    border: &'static str,
    border_strong: &'static str,
    text: &'static str,
    text_secondary: &'static str,
    text_tertiary: &'static str,
    line: &'static str,
    cluster: &'static str,
    note_background: &'static str,
    note_border: &'static str,
}

/// 浅色：白色节点浮在 surface-1 上。
const LIGHT: Tones = Tones {
    background: "#fafaf8",
    node: "#ffffff",
    node_alt: "#f5f5f3",
    node_soft: "#efefec",
    border: "#dcdbd7",
    border_strong: "#c4c3be",
    text: "#151515",
    text_secondary: "#525252",
    text_tertiary: "#6b6b69",
    line: "#6b6b69",
    cluster: "rgba(0, 0, 0, 0.02)",
    note_background: "#f7e8e1",
    note_border: "#d97757",
};

/// 深色：暖调深灰节点，surface 色阶递进。
const DARK: Tones = Tones {
    background: "#161615",
    node: "#20201e",
    node_alt: "#262624",
    node_soft: "#1a1a19",
    border: "#3a3936",
    border_strong: "#4f4e4a",
    text: "#f2f1ee",
    text_secondary: "#a9a7a1",
    text_tertiary: "#8b8984",
    line: "#a9a7a1",
    cluster: "rgba(255, 255, 255, 0.03)",
    note_background: "rgba(217, 119, 87, 0.15)",
    note_border: "#d97757",
};

const FONT_FAMILY: &str = "-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, 'PingFang SC', 'Hiragino Sans GB', 'Noto Sans SC', 'Microsoft YaHei', sans-serif";

fn theme_variables(t: &Tones) -> Value {
    json!({
        "background": t.background,
        // 节点填充：surface 色阶递进（主/次/三级），卡片质感
        "primaryColor": t.node,
        "secondaryColor": t.node_alt,
        "tertiaryColor": t.node_soft,
        "mainBkg": t.node,
        "nodeBkg": t.node,
        "secondBkg": t.node_alt,
        "primaryBorderColor": t.border,
        "secondaryBorderColor": t.border,
        "tertiaryBorderColor": t.border,
        "nodeBorder": t.border,
        "clusterBorder": t.border,
        "labelBoxBorderColor": t.border,
        "primaryTextColor": t.text,
        "secondaryTextColor": t.text_secondary,
        "tertiaryTextColor": t.text_tertiary,
        "textColor": t.text,
        "nodeTextColor": t.text,
        "titleColor": t.text,
        "classText": t.text,
        "labelTextColor": t.text,
        "lineColor": t.line,
        "defaultLinkColor": t.line,
        "arrowheadColor": t.line,
        // 时序图
        "actorBkg": t.node,
        "actorBorder": t.border,
        "actorTextColor": t.text,
        "actorLineColor": t.line,
        "signalColor": t.line,
        "signalTextColor": t.text,
        "loopTextColor": t.text,
        "sequenceNumberColor": t.background,
        "activationBkgColor": t.node_alt,
        "activationBorderColor": t.border_strong,
        // 边标签 / 子图背景
        "edgeLabelBackground": t.background,
        "labelBoxBkgColor": t.background,
        "clusterBkg": t.cluster,
        // 注释：陶土浅底
        "noteBkgColor": t.note_background,
        "noteBorderColor": t.note_border,
        "noteTextColor": t.text,
        "fontFamily": FONT_FAMILY,
        "fontSize": "16px",
    })
}

/// 站点级 Mermaid 配置；`node_spacing` 仅在子图框重叠、需要加大间距重排时传入。
pub(super) fn site_config(dark: bool, node_spacing: Option<u32>) -> Value {
    let mut flowchart = json!({
        "curve": "basis",
        "diagramPadding": 16,
        "useMaxWidth": true,
        "htmlLabels": true,
    });
    if let Some(spacing) = node_spacing {
        flowchart["nodeSpacing"] = json!(spacing);
    }
    json!({
        "layout": "dagre",
        "look": "classic",
        "theme": "base",
        "darkMode": dark,
        "securityLevel": "strict",
        "flowchart": flowchart,
        "themeVariables": theme_variables(if dark { &DARK } else { &LIGHT }),
    })
}
