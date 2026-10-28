//! 分享图渲染：文章图与站点图，均为 1200×630 PNG（见 docs/v1.0-重设计方案.md「分享图」）。
//!
//! 版式按效果稿「网站图标与分享图」画板放大一倍：文章图浅底，元信息、衬线标题（最多
//! 三行）、摘要（最多两行）与底部文字标，右侧一条陶土橙细边；站点图深底，文字标、
//! 两行标语与「地址·篇数」。
//!
//! 字体随二进制嵌入（`scripts/subset-og-fonts.ts` 裁剪的 Noto Serif / Sans SC），不读
//! 系统字体，渲染结果与部署环境无关。SVG 文字不会自动折行，这里按字体字宽量出每行；
//! 字体缺字时返回 [`OgError::MissingGlyph`]，由调用方改用站点图。
#![cfg(feature = "server")]

use std::fmt;
use std::sync::{Arc, LazyLock};

use resvg::{tiny_skia, usvg};
use skrifa::prelude::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

use crate::models::share::PostShareCard;
use crate::utils::html::escape_html;

pub const WIDTH: u32 = 1200;
pub const HEIGHT: u32 = 630;

static SERIF: &[u8] = include_bytes!("fonts/NotoSerifSC-Medium.otf");
static SANS: &[u8] = include_bytes!("fonts/NotoSansSC-Regular.otf");
static SANS_BOLD: &[u8] = include_bytes!("fonts/NotoSansSC-Bold.otf");

static FONT_DB: LazyLock<Arc<usvg::fontdb::Database>> = LazyLock::new(|| {
    let mut db = usvg::fontdb::Database::new();
    for data in [SERIF, SANS, SANS_BOLD] {
        db.load_font_source(usvg::fontdb::Source::Binary(Arc::new(data)));
    }
    db.set_serif_family("Noto Serif SC");
    db.set_sans_serif_family("Noto Sans SC");
    Arc::new(db)
});

const CLAY: &str = "#d97757";

/// 站点分享图上的文字。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SiteShareCard {
    /// 标语第一行。
    pub title: String,
    /// 标语第二行（次要色），可为空。
    pub subtitle: String,
    /// 底部「站点地址·N 篇文章」，可为空。
    pub footer: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OgError {
    /// 嵌入字体没有这个字。
    MissingGlyph(char),
    Svg(String),
    Encode,
}

impl fmt::Display for OgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OgError::MissingGlyph(c) => write!(f, "分享图字体缺字：{c:?}"),
            OgError::Svg(e) => write!(f, "分享图 SVG 解析失败：{e}"),
            OgError::Encode => write!(f, "分享图 PNG 编码失败"),
        }
    }
}

impl std::error::Error for OgError {}

/// 一种文字样式。尺寸为 1200×630 画布上的像素。
#[derive(Clone, Copy)]
struct Style {
    font: &'static [u8],
    family: &'static str,
    weight: u16,
    size: f32,
    line_height: f32,
    letter_spacing: f32,
}

const POST_META: Style = Style {
    font: SANS,
    family: "Noto Sans SC",
    weight: 400,
    size: 26.0,
    line_height: 42.0,
    letter_spacing: 0.0,
};
const POST_TITLE: Style = Style {
    font: SERIF,
    family: "Noto Serif SC",
    weight: 500,
    size: 60.0,
    line_height: 79.0,
    letter_spacing: -0.6,
};
const POST_SUMMARY: Style = Style {
    font: SANS,
    family: "Noto Sans SC",
    weight: 400,
    size: 28.0,
    line_height: 48.0,
    letter_spacing: 0.0,
};
const WORDMARK: Style = Style {
    font: SANS_BOLD,
    family: "Noto Sans SC",
    weight: 700,
    size: 30.0,
    line_height: 48.0,
    letter_spacing: -0.3,
};
const SITE_WORDMARK: Style = Style {
    size: 36.0,
    line_height: 58.0,
    letter_spacing: -0.36,
    ..WORDMARK
};
const SITE_TAGLINE: Style = Style {
    size: 68.0,
    line_height: 88.0,
    letter_spacing: -0.68,
    ..POST_TITLE
};
const SITE_FOOTER: Style = POST_META;

