// mhchem 状态机转移表 + 机器局部动作（由 mhchem.rs `include!`）。
// 数据机械移植自 mhchemParser 4.2.2 的 `stateMachines`。
// （被 include 进 mhchem.rs 中部，故不能用 `//!` 内部文档注释。）

// ── Task 构造器 ──────────────────────────────────────────────────────────

fn task(acts: &[(&str, Option<&str>)], next: Option<&str>, revisit: bool, cont: bool) -> Task {
    Task {
        actions: acts
            .iter()
            .map(|(t, o)| ActionRef {
                type_: (*t).to_string(),
                option: o.map(|s| s.to_string()),
            })
            .collect(),
        next_state: next.map(|s| s.to_string()),
        revisit,
        to_continue: cont,
    }
}

fn raw(patterns: &'static str, states: &'static str, task: Task) -> RawEntry {
    RawEntry {
        patterns,
        states,
        task,
    }
}

// ── 转移表取用 ────────────────────────────────────────────────────────────

fn transitions_for(machine: &str) -> &'static HashMap<String, Vec<Transition>> {
    match machine {
        "ce" => &CE,
        "a" => &A,
        "o" => &O,
        "text" => &TEXT,
        "pq" => &PQ,
        "bd" => &BD,
        "oxidation" => &OXIDATION,
        "tex-math" => &TEX_MATH,
        "tex-math tight" => &TEX_MATH_TIGHT,
        "9,9" => &NUM99,
        "pu" => &PU,
        "pu-2" => &PU2,
        "pu-9,9" => &PU99,
        _ => &CE,
    }
}

// =========================================================================
// ce 状态机（主解析器）
// =========================================================================

