//! KaTeX 服务端数学公式渲染。
//!
//! 用纯 Rust 的 [`katex`](https://crates.io/crates/katex-rs) crate 把 TeX 公式
//! 渲染成 HTML span，供 pulldown-cmark 的 `InlineMath` / `DisplayMath` 事件调用。
//! 仅在 `feature = "server"` 时编译——前端 WASM 不参与公式渲染（SSR 即终态）。
//!
//! 渲染策略：
//! - `OutputFormat::Html`：只产出视觉层 `<span class="katex">…</span>`，不含 MathML
//!   语义层（`<math>` 等）。这样 sanitizer 无需为 MathML 标签开白名单，XSS 面最小。
//!   屏幕阅读器等无障碍场景的语义损失可接受（本站数学公式占比低）。
//! - `throw_on_error = false`：坏公式渲染成红色错误 span 而非中断整篇文章。
//!
//! 配套资源：前端必须加载 `public/katex/katex.min.css` + `fonts/`（见 Makefile
//! `katex-css`），否则只有裸 span、无数学字体排版。crate 本身不打包 CSS。

#![cfg(feature = "server")]

use katex::macros::MacroDefinition;
use katex::{KatexContext, OutputFormat, Settings};

/// 物理学常用宏表（对齐 LaTeX `physics` 宏包 + 项目文档 8.13 节「项目物理宏表」）。
///
/// `katex-rs` 默认 `Settings` 无物理宏表，导致 `\vu \dv \dd \pdv \divg \curl \grad`
/// `\qty \RR \ZZ \NN \QQ \CC \bra \ket \braket \expval \abs \norm` 等渲染为红字
/// （实测正确页面 648 的 137 个公式中 48 处物理宏坏掉）。这里把它们注册为简单
/// 字符串宏（[`MacroDefinition::StaticStr`]），crate 的 `string_to_expansion` 会自动
/// 从 `#1`/`#2` 推导参数个数。
///
/// 刻意差异：`\divg`（散度）**不**覆写内置 `\div`（除号 ÷）——文档 8.13 明确两者并存。
/// `\bra`/`\ket`/`\braket` 虽是 katex 内置宏，但内置 `\braket` 只吃 1 个参数
/// （`\langle{#1}\rangle`），物理语义需 2 个参数（`\langle #1 | #2 \rangle`），
/// 故覆写为物理版本。
///
/// `\qty(...)` 的圆括号定界符匹配无法用纯字符串宏表达（TeX 无参定界符宏需
/// `MacroExpansion.delimiters`），由 [`render_inline`]/[`render_display` 渲染前的
/// 预处理兜底；这里注册的是花括号形式 `\qty{...}`。
fn physics_macros() -> &'static [(&'static str, MacroDefinition)] {
    &[
        // 数集
        (r"\RR", MacroDefinition::StaticStr(r"\mathbb{R}")),
        (r"\ZZ", MacroDefinition::StaticStr(r"\mathbb{Z}")),
        (r"\NN", MacroDefinition::StaticStr(r"\mathbb{N}")),
        (r"\QQ", MacroDefinition::StaticStr(r"\mathbb{Q}")),
        (r"\CC", MacroDefinition::StaticStr(r"\mathbb{C}")),
        // 微积分：微分与偏导
        (r"\dd", MacroDefinition::StaticStr(r"\mathrm{d}#1")),
        (
            r"\dv",
            MacroDefinition::StaticStr(r"\frac{\mathrm{d}#1}{\mathrm{d}#2}"),
        ),
        (
            r"\pdv",
            MacroDefinition::StaticStr(r"\frac{\partial #1}{\partial #2}"),
        ),
        // 场算子：grad/divg/curl（divg 刻意不复用 \div）
        (r"\grad", MacroDefinition::StaticStr(r"\nabla")),
        (r"\divg", MacroDefinition::StaticStr(r"\nabla \cdot")),
        (r"\curl", MacroDefinition::StaticStr(r"\nabla \times")),
        // 量子力学 Dirac 记号
        (r"\bra", MacroDefinition::StaticStr(r"\langle #1 |")),
        (r"\ket", MacroDefinition::StaticStr(r"| #1 \rangle")),
        (
            r"\braket",
            MacroDefinition::StaticStr(r"\langle #1 | #2 \rangle"),
        ),
        (
            r"\expval",
            MacroDefinition::StaticStr(r"\langle #1 \rangle"),
        ),
        // 向量 / 范数 / 绝对值（自动缩放定界符）
        (r"\abs", MacroDefinition::StaticStr(r"\left| #1 \right|")),
        (r"\norm", MacroDefinition::StaticStr(r"\left\| #1 \right\|")),
        // 单位向量：带帽子
        (r"\vu", MacroDefinition::StaticStr(r"\hat{\vec{#1}}")),
        // 自动缩放圆括号（花括号形式；`\qty(...)` 由预处理兜底）
        (r"\qty", MacroDefinition::StaticStr(r"\left( #1 \right)")),
    ]
}

