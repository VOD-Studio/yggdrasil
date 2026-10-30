//! Mermaid 排版用的字宽回调。
//!
//! merman 默认的确定性字宽和浏览器实际字体对不上，HTML 标签会溢出。这里按
//! `font_metrics.bin`（`cd libs/shared && bun run mermaid-font-metrics <字体目录>` 生成）
//! 里 5 款界面字体（SF Pro Text、Selawik、Roboto、Liberation Sans、Noto Sans）分别求整串宽度，
//! 西文字体缺字形时用 Noto Sans CJK 的宽度，取 5 个结果的最大值：读者实际用哪款字体都放得下。

use std::sync::LazyLock;

use merman::svg::{
    DeterministicTextMeasurer, MeasurementProfileId, SvgEnvironment, TextMeasurementPolicy,
    TextMeasurementProfile, TextMeasurementProfileIdentity, TextStyle,
};

/// 码点连续且字宽相同的一段；`milli` 为千分之一 em（向上取整）。
struct Run {
    start: u16,
    end: u16,
    milli: u16,
}

/// 同一字重的 5 款西文字体与对应的 CJK 回退字体。
struct FontSet {
    latin: [Vec<Run>; 5],
    cjk: Vec<Run>,
}

struct FontTables {
    regular: FontSet,
    bold: FontSet,
}

/// 表顺序：0–4 常规西文、5 常规 CJK、6–10 粗体西文、11 粗体 CJK（与生成脚本一致）。
static TABLES: LazyLock<FontTables> = LazyLock::new(|| decode(include_bytes!("font_metrics.bin")));

fn decode(bytes: &[u8]) -> FontTables {
    const BAD: &str = "font_metrics.bin 格式不符，请用 bun run mermaid-font-metrics 重新生成";
    let u16_at = |at: usize| -> u16 {
        let b = bytes.get(at..at + 2).expect(BAD);
        u16::from_le_bytes([b[0], b[1]])
    };
    assert!(bytes.get(..6) == Some(b"YMFM\x01\x0c"), "{BAD}");
    let mut at = 6;
    let mut tables: Vec<Vec<Run>> = (0..12)
        .map(|_| {
            let count = usize::from(u16_at(at));
            at += 2;
            let runs = (0..count)
                .map(|i| {
                    let base = at + 6 * i;
                    Run {
                        start: u16_at(base),
                        end: u16_at(base + 2),
                        milli: u16_at(base + 4),
                    }
                })
                .collect();
            at += 6 * count;
            runs
        })
        .collect();
    assert_eq!(at, bytes.len(), "{BAD}");
    let into_set = |mut set: Vec<Vec<Run>>| -> FontSet {
        let cjk = set.pop().expect(BAD);
        let Ok(latin) = <[Vec<Run>; 5]>::try_from(set) else {
            panic!("{BAD}");
        };
        FontSet { latin, cjk }
    };
    let bold = into_set(tables.split_off(6));
    FontTables {
        regular: into_set(tables),
        bold,
    }
}

fn lookup(runs: &[Run], cp: u16) -> Option<u16> {
    let run = runs.get(runs.partition_point(|r| r.end < cp))?;
    (run.start <= cp).then_some(run.milli)
}

/// 浏览器排版时不占宽度的字符：软连字符、零宽空格与方向符、变体选择符、组合附加符号。
fn zero_width(ch: char) -> bool {
    matches!(
        ch,
        '\u{AD}' | '\u{200B}'..='\u{200F}' | '\u{2060}' | '\u{FEFF}' | '\u{FE00}'..='\u{FE0F}'
    ) || ('\u{0300}'..='\u{036F}').contains(&ch)
}

/// 5 款西文字体各求一遍整串宽度（em），取最大值。
fn width_em(set: &FontSet, text: &str) -> f64 {
    let mut sums = [0u64; 5];
    for ch in text.chars().filter(|c| !zero_width(*c)) {
        let Ok(cp) = u16::try_from(u32::from(ch)) else {
            // 基本多文种平面以外（emoji 等）按 1 em 计。
            sums.iter_mut().for_each(|sum| *sum += 1000);
            continue;
        };
        let cjk = lookup(&set.cjk, cp);
        for (sum, runs) in sums.iter_mut().zip(&set.latin) {
            *sum += u64::from(lookup(runs, cp).or(cjk).unwrap_or(1000));
        }
    }
    sums.into_iter().max().unwrap_or(0) as f64 / 1000.0
}

fn is_bold(style: &TextStyle) -> bool {
    match style.font_weight.as_deref().map(str::trim) {
        Some("bold" | "bolder") => true,
        Some(w) => w.parse::<u32>().is_ok_and(|w| w >= 600),
        None => false,
    }
}

/// merman 的字宽回调：返回 `text` 按 `style` 排版后的像素宽度。
pub(super) fn text_width_px(text: &str, style: &TextStyle) -> f64 {
    let set = if is_bold(style) {
        &TABLES.bold
    } else {
        &TABLES.regular
    };
    width_em(set, text) * style.font_size
}

/// 使用站点字宽表的渲染环境。每次新建：回调只引用 `'static` 的字宽表，不依赖 merman 类型是否 `Sync`。
pub(super) fn environment() -> SvgEnvironment {
    let measurer = DeterministicTextMeasurer::default().with_width_callback(text_width_px);
    let id = MeasurementProfileId::new("ygg-ui-fonts").expect("测量配置 id 合法");
    let identity = TextMeasurementProfileIdentity::new(id, "1").expect("测量配置版本合法");
    let profile = TextMeasurementProfile::new(identity, measurer);
    SvgEnvironment::deterministic()
        .with_text_measurement_policy(TextMeasurementPolicy::uniform(profile))
}
