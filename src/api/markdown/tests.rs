use super::*;

#[test]
fn render_markdown_table_wrapped_in_scroll_container() {
    // 端到端:markdown table 经渲染管线后应被 table-wrap div 包裹。
    let result = render_markdown_enhanced("| A | B |\n|---|---|\n| 1 | 2 |\n");
    assert!(
        result.html.contains(r#"<div class="table-wrap">"#),
        "markdown table 应被 table-wrap 包裹, got: {}",
        result.html
    );
    assert!(result.html.contains("<table>"));
}

#[test]
fn render_markdown_table_with_inline_math_keeps_td_cells() {
    // 回归：单元格含 $…$ 曾触发中途 flush，后续单元格被渲染成 <th>。
    let md = "| 名 | 式 |\n|---|---|\n| a | $R_s=1$ |\n| b | $\\tfrac{1}{2}$ |\n| c | tail |\n";
    let result = render_markdown_enhanced(md);
    let tbody = &result.html[result.html.find("<tbody>").expect("应有 tbody")..];
    assert!(
        !tbody.contains("<th"),
        "tbody 不应出现 <th>: {}",
        result.html
    );
    assert!(
        !tbody.contains("</th>"),
        "不应出现错配 </th>: {}",
        result.html
    );
    assert_eq!(
        tbody.matches("<td").count(),
        6,
        "3 行 × 2 列应全为 <td>: {}",
        result.html
    );
    assert!(
        tbody.contains("katex"),
        "公式应渲染为 KaTeX: {}",
        result.html
    );
}

#[test]
fn render_markdown_table_with_footnote_ref_keeps_td_cells() {
    // FootnoteReference 与 InlineMath 走同一 flush 路径，同约防御。
    let md = "| A | B |\n|---|---|\n| x[^n] | $1+1$ |\n\n[^n]: note\n";
    let result = render_markdown_enhanced(md);
    let tbody = &result.html[result.html.find("<tbody>").expect("应有 tbody")..];
    assert!(
        !tbody.contains("<th") && !tbody.contains("</th>"),
        "tbody 不应出现 th: {}",
        result.html
    );
}

#[test]
fn render_markdown_heading_math_stays_inside_heading() {
    // in_heading 直写分支：公式必须落在 h 标签内（顺序不漂移到下一个 flush 点）。
    let md = "## 公式 $E=mc^2$ 标题\n\n正文\n";
    let result = render_markdown_enhanced(md);
    let h2_start = result.html.find("<h2").expect("应有 h2");
    let h2_end = result.html.find("</h2>").expect("应有 </h2>");
    let katex_pos = result.html.find("katex").expect("公式应渲染");
    assert!(
        h2_start < katex_pos && katex_pos < h2_end,
        "公式应在 h2 内部: {}",
        result.html
    );
}

#[test]
fn clean_html_allows_safe_tags() {
    let input = "<p>Hello <strong>world</strong></p>";
    assert_eq!(clean_html(input), input);
}

#[test]
fn clean_html_removes_script() {
    let input = "<script>alert('xss')</script><p>safe</p>";
    let result = clean_html(input);
    assert!(!result.contains("script"));
    assert!(result.contains("safe"));
}

#[test]
fn clean_html_allows_id_attribute() {
    let input = "<h2 id=\"my-heading\">Title</h2>";
    let result = clean_html(input);
    assert!(result.contains("id=\"my-heading\""));
}

#[test]
fn clean_html_allows_class_attribute() {
    let input = "<span class=\"highlight\">text</span>";
    let result = clean_html(input);
    assert!(result.contains("class=\"highlight\""));
}

#[test]
fn render_markdown_simple_paragraph() {
    let result = render_markdown_enhanced("Hello **world**");
    assert!(result.html.contains("<strong>world</strong>"));
    assert!(result.toc_html.is_empty());
}

#[test]
fn render_markdown_with_heading_generates_toc() {
    let result = render_markdown_enhanced("## My Heading\n\nSome text.");
    assert!(result.toc_html.contains("My Heading"));
    assert!(result.html.contains("id=\"my-heading\""));
}

#[test]
fn render_markdown_empty() {
    let result = render_markdown_enhanced("");
    assert_eq!(result.html, "");
    assert_eq!(result.toc_html, "");
}

#[test]
fn render_markdown_code_block() {
    let result = render_markdown_enhanced("```rust\nfn main() {}\n```");
    assert!(result.html.contains(r#"<pre><code class="language-rust">"#));
    assert!(result
        .html
        .contains(r#"<span class="entity name function rust">main</span>"#));
    assert!(result
        .html
        .contains(r#"<span class="storage type function rust">fn</span>"#));
}

#[test]
fn render_markdown_code_block_without_language() {
    let result = render_markdown_enhanced("```\nplain text\n```");
    assert!(result.html.contains("<pre><code>"));
    assert!(!result.html.contains("class=\"language-"));
    assert!(result.html.contains("plain text"));
}

#[test]
fn render_markdown_runnable_block_is_highlighted() {
    let result = render_markdown_enhanced("```python runnable\ndef f():\n    pass\n```");
    assert!(
        !result.html.contains("text plain"),
        "可运行块应按语言高亮, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_runnable_block_emits_data_attrs() {
    // `python runnable` 围栏：pre 上挂 data-runnable / data-lang / data-overrides / data-source，
    // 阅读器据此原地挂载 CodeRunner 组件。
    let result = render_markdown_enhanced("```python runnable\nprint('hi')\n```");
    assert!(
        result.html.contains(r#"data-runnable="true""#),
        "应输出 data-runnable, got: {}",
        result.html
    );
    assert!(
        result.html.contains(r#"data-lang="python""#),
        "应输出 data-lang=python, got: {}",
        result.html
    );
    // 无 overrides 时 data-overrides 为空。
    assert!(
        result.html.contains(r#"data-overrides="""#),
        "无 overrides 时 data-overrides 应为空, got: {}",
        result.html
    );
    // data-source 携带 HTML 转义后的原始源码（单引号转义为 &#x27;）。
    assert!(
        result
            .html
            .contains(r#"data-source="print(&#x27;hi&#x27;)"#),
        "data-source 应含转义后的源码, got: {}",
        result.html
    );
    // 内部仍带高亮 code 与 language-python。
    assert!(
        result.html.contains(r#"<code class="language-python">"#),
        "应保留高亮 code, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_runnable_block_with_overrides() {
    let result = render_markdown_enhanced(
        r#"```node runnable {"timeout_secs":10,"memory_mb":512,"allow_network":false,"cpu_cores":1.0,"output_bytes":1024}
console.log(1)
```"#,
    );
    assert!(result.html.contains(r#"data-lang="node""#));
    // overrides JSON 应被 HTML 转义后放入属性（双引号变 &quot;），不得出现裸引号越界。
    // 字段顺序按 serde 派生默认（字母序），cpu_cores 在前。
    assert!(
        result
            .html
            .contains("data-overrides=\"{&quot;cpu_cores&quot;:1.0"),
        "overrides 应 HTML 转义, got: {}",
        result.html
    );
    // 安全：不得出现可越出属性边界的裸双引号 JSON。
    assert!(
        !result.html.contains(r#"data-overrides="{"timeout""#),
        "overrides 裸引号越界, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_runnable_block_alias_normalized_to_canonical() {
    // 关键契约：parse_fence_info 在 markdown 渲染期就把别名归一为 canonical key，
    // 故 `js runnable` 渲染出的 data-lang 是 "node"（而非 "js"）。
    // 这保证阅读器 CodeRunner 拿到的 language 是 canonical，StartExec 的
    // LANGUAGES.get 能直接命中，无需前端再做别名映射。
    let cases = [
        ("js", "node"),
        ("javascript", "node"),
        ("rs", "rust"),
        ("ts", "bun"),
        ("typescript", "bun"),
        // 大小写不敏感。
        ("JavaScript", "node"),
        ("TypeScript", "bun"),
    ];
    for (alias, canonical) in cases {
        let src = format!("```{alias} runnable\nconsole.log(1)\n```");
        let result = render_markdown_enhanced(&src);
        assert!(
            result.html.contains(&format!(r#"data-lang="{canonical}""#)),
            "别名 {alias} 应归一为 data-lang={canonical}, got: {}",
            result.html
        );
        // 不应残留原始别名作为 data-lang。
        let bad = format!(r#"data-lang="{alias}""#);
        assert!(
            !result.html.contains(&bad),
            "data-lang 不应保留别名 {alias}, got: {}",
            result.html
        );
    }
}

#[test]
fn render_markdown_runnable_block_bun_canonical() {
    // bun 自身是 canonical（不是别名），runnable 块以 bun 执行。
    let result = render_markdown_enhanced("```bun runnable\nconsole.log('hi')\n```");
    assert!(
        result.html.contains(r#"data-lang="bun""#),
        "got: {}",
        result.html
    );
}

#[test]
fn render_markdown_runnable_marker_on_unsupported_lang_ignored() {
    // 语言不在白名单(rust 已在白名单,改用 ruby)：runnable 标记被忽略,输出普通代码块。
    let result = render_markdown_enhanced("```ruby runnable\nputs 'hi'\n```");
    assert!(
        !result.html.contains("data-runnable"),
        "不支持的语言不应挂 data-runnable, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_plain_fence_without_runnable_no_data_attrs() {
    let result = render_markdown_enhanced("```python\nprint(1)\n```");
    assert!(!result.html.contains("data-runnable"));
    assert!(result
        .html
        .contains(r#"<pre><code class="language-python">"#));
}

#[test]
fn render_markdown_code_block_preserves_html_content() {
    let result = render_markdown_enhanced("```html\n<script>alert(1)</script>\n```");
    assert!(result.html.contains("<pre><code class=\"language-html\">"));
    assert!(!result.html.contains("<script>"));
    assert!(result
        .html
        .contains(r#"<span class="variable function js">alert</span>"#));
    assert!(result
        .html
        .contains(r#"<span class="constant numeric js">1</span>"#));
}

#[test]
fn render_markdown_data_uri_image_removed() {
    let result = render_markdown_enhanced("![alt](data:image/svg+xml,%3csvg%3e%3c/svg%3e)");
    // 出于 XSS 防护，文章正文不再保留 data URI src。
    assert!(
        !result.html.contains("data:image/svg+xml"),
        "data URI should be removed from img src, got: {}",
        result.html
    );
    assert!(result.html.contains("alt=\"alt\""));
}

#[test]
fn render_markdown_task_list() {
    // 端到端验证：pulldown-cmark 解析任务列表 → sanitizer 清理后 checkbox 不丢失。
    // 覆盖「编辑器写入 → 入库 → 服务端重渲染」链路的最终 HTML 形态。
    let result = render_markdown_enhanced("- [ ] 未完成\n- [x] 已完成\n");
    // 两个 checkbox 都应保留
    assert!(
        result.html.contains(r#"type="checkbox""#),
        "checkbox type 应保留, got: {}",
        result.html
    );
    // 已勾选项的 checked 属性应保留
    assert!(
        result.html.contains("checked"),
        "checked 属性应保留, got: {}",
        result.html
    );
    // 文本内容保留
    assert!(result.html.contains("未完成"));
    assert!(result.html.contains("已完成"));
}

#[test]
fn render_markdown_footnote_basic() {
    // 端到端：单个脚注引用 + 定义，验证自定义渲染器的完整输出。
    // pulldown-cmark (GFM 模式) 解析 → 自定义事件拦截渲染 → sanitizer 放行。
    let result = render_markdown_enhanced("正文[^a]\n\n[^a]: 脚注内容\n");
    // 脚注引用：上标 + 锚点跳转 + role 语义
    assert!(
        result
            .html
            .contains(r#"<sup class="fn-ref" id="fnref:a-1">"#),
        "脚注引用上标应含正确 id, got: {}",
        result.html
    );
    assert!(
        result.html.contains(r##"href="#fn:a""##) && result.html.contains(r#"role="doc-noteref""#),
        "引用链接应指向定义并带 noteref 角色, got: {}",
        result.html
    );
    // 脚注定义：<aside> + role + aria-labelledby
    assert!(
        result
            .html
            .contains(r#"<aside class="footnote-definition" id="fn:a""#)
            && result.html.contains(r#"role="doc-footnote""#),
        "定义应为 aside + doc-footnote 角色, got: {}",
        result.html
    );
    // back-link：单个引用显示 ↩
    assert!(
        result.html.contains("↩") && result.html.contains(r#"role="doc-backlink""#),
        "单个引用应有 ↩ back-link, got: {}",
        result.html
    );
    // back-link 指向引用位置
    assert!(
        result.html.contains(r##"href="#fnref:a-1""##),
        "back-link 应指向引用位置, got: {}",
        result.html
    );
    // 脚注正文保留
    assert!(result.html.contains("脚注内容"));
    // 不应残留 pulldown-cmark 默认的 <div class="footnote-definition">
    assert!(
        !result.html.contains(r#"<div class="footnote-definition""#),
        "不应使用默认 div, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_footnote_multiple_refs() {
    // 同一脚注被多次引用：每个引用独立 id，定义末尾输出 N 个 back-link（↩、↩²、↩³）。
    let result = render_markdown_enhanced("第一处[^x]，第二处[^x]，第三处[^x]\n\n[^x]: 注\n");
    // 3 个引用，各有独立 id 后缀
    assert!(
        result.html.contains(r#"id="fnref:x-1""#)
            && result.html.contains(r#"id="fnref:x-2""#)
            && result.html.contains(r#"id="fnref:x-3""#),
        "3 次引用应有 3 个独立 id, got: {}",
        result.html
    );
    // back-link 数量 = 引用次数（3 个）
    let backref_count = result.html.matches(r#"class="fn-backref""#).count();
    assert_eq!(
        backref_count, 3,
        "3 次引用应产出 3 个 back-link, got: {}",
        result.html
    );
    // 首个 back-link 无数字上标（↩），后续带数字（↩²、↩³）
    assert!(
        result.html.contains(r#">↩</a>"#),
        "首个 back-link 应为裸 ↩, got: {}",
        result.html
    );
    assert!(
        result
            .html
            .contains("↩<sup class=\"fn-backref-num\">2</sup>")
            && result
                .html
                .contains("↩<sup class=\"fn-backref-num\">3</sup>"),
        "第 2、3 个 back-link 应带数字上标, got: {}",
        result.html
    );
    // 每个 back-link 指向不同引用位置
    assert!(
        result.html.contains(r##"href="#fnref:x-1""##)
            && result.html.contains(r##"href="#fnref:x-2""##)
            && result.html.contains(r##"href="#fnref:x-3""##),
        "3 个 back-link 应分别指向 3 个引用位置, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_footnote_numbering_order() {
    // 多个不同脚注：编号按 label 首次出现顺序分配（1、2、3…），与定义位置无关。
    let result =
        render_markdown_enhanced("先引用第二个[^b]，再第一个[^a]\n\n[^b]: B注\n\n[^a]: A注\n");
    // b 先在正文被引用 → 编号 1；a 后被引用 → 编号 2。
    // 分别断言各自的引用链接块（由 id 唯一定位）。
    // b 的引用：id=fnref:b-1，aria-label=脚注 1
    assert!(
        result.html.contains(r#"id="fnref:b-1""#) && result.html.contains(r#"aria-label="脚注 1""#),
        "b(先出现)引用编号应为 1, got: {}",
        result.html
    );
    // a 的引用：id=fnref:a-1，aria-label=脚注 2
    assert!(
        result.html.contains(r#"id="fnref:a-1""#) && result.html.contains(r#"aria-label="脚注 2""#),
        "a(后出现)引用编号应为 2, got: {}",
        result.html
    );
    // 两个定义都应存在
    assert!(
        result.html.contains(r#"id="fn:b""#) && result.html.contains(r#"id="fn:a""#),
        "两个脚注定义都应存在, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_footnote_id_safety() {
    // label 含空格/标点：id 应被清洗，ref↔def 双向匹配，不含破坏属性的字符。
    let result = render_markdown_enhanced("引文[^my note]\n\n[^my note]: 内容\n");
    // 空格应被转成 -，ref 与 def 用同一个清洗后 id
    assert!(
        result.html.contains(r##"href="#fn:my-note""##),
        "ref href 应用清洗后 id, got: {}",
        result.html
    );
    assert!(
        result.html.contains(r#"id="fn:my-note""#),
        "def id 应用清洗后 id, got: {}",
        result.html
    );
    // 不应含未转义的空格在 id/href 属性值中（会破坏属性或 URL）
    assert!(
        !result.html.contains("fn:my note"),
        "id 不应含空格, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_footnote_dangling_ref() {
    // GFM 模式下，未定义的悬空引用 [^missing] 被当作字面文本（不发 FootnoteReference 事件）。
    // 这是 GFM 与 OLD 模式的关键差异：OLD 把它渲染成 dangling link，GFM 保持字面。
    // 参见 pulldown-cmark lib.rs:712-713 注释。
    let result = render_markdown_enhanced("这个[^missing]没有定义\n");
    // 字面保留 [^missing]，不渲染成上标
    assert!(
        result.html.contains("[^missing]"),
        "GFM 模式下悬空引用应字面保留, got: {}",
        result.html
    );
    assert!(
        !result.html.contains("fn-ref"),
        "悬空引用不应渲染成脚注上标, got: {}",
        result.html
    );
    assert!(
        !result.html.contains("footnote-definition"),
        "悬空引用不应有定义块, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_footnote_gfm_mode() {
    // GFM 模式验证：定义续行需缩进。未缩进的续行不会被纳入脚注定义。
    // 这是 GFM 与 OLD 模式的关键差异——确认我们走的是 GFM。
    // 用一个 GFM 下会严格解析的输入：定义后紧跟的未缩进段落应独立于脚注。
    let result = render_markdown_enhanced("正文[^g]\n\n[^g]: 脚注第一行\n独立段落\n");
    // 脚注定义存在
    assert!(
        result.html.contains(r#"id="fn:g""#),
        "脚注定义应存在, got: {}",
        result.html
    );
    // GFM 模式下「独立段落」是独立的 <p>，不在脚注定义内
    assert!(
        result.html.contains("独立段落"),
        "独立段落内容应保留, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_inline_math() {
    // $...$ 内联公式：pulldown-cmark (ENABLE_MATH) 解析 → katex 渲染成 span。
    // sanitizer 放行 span 的 class/style，KaTeX 输出应原样保留。
    let result = render_markdown_enhanced("公式 $E = mc^2$ 很重要");
    assert!(
        result.html.contains("katex"),
        "内联公式应渲染为 katex span, got: {}",
        result.html
    );
    // 前后文本保留
    assert!(result.html.contains("公式"));
    assert!(result.html.contains("很重要"));
}

#[test]
fn render_markdown_inline_math_text_mode_middle_dot() {
    // issue #13 端到端：完整段落经 pulldown-cmark → katex → sanitizer，
    // `\text{m·K}` 的 `·` 不应渲染成红字 `\cdotp`（katex-rs 错误输出为
    // 逐字母 span 的 color node，故断言 `#cc0000` 不存在）。
    let md = "给定温度 $T$（开尔文），峰值波长由维恩位移定律 $\\lambda_{\\max} = b/T$ 决定（$b \\approx 2.898\\times10^{-3}\\,\\text{m·K}$）。";
    let result = render_markdown_enhanced(md);
    assert!(
        !result.html.contains("#cc0000"),
        "issue #13 段落不应有 KaTeX 红字错误, got: {}",
        result.html
    );
    assert!(
        result.html.contains('⋅'),
        "应渲染 U+22C5 middle dot glyph, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_display_math() {
    // $$...$$ 块级公式：应产出 <p class="math-display"> + katex-display。
    let result = render_markdown_enhanced("$$\\frac{a}{b}$$");
    assert!(
        result.html.contains("math-display"),
        "块级公式应用 math-display 段落包裹, got: {}",
        result.html
    );
    assert!(
        result.html.contains("katex-display"),
        "KaTeX 块级输出应含 katex-display, got: {}",
        result.html
    );
}
#[test]
fn render_markdown_sqrt_and_matrix_preserves_svg() {
    // \sqrt 与 \begin{pmatrix} 等 LaTeX 渲染需依赖 KaTeX 产生的 SVG 根号线与矩阵括号/竖线，
    // 验证经过 sanitizer clean_html 后 <svg> 与 <path> 不会被误丢。
    let result = render_markdown_enhanced(
        "$$\\sqrt{\\pi} + \\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}$$",
    );
    assert!(
        result.html.contains("<svg"),
        "KaTeX 根号/矩阵渲染的 <svg> 应保留, got: {}",
        result.html
    );
    assert!(
        result.html.contains("<path"),
        "KaTeX 根号/矩阵渲染的 <path> 应保留, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_inline_math_in_heading() {
    // 标题内的 $...$ 按内联公式渲染（不应崩，也不应产生块级 <p>）。
    let result = render_markdown_enhanced("## 勾股 $a^2 + b^2 = c^2$ 定理");
    assert!(
        result.html.contains("katex"),
        "标题内联公式应渲染, got: {}",
        result.html
    );
    // 标题里不应出现块级公式的 <p class="math-display">
    assert!(
        !result.html.contains("math-display"),
        "标题内不应有块级公式包裹, got: {}",
        result.html
    );
}

#[test]
fn render_markdown_bad_math_does_not_break() {
    // 坏 TeX 不应中断整篇渲染：throw_on_error=false 回退到错误 span。
    let result = render_markdown_enhanced("正常文本 $\\undefinedmacro{$ 后续文本");
    assert!(
        result.html.contains("正常文本"),
        "坏公式不应破坏前文, got: {}",
        result.html
    );
    assert!(
        result.html.contains("后续文本"),
        "坏公式不应破坏后文, got: {}",
        result.html
    );
}