static CE: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "*", task(&[("output", None)], None, false, false)),
        raw(
            "else",
            "0|1|2",
            task(&[("beginsWithBond=false", None)], None, true, true),
        ),
        raw(
            "oxidation$",
            "0",
            task(&[("oxidation-output", None)], None, false, false),
        ),
        raw(
            "CMT",
            "r",
            task(&[("rdt=", None)], Some("rt"), false, false),
        ),
        raw(
            "CMT",
            "rd",
            task(&[("rqt=", None)], Some("rdt"), false, false),
        ),
        raw(
            "arrowUpDown",
            "0|1|2|as",
            task(&[("sb=false", None), ("output", None), ("operator", None)], Some("1"), false, false),
        ),
        raw(
            "uprightEntities",
            "0|1|2",
            task(&[("o=", None), ("output", None)], Some("1"), false, false),
        ),
        raw("orbital", "0|1|2|3", task(&[("o=", None)], Some("o"), false, false)),
        raw("->", "0|1|2|3", task(&[("r=", None)], Some("r"), false, false)),
        raw(
            "->",
            "a|as",
            task(&[("output", None), ("r=", None)], Some("r"), false, false),
        ),
        raw(
            "->",
            "*",
            task(&[("output", None), ("r=", None)], Some("r"), false, false),
        ),
        raw("+", "o", task(&[("d= kv", None)], Some("d"), false, false)),
        raw("+", "d|D", task(&[("d=", None)], Some("d"), false, false)),
        raw("+", "q", task(&[("d=", None)], Some("qd"), false, false)),
        raw("+", "qd|qD", task(&[("d=", None)], Some("qd"), false, false)),
        raw(
            "+",
            "dq",
            task(&[("output", None), ("d=", None)], Some("d"), false, false),
        ),
        raw(
            "+",
            "3",
            task(&[("sb=false", None), ("output", None), ("operator", None)], Some("0"), false, false),
        ),
        raw("amount", "0|2", task(&[("a=", None)], Some("a"), false, false)),
        raw(
            "pm-operator",
            "0|1|2|a|as",
            task(&[("sb=false", None), ("output", None), ("operator", Some("\\pm"))], Some("0"), false, false),
        ),
        raw(
            "operator",
            "0|1|2|a|as",
            task(&[("sb=false", None), ("output", None), ("operator", None)], Some("0"), false, false),
        ),
        raw(
            "-$",
            "o|q",
            task(&[("charge or bond", None), ("output", None)], Some("qd"), false, false),
        ),
        raw("-$", "d", task(&[("d=", None)], Some("d"), false, false)),
        raw(
            "-$",
            "D",
            task(&[("output", None), ("bond", Some("-"))], Some("3"), false, false),
        ),
        raw("-$", "q", task(&[("d=", None)], Some("qd"), false, false)),
        raw("-$", "qd", task(&[("d=", None)], Some("qd"), false, false)),
        raw(
            "-$",
            "qD|dq",
            task(&[("output", None), ("bond", Some("-"))], Some("3"), false, false),
        ),
        raw(
            "-9",
            "3|o",
            task(&[("output", None), ("insert", Some("hyphen"))], Some("3"), false, false),
        ),
        raw(
            "- orbital overlap",
            "o",
            task(&[("output", None), ("insert", Some("hyphen"))], Some("2"), false, false),
        ),
        raw(
            "- orbital overlap",
            "d",
            task(&[("output", None), ("insert", Some("hyphen"))], Some("2"), false, false),
        ),
        raw(
            "-",
            "0|1|2",
            task(&[("output", Some("1")), ("beginsWithBond=true", None), ("bond", Some("-"))], Some("3"), false, false),
        ),
        raw("-", "3", task(&[("bond", Some("-"))], None, false, false)),
        raw(
            "-",
            "a",
            task(&[("output", None), ("insert", Some("hyphen"))], Some("2"), false, false),
        ),
        raw(
            "-",
            "as",
            task(&[("output", Some("2")), ("bond", Some("-"))], Some("3"), false, false),
        ),
        raw("-", "b", task(&[("b=", None)], None, false, false)),
        raw(
            "-",
            "o",
            task(&[("- after o/d", Some("false"))], Some("2"), false, false),
        ),
        raw(
            "-",
            "q",
            task(&[("- after o/d", Some("false"))], Some("2"), false, false),
        ),
        raw(
            "-",
            "d|qd|dq",
            task(&[("- after o/d", Some("true"))], Some("2"), false, false),
        ),
        raw(
            "-",
            "D|qD|p",
            task(&[("output", None), ("bond", Some("-"))], Some("3"), false, false),
        ),
        raw("amount2", "1|3", task(&[("a=", None)], Some("a"), false, false)),
        raw(
            "letters",
            "0|1|2|3|a|as|b|p|bp|o",
            task(&[("o=", None)], Some("o"), false, false),
        ),
        raw(
            "letters",
            "q|dq",
            task(&[("output", None), ("o=", None)], Some("o"), false, false),
        ),
        raw(
            "letters",
            "d|D|qd|qD",
            task(&[("o after d", None)], Some("o"), false, false),
        ),
        raw("digits", "o", task(&[("q=", None)], Some("q"), false, false)),
        raw("digits", "d|D", task(&[("q=", None)], Some("dq"), false, false)),
        raw(
            "digits",
            "q",
            task(&[("output", None), ("o=", None)], Some("o"), false, false),
        ),
        raw("digits", "a", task(&[("o=", None)], Some("o"), false, false)),
        raw("space A", "b|p|bp", task(&[], None, false, false)),
        raw("space", "a", task(&[], Some("as"), false, false)),
        raw("space", "0", task(&[("sb=false", None)], None, false, false)),
        raw("space", "1|2", task(&[("sb=true", None)], None, false, false)),
        raw(
            "space",
            "r|rt|rd|rdt|rdq",
            task(&[("output", None)], Some("0"), false, false),
        ),
        raw(
            "space",
            "*",
            task(&[("output", None), ("sb=true", None)], Some("1"), false, false),
        ),
        raw(
            "1st-level escape",
            "1|2",
            task(&[("output", None), ("insert+p1", Some("1st-level escape"))], None, false, false),
        ),
        raw(
            "1st-level escape",
            "*",
            task(&[("output", None), ("insert+p1", Some("1st-level escape"))], Some("0"), false, false),
        ),
        raw("[(...)]", "r|rt", task(&[("rd=", None)], Some("rd"), false, false)),
        raw(
            "[(...)]",
            "rd|rdt",
            task(&[("rq=", None)], Some("rdq"), false, false),
        ),
        raw(
            "...",
            "o|d|D|dq|qd|qD",
            task(&[("output", None), ("bond", Some("..."))], Some("3"), false, false),
        ),
        raw(
            "...",
            "*",
            task(&[("output", Some("1")), ("insert", Some("ellipsis"))], Some("1"), false, false),
        ),
        raw(
            ". __* ",
            "*",
            task(&[("output", None), ("insert", Some("addition compound"))], Some("1"), false, false),
        ),
        raw(
            "state of aggregation $",
            "*",
            task(&[("output", None), ("state of aggregation", None)], Some("1"), false, false),
        ),
        raw(
            "{[(",
            "a|as|o",
            task(&[("o=", None), ("output", None), ("parenthesisLevel++", None)], Some("2"), false, false),
        ),
        raw(
            "{[(",
            "0|1|2|3",
            task(&[("o=", None), ("output", None), ("parenthesisLevel++", None)], Some("2"), false, false),
        ),
        raw(
            "{[(",
            "*",
            task(&[("output", None), ("o=", None), ("output", None), ("parenthesisLevel++", None)], Some("2"), false, false),
        ),
        raw(
            ")]}",
            "0|1|2|3|b|p|bp|o",
            task(&[("o=", None), ("parenthesisLevel--", None)], Some("o"), false, false),
        ),
        raw(
            ")]}",
            "a|as|d|D|q|qd|qD|dq",
            task(&[("output", None), ("o=", None), ("parenthesisLevel--", None)], Some("o"), false, false),
        ),
        raw(
            ", ",
            "*",
            task(&[("output", None), ("comma", None)], Some("0"), false, false),
        ),
        raw("^_", "*", task(&[], None, false, false)),
        raw(
            "^{(...)}|^($...$)",
            "0|1|2|as",
            task(&[("b=", None)], Some("b"), false, false),
        ),
        raw(
            "^{(...)}|^($...$)",
            "p",
            task(&[("b=", None)], Some("bp"), false, false),
        ),
        raw(
            "^{(...)}|^($...$)",
            "3|o",
            task(&[("d= kv", None)], Some("D"), false, false),
        ),
        raw(
            "^{(...)}|^($...$)",
            "q",
            task(&[("d=", None)], Some("qD"), false, false),
        ),
        raw(
            "^{(...)}|^($...$)",
            "d|D|qd|qD|dq",
            task(&[("output", None), ("d=", None)], Some("D"), false, false),
        ),
        raw(
            "^a|^\\x{}{}|^\\x{}|^\\x|'",
            "0|1|2|as",
            task(&[("b=", None)], Some("b"), false, false),
        ),
        raw(
            "^a|^\\x{}{}|^\\x{}|^\\x|'",
            "p",
            task(&[("b=", None)], Some("bp"), false, false),
        ),
        raw(
            "^a|^\\x{}{}|^\\x{}|^\\x|'",
            "3|o",
            task(&[("d= kv", None)], Some("d"), false, false),
        ),
        raw(
            "^a|^\\x{}{}|^\\x{}|^\\x|'",
            "q",
            task(&[("d=", None)], Some("qd"), false, false),
        ),
        raw(
            "^a|^\\x{}{}|^\\x{}|^\\x|'",
            "d|qd|D|qD",
            task(&[("d=", None)], None, false, false),
        ),
        raw(
            "^a|^\\x{}{}|^\\x{}|^\\x|'",
            "dq",
            task(&[("output", None), ("d=", None)], Some("d"), false, false),
        ),
        raw(
            "_{(state of aggregation)}$",
            "d|D|q|qd|qD|dq",
            task(&[("output", None), ("q=", None)], Some("q"), false, false),
        ),
        raw(
            "_{(...)}|_($...$)|_9|_\\x{}{}|_\\x{}|_\\x",
            "0|1|2|as",
            task(&[("p=", None)], Some("p"), false, false),
        ),
        raw(
            "_{(...)}|_($...$)|_9|_\\x{}{}|_\\x{}|_\\x",
            "b",
            task(&[("p=", None)], Some("bp"), false, false),
        ),
        raw(
            "_{(...)}|_($...$)|_9|_\\x{}{}|_\\x{}|_\\x",
            "3|o",
            task(&[("q=", None)], Some("q"), false, false),
        ),
        raw(
            "_{(...)}|_($...$)|_9|_\\x{}{}|_\\x{}|_\\x",
            "d|D",
            task(&[("q=", None)], Some("dq"), false, false),
        ),
        raw(
            "_{(...)}|_($...$)|_9|_\\x{}{}|_\\x{}|_\\x",
            "q|qd|qD|dq",
            task(&[("output", None), ("q=", None)], Some("q"), false, false),
        ),
        raw(
            "=<>",
            "0|1|2|3|a|as|o|q|d|D|qd|qD|dq",
            task(&[("output", Some("2")), ("bond", None)], Some("3"), false, false),
        ),
        raw(
            "#",
            "0|1|2|3|a|as|o",
            task(&[("output", Some("2")), ("bond", Some("#"))], Some("3"), false, false),
        ),
        raw(
            "{}^",
            "*",
            task(&[("output", Some("1")), ("insert", Some("tinySkip"))], Some("1"), false, false),
        ),
        raw("{}", "*", task(&[("output", Some("1"))], Some("1"), false, false)),
        raw(
            "{...}",
            "0|1|2|3|a|as|b|p|bp",
            task(&[("o=", None)], Some("o"), false, false),
        ),
        raw(
            "{...}",
            "o|d|D|q|qd|qD|dq",
            task(&[("output", None), ("o=", None)], Some("o"), false, false),
        ),
        raw("$...$", "a", task(&[("a=", None)], None, false, false)),
        raw(
            "$...$",
            "0|1|2|3|as|b|p|bp|o",
            task(&[("o=", None)], Some("o"), false, false),
        ),
        raw("$...$", "as|o", task(&[("o=", None)], None, false, false)),
        raw(
            "$...$",
            "q|d|D|qd|qD|dq",
            task(&[("output", None), ("o=", None)], Some("o"), false, false),
        ),
        raw(
            "\\bond{(...)}",
            "*",
            task(&[("output", Some("2")), ("bond", None)], Some("3"), false, false),
        ),
        raw(
            "\\frac{(...)}",
            "*",
            task(&[("output", Some("1")), ("frac-output", None)], Some("3"), false, false),
        ),
        raw(
            "\\overset{(...)}",
            "*",
            task(&[("output", Some("2")), ("overset-output", None)], Some("3"), false, false),
        ),
        raw(
            "\\underset{(...)}",
            "*",
            task(&[("output", Some("2")), ("underset-output", None)], Some("3"), false, false),
        ),
        raw(
            "\\underbrace{(...)}",
            "*",
            task(&[("output", Some("2")), ("underbrace-output", None)], Some("3"), false, false),
        ),
        raw(
            "\\color{(...)}{(...)}",
            "*",
            task(&[("output", Some("2")), ("color-output", None)], Some("3"), false, false),
        ),
        raw(
            "\\color{(...)}",
            "*",
            task(&[("output", Some("2")), ("color0-output", None)], None, false, false),
        ),
        raw(
            "\\ce{(...)}",
            "*",
            task(&[("output", Some("2")), ("ce", None)], Some("3"), false, false),
        ),
        raw(
            "\\,",
            "*",
            task(&[("output", Some("1")), ("copy", None)], Some("1"), false, false),
        ),
        raw(
            "\\pu{(...)}",
            "*",
            task(&[("output", None), ("write", Some("{")), ("pu", None), ("write", Some("}"))], Some("3"), false, false),
        ),
        raw(
            "\\x{}{}|\\x{}|\\x",
            "0|1|2|3|a|as|b|p|bp|o|c0",
            task(&[("o=", None), ("output", None)], Some("3"), false, false),
        ),
        raw(
            "\\x{}{}|\\x{}|\\x",
            "*",
            task(&[("output", None), ("o=", None), ("output", None)], Some("3"), false, false),
        ),
        raw(
            "others",
            "*",
            task(&[("output", Some("1")), ("copy", None)], Some("3"), false, false),
        ),
        raw(
            "else2",
            "a",
            task(&[("a to o", None)], Some("o"), true, false),
        ),
        raw(
            "else2",
            "as",
            task(&[("output", None), ("sb=true", None)], Some("1"), true, false),
        ),
        raw(
            "else2",
            "r|rt|rd|rdt|rdq",
            task(&[("output", None)], Some("0"), true, false),
        ),
        raw(
            "else2",
            "*",
            task(&[("output", None), ("copy", None)], Some("3"), true, false),
        ),
    ])
});

