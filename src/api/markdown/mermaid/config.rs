//! Mermaid 站点配置与配色。
//!
//! 用 `theme: "base"`：base 主题不硬编码颜色，themeVariables 能完全控制调色板。
//! 颜色烤进 SVG 内联样式，不能用 CSS 变量，取值对应 `styles/theme.css` 的 token；
//! 浅色、深色各渲染一份，由 CSS 按 `html.dark` 显示其一。
//! 设计：节点是浮在代码块底色上的卡片（surface 色阶 + 发丝线边框），文字用 fg，连线用 fg-3，
//! 注释用陶土浅底；强调色留给作者用 classDef 手动指定。
//!
//! base 主题只给通用变量，各图专用的颜色（甘特任务、ER 行、饼图、git 分支、时间线/思维导图的
//! cScale 等）由 `primaryColor` 推导：白色节点推出的是一片灰，深色模式更会推出黑底黑字、
//! 浅底白字。所以这里把专用变量全部显式给出；所有颜色都必须不透明，
//! Mermaid 的 `fade()` 会直接替换 alpha（半透明的子图底色到了方块图里会变成 50% 黑）。

use serde_json::{json, Map, Value};

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
    /// 子图、甘特分区底色：背景上叠一层极淡的墨色（已合成为不透明色）。
    cluster: &'static str,
    note_background: &'static str,
    note_border: &'static str,
    /// 系列色：图表数据、分支、曲线（陶土、鼠尾草、石板蓝、赭黄、梅紫、青、玫瑰、橄榄）。
    series: [&'static str; 8],
    /// 系列色对应的分区底色（前 6 个系列），时间线、思维导图、看板、矩形树图的分区用。
    sections: [&'static str; 6],
    /// 系列色上的文字（git 分支名、饼图百分比）。
    on_series: &'static str,
    crit_fill: &'static str,
    crit_border: &'static str,
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
    cluster: "#f5f5f3",
    note_background: "#f7e8e1",
    note_border: "#d97757",
    series: [
        "#d97757", "#5f8f6e", "#5b7fa6", "#c49a3c", "#93689c", "#4a9a92", "#c46b7d", "#8a8f4a",
    ],
    sections: [
        "#f3ddd3", "#dce9df", "#dbe4ef", "#f1e6cc", "#e9dcec", "#d5eae8",
    ],
    on_series: "#ffffff",
    crit_fill: "#fbeaea",
    crit_border: "#c93a3a",
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
    cluster: "#1d1d1c",
    note_background: "#33251f",
    note_border: "#d97757",
    series: [
        "#e08a6c", "#8fbf9f", "#8fabd0", "#ddb965", "#b993c2", "#7cc4bb", "#e0939f", "#b4b874",
    ],
    sections: [
        "#4a2e24", "#2b3d31", "#2a3443", "#463c25", "#3d2f42", "#233f3d",
    ],
    on_series: "#161615",
    crit_fill: "#3d2222",
    crit_border: "#f87171",
};

/// Mermaid 的 cScale 色阶有 12 档，git 分支 8 档，饼图 12 档。
const SCALE_LEN: usize = 12;
const GIT_LEN: usize = 8;

const FONT_FAMILY: &str = "-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, 'PingFang SC', 'Hiragino Sans GB', 'Noto Sans SC', 'Microsoft YaHei', sans-serif";

fn tones(dark: bool) -> &'static Tones {
    if dark {
        &DARK
    } else {
        &LIGHT
    }
}