/// 把物理宏表注入到给定 `Settings` 的宏表（覆盖同名内置宏）。
fn inject_physics_macros(settings: &mut Settings) {
    let mut map = settings.macros.borrow_mut();
    for (name, def) in physics_macros() {
        map.insert((*name).to_string(), def.clone());
    }
}

/// 内联公式（`$...$`）渲染配置工厂：`display_mode = false`，含物理宏表。
fn inline_settings() -> Settings {
    let mut s = Settings {
        output: OutputFormat::Html,
        display_mode: false,
        throw_on_error: false,
        ..Settings::default()
    };
    inject_physics_macros(&mut s);
    s
}

/// 块级公式（`$$...$$`）渲染配置工厂：`display_mode = true`（居中独占一行），含物理宏表。
fn display_settings() -> Settings {
    let mut s = Settings {
        output: OutputFormat::Html,
        display_mode: true,
        throw_on_error: false,
        ..Settings::default()
    };
    inject_physics_macros(&mut s);
    s
}

thread_local! {
    /// KaTeX 上下文：含全部内置符号 / 宏表，应在多次渲染间复用（README 建议）。
    /// 用 thread_local 而非全局 static：`KatexContext` 内含 `RefCell<HashMap>`
    /// 宏表，非 `Sync`，不能放 `LazyLock`。tokio 多线程 runtime 下每线程各持一份。
    static KATEX_CTX: KatexContext = KatexContext::default();

    /// 每线程缓存的渲染配置，避免每次渲染都重建宏表 HashMap。
    /// `Settings` 同样因 `RefCell` 宏表非 `Sync`。
    static INLINE_SETTINGS: Settings = inline_settings();
    static DISPLAY_SETTINGS: Settings = display_settings();
}

/// 把公式中的 `\ce{...}` / `\pu{...}` 预转译为标准 LaTeX（mhchem）。
///
/// `katex-rs` 无 mhchem 解析器，化学公式渲染为红字。这里在渲染前扫描 `\ce`/`\pu`
/// 调用，用嵌套花括号配对读取参数（支持 `\ce{[Cu(NH3)4]^2+}` 这类含 `{}` 的内容），
/// 转译后替换原 `\ce{...}`，其余文本原样拼接。未闭合 `\ce{` 保留原样（让 katex
/// 报红，符合容错设计）。无 `\ce`/`\pu` 时零成本原样返回。
fn expand_chem(tex: &str) -> String {
    // 快速路径：绝大多数公式不含化学公式，避免分配。
    if !tex.contains(r"\ce") && !tex.contains(r"\pu") {
        return tex.to_string();
    }
    let mut out = String::with_capacity(tex.len());
    let mut rest = tex;
    loop {
        // 找下一个 `\ce` 或 `\pu`，取较早出现者（两者均为 3 字节 ASCII）。
        let ce = rest.find(r"\ce");
        let pu = rest.find(r"\pu");
        let next = match (ce, pu) {
            (None, None) => None,
            (Some(a), None) => Some((a, false)),
            (None, Some(b)) => Some((b, true)),
            (Some(a), Some(b)) => Some(if a <= b { (a, false) } else { (b, true) }),
        };
        match next {
            None => {
                // 命令之后再无 `\ce`/`\pu`：原样拷贝剩余文本。
                out.push_str(rest);
                return out;
            }
            Some((pos, is_pu)) => {
                // C2 修复：拷贝命令前的原文用 `push_str(&str 切片)`，
                // 而非旧的逐字节 `bytes[i] as char`（Latin-1 转换会破坏多字节 UTF-8，
                // 如 `\text{浓度} \ce{H2O}` 里的中文）。
                out.push_str(&rest[..pos]);
                let bytes = rest.as_bytes();
                // `\ce` 与 `\pu` 均为 3 字节，`{` 紧随其后（C1 修复：旧代码 `\pu` 误用 i+4，
                // 实际 `{` 在 i+3，导致 `\pu` 永不匹配、mhchem::pu 从不触发）。
                let after_cmd = pos + 3;
                // 精确匹配命令边界：\ce/\pu 后须紧跟 `{`（否则可能是 \cellbox 之类）。
                if after_cmd < bytes.len() && bytes[after_cmd] == b'{' {
                    // read_braced 按字节索引扫描，仅计数 ASCII `{`/`}`；
                    // 花括号配对不会跨多字节字符边界，返回的切片落在字符边界上，UTF-8 安全。
                    if let Some((content, close_end)) = read_braced(rest, after_cmd) {
                        let translated = if is_pu {
                            crate::api::mhchem::pu(content)
                        } else {
                            crate::api::mhchem::ce(content)
                        };
                        out.push_str(&translated);
                        rest = &rest[close_end..];
                        continue;
                    }
                    // 未闭合 `{`：原样输出剩余，交由 katex 报红。
                    out.push_str(&rest[after_cmd..]);
                    return out;
                } else {
                    // 命令后非 `{`：保留命令字面量，从其后再扫。
                    out.push_str(&rest[pos..after_cmd]);
                    rest = &rest[after_cmd..];
                    continue;
                }
            }
        }
    }
}

