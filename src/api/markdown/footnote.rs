//! 脚注：label → 锚点 id 与引用/定义/回链 HTML 片段。

use std::fmt::Write as _;

/// 将脚注 label 转换为可用于 HTML id / href 锚点的安全标识符。
///
/// 与 `slugify_heading` 不同：脚注 label 是用户自定义的稳定引用键（`[^key]`），
/// **不应**转拼音或回退通用词——需保留 label 原文形态以保证 ref↔def 双向一致、可读。
/// 策略：ASCII 字母数字与 `-`/`_` 保留；其余 ASCII 字符（空格、标点、引号）转 `-`；
/// 非 ASCII（中文、emoji 等）保留原样。合并连续 `-`、去首尾 `-`。
///
/// GFM 模式下 label 单行不含换行，但仍可能含空格/标点，此函数确保产出的 id：
/// (1) 不含 `"`/`<`/`>`/`&`/空格等破坏 HTML 属性或 URL 的字符；(2) 同一 label 必产出同一 id。
pub(super) fn footnote_id(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut prev_dash = true; // true = 当前不应再输出 `-`（开头或上一字符已是 `-`）

    for c in label.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
            prev_dash = false;
        } else if !c.is_ascii() {
            // 非 ASCII（中文等）直接保留——id 允许任意非空白字符。
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            // 其余 ASCII 字符（空格、标点、引号）转 `-`。
            out.push('-');
            prev_dash = true;
        }
    }

    // 去除尾部可能的 `-`（prev_dash 合并已保证首部无 `-`）。
    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        // label 全为特殊字符的极端情况，给一个确定性回退。
        out.push_str("fn");
    }

    out
}

/// 脚注引用的上标 HTML：`id` 为 label 的锚点 id，`n` 为本 label 的第 n 次引用（1-based），
/// `num` 为显示编号。
pub(super) fn reference_html(id: &str, n: usize, num: usize) -> String {
    // 上标引用：id 供 back-link 回跳，href 跳到定义，role=doc-noteref 语义化。
    let mut rendered = String::new();
    let _ = write!(
        rendered,
        r##"<sup class="fn-ref" id="fnref:{id}-{n}"><a href="#fn:{id}" class="fn-ref-link" role="doc-noteref" aria-label="脚注 {num}">{num}</a></sup>"##
    );
    rendered
}

/// 输出脚注定义的开启标签（`<aside>` + 编号 `<sup>`）。
pub(super) fn push_definition_open(html: &mut String, id: &str, num: usize) {
    // <aside role="doc-footnote">：取 pulldown-cmark 默认 <div> 的语义升级。
    // aria-labelledby 指向 label 的 sup，让屏幕阅读器朗读「脚注 N」。
    let _ = write!(
        html,
        r##"<aside class="footnote-definition" id="fn:{id}" role="doc-footnote" aria-labelledby="fn:{id}-label"><sup class="footnote-definition-label" id="fn:{id}-label">{num}</sup> "##
    );
}

/// 输出脚注定义末尾的回链序列。
pub(super) fn push_backlinks(html: &mut String, id: &str, ref_count: usize) {
    // back-link 上标符号序列：1 个引用用 ↩；N 个引用用 ↩¹ ↩²…（首个不加数字）。
    // 每个链接指向对应引用位置 fnref:{id}-{n}，role=doc-backlink 语义化。
    // ref_count=0（悬空定义）时不输出 back-link（无引用可回跳）。
    if ref_count > 0 {
        // 首个 back-link：裸 ↩。
        let _ = write!(
            html,
            r##"<a href="#fnref:{id}-1" class="fn-backref" role="doc-backlink" aria-label="返回正文">↩</a>"##
        );
        // 从第 2 个引用起加数字上标。
        for n in 2..=ref_count {
            let _ = write!(
                html,
                r##" <a href="#fnref:{id}-{n}" class="fn-backref" role="doc-backlink" aria-label="返回正文 {n}">↩<sup class="fn-backref-num">{n}</sup></a>"##
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- footnote_id 单元测试 ----

    #[test]
    fn footnote_id_preserves_alphanumeric() {
        assert_eq!(footnote_id("abc123"), "abc123");
        assert_eq!(footnote_id("a_b-c"), "a_b-c");
    }

    #[test]
    fn footnote_id_preserves_non_ascii() {
        // 中文、emoji 等非 ASCII 字符原样保留（HTML id 允许任意非空白字符）。
        assert_eq!(footnote_id("参考文献1"), "参考文献1");
    }

    #[test]
    fn footnote_id_replaces_spaces_and_punctuation() {
        // 空格、标点转 -，连续合并。
        assert_eq!(footnote_id("my note"), "my-note");
        assert_eq!(footnote_id("a b!c?d"), "a-b-c-d");
    }

    #[test]
    fn footnote_id_deterministic() {
        // 同一 label 多次调用必产生同一 id（ref↔def 双向一致的前提）。
        for label in ["a", "my note", "参考文献", "a!b@c#"] {
            assert_eq!(
                footnote_id(label),
                footnote_id(label),
                "label {:?} 不确定",
                label
            );
        }
    }

    #[test]
    fn footnote_id_no_attribute_breaking_chars() {
        // 产出的 id 不得含破坏 HTML 属性或 URL 的字符：" ' < > & 空格。
        for label in ["a\"b", "x'y", "a<b>", "c&d", "e f", "a!@#$%^&*()b"] {
            let id = footnote_id(label);
            assert!(
                !id.contains('"'),
                "id {:?} 含双引号 (label {:?})",
                id,
                label
            );
            assert!(
                !id.contains('\''),
                "id {:?} 含单引号 (label {:?})",
                id,
                label
            );
            assert!(!id.contains('<'), "id {:?} 含 < (label {:?})", id, label);
            assert!(!id.contains('>'), "id {:?} 含 > (label {:?})", id, label);
            assert!(!id.contains('&'), "id {:?} 含 & (label {:?})", id, label);
            assert!(!id.contains(' '), "id {:?} 含空格 (label {:?})", id, label);
        }
    }

    #[test]
    fn footnote_id_empty_fallback() {
        // 全特殊字符的极端情况回退为 "fn"。
        assert_eq!(footnote_id("!@#$"), "fn");
        assert_eq!(footnote_id("!!!"), "fn");
    }
}
