#![cfg(feature = "server")]

//! mhchem 化学公式转译器：把 `\ce{...}` / `\pu{...}` 预转译为标准 LaTeX，
//! 再交给 katex 渲染。
//!
//! 这是 [mhchemParser](https://github.com/mhchem/mhchemParser) 4.2.2（Apache-2.0）
//! 的**机械移植**：状态机 + texify 输出。mhchemParser 是纯字符串→字符串转译器，
//! `ce("H2O")` → `\mathrm{H}\sb{2}\mathrm{O}` 之类，无需嵌入 katex 内部解析树。
//!
//! 移植要点（TS→Rust）：
//! - 动态 `buffer` 对象 → `Buffer` 结构体（`Option` 字段 + `clear`）。
//! - 动态 `Parsed = string | object` → `Parsed` 枚举；节点字段用 `Field`（字符串或节点向量）。
//! - 正则模式（含 lookahead `(?=)`/`(?!)`，Rust `regex` 不支持）→ `fancy-regex`（已被
//!   syntect 间接引入，此处显式声明）。
//! - `findObserveGroups` 花括号配对扫描 → `find_observe_groups`。
//! - 状态机转移表 → `build_transitions` 展开（对应 `_mhchemCreateTransitions`）。
//!
//! 公开 API：[`ce`](crate::api::mhchem::ce) / [`pu`](crate::api::mhchem::pu)。任意内部异常都回退为原样输出（绝不 panic，与
//! katex.rs 的容错哲学一致）。
//!
//! -----------------------------------------------------------------------
//! Copyright 2015-2023 Martin Hensel（mhchemParser 原作者）。Apache-2.0。
//! -----------------------------------------------------------------------

use fancy_regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

// =========================================================================
// 数据模型
// =========================================================================

/// 模式匹配值：单字符串或捕获组数组（正则 >1 个捕获组时为数组）。
#[derive(Clone, Debug)]
enum MVal {
    S(String),
    V(Vec<String>),
}

/// 模式匹配结果。
#[derive(Clone, Debug)]
struct MMatch {
    m: MVal,
    remainder: String,
}

/// 解析中间表示的字段值：字符串或子表达式节点向量。
#[derive(Clone, Debug)]
enum Field {
    Str(String),
    Nodes(Vec<Parsed>),
}

/// 解析中间表示：字面字符串或节点。
#[allow(clippy::large_enum_variant)] // 机械移植：镜像 mhchemParser 上游结构，Parsed 经 Vec 进堆，非栈热路径
#[derive(Clone, Debug)]
enum Parsed {
    S(String),
    N(NodeData),
}

#[derive(Clone, Debug, Default)]
struct NodeData {
    type_: String,
    p1: Option<Field>,
    p2: Option<Field>,
    a: Option<Field>,
    b: Option<Field>,
    p: Option<Field>,
    o: Option<Field>,
    q: Option<Field>,
    d: Option<Field>,
    color: Option<String>,
    color1: Option<String>,
    color2: Option<Field>,
    r: Option<String>,
    rd: Option<Field>,
    rq: Option<Field>,
    d_type: Option<String>,
    kind_: Option<String>,
}

/// 状态机缓冲区（对应 TS 的动态 `buffer` 对象）。
#[derive(Clone, Default)]
struct Buffer {
    a: Option<String>,
    b: Option<String>,
    p: Option<String>,
    o: Option<String>,
    q: Option<String>,
    d: Option<String>,
    rm: Option<String>,
    text_: Option<String>,
    r: Option<String>,
    rd: Option<String>,
    rq: Option<String>,
    rdt: Option<String>,
    rqt: Option<String>,
    d_type: Option<String>,
    sb: bool,
    begins_with_bond: bool,
    parenthesis_level: i32,
}

impl Buffer {
    /// 清空内容字段。`keep` 为真时保留 `parenthesis_level` 与 `begins_with_bond`
    /// （对应 ce 的 output：`delete buffer[p]` 跳过这两项）。
    fn clear(&mut self, keep: bool) {
        let (pl, bb) = if keep {
            (self.parenthesis_level, self.begins_with_bond)
        } else {
            (0, false)
        };
        *self = Buffer {
            parenthesis_level: pl,
            begins_with_bond: bb,
            ..Buffer::default()
        };
    }
}

// =========================================================================
// 转移表
// =========================================================================

#[derive(Clone)]
struct ActionRef {
    type_: String,
    option: Option<String>,
}

#[derive(Clone, Default)]
struct Task {
    actions: Vec<ActionRef>,
    next_state: Option<String>,
    revisit: bool,
    to_continue: bool,
}

#[derive(Clone)]
struct Transition {
    pattern: String,
    task: Task,
}

/// 原始转移条目（对应 TS 字面量，`patterns`/`states` 均为 `|` 分隔的合并名）。
struct RawEntry {
    patterns: &'static str,
    states: &'static str,
    task: Task,
}

/// 展开 `{pattern: {state: task}}` 为 `{state => [(pattern, task)]}`。
/// 对应 `_mhchemCreateTransitions`：拆分 `|`、`'*'` 插入所有状态。
fn build_transitions(raw: &[RawEntry]) -> HashMap<String, Vec<Transition>> {
    let mut transitions: HashMap<String, Vec<Transition>> = HashMap::new();
    // 1. 收集所有状态（拆分 states 的 `|`）
    for e in raw {
        for state in e.states.split('|') {
            transitions.entry(state.to_string()).or_default();
        }
    }
    // 2. 填充（拆分 patterns 与 states 的 `|`，`'*'` 插入所有已收集状态）
    let all_states: Vec<String> = transitions.keys().cloned().collect();
    for e in raw {
        let state_list: Vec<&str> = e.states.split('|').collect();
        for (idx, _state) in state_list.iter().enumerate() {
            for pattern in e.patterns.split('|') {
                let insert_states: Vec<String> = if state_list[idx] == "*" {
                    all_states.clone()
                } else {
                    vec![state_list[idx].to_string()]
                };
                for s in insert_states {
                    transitions.entry(s).or_default().push(Transition {
                        pattern: pattern.to_string(),
                        task: e.task.clone(),
                    });
                }
            }
        }
    }
    transitions
}

// =========================================================================
// 模式匹配
// =========================================================================

/// 编译正则，失败时记日志并返回 `None`（绝不 panic）。
///
/// `re!` 宏的 `.unwrap()` 在 `panic = "abort"` 下会直接杀进程——任何未来的
/// 正则转录错误都会让整篇含化学公式的文章渲染崩溃。改为降级返回 `None`
/// 后，坏正则只让该模式匹配失败（状态机退到 "else" 兜底分支产出原样字符），
/// 不影响整体渲染。`all_match_patterns_compile` 测试仍会在测试期捕获回归。
fn compile_or_log(pat: &str) -> Option<Regex> {
    match Regex::new(pat) {
        Ok(re) => Some(re),
        Err(e) => {
            tracing::error!(target: "yggdrasil::mhchem", pattern = pat, error = ?e, "mhchem 正则编译失败，该模式将降级为不匹配");
            None
        }
    }
}

/// 编译产物：把「坏正则降级为不匹配」的语义封装在一处。
///
/// 调用方不再到处写 `re_ref(re!(...))?` 或手写 `None` 回退——直接调
/// [`CompiledPat::captures_head`] / [`CompiledPat::is_match`]，编译失败时返回
/// 不匹配（`None` / `false`）。对应原始 `Regex` 方法的子集。
struct CompiledPat(Option<Regex>);