/// 渲染文章分享图。
pub fn render_post(card: &PostShareCard) -> Result<Vec<u8>, OgError> {
    const PAD_X: f32 = 80.0;
    const PAD_Y: f32 = 72.0;
    let max_width = WIDTH as f32 - PAD_X * 2.0;

    let meta = wrap(&POST_META, &card.meta, max_width, 1)?;
    let title = wrap(&POST_TITLE, &card.title, max_width, 3)?;
    let summary = wrap(&POST_SUMMARY, &card.summary, max_width, 2)?;

    let mut svg = svg_open("#f5f5f3");
    let mut y = PAD_Y;
    y = text_block(&mut svg, &POST_META, &meta, PAD_X, y, |_| "#6b6b69")?;
    y = text_block(&mut svg, &POST_TITLE, &title, PAD_X, y + 28.0, |_| {
        "#151515"
    })?;
    if !summary.is_empty() {
        text_block(&mut svg, &POST_SUMMARY, &summary, PAD_X, y + 24.0, |_| {
            "#525252"
        })?;
    }
    let mark_top = HEIGHT as f32 - PAD_Y - WORDMARK.line_height;
    wordmark(&mut svg, &WORDMARK, PAD_X, mark_top, "#151515")?;
    svg.push_str(&format!(
        r#"<rect x="{}" y="0" width="12" height="{HEIGHT}" fill="{CLAY}"/>"#,
        WIDTH - 12
    ));
    svg.push_str("</svg>");
    rasterize(&svg)
}

/// 渲染站点分享图。标语两行各自折行，合计最多三行。
pub fn render_site(card: &SiteShareCard) -> Result<Vec<u8>, OgError> {
    const PAD: f32 = 80.0;
    let max_width = WIDTH as f32 - PAD * 2.0;

    let mut lines: Vec<(String, &str)> = wrap(&SITE_TAGLINE, &card.title, max_width, 3)?
        .into_iter()
        .map(|line| (line, "#f2f1ee"))
        .collect();
    let rest = 3usize.saturating_sub(lines.len());
    if rest > 0 {
        lines.extend(
            wrap(&SITE_TAGLINE, &card.subtitle, max_width, rest)?
                .into_iter()
                .map(|line| (line, "#8b8984")),
        );
    }
    let footer = wrap(&SITE_FOOTER, &card.footer, max_width, 1)?;

    let mut svg = svg_open("#0f0f0e");
    wordmark(&mut svg, &SITE_WORDMARK, PAD, PAD, "#f2f1ee")?;

    // 与效果稿的 space-between 一致：标语在文字标与页脚之间居中。
    let footer_top = HEIGHT as f32 - PAD - SITE_FOOTER.line_height;
    let tagline_height = lines.len() as f32 * SITE_TAGLINE.line_height;
    let free = footer_top - (PAD + SITE_WORDMARK.line_height) - tagline_height;
    let tagline_top = PAD + SITE_WORDMARK.line_height + free / 2.0;
    let texts: Vec<String> = lines.iter().map(|(line, _)| line.clone()).collect();
    text_block(&mut svg, &SITE_TAGLINE, &texts, PAD, tagline_top, |i| {
        lines[i].1
    })?;
    text_block(&mut svg, &SITE_FOOTER, &footer, PAD, footer_top, |_| {
        "#8b8984"
    })?;
    svg.push_str("</svg>");
    rasterize(&svg)
}

fn svg_open(background: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}"><rect width="{WIDTH}" height="{HEIGHT}" fill="{background}"/>"#
    )
}

/// 文字标：陶土橙方点加「Yggdrasil」。方点边长约为字号的 0.53，圆角为边长的 0.25，
/// 与字之间留约一个方点宽。
fn wordmark(svg: &mut String, style: &Style, x: f32, top: f32, color: &str) -> Result<(), OgError> {
    let dot = (style.size * 0.53).round();
    let dot_y = top + (style.line_height - dot) / 2.0;
    svg.push_str(&format!(
        r#"<rect x="{x}" y="{dot_y}" width="{dot}" height="{dot}" rx="{}" fill="{CLAY}"/>"#,
        dot / 4.0
    ));
    text_block(
        svg,
        style,
        &["Yggdrasil".to_string()],
        x + dot * 2.0,
        top,
        |_| color,
    )?;
    Ok(())
}