// =========================================================================
// a / o / text / pq / bd / oxidation / tex-math / tex-math tight / 9,9
// =========================================================================

static A: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "*", task(&[], None, false, false)),
        raw("1/2$", "0", task(&[("1/2", None)], None, false, false)),
        raw("else", "0", task(&[], Some("1"), true, false)),
        raw(
            "${(...)}$__$(...)$",
            "*",
            task(&[("tex-math tight", None)], Some("1"), false, false),
        ),
        raw(
            ",",
            "*",
            task(&[("insert", Some("commaDecimal"))], None, false, false),
        ),
        raw("else2", "*", task(&[("copy", None)], None, false, false)),
    ])
});

static O: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "*", task(&[], None, false, false)),
        raw("1/2$", "0", task(&[("1/2", None)], None, false, false)),
        raw("else", "0", task(&[], Some("1"), true, false)),
        raw("letters", "*", task(&[("rm", None)], None, false, false)),
        raw(
            "\\ca",
            "*",
            task(&[("insert", Some("circa"))], None, false, false),
        ),
        raw(
            "\\pu{(...)}",
            "*",
            task(&[("write", Some("{")), ("pu", None), ("write", Some("}"))], None, false, false),
        ),
        raw("\\x{}{}|\\x{}|\\x", "*", task(&[("copy", None)], None, false, false)),
        raw(
            "${(...)}$__$(...)$",
            "*",
            task(&[("tex-math", None)], None, false, false),
        ),
        raw(
            "{(...)}",
            "*",
            task(&[("write", Some("{")), ("text", None), ("write", Some("}"))], None, false, false),
        ),
        raw("else2", "*", task(&[("copy", None)], None, false, false)),
    ])
});