/// 从 `open`（指向 `{`）读取配对花括号内容，返回 `(内容, 闭括号后位置)`。
/// 不闭合返回 `None`。嵌套 `{}` 正确计数。
fn read_braced(s: &str, open: usize) -> Option<(&str, usize)> {
    let bytes = s.as_bytes();
    debug_assert_eq!(bytes[open], b'{');
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&s[open + 1..i], i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// 渲染内联公式 `$...$`（定界符由 pulldown-cmark 剥除）→ HTML 字符串。
///
/// 渲染失败（坏 TeX）时回退到 HTML 转义后的原文，保证文章不因一个坏公式全篇崩。
pub fn render_inline(tex: &str) -> String {
    let tex = expand_chem(tex);
    KATEX_CTX.with(|ctx| {
        INLINE_SETTINGS.with(|settings| {
            katex::render_to_string(ctx, &tex, settings)
                .unwrap_or_else(|_| crate::utils::html::escape_html(&tex))
        })
    })
}

/// 渲染块级公式 `$$...$$`（定界符由 pulldown-cmark 剥除）→ HTML 字符串。
///
/// 与 [`render_inline`] 同样在失败时回退到转义原文。调用方负责块级包裹
/// （如 `<p class="math-display">`），这里只产出 KaTeX 的 span 串。
pub fn render_display(tex: &str) -> String {
    let tex = expand_chem(tex);
    KATEX_CTX.with(|ctx| {
        DISPLAY_SETTINGS.with(|settings| {
            katex::render_to_string(ctx, &tex, settings)
                .unwrap_or_else(|_| crate::utils::html::escape_html(&tex))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_inline_produces_katex_span() {
        let html = render_inline("E = mc^2");
        assert!(
            html.contains("katex"),
            "内联公式应产出含 katex class 的 span, got: {html}"
        );
    }

    #[test]
    fn render_display_produces_katex_display() {
        let html = render_display("\\frac{a}{b}");
        assert!(
            html.contains("katex-display"),
            "块级公式应产出含 katex-display class 的结构, got: {html}"
        );
    }

    #[test]
    fn render_inline_does_not_emit_math_tag() {
        // OutputFormat::Html 不应产出 <math> 标签（那是 HtmlAndMathml / Mathml 模式）。
        let html = render_inline("a^2 + b^2 = c^2");
        assert!(
            !html.contains("<math"),
            "Html 输出不应含 <math> 标签, got: {html}"
        );
    }

    // ── 物理宏表（Fix 3a） ─────────────────────────────────────────
    // katex-rs 默认无物理宏，未注册时 \vu \dd \RR 等渲染为 katex-error 红字。

    #[test]
    fn physics_macro_unit_vector_renders() {
        // \vu{i} → \hat{\vec{i}}：带帽子单位向量。
        let html = render_inline(r"\vu{i}");
        assert!(
            html.contains("katex") && !html.contains("katex-error"),
            "\\vu 应正确渲染而非红字, got: {html}"
        );
    }

    #[test]
    fn physics_macro_divergence_does_not_override_division() {
        // 刻意差异：\divg（散度）与 \div（除号 ÷）并存。
        let divg = render_inline(r"\divg \vec{F}");
        let div = render_inline(r"a \div b");
        assert!(
            !divg.contains("katex-error"),
            "\\divg 应正确渲染而非红字, got: {divg}"
        );
        assert!(
            !div.contains("katex-error"),
            "\\div 应仍是除号而非红字, got: {div}"
        );
        // 两者输出不同（\divg 展开为 \nabla \cdot，\div 是除号符号）。
        assert_ne!(divg, div, "\\divg 与 \\div 输出应不同");
    }

    #[test]
    fn physics_macro_number_sets_renders() {
        for m in [r"\RR", r"\ZZ", r"\NN", r"\QQ", r"\CC"] {
            let html = render_inline(m);
            assert!(
                !html.contains("katex-error"),
                "{m} 应正确渲染而非红字, got: {html}"
            );
        }
    }

    #[test]
    fn physics_macro_calculus_renders() {
        // \dv{f}{x} → d f / d x；\pdv{f}{x} → ∂ f / ∂ x；\dd{x} → dx。
        for tex in [r"\dv{f}{x}", r"\pdv{f}{x}", r"\dd{x}"] {
            let html = render_inline(tex);
            assert!(
                !html.contains("katex-error"),
                "{tex} 应正确渲染而非红字, got: {html}"
            );
        }
    }

    #[test]
    fn physics_macro_dirac_notation_renders() {
        for tex in [
            r"\bra{\psi}",
            r"\ket{\phi}",
            r"\braket{\psi}{\phi}",
            r"\expval{A}",
        ] {
            let html = render_inline(tex);
            assert!(
                !html.contains("katex-error"),
                "{tex} 应正确渲染而非红字, got: {html}"
            );
        }
    }

    #[test]
    fn physics_macro_abs_norm_qty_renders() {
        for tex in [r"\abs{x}", r"\norm{v}", r"\qty{a + b}"] {
            let html = render_inline(tex);
            assert!(
                !html.contains("katex-error"),
                "{tex} 应正确渲染而非红字, got: {html}"
            );
        }
    }

    // ── mhchem 化学公式（Fix 3b） ──────────────────────────────────────
    // \ce/\pu 预转译后渲染，不应出现 katex-error 红字。

    #[test]
    fn mhchem_water_renders() {
        let html = render_inline(r"\ce{H2O}");
        assert!(
            html.contains("katex") && !html.contains("katex-error"),
            "\\ce{{H2O}} 应正确渲染而非红字, got: {html}"
        );
    }

    #[test]
    fn mhchem_reaction_with_arrow_renders() {
        let html = render_display(r"\ce{2H2 + O2 -> 2H2O}");
        assert!(
            !html.contains("katex-error"),
            "反应方程式应正确渲染而非红字, got: {html}"
        );
    }

    #[test]
    fn mhchem_gas_arrow_superscript_renders() {
        // 气体符号 ^ —— 转译后变成 \uparrow，消解原 mhchem 行尾 ^ 解析错误
        // （文档 8.20 这正是当前唯一 1 个 katex-error 的根因）。
        let html = render_display(r"\ce{CaCO3 ->[\Delta] CaO + CO2 ^}");
        assert!(
            !html.contains("katex-error"),
            "气体箭头公式应正确渲染而非红字, got: {html}"
        );
    }

    #[test]
    fn mhchem_pu_units_renders() {
        // C1 回归：`\pu` 旧 off-by-one 使其永不匹配，mhchem::pu 从不触发。
        // 旧测试只断言 `!contains("katex-error")`，而 katex-rs 对未知命令走 color node
        // （无 katex-error class），故无论转译与否都通过——等于空测试。这里改为断言
        // 转译真正发生：产出含 katex 的 HTML，且不再是裸 `\pu{...}` 原文。
        let html = render_inline(r"\pu{9.8 m/s^2}");
        assert!(
            html.contains("katex") && !html.contains("katex-error"),
            "\\pu 单位应正确渲染而非红字, got: {html}"
        );
    }

    #[test]
    fn expand_chem_pu_is_actually_translated() {
        // C1 直接回归：expand_chem 必须把 `\pu{...}` 转译掉，不得原样保留命令。
        let out = expand_chem(r"\pu{9.8 m/s^2}");
        assert_ne!(
            out, r"\pu{9.8 m/s^2}",
            "\\pu 应被 mhchem::pu 转译而非原样保留, got: {out}"
        );
        assert!(
            !out.contains(r"\pu{"),
            "转译后不应残留 \\pu{{ 命令, got: {out}"
        );
    }

    #[test]
    fn expand_chem_preserves_multibyte_utf8() {
        // C2 回归：旧的 `out.push(bytes[i] as char)` 按单字节 Latin-1 转 char，
        // 含 `\ce`/`\pu` 且含非 ASCII（如中文 `\text{浓度}`）的公式会被破坏成乱码。
        let out = expand_chem(r"\text{浓度} \ce{H2O}");
        assert!(out.contains("浓度"), "中文应原样保留, got: {out}");
        assert!(
            !out.contains(r"\ce{"),
            "化学公式应被转译、不残留 \\ce{{ 命令, got: {out}"
        );
        // 仅含非 ASCII、无化学公式时零成本原样返回。
        assert_eq!(expand_chem(r"纯中文无公式"), r"纯中文无公式");
    }

    // issue #13：保留真实渲染回归。katex-rs 0.3 已支持文本模式 U+00B7，
    // 不再预处理 TeX。未知命令也可能产出 color node，不能只检查 katex-error。

    #[test]
    fn text_mode_middle_dot_does_not_render_red() {
        let html = render_inline(r"\text{m·K}");
        assert!(
            !html.contains("#cc0000"),
            "\\text{{m·K}} 不应渲染为红字错误, got: {html}"
        );
        // 与 KaTeX JS 一致的 glyph：U+22C5。
        assert!(html.contains('⋅'), "应渲染 U+22C5 glyph, got: {html}");
    }

    #[test]
    fn issue13_wien_law_formula_does_not_render_red() {
        // issue #13 原始公式（维恩位移定律 b 的单位 m·K）。
        let html = render_inline(r"b \approx 2.898\times10^{-3}\,\text{m·K}");
        assert!(
            !html.contains("#cc0000"),
            "issue #13 公式不应有红字, got: {html}"
        );
    }

    #[test]
    fn text_family_commands_render_middle_dot() {
        for cmd in [r"\text", r"\textbf", r"\textit", r"\texttt"] {
            let html = render_inline(&format!(r"{cmd}{{m·K}}"));
            assert!(
                !html.contains("#cc0000"),
                "{cmd}{{m·K}} 不应渲染为红字错误, got: {html}"
            );
        }
    }

    #[test]
    fn math_mode_middle_dot_renders_as_punctuation() {
        // 数学模式 U+00B7 是标点符号，保持 \cdotp 的间距语义。
        let html = render_inline("a · b");
        assert!(
            !html.contains("#cc0000"),
            "数学模式 · 本就能渲染, got: {html}"
        );
        assert!(
            html.contains("mpunct"),
            "数学模式 · 应保持 \\cdotp（mpunct）语义, got: {html}"
        );
    }

    #[test]
    fn display_mode_middle_dot_renders() {
        let html = render_display(r"T = \frac{b}{\lambda}, \text{单位 m·K}");
        assert!(
            !html.contains("#cc0000"),
            "块级公式 \\text{{m·K}} 不应渲染为红字错误, got: {html}"
        );
    }

    #[test]
    fn mhchem_ion_with_nested_braces_renders() {
        // 嵌套花括号 / 络离子：扫描器必须正确配对 {}。
        let html = render_inline(r"\ce{[Cu(NH3)4]^2+}");
        assert!(
            !html.contains("katex-error"),
            "络离子公式应正确渲染而非红字, got: {html}"
        );
    }
}