/// 自 `top` 起逐行写出文字，返回最后一行行框的底边。
fn text_block<'a>(
    svg: &mut String,
    style: &Style,
    lines: &[String],
    x: f32,
    top: f32,
    color: impl Fn(usize) -> &'a str,
) -> Result<f32, OgError> {
    let baseline = baseline_offset(style)?;
    for (i, line) in lines.iter().enumerate() {
        let y = top + style.line_height * i as f32 + baseline;
        svg.push_str(&format!(
            r#"<text x="{x}" y="{y:.1}" font-family="{}" font-weight="{}" font-size="{}" letter-spacing="{}" fill="{}">{}</text>"#,
            style.family,
            style.weight,
            style.size,
            style.letter_spacing,
            color(i),
            escape_html(line),
        ));
    }
    Ok(top + style.line_height * lines.len() as f32)
}

/// 行框顶到基线的距离：与 CSS 一样把行距差平分到字面上下。
fn baseline_offset(style: &Style) -> Result<f32, OgError> {
    let font = font_ref(style.font);
    let metrics = font.metrics(Size::new(style.size), LocationRef::default());
    let content = metrics.ascent - metrics.descent;
    Ok((style.line_height - content) / 2.0 + metrics.ascent)
}

fn font_ref(data: &'static [u8]) -> FontRef<'static> {
    FontRef::new(data).expect("内嵌分享图字体可以解析")
}

/// 文字宽度（含字距）。缺字时报错。
fn measure(style: &Style, text: &str) -> Result<f32, OgError> {
    let font = font_ref(style.font);
    let charmap = font.charmap();
    let metrics = font.glyph_metrics(Size::new(style.size), LocationRef::default());
    let mut width = 0.0;
    for c in text.chars() {
        let glyph = charmap.map(c).ok_or(OgError::MissingGlyph(c))?;
        width += metrics.advance_width(glyph).unwrap_or(0.0) + style.letter_spacing;
    }
    Ok(width)
}

/// 按宽度折行，最多 `max_lines` 行；超出时末行以「…」收尾。
fn wrap(
    style: &Style,
    text: &str,
    max_width: f32,
    max_lines: usize,
) -> Result<Vec<String>, OgError> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut width = 0.0;
    for token in tokens(text) {
        if token == " " && line.is_empty() {
            continue;
        }
        let token_width = measure(style, &token)?;
        if width + token_width <= max_width {
            line.push_str(&token);
            width += token_width;
            continue;
        }
        if !line.is_empty() {
            lines.push(line.trim_end().to_string());
            line.clear();
            width = 0.0;
            if token == " " {
                continue;
            }
        }
        if token_width <= max_width {
            line.push_str(&token);
            width = token_width;
            continue;
        }
        // 比整行还宽的单个记号（长网址、长标识符）按字拆开。
        for c in token.chars() {
            let char_width = measure(style, c.encode_utf8(&mut [0; 4]))?;
            if width + char_width > max_width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                width = 0.0;
            }
            line.push(c);
            width += char_width;
        }
    }
    if !line.trim().is_empty() {
        lines.push(line.trim_end().to_string());
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            *last = ellipsize(style, last, max_width)?;
        }
    }
    Ok(lines)
}

/// 末行加省略号，放不下时从尾部逐字退回；同时去掉省略号前的空格与句读。
fn ellipsize(style: &Style, line: &str, max_width: f32) -> Result<String, OgError> {
    let mut chars: Vec<char> = line.chars().collect();
    loop {
        while chars
            .last()
            .is_some_and(|c| c.is_whitespace() || is_no_break_before(*c))
        {
            chars.pop();
        }
        let candidate: String = chars.iter().chain(['…'].iter()).collect();
        if chars.is_empty() || measure(style, &candidate)? <= max_width {
            return Ok(candidate);
        }
        chars.pop();
    }
}

/// 切成折行的最小单位：一个汉字、一个西文词或一个空格。句读与闭括号并入前一个单位，
/// 不出现在行首；开括号与开引号并入后一个单位，不留在行尾。
fn tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut opening = String::new();
    for c in text.chars() {
        if c.is_whitespace() {
            flush(&mut word, &mut out);
            flush(&mut opening, &mut out);
            if out.last().is_some_and(|t| t != " ") {
                out.push(" ".to_string());
            }
        } else if is_no_break_before(c) && opening.is_empty() {
            if !word.is_empty() {
                word.push(c);
            } else if let Some(last) = out.last_mut().filter(|t| *t != " ") {
                last.push(c);
            } else {
                out.push(c.to_string());
            }
        } else if is_no_break_after(c) {
            flush(&mut word, &mut out);
            opening.push(c);
        } else if is_cjk(c) {
            flush(&mut word, &mut out);
            let mut token = std::mem::take(&mut opening);
            token.push(c);
            out.push(token);
        } else {
            if word.is_empty() {
                word = std::mem::take(&mut opening);
            }
            word.push(c);
        }
    }
    flush(&mut word, &mut out);
    flush(&mut opening, &mut out);
    while out.last().is_some_and(|t| t == " ") {
        out.pop();
    }
    out
}