static TEXT: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "*", task(&[("output", None)], None, false, false)),
        raw("{...}", "*", task(&[("text=", None)], None, false, false)),
        raw(
            "${(...)}$__$(...)$",
            "*",
            task(&[("tex-math", None)], None, false, false),
        ),
        raw(
            "\\greek",
            "*",
            task(&[("output", None), ("rm", None)], None, false, false),
        ),
        raw(
            "\\pu{(...)}",
            "*",
            task(&[("output", None), ("write", Some("{")), ("pu", None), ("write", Some("}"))], None, false, false),
        ),
        raw(
            "\\,|\\x{}{}|\\x{}|\\x",
            "*",
            task(&[("output", None), ("copy", None)], None, false, false),
        ),
        raw("else", "*", task(&[("text=", None)], None, false, false)),
    ])
});

static PQ: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "*", task(&[], None, false, false)),
        raw(
            "state of aggregation $",
            "*",
            task(&[("state of aggregation", None)], None, false, false),
        ),
        raw("i$", "0", task(&[], Some("!f"), true, false)),
        raw("(KV letters),", "0", task(&[("rm", None)], Some("0"), false, false)),
        raw("formula$", "0", task(&[], Some("f"), true, false)),
        raw("1/2$", "0", task(&[("1/2", None)], None, false, false)),
        raw("else", "0", task(&[], Some("!f"), true, false)),
        raw(
            "${(...)}$__$(...)$",
            "*",
            task(&[("tex-math", None)], None, false, false),
        ),
        raw("{(...)}", "*", task(&[("text", None)], None, false, false)),
        raw("a-z", "f", task(&[("tex-math", None)], None, false, false)),
        raw("letters", "*", task(&[("rm", None)], None, false, false)),
        raw("-9.,9", "*", task(&[("9,9", None)], None, false, false)),
        raw(
            ",",
            "*",
            task(&[("insert+p1", Some("comma enumeration S"))], None, false, false),
        ),
        raw(
            "\\color{(...)}{(...)}",
            "*",
            task(&[("color-output", None)], None, false, false),
        ),
        raw(
            "\\color{(...)}",
            "*",
            task(&[("color0-output", None)], None, false, false),
        ),
        raw("\\ce{(...)}", "*", task(&[("ce", None)], None, false, false)),
        raw(
            "\\pu{(...)}",
            "*",
            task(&[("write", Some("{")), ("pu", None), ("write", Some("}"))], None, false, false),
        ),
        raw("\\,|\\x{}{}|\\x{}|\\x", "*", task(&[("copy", None)], None, false, false)),
        raw("else2", "*", task(&[("copy", None)], None, false, false)),
    ])
});

static BD: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "*", task(&[], None, false, false)),
        raw("x$", "0", task(&[], Some("!f"), true, false)),
        raw("formula$", "0", task(&[], Some("f"), true, false)),
        raw("else", "0", task(&[], Some("!f"), true, false)),
        raw("-9.,9 no missing 0", "*", task(&[("9,9", None)], None, false, false)),
        raw(
            ".",
            "*",
            task(&[("insert", Some("electron dot"))], None, false, false),
        ),
        raw("a-z", "f", task(&[("tex-math", None)], None, false, false)),
        raw("x", "*", task(&[("insert", Some("KV x"))], None, false, false)),
        raw("letters", "*", task(&[("rm", None)], None, false, false)),
        raw("'", "*", task(&[("insert", Some("prime"))], None, false, false)),
        raw(
            "${(...)}$__$(...)$",
            "*",
            task(&[("tex-math", None)], None, false, false),
        ),
        raw("{(...)}", "*", task(&[("text", None)], None, false, false)),
        raw(
            "\\color{(...)}{(...)}",
            "*",
            task(&[("color-output", None)], None, false, false),
        ),
        raw(
            "\\color{(...)}",
            "*",
            task(&[("color0-output", None)], None, false, false),
        ),
        raw("\\ce{(...)}", "*", task(&[("ce", None)], None, false, false)),
        raw(
            "\\pu{(...)}",
            "*",
            task(&[("write", Some("{")), ("pu", None), ("write", Some("}"))], None, false, false),
        ),
        raw("\\,|\\x{}{}|\\x{}|\\x", "*", task(&[("copy", None)], None, false, false)),
        raw("else2", "*", task(&[("copy", None)], None, false, false)),
    ])
});

static OXIDATION: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "*", task(&[("roman-numeral", None)], None, false, false)),
        raw(
            "pm-operator",
            "*",
            task(&[("o=+p1", Some("\\pm"))], None, false, false),
        ),
        raw("else", "*", task(&[("o=", None)], None, false, false)),
    ])
});

static TEX_MATH: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "*", task(&[("output", None)], None, false, false)),
        raw(
            "\\ce{(...)}",
            "*",
            task(&[("output", None), ("ce", None)], None, false, false),
        ),
        raw(
            "\\pu{(...)}",
            "*",
            task(&[("output", None), ("write", Some("{")), ("pu", None), ("write", Some("}"))], None, false, false),
        ),
        raw("{...}|\\,|\\x{}{}|\\x{}|\\x", "*", task(&[("o=", None)], None, false, false)),
        raw("else", "*", task(&[("o=", None)], None, false, false)),
    ])
});

static TEX_MATH_TIGHT: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "*", task(&[("output", None)], None, false, false)),
        raw(
            "\\ce{(...)}",
            "*",
            task(&[("output", None), ("ce", None)], None, false, false),
        ),
        raw(
            "\\pu{(...)}",
            "*",
            task(&[("output", None), ("write", Some("{")), ("pu", None), ("write", Some("}"))], None, false, false),
        ),
        raw("{...}|\\,|\\x{}{}|\\x{}|\\x", "*", task(&[("o=", None)], None, false, false)),
        raw("-|+", "*", task(&[("tight operator", None)], None, false, false)),
        raw("else", "*", task(&[("o=", None)], None, false, false)),
    ])
});

static NUM99: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "*", task(&[], None, false, false)),
        raw(",", "*", task(&[("comma", None)], None, false, false)),
        raw("else", "*", task(&[("copy", None)], None, false, false)),
    ])
});

// =========================================================================
// pu / pu-2 / pu-9,9 状态机
// =========================================================================