/// `radar` 为 true 时 cScale 用系列色：雷达图拿 cScale 画曲线（描边 + 半透明填充），
/// 分区底色那样的浅色会看不见；其余图拿 cScale 画分区底色。
fn theme_variables(t: &Tones, radar: bool) -> Value {
    let mut v = json!({
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
    });
    // 各图专用变量；与通用变量分成两段，单个 json! 会超出宏递归上限。
    let diagrams = json!({
        // ER 属性表：奇偶行在节点色阶里交替
        "rowOdd": t.node,
        "rowEven": t.node_alt,
        "attributeBackgroundColorOdd": t.node,
        "attributeBackgroundColorEven": t.node_alt,
        // 甘特图：分区几乎透明，普通任务蓝、进行中陶土、已完成灰、关键任务红
        "sectionBkgColor": t.cluster,
        "sectionBkgColor2": t.cluster,
        "altSectionBkgColor": t.background,
        "excludeBkgColor": t.node_soft,
        "gridColor": t.border,
        "todayLineColor": t.series[0],
        "vertLineColor": t.line,
        "taskBkgColor": t.sections[2],
        "taskBorderColor": t.series[2],
        "activeTaskBkgColor": t.sections[0],
        "activeTaskBorderColor": t.series[0],
        "doneTaskBkgColor": t.node_alt,
        "doneTaskBorderColor": t.border_strong,
        "critBkgColor": t.crit_fill,
        "critBorderColor": t.crit_border,
        "taskTextColor": t.text,
        "taskTextLightColor": t.text,
        "taskTextDarkColor": t.text,
        "taskTextOutsideColor": t.text,
        "taskTextClickableColor": t.text,
        // git 图的提交标签与标记
        "commitLabelColor": t.text_secondary,
        "commitLabelBackground": t.background,
        "tagLabelColor": t.text,
        "tagLabelBackground": t.node,
        "tagLabelBorder": t.border_strong,
        // 饼图：系列色实心扇区，扇区间用背景色分隔
        "pieStrokeColor": t.background,
        "pieOuterStrokeColor": t.background,
        "pieOpacity": "1",
        "pieSectionTextColor": t.on_series,
        "pieTitleTextColor": t.text,
        "pieLegendTextColor": t.text,
        // 象限图：四个象限同色，靠分隔线区分
        "quadrant1Fill": t.node,
        "quadrant2Fill": t.node,
        "quadrant3Fill": t.node,
        "quadrant4Fill": t.node,
        "quadrantPointFill": t.series[0],
        "quadrantPointTextFill": t.text,
        "quadrantInternalBorderStrokeFill": t.border,
        "quadrantExternalBorderStrokeFill": t.border_strong,
        // 架构图
        "archEdgeColor": t.line,
        "archEdgeArrowColor": t.line,
        "archEdgeWidth": "2",
        "archGroupBorderColor": t.border_strong,
        "xyChart": {
            "backgroundColor": t.background,
            "titleColor": t.text,
            "dataLabelColor": t.text,
            "legendTextColor": t.text,
            "xAxisTitleColor": t.text_secondary,
            "xAxisLabelColor": t.text_secondary,
            "xAxisTickColor": t.border_strong,
            "xAxisLineColor": t.border_strong,
            "yAxisTitleColor": t.text_secondary,
            "yAxisLabelColor": t.text_secondary,
            "yAxisTickColor": t.border_strong,
            "yAxisLineColor": t.border_strong,
            "plotColorPalette": t.series.join(","),
        },
        "radar": {
            "axisColor": t.line,
            "graticuleColor": t.node_alt,
            "graticuleOpacity": 0.5,
            "curveOpacity": 0.3,
        },
    });
    let vars = v.as_object_mut().expect("themeVariables 是对象");
    if let Value::Object(diagrams) = diagrams {
        vars.extend(diagrams);
    }
    // cScale：时间线、思维导图、看板、矩形树图的分区底色；描边、分隔线用对应系列色。
    for i in 0..SCALE_LEN {
        let fill = if radar {
            t.series[i % t.series.len()]
        } else {
            t.sections[i % t.sections.len()]
        };
        let accent = t.series[i % t.sections.len()];
        set(vars, format!("cScale{i}"), fill);
        set(vars, format!("cScalePeer{i}"), accent);
        set(vars, format!("cScaleInv{i}"), accent);
        set(vars, format!("cScaleLabel{i}"), t.text);
        set(vars, format!("pie{}", i + 1), t.series[i % t.series.len()]);
    }
    for i in 0..GIT_LEN {
        set(vars, format!("git{i}"), t.series[i]);
        // 合并提交等空心点的填充
        set(vars, format!("gitInv{i}"), t.background);
        set(vars, format!("gitBranchLabel{i}"), t.on_series);
    }
    v
}