fn flush(buf: &mut String, out: &mut Vec<String>) {
    if !buf.is_empty() {
        out.push(std::mem::take(buf));
    }
}

fn is_no_break_before(c: char) -> bool {
    "，。、；：！？）》」』】〕〉”’…—～·%‰,.;:!?)]}".contains(c)
}

fn is_no_break_after(c: char) -> bool {
    "（《「『【〔〈“‘".contains(c)
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF)
}

fn rasterize(svg: &str) -> Result<Vec<u8>, OgError> {
    let options = usvg::Options {
        fontdb: FONT_DB.clone(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_str(svg, &options).map_err(|e| OgError::Svg(e.to_string()))?;
    let mut pixmap = tiny_skia::Pixmap::new(WIDTH, HEIGHT).ok_or(OgError::Encode)?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|_| OgError::Encode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::settings::{DEFAULT_HOME_SUBTITLE, DEFAULT_HOME_TITLE};

    fn is_png(bytes: &[u8]) -> bool {
        bytes.starts_with(b"\x89PNG\r\n\x1a\n")
    }

    #[test]
    fn tokens_keep_punctuation_off_line_edges() {
        assert_eq!(
            tokens("让时间成为电路，「Hesper」的 Apple I"),
            [
                "让",
                "时",
                "间",
                "成",
                "为",
                "电",
                "路，",
                "「Hesper」",
                "的",
                " ",
                "Apple",
                " ",
                "I"
            ]
        );
        assert_eq!(tokens("  a  b "), ["a", " ", "b"]);
    }

    #[test]
    fn wrap_breaks_by_width_and_ellipsizes_overflow() {
        let text = "让时间成为电路的一部分：Hesper 的 Apple I 实现，从一个字符如何抵达屏幕出发，\
                    沿着主时钟、PIA 握手、循环显示存储器与数字视频链一路读下去";
        let lines = wrap(&POST_TITLE, text, 1040.0, 3).unwrap();
        assert_eq!(lines.len(), 3);
        for line in &lines {
            assert!(measure(&POST_TITLE, line).unwrap() <= 1040.0, "{line}");
            assert!(!line.starts_with(['，', '：', ' ']), "{line}");
        }
        assert!(lines[2].ends_with('…'));

        let short = wrap(&POST_TITLE, "短标题", 1040.0, 3).unwrap();
        assert_eq!(short, ["短标题"]);
    }

    #[test]
    fn wrap_splits_tokens_wider_than_a_line() {
        let url = "https://example.com/".to_string() + &"a".repeat(200);
        let lines = wrap(&POST_SUMMARY, &url, 1040.0, 2).unwrap();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('…'));
    }

    #[test]
    fn missing_glyph_is_reported() {
        assert_eq!(
            measure(&POST_TITLE, "𠀀").unwrap_err(),
            OgError::MissingGlyph('𠀀')
        );
    }

    #[test]
    fn renders_post_card() {
        let card = PostShareCard {
            title: "让时间成为电路的一部分：Hesper 的 Apple I 实现".into(),
            summary: "从一个字符如何抵达屏幕出发，沿着 Hesper 的主时钟、PIA 握手、\
                      循环显示存储器与数字视频链，理解 Apple I 的实现取舍。"
                .into(),
            meta: "2026.09.16·19 分钟·源码剖析".into(),
        };
        let png = render_post(&card).unwrap();
        assert!(is_png(&png));
    }

    #[test]
    fn renders_default_site_card() {
        let card = SiteShareCard {
            title: DEFAULT_HOME_TITLE.into(),
            subtitle: DEFAULT_HOME_SUBTITLE.into(),
            footer: "rua.plus·41 篇文章".into(),
        };
        assert!(is_png(&render_site(&card).unwrap()));
    }
}