static PU: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "*", task(&[("output", None)], None, false, false)),
        raw(
            "space$",
            "*",
            task(&[("output", None), ("space", None)], None, false, false),
        ),
        raw("{[(|)]}", "0|a", task(&[("copy", None)], None, false, false)),
        raw(
            "(-)(9)^(-9)",
            "0",
            task(&[("number^", None)], Some("a"), false, false),
        ),
        raw(
            "(-)(9.,9)(e)(99)",
            "0",
            task(&[("enumber", None)], Some("a"), false, false),
        ),
        raw("space", "0|a", task(&[], None, false, false)),
        raw(
            "pm-operator",
            "0|a",
            task(&[("operator", Some("\\pm"))], Some("0"), false, false),
        ),
        raw("operator", "0|a", task(&[("copy", None)], Some("0"), false, false)),
        raw("//", "d", task(&[("o=", None)], Some("/"), false, false)),
        raw("/", "d", task(&[("o=", None)], Some("/"), false, false)),
        raw(
            "{...}|else",
            "0|d",
            task(&[("d=", None)], Some("d"), false, false),
        ),
        raw(
            "{...}|else",
            "a",
            task(&[("space", None), ("d=", None)], Some("d"), false, false),
        ),
        raw(
            "{...}|else",
            "/|q",
            task(&[("q=", None)], Some("q"), false, false),
        ),
    ])
});

static PU2: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "*", task(&[("output", None)], None, false, false)),
        raw(
            "*",
            "*",
            task(&[("output", None), ("cdot", None)], Some("0"), false, false),
        ),
        raw("\\x", "*", task(&[("rm=", None)], None, false, false)),
        raw(
            "space",
            "*",
            task(&[("output", None), ("space", None)], Some("0"), false, false),
        ),
        raw(
            "^{(...)}|^(-1)",
            "1",
            task(&[("^(-1)", None)], None, false, false),
        ),
        raw("-9.,9", "0", task(&[("rm=", None)], Some("0"), false, false)),
        raw(
            "-9.,9",
            "1",
            task(&[("^(-1)", None)], Some("0"), false, false),
        ),
        raw("{...}|else", "*", task(&[("rm=", None)], Some("1"), false, false)),
    ])
});

static PU99: LazyLock<HashMap<String, Vec<Transition>>> = LazyLock::new(|| {
    build_transitions(&[
        raw("empty", "0", task(&[("output-0", None)], None, false, false)),
        raw("empty", "o", task(&[("output-o", None)], None, false, false)),
        raw(
            ",",
            "0",
            task(&[("output-0", None), ("comma", None)], Some("o"), false, false),
        ),
        raw(
            ".",
            "0",
            task(&[("output-0", None), ("copy", None)], Some("o"), false, false),
        ),
        raw("else", "*", task(&[("text=", None)], None, false, false)),
    ])
});

// =========================================================================
// 机器局部动作分发
// =========================================================================

fn machine_action(
    machine: &str,
    buf: &mut Buffer,
    m: &MVal,
    opt: &Option<String>,
    type_: &str,
) -> Option<Out> {
    match machine {
        "ce" => ce_action(buf, m, opt, type_),
        "text" => text_action(buf, type_),
        "pq" => pq_action(buf, m, type_),
        "bd" => bd_action(buf, m, type_),
        "oxidation" => oxidation_action(buf, type_),
        "tex-math" | "tex-math tight" => texmath_action(machine, buf, m, type_),
        "9,9" => num99_action(type_),
        "pu" => pu_action(buf, m, opt, type_),
        "pu-2" => pu2_action(buf, m, type_),
        "pu-9,9" => pu99_action(buf, type_),
        _ => None,
    }
}

fn gof(opt_str: &Option<String>, m: &str) -> Field {
    Field::Nodes(go(opt_str.as_deref().unwrap_or(""), m))
}

fn arrow_sub(buf_field: &Option<String>, rdt: &Option<String>, ce_m: &str) -> Field {
    match rdt.as_deref() {
        Some("M") => Field::Nodes(go(buf_field.as_deref().unwrap_or(""), "tex-math")),
        Some("T") => Field::Nodes(vec![Parsed::N(NodeData {
            type_: "text".into(),
            p1: Some(Field::Str(buf_field.clone().unwrap_or_default())),
            ..Default::default()
        })]),
        _ => Field::Nodes(go(buf_field.as_deref().unwrap_or(""), ce_m)),
    }
}