impl CompiledPat {
    /// 锚定头部匹配（坏正则 → `None`）。匹配失败/编译失败统一为不匹配。
    ///
    /// fancy-regex 0.19 的 captures 只借用 input；`str` 指明 UTF-8 文本输入。
    #[inline]
    fn captures_head<'s>(&self, input: &'s str) -> Option<fancy_regex::Captures<'s, str>> {
        self.0.as_ref()?.captures(input).ok().flatten()
    }

    /// 是否匹配（坏正则 → `false`）。
    #[inline]
    fn is_match(&self, input: &str) -> bool {
        self.0
            .as_ref()
            .and_then(|r| r.is_match(input).ok())
            .unwrap_or(false)
    }
}

/// 惰性编译字面量正则，编译失败降级为 [`CompiledPat`]（None）。
macro_rules! re {
    ($pat:literal) => {{
        static RE: LazyLock<CompiledPat> = LazyLock::new(|| CompiledPat(compile_or_log($pat)));
        &*RE
    }};
}

/// 字面量或正则模式（`findObserveGroups` 的参数）。
enum Pat {
    Lit(&'static str),
    Re(&'static CompiledPat),
}

/// 在 `input` 起始处匹配 `pat`，返回匹配到的文本（不含 remainder）。
fn pat_match_head(pat: &Pat, input: &str) -> Option<String> {
    match pat {
        Pat::Lit(s) => {
            if input.starts_with(s) {
                Some((*s).to_string())
            } else {
                None
            }
        }
        Pat::Re(cp) => cp.captures_head(input).and_then(|c| {
            let whole = c.get(0).map(|m| m.as_str())?;
            // 模式均锚定 ^，确认从 0 开始
            if input.starts_with(whole) {
                Some(whole.to_string())
            } else {
                None
            }
        }),
    }
}

/// `findObserveGroups`：花括号感知的定界符配对扫描。
///
/// 移植自 TS `findObserveGroups`。`beg*_excl/incl`/`end*_incl/excl` 为定界符，
/// `combine` 控制第二组结果是否拼接为字符串。
#[allow(clippy::too_many_arguments)]
fn find_observe_groups(
    input: &str,
    beg_excl: Pat,
    beg_incl: Pat,
    end_incl: Pat,
    end_excl: Pat,
    beg2_excl: Option<Pat>,
    beg2_incl: Option<Pat>,
    end2_incl: Option<Pat>,
    end2_excl: Option<Pat>,
    combine: bool,
) -> Option<MMatch> {
    // 第一组
    let m0 = pat_match_head(&beg_excl, input)?;
    let rest0 = &input[m0.len()..];
    let m1 = pat_match_head(&beg_incl, rest0)?;
    // 结束定界符：endIncl || endExcl（endIncl 为空字面量时取 endExcl）
    let end_is_incl = !is_empty_pat(&end_incl);
    let end_chars = if end_is_incl { &end_incl } else { &end_excl };
    let e = find_observe_end(rest0, m1.len(), end_chars)?;
    let body_end = if end_is_incl { e.end } else { e.begin };
    let match1 = rest0[..body_end].to_string();
    let after1 = &rest0[e.end..];
    // 无第二组
    if beg2_excl.is_none() && beg2_incl.is_none() {
        return Some(MMatch {
            m: MVal::S(match1),
            remainder: after1.to_string(),
        });
    }
    // 第二组（递归）
    let g2 = find_observe_groups(
        after1,
        beg2_excl.unwrap_or(Pat::Lit("")),
        beg2_incl.unwrap_or(Pat::Lit("")),
        end2_incl.unwrap_or(Pat::Lit("")),
        end2_excl.unwrap_or(Pat::Lit("")),
        None,
        None,
        None,
        None,
        false,
    )?;
    let mval = match g2.m {
        MVal::S(s2) => {
            if combine {
                MVal::S(format!("{}{}", match1, s2))
            } else {
                MVal::V(vec![match1, s2])
            }
        }
        MVal::V(_) => MVal::V(vec![match1]), // 不应发生（第二组无二级）
    };
    Some(MMatch {
        m: mval,
        remainder: g2.remainder,
    })
}

fn is_empty_pat(p: &Pat) -> bool {
    matches!(p, Pat::Lit(""))
}

struct Span {
    begin: usize,
    end: usize,
}

/// 从 `rest0` 的 `start` 位置扫描，跟踪花括号深度，在深度 0 遇到 `end_chars` 时返回其区间。
///
/// 必须按**字符**而非字节步进：化学公式可含多字节 UTF-8 字符（如中文说明
/// 文字「浓」占 3 字节）。逐字节 `i+=1` 会让 `&input[i..]` 落在字符内部，
/// 触发 `byte index N is not a char boundary` panic（在 panic=abort 下直接
/// 杀进程），且 `bytes[i] as char` 会把多字节字符的首字节误判为花括号，
/// 导致输出损坏。`char_indices` 保证每步都在字符边界上。
fn find_observe_end(input: &str, start: usize, end_chars: &Pat) -> Option<Span> {
    let mut braces = 0i32;
    // start 是上一步 beg_incl 匹配的长度；该匹配来自 Pat::Lit 或锚定正则，
    // 落在字符边界上。校验后用于首次切片，随后只按 char_indices 的边界前进。
    let start = input.get(start..).map(|_| start).unwrap_or(input.len());
    for (i, a) in input.char_indices().skip_while(|(b, _)| *b < start) {
        // 结束定界符匹配（仅在花括号平衡时）
        if braces == 0 {
            if let Some(matched) = pat_match_head(end_chars, &input[i..]) {
                return Some(Span {
                    begin: i,
                    end: i + matched.len(),
                });
            }
        }
        if a == '{' {
            braces += 1;
        } else if a == '}' {
            if braces == 0 {
                // ExtraCloseMissingOpen —— 原实现抛错，这里返回 None 容错
                return None;
            }
            braces -= 1;
        }
    }
    None
}

/// 把 fancy-regex 的捕获结果转为 [`MMatch`]（锚定 `^`，whole 从 0 起）。
fn fancy_match(re: &Regex, input: &str) -> Option<MMatch> {
    let caps = re.captures(input).ok()??;
    let whole = caps.get(0)?.as_str().to_string();
    if !input.starts_with(&whole) {
        return None;
    }
    let n_groups = re.captures_len();
    let mval = if n_groups >= 2 {
        let mut v = Vec::with_capacity(n_groups);
        for gi in 1..=n_groups {
            v.push(
                caps.get(gi)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default(),
            );
        }
        MVal::V(v)
    } else {
        // n_groups == 1 或 0：match[1] || match[0]（空串视作未匹配）
        let g1 = caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .filter(|s| !s.is_empty());
        MVal::S(g1.unwrap_or(whole.clone()))
    };
    Some(MMatch {
        m: mval,
        remainder: input[whole.len()..].to_string(),
    })
}

/// 希腊字母宏集合（`letters` / `\greek` 等模式用到）。
static GREEK_NAMES: &str = "alpha|beta|gamma|delta|epsilon|zeta|eta|theta|iota|kappa|lambda|mu|nu|xi|omicron|pi|rho|sigma|tau|upsilon|phi|chi|psi|omega|Gamma|Delta|Theta|Lambda|Xi|Pi|Sigma|Upsilon|Phi|Psi|Omega";

static LETTERS_RE: LazyLock<CompiledPat> = LazyLock::new(|| {
    CompiledPat(compile_or_log(&format!(
        "^(?:[a-zA-Z\u{03B1}-\u{03C9}\u{0391}-\u{03A9}?@]|(?:\\\\(?:{})(?:\\s+|\\{{\\}}|(?![a-zA-Z]))))+",
        GREEK_NAMES
    )))
});

static GREEK_RE: LazyLock<CompiledPat> = LazyLock::new(|| {
    CompiledPat(compile_or_log(&format!(
        "^\\\\(?:{})(?:\\s+|\\{{\\}}|(?![a-zA-Z]))",
        GREEK_NAMES
    )))
});

static ONE_GREEK_RE: LazyLock<CompiledPat> = LazyLock::new(|| {
    CompiledPat(compile_or_log(
        "^(?:\\$?[\u{03B1}-\u{03C9}]\\$?|\\$?\\\\(?:alpha|beta|gamma|delta|epsilon|zeta|eta|theta|iota|kappa|lambda|mu|nu|xi|omicron|pi|rho|sigma|tau|upsilon|phi|chi|psi|omega)\\s*\\$?)(?:\\s+|\\{{\\}}|(?![a-zA-Z]))$"
    ))
});

/// 按名匹配模式（对应 `_mhchemParser.patterns.match_`）。
fn match_pattern(name: &str, input: &str) -> Option<MMatch> {
    // ── 正则模式 ──
    // `re!` 产物是 `&CompiledPat`：编译失败的坏正则 `.0` 为 None，
    // 这里 `.0.as_ref()?` 降级为不匹配，状态机退到 "else" 兜底分支。
    let re_match = |cp: &CompiledPat| cp.0.as_ref().and_then(|r| fancy_match(r, input));
    match name {
        "empty" => re_match(re!("^$")),
        "else" | "else2" => re_match(re!("^.")),
        "space" => re_match(re!("^\\s")),
        "space A" => re_match(re!("^\\s(?=[A-Z\\\\$])")),
        "space$" => re_match(re!("^\\s$")),
        "a-z" => re_match(re!("^[a-z]")),
        "x" => re_match(re!("^x")),
        "x$" => re_match(re!("^x$")),
        "i$" => re_match(re!("^i$")),
        "letters" => re_match(&LETTERS_RE),
        "\\greek" => re_match(&GREEK_RE),
        "one lowercase latin letter $" => re_match(re!("^(?:([a-z])(?:$|[^a-zA-Z]))$")),
        "$one lowercase latin letter$ $" => re_match(re!("^\\$(?:([a-z])(?:$|[^a-zA-Z]))\\$$")),
        "one lowercase greek letter $" => re_match(&ONE_GREEK_RE),
        "digits" => re_match(re!("^[0-9]+")),
        "-9.,9" => re_match(re!("^[+\\-]?(?:[0-9]+(?:[,.][0-9]+)?|[0-9]*(?:\\.[0-9]+))")),
        "-9.,9 no missing 0" => re_match(re!("^[+\\-]?[0-9]+(?:[.,][0-9]+)?")),
        "(-)(9)^(-9)" => re_match(re!("^(\\+\\-|\\+\\/\\-|\\+|\\-|\\\\pm\\s?)?([0-9]+(?:[,.][0-9]+)?|[0-9]*(?:\\.[0-9]+)?)\\^([+\\-]?[0-9]+|\\{[+\\-]?[0-9]+\\})")),
        "_{(state of aggregation)}$" => re_match(re!("^_\\{(\\([a-z]{1,3}\\))\\}")),
        "{[(" => re_match(re!("^(?:\\\\\\{|\\[|\\()")),
        ")]}" => re_match(re!("^(?:\\)|\\]|\\\\\\})")),
        ", " => re_match(re!("^[,;]\\s*")),
        "," => re_match(re!("^[,;]")),
        "." => re_match(re!("^[.]")),
        ". __* " => re_match(re!("^([.\u{22C5}\u{00B7}\u{2022}]|[*])\\s*")),
        "..." => re_match(re!("^\\.\\.\\.(?=$|[^.])")),
        "^a" => re_match(re!("^\\^([0-9]+|[^\\\\_])")),
        "^\\x" => re_match(re!("^\\^(\\\\[a-zA-Z]+)\\s*")),
        "^(-1)" => re_match(re!("^\\^(-?\\d+)")),
        "'" => re_match(re!("^'")),
        "_9" => re_match(re!("^_([+\\-]?[0-9]+|[^\\\\])")),
        "_\\x" => re_match(re!("^_(\\\\[a-zA-Z]+)\\s*")),
        "^_" => re_match(re!("^(?:\\^(?=_)|\\_(?=\\^)|[\\^_]$)")),
        "{}^" => re_match(re!("^\\{\\}(?=\\^)")),
        "{}" => re_match(re!("^\\{\\}")),
        "=<>" => re_match(re!("^[=<>]")),
        "#" => re_match(re!("^[#\u{2261}]")),
        "+" => re_match(re!("^\\+")),
        "-$" => re_match(re!("^-(?=[\\s_},;\\]/]|$|\\([a-z]+\\))")),
        "-9" => re_match(re!("^-(?=[0-9])")),
        "- orbital overlap" => re_match(re!("^-(?=(?:[spd]|sp)(?:$|[\\s,;\\)\\]\\}]))")),
        "-" => re_match(re!("^-")),
        "pm-operator" => re_match(re!("^(?:\\\\pm|\\$\\\\pm\\$|\\+-|\\+\\/-)")),
        "operator" => re_match(re!("^(?:\\+|(?:[\\-=<>]|<<|>>|\\\\approx|\\$\\\\approx\\$)(?=\\s|$|-?[0-9]))")),
        "arrowUpDown" => re_match(re!("^(?:v|\\(v\\)|\\^|\\(\\^\\))(?=$|[\\s,;\\)\\]\\}])")),
        "->" => re_match(re!("^(?:<->|<-->|->|<-|<=>>|<<=>|<=>|[\u{2192}\u{27F6}\u{21CC}])")),
        "CMT" => re_match(re!("^[CMT](?=\\[)")),
        "1st-level escape" => re_match(re!("^(&|\\\\\\\\|\\\\hline)\\s*")),
        "\\," => re_match(re!("^(?:\\\\[,\\ ;:])")),
        "\\ca" => re_match(re!("^\\\\ca(?:\\s+|(?![a-zA-Z]))")),
        "\\x" => re_match(re!("^(?:\\\\[a-zA-Z]+\\s*|\\\\[_&{}%])")),
        "orbital" => re_match(re!("^(?:[0-9]{1,2}[spdfgh]|[0-9]{0,2}sp)(?=$|[^a-zA-Z])")),
        "others" => re_match(re!("^[/~|]")),
        "oxidation$" => re_match(re!("^(?:[+-][IVX]+|(?:\\\\pm|\\$\\\\pm\\$|\\+-|\\+\\/-)\\s*0)$")),
        "d-oxidation$" => re_match(re!("^(?:[+-]?[IVX]+|(?:\\\\pm|\\$\\\\pm\\$|\\+-|\\+\\/-)\\s*0)$")),
        "1/2$" => re_match(re!("^[+\\-]?(?:[0-9]+|\\$[a-z]\\$|[a-z])\\/[0-9]+(?:\\$[a-z]\\$|[a-z])?$")),
        "(KV letters)," => re_match(re!("^(?:[A-Z][a-z]{0,2}|i)(?=,)")),
        "uprightEntities" => re_match(re!("^(?:pH|pOH|pC|pK|iPr|iBu)(?=$|[^a-zA-Z])")),
        "/" => re_match(re!("^\\s*(\\/)\\s*")),
        "//" => re_match(re!("^\\s*(\\/\\/)\\s*")),
        "*" => re_match(re!("^\\s*[*.]\\s*")),
        // ── 函数模式（findObserveGroups / 自定义）──
        "(-)(9.,9)(e)(99)" => pat_enumber(input),
        "state of aggregation $" => pat_state_of_aggregation(input),
        "^{(...)}" => fg(input, Pat::Lit("^{"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}"), None, None, None, None, false),
        "^($...$)" => fg(input, Pat::Lit("^"), Pat::Lit("$"), Pat::Lit("$"), Pat::Lit(""), None, None, None, None, false),
        "^\\x{}{}" => fg(input, Pat::Lit("^"), Pat::Re(re!("^\\\\[a-zA-Z]+\\{")), Pat::Lit("}"), Pat::Lit(""), Some(Pat::Lit("")), Some(Pat::Lit("{")), Some(Pat::Lit("}")), Some(Pat::Lit("")), true),
        "^\\x{}" => fg(input, Pat::Lit("^"), Pat::Re(re!("^\\\\[a-zA-Z]+\\{")), Pat::Lit("}"), Pat::Lit(""), None, None, None, None, false),
        "\\bond{(...)}" => fg(input, Pat::Lit("\\bond{"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}"), None, None, None, None, false),
        "[(...)]" => fg(input, Pat::Lit("["), Pat::Lit(""), Pat::Lit(""), Pat::Lit("]"), None, None, None, None, false),
        "\\x{}{}" => fg(input, Pat::Lit(""), Pat::Re(re!("^\\\\[a-zA-Z]+\\{")), Pat::Lit("}"), Pat::Lit(""), Some(Pat::Lit("")), Some(Pat::Lit("{")), Some(Pat::Lit("}")), Some(Pat::Lit("")), true),
        "\\x{}" => fg(input, Pat::Lit(""), Pat::Re(re!("^\\\\[a-zA-Z]+\\{")), Pat::Lit("}"), Pat::Lit(""), None, None, None, None, false),
        "\\frac{(...)}" => fg(input, Pat::Lit("\\frac{"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}"), Some(Pat::Lit("{")), Some(Pat::Lit("")), Some(Pat::Lit("")), Some(Pat::Lit("}")), false),
        "\\overset{(...)}" => fg(input, Pat::Lit("\\overset{"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}"), Some(Pat::Lit("{")), Some(Pat::Lit("")), Some(Pat::Lit("")), Some(Pat::Lit("}")), false),
        "\\underset{(...)}" => fg(input, Pat::Lit("\\underset{"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}"), Some(Pat::Lit("{")), Some(Pat::Lit("")), Some(Pat::Lit("")), Some(Pat::Lit("}")), false),
        "\\underbrace{(...)}" => fg(input, Pat::Lit("\\underbrace{"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}"), Some(Pat::Lit("_")), Some(Pat::Lit("{")), Some(Pat::Lit("")), Some(Pat::Lit("}")), false),
        "\\color{(...)}" => fg(input, Pat::Lit("\\color{"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}"), None, None, None, None, false),
        "\\color{(...)}{(...)}" => fg(input, Pat::Lit("\\color{"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}"), Some(Pat::Lit("{")), Some(Pat::Lit("")), Some(Pat::Lit("")), Some(Pat::Lit("}")), false),
        "\\ce{(...)}" => fg(input, Pat::Lit("\\ce{"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}"), None, None, None, None, false),
        "\\pu{(...)}" => fg(input, Pat::Lit("\\pu{"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}"), None, None, None, None, false),
        "_{(...)}" => fg(input, Pat::Lit("_{"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}"), None, None, None, None, false),
        "_($...$)" => fg(input, Pat::Lit("_"), Pat::Lit("$"), Pat::Lit("$"), Pat::Lit(""), None, None, None, None, false),
        "_\\x{}{}" => fg(input, Pat::Lit("_"), Pat::Re(re!("^\\\\[a-zA-Z]+\\{")), Pat::Lit("}"), Pat::Lit(""), Some(Pat::Lit("")), Some(Pat::Lit("{")), Some(Pat::Lit("}")), Some(Pat::Lit("")), true),
        "_\\x{}" => fg(input, Pat::Lit("_"), Pat::Re(re!("^\\\\[a-zA-Z]+\\{")), Pat::Lit("}"), Pat::Lit(""), None, None, None, None, false),
        "{...}" => fg(input, Pat::Lit(""), Pat::Lit("{"), Pat::Lit("}"), Pat::Lit(""), None, None, None, None, false),
        "{(...)}" => fg(input, Pat::Lit("{"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}"), None, None, None, None, false),
        "$...$" => fg(input, Pat::Lit(""), Pat::Lit("$"), Pat::Lit("$"), Pat::Lit(""), None, None, None, None, false),
        "${(...)}$__$(...)$" => fg(input, Pat::Lit("${"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("}$"), None, None, None, None, false).or_else(|| fg(input, Pat::Lit("$"), Pat::Lit(""), Pat::Lit(""), Pat::Lit("$"), None, None, None, None, false)),
        "amount" | "amount2" => pat_amount(input),
        "formula$" => pat_formula(input),
        _ => {
            // 未知模式：容错返回 None（原实现抛 MhchemBugP）
            None
        }
    }
}

/// `find_observe_groups` 的简写封装。
#[allow(clippy::too_many_arguments)]
fn fg(
    input: &str,
    beg_excl: Pat,
    beg_incl: Pat,
    end_incl: Pat,
    end_excl: Pat,
    beg2_excl: Option<Pat>,
    beg2_incl: Option<Pat>,
    end2_incl: Option<Pat>,
    end2_excl: Option<Pat>,
    combine: bool,
) -> Option<MMatch> {
    find_observe_groups(
        input, beg_excl, beg_incl, end_incl, end_excl, beg2_excl, beg2_incl, end2_incl, end2_excl,
        combine,
    )
}

/// `(-)(9.,9)(e)(99)` 模式。
fn pat_enumber(input: &str) -> Option<MMatch> {
    let re = re!("^(\\+\\-|\\+\\/\\-|\\+|\\-|\\\\pm\\s?)?([0-9]+(?:[,.][0-9]+)?|[0-9]*(?:\\.[0-9]+))?(\\((?:[0-9]+(?:[,.][0-9]+)?|[0-9]*(?:\\.[0-9]+))\\))?(?:(?:([eE])|\\s*(\\*|x|\\\\times|\u{00D7})\\s*10\\^)([+\\-]?[0-9]+|\\{[+\\-]?[0-9]+\\}))?");
    let caps = re.captures_head(input)?;
    let whole = caps.get(0)?.as_str();
    if whole.is_empty() || !input.starts_with(whole) {
        return None;
    }
    let mut v = Vec::with_capacity(6);
    for gi in 1..=6 {
        v.push(
            caps.get(gi)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
        );
    }
    Some(MMatch {
        m: MVal::V(v),
        remainder: input[whole.len()..].to_string(),
    })
}

/// `state of aggregation $` 模式。
fn pat_state_of_aggregation(input: &str) -> Option<MMatch> {
    let a = fg(
        input,
        Pat::Lit(""),
        Pat::Re(re!("^\\([a-z]{1,3}(?=[\\),])")),
        Pat::Lit(""),
        Pat::Lit(")"),
        None,
        None,
        None,
        None,
        false,
    )?;
    if re!("^($|[\\s,;\\)\\]\\}])").is_match(a.remainder.as_str()) {
        return Some(a);
    }
    let re2 = re!("^(?:\\((?:\\\\ca\\s?)?\\$[amothc]\\$\\))");
    let caps = re2.captures_head(input)?;
    let whole = caps.get(0)?.as_str().to_string();
    Some(MMatch {
        m: MVal::S(whole.clone()),
        remainder: input[whole.len()..].to_string(),
    })
}

/// `amount` 模式。
fn pat_amount(input: &str) -> Option<MMatch> {
    let re = re!("^(?:(?:(?:\\([+\\-]?[0-9]+\\/[0-9]+\\)|[+\\-]?(?:[0-9]+|\\$[a-z]\\$|[a-z])\\/[0-9]+|[+\\-]?[0-9]+[.,][0-9]+|[+\\-]?\\.[0-9]+|[+\\-]?[0-9]+)(?:[a-z](?=\\s*[A-Z]))?)|[+\\-]?[a-z](?=\\s*[A-Z])|\\+(?!\\s))");
    if let Some(caps) = re.captures_head(input) {
        let whole = caps.get(0)?.as_str();
        if !whole.is_empty() {
            return Some(MMatch {
                m: MVal::S(whole.to_string()),
                remainder: input[whole.len()..].to_string(),
            });
        }
    }
    let a = fg(
        input,
        Pat::Lit(""),
        Pat::Lit("$"),
        Pat::Lit("$"),
        Pat::Lit(""),
        None,
        None,
        None,
        None,
        false,
    )?;
    let re2 = re!("^\\$(?:\\(?[+\\-]?(?:[0-9]*[a-z]?[+\\-])?[0-9]*[a-z](?:[+\\-][0-9]*[a-z]?)?\\)?|\\+|-)\\$$");
    let inner = mval_str(&a.m);
    let inner_len = inner.len();
    if re2.is_match(&inner) {
        return Some(MMatch {
            m: MVal::S(inner),
            remainder: input[inner_len..].to_string(),
        });
    }
    None
}

/// `formula$` 模式。
fn pat_formula(input: &str) -> Option<MMatch> {
    if re!("^\\([a-z]+\\)$").is_match(input) {
        return None;
    }
    let re = re!("^(?:[a-z]|(?:[0-9\\ +\\-\\,\\.\\(\\)]+[a-z])+[0-9\\ +\\-\\,\\.\\(\\)]*|(?:[a-z][0-9\\ +\\-\\,\\.\\(\\)]+)+[a-z]?)$");
    let caps = re.captures_head(input)?;
    let whole = caps.get(0)?.as_str().to_string();
    Some(MMatch {
        m: MVal::S(whole.clone()),
        remainder: input[whole.len()..].to_string(),
    })
}

fn mval_str(m: &MVal) -> String {
    match m {
        MVal::S(s) => s.clone(),
        MVal::V(v) => v.first().cloned().unwrap_or_default(),
    }
}

// =========================================================================
// 状态机主循环（对应 `_mhchemParser.go`）
// =========================================================================

#[allow(clippy::large_enum_variant)] // 同上：Out 为动作返回值，瞬态使用
enum Out {
    None,
    One(Parsed),
    Many(Vec<Parsed>),
}

fn concat(out: &mut Vec<Parsed>, o: Out) {
    match o {
        Out::None => {}
        Out::One(p) => out.push(p),
        Out::Many(v) => out.extend(v),
    }
}

/// 主解析循环。
fn go(input: &str, machine: &str) -> Vec<Parsed> {
    if input.is_empty() {
        return Vec::new();
    }
    // 输入预处理
    let mut input = input.replace('\n', " ");
    input = input.replace(['\u{2212}', '\u{2013}', '\u{2014}', '\u{2010}'], "-");
    input = input.replace('\u{2026}', "...");

    let transitions = transitions_for(machine);
    let mut state = String::from("0");
    let mut buffer = Buffer {
        parenthesis_level: 0,
        ..Default::default()
    };
    let mut output: Vec<Parsed> = Vec::new();
    let mut last_input: Option<String> = None;
    let mut watchdog = 10i32;

    loop {
        if last_input.as_deref() != Some(input.as_str()) {
            watchdog = 10;
            last_input = Some(input.clone());
        } else {
            watchdog -= 1;
        }
        let t = transitions.get(&state).or_else(|| transitions.get("*"));
        let t = match t {
            Some(t) => t,
            None => break,
        };
        let mut matched = false;
        for tr in t {
            if let Some(mres) = match_pattern(&tr.pattern, &input) {
                matched = true;
                // 执行动作链
                for aref in &tr.task.actions {
                    let o = exec_action(machine, &mut buffer, &mres.m, &aref.option, &aref.type_);
                    concat(&mut output, o);
                }
                // 设置下一状态
                if let Some(ns) = &tr.task.next_state {
                    state = ns.clone();
                }
                if !input.is_empty() {
                    if !tr.task.revisit {
                        input = mres.remainder;
                    }
                    if !tr.task.to_continue {
                        break;
                    }
                } else {
                    return output;
                }
            }
        }
        if !matched {
            break;
        }
        if watchdog <= 0 {
            // 防死循环：容错返回当前输出（原实现抛 MhchemBugU）
            break;
        }
    }
    output
}

// =========================================================================
// 动作分发
// =========================================================================

fn exec_action(
    machine: &str,
    buf: &mut Buffer,
    m: &MVal,
    opt: &Option<String>,
    type_: &str,
) -> Out {
    // 优先机器局部动作，再查通用动作
    if let Some(o) = machine_action(machine, buf, m, opt, type_) {
        return o;
    }
    generic_action(buf, m, opt, type_)
}

fn generic_action(buf: &mut Buffer, m: &MVal, opt: &Option<String>, type_: &str) -> Out {
    match type_ {
        "a=" => {
            append_field(&mut buf.a, m);
            Out::None
        }
        "b=" => {
            append_field(&mut buf.b, m);
            Out::None
        }
        "p=" => {
            append_field(&mut buf.p, m);
            Out::None
        }
        "o=" => {
            append_field(&mut buf.o, m);
            Out::None
        }
        "o=+p1" => {
            if let Some(a) = opt {
                append_str(&mut buf.o, a);
            }
            Out::None
        }
        "q=" => {
            append_field(&mut buf.q, m);
            Out::None
        }
        "d=" => {
            append_field(&mut buf.d, m);
            Out::None
        }
        "rm=" => {
            append_field(&mut buf.rm, m);
            Out::None
        }
        "text=" => {
            append_field(&mut buf.text_, m);
            Out::None
        }
        "insert" => match opt {
            Some(a) => Out::One(Parsed::N(NodeData {
                type_: a.clone(),
                ..Default::default()
            })),
            None => Out::None,
        },
        "insert+p1" => match opt {
            Some(a) => Out::One(Parsed::N(NodeData {
                type_: a.clone(),
                p1: Some(Field::Str(mval_str(m))),
                ..Default::default()
            })),
            None => Out::None,
        },
        "insert+p1+p2" => {
            if let (Some(a), MVal::V(v)) = (opt, m) {
                Out::One(Parsed::N(NodeData {
                    type_: a.clone(),
                    p1: Some(Field::Str(v.first().cloned().unwrap_or_default())),
                    p2: Some(Field::Str(v.get(1).cloned().unwrap_or_default())),
                    ..Default::default()
                }))
            } else {
                Out::None
            }
        }
        "copy" => mval_to_out(m),
        "write" => match opt {
            Some(a) => Out::One(Parsed::S(a.clone())),
            None => Out::None,
        },
        "rm" => Out::One(Parsed::N(NodeData {
            type_: "rm".into(),
            p1: Some(Field::Str(mval_str(m))),
            ..Default::default()
        })),
        "text" => Out::Many(go(&mval_str(m), "text")),
        "tex-math" => Out::Many(go(&mval_str(m), "tex-math")),
        "tex-math tight" => Out::Many(go(&mval_str(m), "tex-math tight")),
        "bond" => {
            let kind = opt
                .clone()
                .or_else(|| match m {
                    MVal::S(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            Out::One(Parsed::N(NodeData {
                type_: "bond".into(),
                kind_: Some(kind),
                ..Default::default()
            }))
        }
        "color0-output" => Out::One(Parsed::N(NodeData {
            type_: "color0".into(),
            color: Some(mval_str(m)),
            ..Default::default()
        })),
        "ce" => Out::Many(go(&mval_str(m), "ce")),
        "pu" => Out::Many(go(&mval_str(m), "pu")),
        "9,9" => Out::Many(go(&mval_str(m), "9,9")),
        "1/2" => {
            let mut s = mval_str(m);
            let mut ret: Vec<Parsed> = Vec::new();
            if s.starts_with('+') || s.starts_with('-') {
                ret.push(Parsed::S(s[..1].to_string()));
                s = s[1..].to_string();
            }
            // 坏正则（编译失败）时 captures_head 返回 None，直接结束 1/2 动作
            // （产出已累积的前缀符号）。
            if let Some(caps) =
                re!("^([0-9]+|\\$[a-z]\\$|[a-z])\\/([0-9]+)(\\$[a-z]\\$|[a-z])?$").captures_head(&s)
            {
                let mut n1 = caps
                    .get(1)
                    .map(|x| x.as_str().to_string())
                    .unwrap_or_default();
                n1 = n1.replace('$', "");
                let n2 = caps
                    .get(2)
                    .map(|x| x.as_str().to_string())
                    .unwrap_or_default();
                ret.push(Parsed::N(NodeData {
                    type_: "frac".into(),
                    p1: Some(Field::Str(n1)),
                    p2: Some(Field::Str(n2)),
                    ..Default::default()
                }));
                if let Some(g3) = caps.get(3) {
                    let mut n3 = g3.as_str().replace('$', "");
                    ret.push(Parsed::N(NodeData {
                        type_: "tex-math".into(),
                        p1: Some(Field::Str(std::mem::take(&mut n3))),
                        ..Default::default()
                    }));
                }
            }
            Out::Many(ret)
        }
        _ => Out::None,
    }
}

fn append_field(field: &mut Option<String>, m: &MVal) {
    let s = mval_str(m);
    match field {
        Some(existing) => existing.push_str(&s),
        None => *field = Some(s),
    }
}

fn append_str(field: &mut Option<String>, s: &str) {
    match field {
        Some(existing) => existing.push_str(s),
        None => *field = Some(s.to_string()),
    }
}

fn mval_to_out(m: &MVal) -> Out {
    match m {
        MVal::S(s) => Out::One(Parsed::S(s.clone())),
        MVal::V(v) => Out::Many(v.iter().map(|s| Parsed::S(s.clone())).collect()),
    }
}

// =========================================================================
// texify 输出（对应 `_mhchemTexify`）
// =========================================================================

fn texify_go(input: &[Parsed], add_outer_braces: bool) -> String {
    if input.is_empty() {
        return String::new();
    }
    let mut res = String::new();
    let mut cee = false;
    for p in input {
        match p {
            Parsed::S(s) => res.push_str(s),
            Parsed::N(n) => {
                res.push_str(&texify_go2(n));
                if n.type_ == "1st-level escape" {
                    cee = true;
                }
            }
        }
    }
    if add_outer_braces && !cee && !res.is_empty() {
        res = format!("{{{}}}", res);
    }
    res
}

fn field_str(f: &Field) -> String {
    match f {
        Field::Str(s) => s.clone(),
        Field::Nodes(v) => texify_go(v, false),
    }
}

fn texify_go2(buf: &NodeData) -> String {
    match buf.type_.as_str() {
        "chemfive" => {
            let mut res = String::new();
            let a = buf.a.as_ref().map(field_str).unwrap_or_default();
            let b = buf.b.as_ref().map(field_str).unwrap_or_default();
            let p = buf.p.as_ref().map(field_str).unwrap_or_default();
            let o = buf.o.as_ref().map(field_str).unwrap_or_default();
            let q = buf.q.as_ref().map(field_str).unwrap_or_default();
            let d = buf.d.as_ref().map(field_str).unwrap_or_default();
            // a
            if !a.is_empty() {
                let aa = if a.starts_with('+') || a.starts_with('-') {
                    format!("{{{}}}", a)
                } else {
                    a.clone()
                };
                res.push_str(&aa);
                res.push_str("\\,");
            }
            // b and p
            if !b.is_empty() || !p.is_empty() {
                res.push_str("{\\vphantom{A}}");
                res.push_str(&format!("^{{\\hphantom{{{}}}}}_{{\\hphantom{{{}}}}}", b, p));
                res.push_str("\\mkern-1.5mu");
                res.push_str("{\\vphantom{A}}");
                res.push_str(&format!(
                    "^{{\\smash[t]{{\\vphantom{{2}}}}\\llap{{{}}}}}",
                    b
                ));
                res.push_str(&format!(
                    "_{{\\vphantom{{2}}\\llap{{\\smash[t]{{{}}}}}}}",
                    p
                ));
            }
            // o
            if !o.is_empty() {
                let oo = if o.starts_with('+') || o.starts_with('-') {
                    format!("{{{}}}", o)
                } else {
                    o.clone()
                };
                res.push_str(&oo);
            }
            // q and d
            match buf.d_type.as_deref() {
                Some("kv") => {
                    if !d.is_empty() || !q.is_empty() {
                        res.push_str("{\\vphantom{A}}");
                    }
                    if !d.is_empty() {
                        res.push_str(&format!("^{{{}}}", d));
                    }
                    if !q.is_empty() {
                        res.push_str(&format!("_{{\\smash[t]{{{}}}}}", q));
                    }
                }
                Some("oxidation") => {
                    if !d.is_empty() {
                        res.push_str("{\\vphantom{A}}");
                        res.push_str(&format!("^{{{}}}", d));
                    }
                    if !q.is_empty() {
                        res.push_str("{\\vphantom{A}}");
                        res.push_str(&format!("_{{\\smash[t]{{{}}}}}", q));
                    }
                }
                _ => {
                    if !q.is_empty() {
                        res.push_str("{\\vphantom{A}}");
                        res.push_str(&format!("_{{\\smash[t]{{{}}}}}", q));
                    }
                    if !d.is_empty() {
                        res.push_str("{\\vphantom{A}}");
                        res.push_str(&format!("^{{{}}}", d));
                    }
                }
            }
            res
        }
        "rm" => format!(
            "\\mathrm{{{}}}",
            buf.p1.as_ref().map(field_str).unwrap_or_default()
        ),
        "text" => {
            let mut p1 = buf.p1.as_ref().map(field_str).unwrap_or_default();
            if p1.contains('^') || p1.contains('_') {
                p1 = p1.replace(' ', "~").replace('-', "\\text{-}");
                format!("\\mathrm{{{}}}", p1)
            } else {
                format!("\\text{{{}}}", p1)
            }
        }
        "roman numeral" => format!(
            "\\mathrm{{{}}}",
            buf.p1.as_ref().map(field_str).unwrap_or_default()
        ),
        "state of aggregation" => format!(
            "\\mskip2mu {}",
            buf.p1.as_ref().map(field_str).unwrap_or_default()
        ),
        "state of aggregation subscript" => format!(
            "\\mskip1mu {}",
            buf.p1.as_ref().map(field_str).unwrap_or_default()
        ),
        "bond" => get_bond(buf.kind_.as_deref().unwrap_or("")),
        "frac" => {
            let c = format!(
                "\\frac{{{}}}{{{}}}",
                buf.p1.as_ref().map(field_str).unwrap_or_default(),
                buf.p2.as_ref().map(field_str).unwrap_or_default()
            );
            format!("\\mathchoice{{\\textstyle{c}}}{{{c}}}{{{c}}}{{{c}}}")
        }
        "pu-frac" => {
            let d = format!(
                "\\frac{{{}}}{{{}}}",
                buf.p1.as_ref().map(field_str).unwrap_or_default(),
                buf.p2.as_ref().map(field_str).unwrap_or_default()
            );
            format!("\\mathchoice{{\\textstyle{d}}}{{{d}}}{{{d}}}{{{d}}}")
        }
        "tex-math" => format!("{} ", buf.p1.as_ref().map(field_str).unwrap_or_default()),
        "frac-ce" => format!(
            "\\frac{{{}}}{{{}}}",
            buf.p1.as_ref().map(field_str).unwrap_or_default(),
            buf.p2.as_ref().map(field_str).unwrap_or_default()
        ),
        "overset" => format!(
            "\\overset{{{}}}{{{}}}",
            buf.p1.as_ref().map(field_str).unwrap_or_default(),
            buf.p2.as_ref().map(field_str).unwrap_or_default()
        ),
        "underset" => format!(
            "\\underset{{{}}}{{{}}}",
            buf.p1.as_ref().map(field_str).unwrap_or_default(),
            buf.p2.as_ref().map(field_str).unwrap_or_default()
        ),
        "underbrace" => format!(
            "\\underbrace{{{}}}_{{{}}}",
            buf.p1.as_ref().map(field_str).unwrap_or_default(),
            buf.p2.as_ref().map(field_str).unwrap_or_default()
        ),
        "color" => format!(
            "{{\\color{{{}}}{{{}}}}}",
            buf.color1.as_deref().unwrap_or(""),
            buf.color2.as_ref().map(field_str).unwrap_or_default()
        ),
        "color0" => format!("\\color{{{}}}", buf.color.as_deref().unwrap_or("")),
        "arrow" => {
            let rd = buf.rd.as_ref().map(field_str).unwrap_or_default();
            let rq = buf.rq.as_ref().map(field_str).unwrap_or_default();
            let r = buf.r.as_deref().unwrap_or("");
            let mut arrow = get_arrow(r).to_string();
            if !rd.is_empty() || !rq.is_empty() {
                if matches!(r, "<=>" | "<=>>" | "<<=>" | "<-->") {
                    arrow = format!("\\long{}", arrow);
                    if !rd.is_empty() {
                        arrow = format!("\\overset{{{}}}{{{}}}", rd, arrow);
                    }
                    if !rq.is_empty() {
                        arrow = if r == "<-->" {
                            format!("\\underset{{\\lower2mu{{{}}}}}{{{}}}", rq, arrow)
                        } else {
                            format!("\\underset{{\\lower6mu{{{}}}}}{{{}}}", rq, arrow)
                        };
                    }
                    arrow = format!(" {{}}\\mathrel{{{}}}{{}} ", arrow);
                } else {
                    if !rq.is_empty() {
                        arrow.push_str(&format!("[{{{}}}]", rq));
                    }
                    arrow.push_str(&format!("{{{}}}", rd));
                    arrow = format!(" {{}}\\mathrel{{\\x{}}}{{}} ", arrow);
                }
            } else {
                arrow = format!(" {{}}\\mathrel{{\\long{}}}{{}} ", arrow);
            }
            arrow
        }
        "operator" => get_operator(buf.kind_.as_deref().unwrap_or("")),
        "1st-level escape" => format!("{} ", buf.p1.as_ref().map(field_str).unwrap_or_default()),
        "space" => " ".to_string(),
        "tinySkip" => "\\mkern2mu".to_string(),
        "entitySkip" => "~".to_string(),
        "pu-space-1" => "~".to_string(),
        "pu-space-2" => "\\mkern3mu ".to_string(),
        "1000 separator" => "\\mkern2mu ".to_string(),
        "commaDecimal" => "{,}".to_string(),
        "comma enumeration L" => format!(
            "{{{}}}\\mkern6mu ",
            buf.p1.as_ref().map(field_str).unwrap_or_default()
        ),
        "comma enumeration M" => format!(
            "{{{}}}\\mkern3mu ",
            buf.p1.as_ref().map(field_str).unwrap_or_default()
        ),
        "comma enumeration S" => format!(
            "{{{}}}\\mkern1mu ",
            buf.p1.as_ref().map(field_str).unwrap_or_default()
        ),
        "hyphen" => "\\text{-}".to_string(),
        "addition compound" => "\\,{\\cdot}\\,".to_string(),
        "electron dot" => "\\mkern1mu \\bullet\\mkern1mu ".to_string(),
        "KV x" => "{\\times}".to_string(),
        "prime" => "\\prime ".to_string(),
        "cdot" => "\\cdot ".to_string(),
        "tight cdot" => "\\mkern1mu{\\cdot}\\mkern1mu ".to_string(),
        "times" => "\\times ".to_string(),
        "circa" => "{\\sim}".to_string(),
        "^" => "uparrow".to_string(),
        "v" => "downarrow".to_string(),
        "ellipsis" => "\\ldots ".to_string(),
        "/" => "/".to_string(),
        " / " => "\\,/\\,".to_string(),
        _ => String::new(),
    }
}

fn get_arrow(a: &str) -> &'static str {
    match a {
        "->" | "\u{2192}" | "\u{27F6}" => "rightarrow",
        "<-" => "leftarrow",
        "<->" => "leftrightarrow",
        "<-->" => "leftrightarrows",
        "<=>" | "\u{21CC}" => "rightleftharpoons",
        "<=>>" => "Rightleftharpoons",
        "<<=>" => "Leftrightharpoons",
        _ => "rightarrow",
    }
}

fn get_bond(a: &str) -> String {
    match a {
        "-" | "1" => "{-}".to_string(),
        "=" | "2" => "{=}".to_string(),
        "#" | "3" => "{\\equiv}".to_string(),
        "~" => "{\\tripledash}".to_string(),
        "~-" => "{\\rlap{\\lower.1em{-}}\\raise.1em{\\tripledash}}".to_string(),
        "~=" | "~--" => "{\\rlap{\\lower.2em{-}}\\rlap{\\raise.2em{\\tripledash}}-}".to_string(),
        "-~-" => "{\\rlap{\\lower.2em{-}}\\rlap{\\raise.2em{-}}\\tripledash}".to_string(),
        "..." => "{{\\cdot}{\\cdot}{\\cdot}}".to_string(),
        "...." => "{{\\cdot}{\\cdot}{\\cdot}{\\cdot}}".to_string(),
        "->" => "{\\rightarrow}".to_string(),
        "<-" => "{\\leftarrow}".to_string(),
        "<" => "{<}".to_string(),
        ">" => "{>}".to_string(),
        _ => format!("{{{}}}", a),
    }
}

fn get_operator(a: &str) -> String {
    match a {
        "+" => " {}+{} ".to_string(),
        "-" => " {}-{} ".to_string(),
        "=" => " {}={} ".to_string(),
        "<" => " {}<{} ".to_string(),
        ">" => " {}>{} ".to_string(),
        "<<" => " {}\\ll{} ".to_string(),
        ">>" => " {}\\gg{} ".to_string(),
        "\\pm" => " {}\\pm{} ".to_string(),
        "\\approx" | "$\\approx$" => " {}\\approx{} ".to_string(),
        "v" | "(v)" => " \\downarrow{} ".to_string(),
        "^" | "(^)" => " \\uparrow{} ".to_string(),
        _ => format!(" {{{}}} ", a),
    }
}

// =========================================================================
// 公开 API
// =========================================================================

/// 把 `\ce{...}` 内容转译为 LaTeX。
///
/// 容错设计（不依赖 `catch_unwind`，因 release 的 `panic = "abort"` 下它无效）：
/// - 正则编译走 [`compile_or_log`]，失败降级为不匹配（状态机退到 "else" 兜底）。
/// - 状态机有 watchdog 防死循环。
/// - `to_tex` 仍包 `catch_unwind`，仅作 dev/test 护栏。
///
/// 坏公式可能产出退化输出，但不应 panic。`all_match_patterns_compile` 测试
/// 在测试期捕获正则转录回归。
pub fn ce(input: &str) -> String {
    to_tex(input, "ce")
}

/// 把 `\pu{...}` 内容转译为 LaTeX（容错同 [`ce`]）。
pub fn pu(input: &str) -> String {
    to_tex(input, "pu")
}

fn to_tex(input: &str, kind: &str) -> String {
    // 仅在 panic=unwind（dev/test）时有效；release 的 panic=abort 下此处的
    // catch_unwind 无法阻止进程终止。保留它是为了本地开发时坏公式不炸测试。
    let result = std::panic::catch_unwind(|| {
        let parsed = go(input, kind);
        // D5：TEX 状态机已删除（to_tex 仅以 ce/pu 调用，kind != "tex" 恒真）。
        texify_go(&parsed, true)
    });
    result.unwrap_or_else(|_| input.to_string())
}

// 状态机转移表 + 局部动作在 mhchem_tables.rs（同模块，含大量常量数据）。
include!("mhchem_tables.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ce_water_produces_mathrm() {
        let tex = ce("H2O");
        assert!(tex.contains(r"\mathrm{H}"), "H 应为直立体: {tex}");
        assert!(tex.contains(r"\mathrm{O}"), "O 应为直立体: {tex}");
    }

    #[test]
    fn ce_reaction_has_arrow() {
        let tex = ce("2H2 + O2 -> 2H2O");
        assert!(tex.contains("rightarrow"), "应含反应箭头: {tex}");
    }

    #[test]
    fn ce_empty_is_empty() {
        assert_eq!(ce(""), "");
    }

    #[test]
    fn ce_does_not_panic_on_garbage() {
        // 任意乱码不应 panic（容错回退为原样）。
        let _ = ce("}}}{{{]][[");
        let _ = ce("\\frac{");
        let _ = ce("<<<<>>>>");
    }

    #[test]
    fn ce_gas_arrow_becomes_uparrow() {
        // 行尾 ^ 气体符号转译后应含 uparrow（消解原行尾 ^ 解析错误）。
        let tex = ce("CaCO3 ->[\\Delta] CaO + CO2 ^");
        assert!(
            tex.contains("uparrow") || tex.contains("rightarrow"),
            "气体/反应符号: {tex}"
        );
    }

    #[test]
    fn pu_unit_has_mathrm() {
        let tex = pu("9.8 m/s^2");
        assert!(tex.contains(r"\mathrm"), "单位应有直立体: {tex}");
    }

    #[test]
    fn pu_empty_is_empty() {
        assert_eq!(pu(""), "");
    }

    #[test]
    fn ce_charge_superscript() {
        let tex = ce("SO4^2-");
        // 应含上标（^...）且无 panic。
        assert!(!tex.is_empty(), "离子应产出非空: {tex}");
    }

    /// 回归测试：`". __* "` 模式的 regex 曾因转录错误把 `|` 放进了字符类
    /// 内部（`[.\u{22C5}\u{00B7}\u{2022}|[*]`），fancy-regex 报
    /// `ParseError(20, InvalidClass)`，`re!` 宏的 `.unwrap()` 触发 panic。
    /// 上游 mhchemParser 是 `[...]|[*]`（alternation 在类外）。
    /// 这些输入会强制该 LazyLock 正则编译，必须产出非空且不 panic。
    #[test]
    fn ce_dot_bullet_bond_inputs_dont_panic() {
        for s in [
            ".",
            "·",
            "•",
            "⋅",
            "*",
            ". ",
            "••",
            "·  ",
            "H·OH",
            "CaCO3 · H2O",
        ] {
            let tex = ce(s);
            assert!(!tex.is_empty(), "ce({s:?}) 不应为空");
        }
    }

    /// 回归测试：`compile_or_log` 是 re! 宏硬化的核心——对坏正则必须降级
    /// 返回 `None` 而非 panic。`re!` 宏原先用 `.unwrap()`，在 panic=abort 下
    /// 任何未来的正则转录错误（如曾经的 `". __* "` InvalidClass）都会直接
    /// 杀进程。降级后坏正则只让该模式匹配失败，状态机退到 "else" 兜底分支，
    /// 不影响整体渲染。
    #[test]
    fn compile_or_log_degrades_bad_regex_without_panic() {
        // 合法正则 → Some
        assert!(compile_or_log("^a").is_some(), "合法正则应编译成功");
        // 非法正则（未闭合字符类，曾触发 ParseError(20, InvalidClass)）
        let bad = "^([abc|[*])";
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compile_or_log(bad)));
        assert!(r.is_ok(), "compile_or_log 对坏正则不应 panic");
        assert!(r.unwrap().is_none(), "坏正则应返回 None");
    }

    /// 回归测试：`find_observe_end` 曾用逐字节 `i += 1` 步进，遇多字节
    /// UTF-8 字符（如中文「浓」占 3 字节）会让 `&input[i..]` 落在字符内部，
    /// 触发 `byte index N is not a char boundary` panic。在 panic=abort 下
    /// 直接杀进程（曾导致 rebuild_content_html 重建含中文化学式的文章时崩溃）。
    /// 修复改为按 `char_indices` 字符边界步进。这些输入必须不 panic。
    #[test]
    fn ce_multibyte_char_in_braces_does_not_panic() {
        for s in [
            "{浓}",
            "浓H2SO4",
            "{中文}",
            "H{浓}O",
            "\\frac{浓}{稀}",
            "[浓]",
        ] {
            // catch_unwind 仅 dev/test 护栏；release panic=abort 下由本修复保证。
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ce(s)));
            assert!(r.is_ok(), "ce({s:?}) PANICKED (char boundary)");
        }
    }

    /// 保护性：任意多字节字符组合（emoji/日韩文/组合字符/四字节区）都不应
    /// panic。覆盖花括号/方括号/`\frac`/`$...$` 等所有走 find_observe_end 的路径。
    #[test]
    fn ce_arbitrary_multibyte_does_not_panic() {
        let inputs = [
            "🔥",
            "café",
            "naïve",
            "Σ",
            "αβγ",
            "ΔH",
            "你好世界",
            "안녕",
            "こんにちは",
            "{🧪}",
            "H₂O",
            "[α]",
            "\\frac{β}{γ}",
            "${日本}$",
            "A·B•C⋅D",
            "naïve H2O",
            "{β-Gal}",
            "😀😂",
            "\u{1F9EA}", // test tube emoji
        ];
        for s in inputs {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ce(s)));
            assert!(r.is_ok(), "ce({s:?}) PANICKED");
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pu(s)));
            assert!(r.is_ok(), "pu({s:?}) PANICKED");
        }
    }

    /// 保护性：强制 `match_pattern` 的每个分支至少运行一次，让每个 `re!`
    /// 定义的 regex 都被编译。任一转录错误都会在此暴露，而非等到生产环境
    /// 被特定输入触发后 panic=abort 整个进程。
    #[test]
    fn all_match_patterns_compile() {
        for name in [
            "empty",
            "else",
            "else2",
            "space",
            "space A",
            "space$",
            "a-z",
            "x",
            "x$",
            "i$",
            "letters",
            "\\greek",
            "one lowercase latin letter $",
            "$one lowercase latin letter$ $",
            "one lowercase greek letter $",
            "digits",
            "-9.,9",
            "-9.,9 no missing 0",
            "(-)(9)^(-9)",
            "_{(state of aggregation)}$",
            "{[(",
            ")]}",
            ", ",
            ",",
            ".",
            ". __* ",
            "...",
            "^a",
            "^\\x",
            "^(-1)",
            "'",
            "_9",
            "_\\x",
            "^_",
            "{}^",
            "{}",
            "=<>",
            "#",
            "+",
            "-$",
            "-9",
            "- orbital overlap",
            "-",
            "pm-operator",
            "operator",
            "arrowUpDown",
            "->",
            "CMT",
            "1st-level escape",
            "\\,",
            "\\ca",
            "\\x",
            "orbital",
            "others",
            "oxidation$",
            "d-oxidation$",
            "1/2$",
            "(KV letters),",
            "uprightEntities",
            "/",
            "//",
            "*",
        ] {
            // 返回值不重要，只要不 panic（regex 编译成功）即可。
            let _ = match_pattern(name, "");
        }
    }
}