fn set(vars: &mut Map<String, Value>, key: String, color: &str) {
    vars.insert(key, Value::from(color));
}

/// 站点级 Mermaid 配置。
///
/// `node_spacing` 仅在子图框重叠、需要加大间距重排时传入；`source` 用来识别雷达图（见
/// [`theme_variables`]）。
pub(super) fn site_config(dark: bool, node_spacing: Option<u32>, source: &str) -> Value {
    let t = tones(dark);
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
        // C4 与数据包图的颜色是图表配置而不是 themeVariables。
        // C4 形状保留标准的蓝/灰，只把关系线与标签跟随主题（默认 #444 在深色里看不见）；
        // 形状间距从 50 加到 70，否则横向关系的标签压到两侧形状上。
        "c4": {
            "c4ShapeMargin": 70,
            "relationship_text_color": t.text_secondary,
            "relationship_line_color": t.line,
            "boundary_border_color": t.border_strong,
            "boundary_font_color": t.text_secondary,
        },
        "packet": {
            "startByteColor": t.text_tertiary,
            "endByteColor": t.text_tertiary,
            "labelColor": t.text,
            "titleColor": t.text,
            "blockStrokeColor": t.border_strong,
            "blockFillColor": t.node,
        },
        "themeVariables": theme_variables(t, is_radar(source)),
    })
}

/// 图表类型关键字是否为雷达图；跳过空行、`%%` 注释与指令、YAML frontmatter。
fn is_radar(source: &str) -> bool {
    let mut lines = source
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let mut first = lines.next();
    if first == Some("---") {
        first = lines.by_ref().skip_while(|l| *l != "---").nth(1);
    }
    first
        .and_then(|l| l.split_whitespace().next())
        .is_some_and(|kind| kind == "radar-beta" || kind == "radar")
}

/// 主题变量管不到的几处颜色，追加到 SVG 自带的 `<style>`（规则以 SVG id 限定作用域）。
///
/// - 甘特图网格线是 d3 坐标轴的 `<line stroke="currentColor">`，会继承正文文字色（浅色下是黑线），
///   改为继承所在 `.tick` 的 `gridColor`；
/// - ER 图「零」基数的圆圈写死 `fill="white"`，深色下成了实心白点，改成背景色（空心）；
/// - 时序图消息标签描一圈背景色：自调用标签居中压在生命线上，跨参与者的长标签也会穿过
///   中间的生命线，描边让线在文字处断开；
/// - 桑基图连线用 `mix-blend-mode: multiply`（行内样式），深色背景上乘出来接近黑色，
///   深色改用对称的 `screen`。
pub(super) fn fixup_css(svg_id: &str, dark: bool) -> String {
    let background = tones(dark).background;
    let mut css = format!(
        "#{svg_id} .grid .tick line{{stroke:inherit;}}\
         #{svg_id} .marker.er circle{{fill:{background};}}\
         #{svg_id} .messageText{{paint-order:stroke;stroke:{background};stroke-width:4px;stroke-linejoin:round;}}"
    );
    if dark {
        css.push_str(&format!(
            "#{svg_id} .link{{mix-blend-mode:screen!important;}}"
        ));
    }
    css
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::is_radar;

    #[test]
    fn radar_detection_skips_comments_and_frontmatter() {
        assert!(is_radar("radar-beta\n  axis a"));
        assert!(is_radar("\n%% 注释\n%%{init: {}}%%\n  radar-beta\n"));
        assert!(is_radar("---\ntitle: 雷达\n---\nradar-beta\n"));
        assert!(!is_radar("flowchart LR\n  radar --> b"));
        assert!(!is_radar("---\nradar-beta\n---\npie\n"));
        assert!(!is_radar(""));
    }
}