fn ce_action(buf: &mut Buffer, m: &MVal, opt: &Option<String>, type_: &str) -> Option<Out> {
    let out = match type_ {
        "o after d" => {
            let mut ret: Vec<Parsed> = Vec::new();
            let d_is_int = buf
                .d
                .as_ref()
                .map(|d| re!("^[1-9][0-9]*$").is_match(d))
                .unwrap_or(false);
            if d_is_int {
                let tmp = buf.d.take();
                concat(&mut ret, ce_action(buf, m, &None, "output").unwrap_or(Out::None));
                ret.push(Parsed::N(NodeData {
                    type_: "tinySkip".into(),
                    ..Default::default()
                }));
                buf.b = tmp;
            } else {
                concat(&mut ret, ce_action(buf, m, &None, "output").unwrap_or(Out::None));
            }
            append_field(&mut buf.o, m);
            Out::Many(ret)
        }
        "d= kv" => {
            buf.d = Some(mval_str(m));
            buf.d_type = Some("kv".into());
            Out::None
        }
        "charge or bond" => {
            if buf.begins_with_bond {
                let mut ret: Vec<Parsed> = Vec::new();
                concat(&mut ret, ce_action(buf, m, &None, "output").unwrap_or(Out::None));
                ret.push(Parsed::N(NodeData {
                    type_: "bond".into(),
                    kind_: Some("-".into()),
                    ..Default::default()
                }));
                Out::Many(ret)
            } else {
                buf.d = Some(mval_str(m));
                Out::None
            }
        }
        "- after o/d" => {
            let is_after_d = opt.as_deref() == Some("true");
            let o_str = buf.o.clone().unwrap_or_default();
            let c1 = match_pattern("orbital", &o_str);
            let c2 = match_pattern("one lowercase greek letter $", &o_str);
            let c3 = match_pattern("one lowercase latin letter $", &o_str);
            let c4 = match_pattern("$one lowercase latin letter$ $", &o_str);
            let m_str = mval_str(m);
            let hyphen_follows = m_str == "-"
                && ((c1.as_ref().map(|x| x.remainder.is_empty()).unwrap_or(false))
                    || c2.is_some()
                    || c3.is_some()
                    || c4.is_some());
            if hyphen_follows
                && buf.a.is_none()
                && buf.b.is_none()
                && buf.p.is_none()
                && buf.d.is_none()
                && buf.q.is_none()
                && c1.is_none()
                && c3.is_some()
            {
                let ov = buf.o.take().unwrap_or_default();
                buf.o = Some(format!("${}$", ov));
            }
            let mut ret: Vec<Parsed> = Vec::new();
            if hyphen_follows {
                concat(&mut ret, ce_action(buf, m, &None, "output").unwrap_or(Out::None));
                ret.push(Parsed::N(NodeData {
                    type_: "hyphen".into(),
                    ..Default::default()
                }));
            } else {
                let d_str = buf.d.clone().unwrap_or_default();
                let c1d = match_pattern("digits", &d_str);
                if is_after_d && c1d.as_ref().map(|x| x.remainder.is_empty()).unwrap_or(false) {
                    append_field(&mut buf.d, m);
                    concat(&mut ret, ce_action(buf, m, &None, "output").unwrap_or(Out::None));
                } else {
                    concat(&mut ret, ce_action(buf, m, &None, "output").unwrap_or(Out::None));
                    ret.push(Parsed::N(NodeData {
                        type_: "bond".into(),
                        kind_: Some("-".into()),
                        ..Default::default()
                    }));
                }
            }
            Out::Many(ret)
        }
        "a to o" => {
            buf.o = buf.a.take();
            Out::None
        }
        "sb=true" => {
            buf.sb = true;
            Out::None
        }
        "sb=false" => {
            buf.sb = false;
            Out::None
        }
        "beginsWithBond=true" => {
            buf.begins_with_bond = true;
            Out::None
        }
        "beginsWithBond=false" => {
            buf.begins_with_bond = false;
            Out::None
        }
        "parenthesisLevel++" => {
            buf.parenthesis_level += 1;
            Out::None
        }
        "parenthesisLevel--" => {
            buf.parenthesis_level -= 1;
            Out::None
        }
        "state of aggregation" => Out::One(Parsed::N(NodeData {
            type_: "state of aggregation".into(),
            p1: Some(Field::Nodes(go(&mval_str(m), "o"))),
            ..Default::default()
        })),
        "comma" => {
            let raw = mval_str(m);
            let a = raw.trim_end();
            let with_space = a != raw;
            let kind = if with_space && buf.parenthesis_level == 0 {
                "comma enumeration L"
            } else {
                "comma enumeration M"
            };
            Out::One(Parsed::N(NodeData {
                type_: kind.into(),
                p1: Some(Field::Str(a.to_string())),
                ..Default::default()
            }))
        }
        "output" => {
            let entity_follows = match opt.as_deref() {
                Some("1") => 1,
                Some("2") => 2,
                _ => 0,
            };
            let ret: Vec<Parsed> = if buf.r.is_none() {
                let mut ret = Vec::new();
                let empty = buf.a.is_none()
                    && buf.b.is_none()
                    && buf.p.is_none()
                    && buf.o.is_none()
                    && buf.q.is_none()
                    && buf.d.is_none()
                    && entity_follows == 0;
                if !empty {
                    if buf.sb {
                        ret.push(Parsed::N(NodeData {
                            type_: "entitySkip".into(),
                            ..Default::default()
                        }));
                    }
                    let o_none = buf.o.is_none();
                    let q_none = buf.q.is_none();
                    let d_none = buf.d.is_none();
                    let b_none = buf.b.is_none();
                    let p_none = buf.p.is_none();
                    if o_none && q_none && d_none && b_none && p_none && entity_follows != 2 {
                        buf.o = buf.a.take();
                    } else if o_none && q_none && d_none && (buf.b.is_some() || buf.p.is_some()) {
                        buf.o = buf.a.take();
                        buf.d = buf.b.take();
                        buf.q = buf.p.take();
                    } else if buf.o.is_some()
                        && buf.d_type.as_deref() == Some("kv")
                        && match_pattern("d-oxidation$", &buf.d.clone().unwrap_or_default()).is_some()
                    {
                        buf.d_type = Some("oxidation".into());
                    } else if buf.o.is_some() && buf.d_type.as_deref() == Some("kv") && buf.q.is_none() {
                        buf.d_type = None;
                    }
                    let d_machine = if buf.d_type.as_deref() == Some("oxidation") {
                        "oxidation"
                    } else {
                        "bd"
                    };
                    ret.push(Parsed::N(NodeData {
                        type_: "chemfive".into(),
                        a: Some(gof(&buf.a.clone(), "a")),
                        b: Some(gof(&buf.b.clone(), "bd")),
                        p: Some(gof(&buf.p.clone(), "pq")),
                        o: Some(gof(&buf.o.clone(), "o")),
                        q: Some(gof(&buf.q.clone(), "pq")),
                        d: Some(gof(&buf.d.clone(), d_machine)),
                        d_type: buf.d_type.clone(),
                        ..Default::default()
                    }));
                }
                ret
            } else {
                let rd = arrow_sub(&buf.rd.clone(), &buf.rdt.clone(), "ce");
                let rq = arrow_sub(&buf.rq.clone(), &buf.rqt.clone(), "ce");
                vec![Parsed::N(NodeData {
                    type_: "arrow".into(),
                    r: buf.r.clone(),
                    rd: Some(rd),
                    rq: Some(rq),
                    ..Default::default()
                })]
            };
            buf.clear(true);
            Out::Many(ret)
        }
        "oxidation-output" => {
            let mut ret: Vec<Parsed> = vec![Parsed::S("{".into())];
            ret.extend(go(&mval_str(m), "oxidation"));
            ret.push(Parsed::S("}".into()));
            Out::Many(ret)
        }
        "frac-output" | "overset-output" | "underset-output" | "underbrace-output" => {
            let (node_type, p1m, p2m) = match type_ {
                "frac-output" => ("frac-ce", 0usize, 1usize),
                "overset-output" => ("overset", 0, 1),
                "underset-output" => ("underset", 0, 1),
                _ => ("underbrace", 0, 1),
            };
            let (g1, g2) = mval_pair(m, p1m, p2m);
            Out::One(Parsed::N(NodeData {
                type_: node_type.into(),
                p1: Some(Field::Nodes(go(&g1, "ce"))),
                p2: Some(Field::Nodes(go(&g2, "ce"))),
                ..Default::default()
            }))
        }
        "color-output" => {
            let (g1, g2) = mval_pair(m, 0, 1);
            Out::One(Parsed::N(NodeData {
                type_: "color".into(),
                color1: Some(g1),
                color2: Some(Field::Nodes(go(&g2, "ce"))),
                ..Default::default()
            }))
        }
        "r=" => {
            buf.r = Some(mval_str(m));
            Out::None
        }
        "rdt=" => {
            buf.rdt = Some(mval_str(m));
            Out::None
        }
        "rd=" => {
            buf.rd = Some(mval_str(m));
            Out::None
        }
        "rqt=" => {
            buf.rqt = Some(mval_str(m));
            Out::None
        }
        "rq=" => {
            buf.rq = Some(mval_str(m));
            Out::None
        }
        "operator" => Out::One(Parsed::N(NodeData {
            type_: "operator".into(),
            kind_: Some(opt.clone().unwrap_or_else(|| mval_str(m))),
            ..Default::default()
        })),
        _ => return None,
    };
    Some(out)
}

fn mval_pair(m: &MVal, i: usize, j: usize) -> (String, String) {
    match m {
        MVal::V(v) => (
            v.get(i).cloned().unwrap_or_default(),
            v.get(j).cloned().unwrap_or_default(),
        ),
        MVal::S(s) => (s.clone(), String::new()),
    }
}

fn text_action(buf: &mut Buffer, type_: &str) -> Option<Out> {
    if type_ == "output" {
        let ret = buf.text_.take().map(|t| {
            Out::One(Parsed::N(NodeData {
                type_: "text".into(),
                p1: Some(Field::Str(t)),
                ..Default::default()
            }))
        });
        buf.clear(false);
        Some(ret.unwrap_or(Out::None))
    } else {
        None
    }
}

fn pq_action(_buf: &mut Buffer, m: &MVal, type_: &str) -> Option<Out> {
    match type_ {
        "state of aggregation" => Some(Out::One(Parsed::N(NodeData {
            type_: "state of aggregation subscript".into(),
            p1: Some(Field::Nodes(go(&mval_str(m), "o"))),
            ..Default::default()
        }))),
        "color-output" => {
            let (g1, g2) = mval_pair(m, 0, 1);
            Some(Out::One(Parsed::N(NodeData {
                type_: "color".into(),
                color1: Some(g1),
                color2: Some(Field::Nodes(go(&g2, "pq"))),
                ..Default::default()
            })))
        }
        _ => None,
    }
}

fn bd_action(_buf: &mut Buffer, m: &MVal, type_: &str) -> Option<Out> {
    if type_ == "color-output" {
        let (g1, g2) = mval_pair(m, 0, 1);
        Some(Out::One(Parsed::N(NodeData {
            type_: "color".into(),
            color1: Some(g1),
            color2: Some(Field::Nodes(go(&g2, "bd"))),
            ..Default::default()
        })))
    } else {
        None
    }
}

fn oxidation_action(buf: &mut Buffer, type_: &str) -> Option<Out> {
    if type_ == "roman-numeral" {
        Some(Out::One(Parsed::N(NodeData {
            type_: "roman numeral".into(),
            p1: Some(Field::Str(buf.o.clone().unwrap_or_default())),
            ..Default::default()
        })))
    } else {
        None
    }
}

fn texmath_action(machine: &str, buf: &mut Buffer, m: &MVal, type_: &str) -> Option<Out> {
    match type_ {
        "tight operator" => {
            append_str(&mut buf.o, &format!("{{{}}}", mval_str(m)));
            Some(Out::None)
        }
        "output" => {
            let ret = buf.o.take().map(|o| {
                Out::One(Parsed::N(NodeData {
                    type_: "tex-math".into(),
                    p1: Some(Field::Str(o)),
                    ..Default::default()
                }))
            });
            buf.clear(false);
            let _ = machine;
            Some(ret.unwrap_or(Out::None))
        }
        _ => None,
    }
}

/// 只接管 `comma`；`copy` 等交给通用动作，否则数字会被吞掉（H2O 丢下标）。
fn num99_action(type_: &str) -> Option<Out> {
    (type_ == "comma").then(|| {
        Out::One(Parsed::N(NodeData {
            type_: "commaDecimal".into(),
            ..Default::default()
        }))
    })
}

fn pu_action(buf: &mut Buffer, m: &MVal, opt: &Option<String>, type_: &str) -> Option<Out> {
    let out = match type_ {
        "enumber" => {
            let v = match m {
                MVal::V(v) => v.clone(),
                MVal::S(s) => vec![s.clone()],
            };
            let mut ret: Vec<Parsed> = Vec::new();
            let g0 = v.first().map(|s| s.as_str()).unwrap_or("");
            if g0 == "+-" || g0 == "+/-" {
                ret.push(Parsed::S("\\pm ".into()));
            } else if !g0.is_empty() {
                ret.push(Parsed::S(g0.to_string()));
            }
            let g1 = v.get(1).cloned().unwrap_or_default();
            if !g1.is_empty() {
                ret.extend(go(&g1, "pu-9,9"));
                let g2 = v.get(2).cloned().unwrap_or_default();
                if !g2.is_empty() {
                    if g2.contains(',') || g2.contains('.') {
                        ret.extend(go(&g2, "pu-9,9"));
                    } else {
                        ret.push(Parsed::S(g2));
                    }
                }
                let g3 = v.get(3).map(|s| s.as_str()).unwrap_or("");
                let g4 = v.get(4).map(|s| s.as_str()).unwrap_or("");
                if !g3.is_empty() || !g4.is_empty() {
                    if g3 == "e" || g4 == "*" {
                        ret.push(Parsed::N(NodeData {
                            type_: "cdot".into(),
                            ..Default::default()
                        }));
                    } else {
                        ret.push(Parsed::N(NodeData {
                            type_: "times".into(),
                            ..Default::default()
                        }));
                    }
                }
            }
            let g5 = v.get(5).cloned().unwrap_or_default();
            if !g5.is_empty() {
                ret.push(Parsed::S(format!("10^{{{}}}", g5)));
            }
            Out::Many(ret)
        }
        "number^" => {
            let v = match m {
                MVal::V(v) => v.clone(),
                MVal::S(s) => vec![s.clone()],
            };
            let mut ret: Vec<Parsed> = Vec::new();
            let g0 = v.first().map(|s| s.as_str()).unwrap_or("");
            if g0 == "+-" || g0 == "+/-" {
                ret.push(Parsed::S("\\pm ".into()));
            } else if !g0.is_empty() {
                ret.push(Parsed::S(g0.to_string()));
            }
            let g1 = v.get(1).cloned().unwrap_or_default();
            ret.extend(go(&g1, "pu-9,9"));
            let g2 = v.get(2).cloned().unwrap_or_default();
            ret.push(Parsed::S(format!("^{{{}}}", g2)));
            Out::Many(ret)
        }
        "operator" => Out::One(Parsed::N(NodeData {
            type_: "operator".into(),
            kind_: Some(opt.clone().unwrap_or_else(|| mval_str(m))),
            ..Default::default()
        })),
        "space" => Out::One(Parsed::N(NodeData {
            type_: "pu-space-1".into(),
            ..Default::default()
        })),
        "output" => {
            // {(...)} 解包
            if let Some(d) = &buf.d {
                if let Some(md) = match_pattern("{(...)}", d) {
                    if md.remainder.is_empty() {
                        buf.d = Some(mval_str(&md.m));
                    }
                }
            }
            if let Some(q) = &buf.q {
                if let Some(mq) = match_pattern("{(...)}", q) {
                    if mq.remainder.is_empty() {
                        buf.q = Some(mval_str(&mq.m));
                    }
                }
            }
            fn rep_c_f(s: &str) -> String {
                s.replace("\u{00B0}C", "{}^{\\circ}C")
                    .replace("^oC", "{}^{\\circ}C")
                    .replace("^{o}C", "{}^{\\circ}C")
                    .replace("\u{00B0}F", "{}^{\\circ}F")
                    .replace("^oF", "{}^{\\circ}F")
                    .replace("^{o}F", "{}^{\\circ}F")
            }
            if let Some(d) = buf.d.as_mut() {
                *d = rep_c_f(d);
            }
            let ret: Vec<Parsed> = if let Some(q) = buf.q.as_mut() {
                *q = rep_c_f(q);
                let bd = go(&buf.d.clone().unwrap_or_default(), "pu");
                let bq = go(&buf.q.clone().unwrap_or_default(), "pu");
                if buf.o.as_deref() == Some("//") {
                    vec![Parsed::N(NodeData {
                        type_: "pu-frac".into(),
                        p1: Some(Field::Nodes(bd)),
                        p2: Some(Field::Nodes(bq)),
                        ..Default::default()
                    })]
                } else {
                    let mut ret = bd;
                    if ret.len() > 1 || bq.len() > 1 {
                        ret.push(Parsed::N(NodeData {
                            type_: " / ".into(),
                            ..Default::default()
                        }));
                    } else {
                        ret.push(Parsed::N(NodeData {
                            type_: "/".into(),
                            ..Default::default()
                        }));
                    }
                    ret.extend(bq);
                    ret
                }
            } else {
                go(&buf.d.clone().unwrap_or_default(), "pu-2")
            };
            buf.clear(false);
            Out::Many(ret)
        }
        _ => return None,
    };
    Some(out)
}

fn pu2_action(buf: &mut Buffer, m: &MVal, type_: &str) -> Option<Out> {
    let out = match type_ {
        "cdot" => Out::One(Parsed::N(NodeData {
            type_: "tight cdot".into(),
            ..Default::default()
        })),
        "^(-1)" => {
            if let Some(rm) = buf.rm.as_mut() {
                rm.push_str(&format!("^{{{}}}", mval_str(m)));
            }
            Out::None
        }
        "space" => Out::One(Parsed::N(NodeData {
            type_: "pu-space-2".into(),
            ..Default::default()
        })),
        "output" => {
            let ret: Vec<Parsed> = if let Some(rm) = &buf.rm {
                if let Some(mrm) = match_pattern("{(...)}", rm) {
                    if mrm.remainder.is_empty() {
                        go(&mval_str(&mrm.m), "pu")
                    } else {
                        vec![Parsed::N(NodeData {
                            type_: "rm".into(),
                            p1: Some(Field::Str(rm.clone())),
                            ..Default::default()
                        })]
                    }
                } else {
                    vec![Parsed::N(NodeData {
                        type_: "rm".into(),
                        p1: Some(Field::Str(rm.clone())),
                        ..Default::default()
                    })]
                }
            } else {
                Vec::new()
            };
            buf.clear(false);
            Out::Many(ret)
        }
        _ => return None,
    };
    Some(out)
}

fn pu99_action(buf: &mut Buffer, type_: &str) -> Option<Out> {
    let out = match type_ {
        "comma" => Out::One(Parsed::N(NodeData {
            type_: "commaDecimal".into(),
            ..Default::default()
        })),
        "output-0" => {
            let text = buf.text_.clone().unwrap_or_default();
            let ret = thousands_split(&text, true);
            buf.clear(false);
            Out::Many(ret)
        }
        "output-o" => {
            let text = buf.text_.clone().unwrap_or_default();
            let ret = thousands_split(&text, false);
            buf.clear(false);
            Out::Many(ret)
        }
        _ => return None,
    };
    Some(out)
}

/// 千分位拆分（pu-9,9 的 output-0 / output-o）。
fn thousands_split(text: &str, reverse: bool) -> Vec<Parsed> {
    let mut ret: Vec<Parsed> = Vec::new();
    if text.len() > 4 {
        if reverse {
            let mut a = text.len() % 3;
            if a == 0 {
                a = 3;
            }
            let mut i = text.len().saturating_sub(3);
            while i > 0 {
                ret.push(Parsed::S(text[i..i + 3].to_string()));
                ret.push(Parsed::N(NodeData {
                    type_: "1000 separator".into(),
                    ..Default::default()
                }));
                i = i.saturating_sub(3);
            }
            ret.push(Parsed::S(text[..a].to_string()));
            ret.reverse();
        } else {
            let a = text.len() - 3;
            let mut i = 0;
            while i < a {
                ret.push(Parsed::S(text[i..i + 3].to_string()));
                ret.push(Parsed::N(NodeData {
                    type_: "1000 separator".into(),
                    ..Default::default()
                }));
                i += 3;
            }
            ret.push(Parsed::S(text[i..].to_string()));
        }
    } else {
        ret.push(Parsed::S(text.to_string()));
    }
    ret
}
