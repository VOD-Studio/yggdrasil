//! Markdown 渲染与目录生成。
//!
//! 使用 pulldown-cmark 解析 Markdown，为标题生成锚点与目录（TOC），
//! 代码块调用 `highlight` 模块进行语法高亮，最终通过 sanitizer 清理 HTML。
//! 仅在 `feature = "server"` 时执行实际渲染。

#![allow(clippy::unused_unit, deprecated)]

#[cfg(feature = "server")]
/// 对外暴露的 HTML 清理函数，委托给 sanitizer 模块。
pub(crate) fn clean_html(input: &str) -> String {
    crate::api::sanitizer::clean_html(input)
}

#[cfg(feature = "server")]
/// 将标题纯文本转义，用于安全地拼进 TOC 的 `aria-label="..."` 与 `<a>` 正文。
///
/// 复用 `utils::html::escape_html`（转义 `& < > " '`），避免在仓库内
/// 维护第二份转义实现。原先用 `clean_html` 处理属性上下文会漏掉 `"`，标题形如
/// `" onmouseover="alert(1)` 会越出属性边界。
fn escape_heading_text(s: &str) -> String {
    crate::utils::html::escape_html(s)
}

#[derive(Debug, Clone)]
#[cfg(feature = "server")]
/// Markdown 渲染结果。
pub struct RenderedContent {
    /// 清理后的正文 HTML。
    pub html: String,
    /// 目录 HTML（无标题时为空字符串）。
    pub toc_html: String,
}

#[cfg(feature = "server")]
/// 根据 Markdown 标题层级返回对应的 HTML 标签名。
fn heading_tag(level: pulldown_cmark::HeadingLevel) -> &'static str {
    match level {
        pulldown_cmark::HeadingLevel::H1 => "h1",
        pulldown_cmark::HeadingLevel::H2 => "h2",
        pulldown_cmark::HeadingLevel::H3 => "h3",
        pulldown_cmark::HeadingLevel::H4 => "h4",
        pulldown_cmark::HeadingLevel::H5 => "h5",
        pulldown_cmark::HeadingLevel::H6 => "h6",
    }
}

#[cfg(feature = "server")]
/// 增强版 Markdown 渲染：生成 TOC、标题锚点与语法高亮代码块。
pub fn render_markdown_enhanced(md: &str) -> RenderedContent {
    use pulldown_cmark::{Event, HeadingLevel, Options, Tag, TagEnd};
    use std::fmt::Write as _;

    // 两遍遍历使用相同的 Options 与同一份解析结果，避免 TOC 收集与正文渲染对
    // Markdown 扩展语法（表格、删除线、脚注等）的处理不一致。
    //
    // 脚注模式：pulldown-cmark 的 ENABLE_OLD_FOOTNOTES = (1<<9)|(1<<2)，它把
    // ENABLE_FOOTNOTES 的 bit 也打进了 OLD 的位掩码里。Options::all() 同时置两者，
    // 使 has_gfm_footnotes()（=ENABLE_FOOTNOTES && !ENABLE_OLD_FOOTNOTES）返回 false，
    // 走 OLD 模式（续行宽松、label 可含换行）。我们想要 GFM 模式（与 GitHub 一致、
    // 解析可控），所以不能简单地 remove(OLD)——那会连 ENABLE_FOOTNOTES 一起清掉。
    // 正确做法：先 remove(OLD)（清掉 bit 9 + bit 2），再 insert(ENABLE_FOOTNOTES)
    // 单独把 bit 2 加回，使 has_gfm_footnotes() = true。
    let mut opts = Options::all();
    opts.remove(Options::ENABLE_OLD_FOOTNOTES);
    opts.insert(Options::ENABLE_FOOTNOTES);

    // pulldown-cmark 只解析一次，collect 成 Vec<Event> 后两遍遍历复用。
    // 旧实现对同一份 md 调用两次 Parser::new_ext，等于两倍的 tokenize + 解析 CPU。
    // Event 内含 CowStr（借用 md 切片），collect 后仍可重复借用。
    let events: Vec<Event> = pulldown_cmark::Parser::new_ext(md, opts).collect();

    // 1. 第一遍：收集标题（level, text, id），用 iter() 借用不消费 events。
    // (level, text, id)
    let mut headings: Vec<(u8, String, String)> = Vec::new();
    let mut current_heading: Option<(u8, String)> = None;

    // 脚注引用统计：label → (引用次数, 首次出现序号)。
    // pulldown-cmark 不保证定义移到文末、也不保证 ref 先于 def，唯一可靠不变量是
    // 每个唯一 label 的 FootnoteDefinition 只出现一次、FootnoteReference 每次引用触发一次。
    // 所以 back-link 必须按 label 关联，display_num 按 label 首次出现顺序分配。
    // fn_order 记录 label 首次出现顺序，用于分配稳定的显示编号（1, 2, 3…）。
    use std::collections::HashMap;
    let mut fn_refs: HashMap<String, usize> = HashMap::new();
    let mut fn_order: Vec<String> = Vec::new();

    for event in &events {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let lvl = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                current_heading = Some((lvl, String::new()));
            }
            Event::Text(text) => {
                if let Some((_, ref mut content)) = current_heading {
                    content.push_str(text);
                }
            }
            Event::Code(code) => {
                if let Some((_, ref mut content)) = current_heading {
                    content.push_str(code);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((lvl, text)) = current_heading.take() {
                    let id = slugify_heading(&text);
                    headings.push((lvl, text, id));
                }
            }
            // 统计脚注引用：仅对 FootnoteReference 计数（含未被定义的悬空引用）。
            // 悬空引用（[^missing] 无定义）也会产生此事件，但第二遍不会有对应 def，
            // fn_refs 里的条目无害（查不到对应 def 时第二遍不会输出 back-link）。
            Event::FootnoteReference(name) => {
                let label = name.to_string();
                let count = fn_refs.entry(label.clone()).or_insert(0);
                *count += 1;
                if *count == 1 {
                    fn_order.push(label);
                }
            }
            _ => {}
        }
    }

    // 按 label 首次出现顺序分配显示编号（1-based）。脚注定义内查此表取 display_num。
    let fn_num: HashMap<&String, usize> = fn_order
        .iter()
        .enumerate()
        .map(|(i, label)| (label, i + 1))
        .collect();

    // 2. Generate TOC HTML
    let toc_html = generate_toc_html(&headings);

    // 3. 第二遍：生成 HTML，用 into_iter() 消费 events（非标题事件需 move 进 push_html）。
    // HTML 输出通常比 md 长（标签包裹），按 md 长度 + 256 预分配，避免 String::new 的多次 realloc。
    let mut html = String::with_capacity(md.len() + 256);
    let mut heading_idx = 0;
    let mut in_heading = false;
    let mut in_codeblock = false;
    let mut code_lang: Option<String> = None;
    // 可运行代码块的 (lang, html-escaped overrides JSON)。
    // 为 None 表示普通代码块。原始源码在 End 处从 code_buffer 转义后存入 data-source，
    // 供阅读器无损提取（避免从高亮 HTML 反解）。
    let mut code_runnable: Option<(String, String)> = None;
    let mut code_buffer = String::new();
    let mut non_heading_events: Vec<Event> = Vec::new();
    // 第二遍维护的脚注引用计数：label → 已渲染的引用序号（从 1 起）。
    // 用于给每个 ref 分配 id 后缀 fnref:{label}-{n}，并让 def 末尾的 back-link 对应到每个 ref。
    let mut fn_ref_seen: HashMap<String, usize> = HashMap::new();
    // 脚注定义栈：Start(FootnoteDefinition) 压入 label，End 弹出。
    // 脚注定义可嵌套（def 内引用另一个脚注），用栈保证 End 配对到正确的 label。
    let mut fn_def_stack: Vec<String> = Vec::new();

    for event in events {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                // 先把累积的普通事件刷入 HTML，再开始新标题。
                if !non_heading_events.is_empty() {
                    pulldown_cmark::html::push_html(&mut html, non_heading_events.into_iter());
                    non_heading_events = Vec::new();
                }
                in_heading = true;
                if heading_idx < headings.len() {
                    let (_, _, ref id) = headings[heading_idx];
                    let tag = heading_tag(level);
                    // write! 直写目标 String，零中间分配（format! 会先分配临时 String 再 push_str）。
                    let _ = write!(html, "<{tag} id=\"{id}\">");
                }
            }
            Event::End(TagEnd::Heading(level)) => {
                if heading_idx < headings.len() {
                    let (_, _, ref id) = headings[heading_idx];
                    let tag = heading_tag(level);
                    let _ = write!(
                        html,
                        "<a class=\"anchor\" aria-hidden=\"true\" href=\"#{id}\">#</a></{tag}>"
                    );
                    heading_idx += 1;
                }
                in_heading = false;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                // 代码块开始前同样先刷入未处理的普通事件。
                if !non_heading_events.is_empty() {
                    pulldown_cmark::html::push_html(&mut html, non_heading_events.into_iter());
                    non_heading_events = Vec::new();
                }
                in_codeblock = true;
                code_lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                        if lang.is_empty() {
                            None
                        } else {
                            Some(lang.to_string())
                        }
                    }
                    _ => None,
                };
                // 解析围栏 info：识别 `runnable` 标记与可选 ResourceLimits JSON 覆盖。
                // 仅在「标记为 runnable 且语言受支持」时挂 data-*，供阅读器扫描挂载运行器。
                code_runnable = code_lang.as_deref().and_then(|info| {
                    let (lang, runnable, overrides) =
                        crate::api::code_runner::languages::parse_fence_info(info);
                    if runnable && crate::api::code_runner::languages::is_supported_lang(&lang) {
                        // overrides 序列化为 JSON 后 HTML 转义，避免属性注入。
                        let ov_json = overrides
                            .map(|o| serde_json::to_string(&o).unwrap_or_default())
                            .unwrap_or_default();
                        let overrides_escaped = crate::utils::html::escape_html(&ov_json);
                        Some((lang, overrides_escaped))
                    } else {
                        None
                    }
                });
                code_buffer.clear();
            }
            Event::Text(text) if in_codeblock => {
                code_buffer.push_str(&text);
            }
            Event::End(TagEnd::CodeBlock) => {
                // mermaid 代码块：前端 yggdrasil-core 扫描 language-mermaid 渲染成 SVG，
                // 源码不应被 syntect 高亮（无语法定义，且会包 <span> 污染 textContent 提取）。
                // 直接输出转义后的纯源码，前端 textContent 无损拿到原始 mermaid 文本。
                let is_mermaid = code_lang
                    .as_deref()
                    .map(|l| l.split_whitespace().next() == Some("mermaid"))
                    .unwrap_or(false);
                if is_mermaid {
                    let escaped = crate::utils::html::escape_html(&code_buffer);
                    html.push_str(r#"<pre><code class="language-mermaid">"#);
                    html.push_str(&escaped);
                    html.push_str("</code></pre>");
                    in_codeblock = false;
                    continue;
                }
                // 使用 syntect 对代码块进行服务端语法高亮。围栏信息可能带修饰
                //（`python runnable {...}`），只取首个语言 token。
                let highlight_lang = code_lang
                    .as_deref()
                    .and_then(|lang| lang.split_whitespace().next());
                let highlighted =
                    crate::highlight::server::highlight_code(&code_buffer, highlight_lang);
                // 可运行代码块：在 <pre> 上挂 data-runnable / data-lang / data-overrides / data-source，
                // 阅读者（post_content.rs）客户端扫描这些标记原地挂载 CodeRunner 组件。
                // data-source 为 HTML 转义后的原始源码，供阅读器无损提取（避免反解高亮 HTML）。
                if let Some((lang, overrides_escaped)) = code_runnable.take() {
                    let source_escaped = crate::utils::html::escape_html(&code_buffer);
                    let _ = write!(
                        html,
                        r#"<pre data-runnable="true" data-lang="{lang}" data-overrides="{overrides_escaped}" data-source="{source_escaped}"><code class="language-{lang}">"#
                    );
                } else {
                    html.push_str("<pre><code");
                    if let Some(lang) = &code_lang {
                        // 围栏语言可能含 info 修饰（如 `python runnable {...}`），
                        // 高亮的 language-xxx 取纯语言 token（首个空白前）。
                        let clean_lang = lang.split_whitespace().next().unwrap_or("");
                        if !clean_lang.is_empty() {
                            let _ = write!(html, " class=\"language-{clean_lang}\"");
                        }
                    }
                    html.push('>');
                }
                html.push_str(&highlighted);
                html.push_str("</code></pre>");
                in_codeblock = false;
            }
            Event::InlineMath(tex) => {
                // 不 flush 直写，渲染结果作为 InlineHtml 事件并入本批：push_html 的 writer
                // 把表格 thead/tbody 状态存在单次调用内（初始 Head，见 pulldown-cmark html.rs），
                // 表格单元格中途 flush 会让后续单元格在新 writer 里回落成 <th>。
                let rendered = crate::api::katex::render_inline(&tex);
                if in_heading {
                    // 标题内事件不进 non_heading_events（catch-all 直写），此处同样直写以保序。
                    html.push_str(&rendered);
                } else {
                    non_heading_events.push(Event::InlineHtml(rendered.into()));
                }
            }
            Event::DisplayMath(tex) => {
                // 同 InlineMath：并入本批，避免表格中途 flush。
                // 块级公式独占一行：用 <p class="math-display"> 包裹 KaTeX 输出。
                // KaTeX 自身已产出 .katex-display，外层 <p> 负责段间距与居中容器。
                let rendered = format!(
                    "<p class=\"math-display\">{}</p>",
                    crate::api::katex::render_display(&tex)
                );
                if in_heading {
                    html.push_str(&rendered);
                } else {
                    non_heading_events.push(Event::InlineHtml(rendered.into()));
                }
            }
            Event::FootnoteReference(name) => {
                let label = name.to_string();
                // 本 label 的第 n 次引用（1-based），用于 id 后缀。
                let n = {
                    let entry = fn_ref_seen.entry(label.clone()).or_insert(0);
                    *entry += 1;
                    *entry
                };
                let id = footnote_id(&label);
                // display_num：label 首次出现顺序编号；悬空引用（无 def）查不到时回退到引用序号 n。
                let num = fn_num.get(&label).copied().unwrap_or(n);
                // 同 InlineMath：并入本批（格式串与原实现逐字一致），避免表格中途 flush。
                // 上标引用：id 供 back-link 回跳，href 跳到定义，role=doc-noteref 语义化。
                let mut rendered = String::new();
                let _ = write!(
                    rendered,
                    r##"<sup class="fn-ref" id="fnref:{id}-{n}"><a href="#fn:{id}" class="fn-ref-link" role="doc-noteref" aria-label="脚注 {num}">{num}</a></sup>"##
                );
                if in_heading {
                    html.push_str(&rendered);
                } else {
                    non_heading_events.push(Event::InlineHtml(rendered.into()));
                }
            }
            Event::Start(Tag::FootnoteDefinition(name)) => {
                // 脚注定义开始：先刷出累积的普通事件，再用 <aside> 开启语义化容器。
                if !non_heading_events.is_empty() {
                    pulldown_cmark::html::push_html(&mut html, non_heading_events.into_iter());
                    non_heading_events = Vec::new();
                }
                let label = name.to_string();
                let id = footnote_id(&label);
                let num = fn_num.get(&label).copied().unwrap_or_else(|| {
                    // 定义未被任何引用提及（悬空定义）：用 fn_order 长度+1 兜底编号。
                    fn_order.len() + 1
                });
                // <aside role="doc-footnote">：取 pulldown-cmark 默认 <div> 的语义升级。
                // aria-labelledby 指向 label 的 sup，让屏幕阅读器朗读「脚注 N」。
                let _ = write!(
                    html,
                    r##"<aside class="footnote-definition" id="fn:{id}" role="doc-footnote" aria-labelledby="fn:{id}-label"><sup class="footnote-definition-label" id="fn:{id}-label">{num}</sup> "##
                );
                fn_def_stack.push(label);
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                // 脚注定义结束：先刷出累积的脚注正文事件（定义内部的段落/列表等），
                // 再按引用次数输出 N 个 back-link（↩、↩²、↩³…），最后闭合 <aside>。
                if !non_heading_events.is_empty() {
                    pulldown_cmark::html::push_html(&mut html, non_heading_events.into_iter());
                    non_heading_events = Vec::new();
                }
                // 弹栈取当前定义的 label，配对 Start。
                if let Some(label) = fn_def_stack.pop() {
                    let id = footnote_id(&label);
                    let ref_count = fn_refs.get(&label).copied().unwrap_or(0);
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
                html.push_str("</aside>");
            }
            _ => {
                if in_heading {
                    // 标题内部只保留文本与行内代码，避免嵌套块级元素。
                    match event {
                        Event::Text(text) => html.push_str(&clean_html(&text)),
                        Event::Code(code) => {
                            html.push_str("<code>");
                            html.push_str(&clean_html(&code));
                            html.push_str("</code>");
                        }
                        _ => {}
                    }
                } else if !in_codeblock {
                    non_heading_events.push(event);
                }
            }
        }
    }

    // Flush remaining non-heading events
    if !non_heading_events.is_empty() {
        pulldown_cmark::html::push_html(&mut html, non_heading_events.into_iter());
    }

    let html = wrap_images_with_blur(&html);
    // 表格外层套可滚动容器，移动端窄屏可横向滚动而不被外层 overflow-hidden 裁切。
    // sanitizer 不放行 table 的 class/style，但 div 在白名单、class 属全局属性，故包裹 div。
    let html = wrap_tables(&html);
    RenderedContent {
        html: clean_html(&html),
        toc_html,
    }
}

/// 把 HTML 里的 /uploads/ 图片转成 blur-up 双层结构。
///
/// 仅处理 src 以 /uploads/ 开头的 img；外链图保持原样。
/// 对每个匹配的 img：
/// 1. 提取 src，解析出 rel_path（去 /uploads/ 前缀和 query）
/// 2. 查 get_image_dimensions 拿真实宽高，算 --ar（如 "16:9"）
/// 3. 生成 `<span class="blur-img" style="--ar:..">` 包裹两层 img
#[cfg(feature = "server")]
fn wrap_images_with_blur(html: &str) -> String {
    wrap_images_with_blur_with(html, crate::api::image::get_image_dimensions)
}

/// `wrap_images_with_blur` 的纯函数核心，接受 dimensions 查询闭包以便单测。
///
/// `dims_fn(rel_path) -> Option<(u32, u32)>` 注入真实或测试用的 dimensions 来源，
/// 使本函数不依赖文件系统——测试可注入已知宽高，生产注入 get_image_dimensions。
#[cfg(feature = "server")]
fn wrap_images_with_blur_with<F>(html: &str, dims_fn: F) -> String
where
    F: Fn(&str) -> Option<(u32, u32)>,
{
    use regex::Regex;
    use std::sync::LazyLock;

    // 匹配 pulldown-cmark 产出的 <img src="..." alt="..." /> 或 <img src="..." alt="...">
    // pulldown-cmark 格式可控：src 在前，alt 在后，属性用双引号
    static IMG_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"<img\s+src="(/uploads/[^"]+)"(?:\s+alt="([^"]*)")?\s*/?>"#)
            .expect("IMG_RE 正则模式应在编译期通过校验")
    });

    IMG_RE
        .replace_all(html, |caps: &regex::Captures| {
            let src = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let alt = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            // 从 src 解析 rel_path：去 /uploads/ 前缀 + 去 query
            let rel_path = src
                .strip_prefix("/uploads/")
                .unwrap_or(src)
                .split('?')
                .next()
                .unwrap_or("");

            // 查 dimensions，算 aspect-ratio
            // 注意：CSS aspect-ratio 用斜杠分隔（width / height），不是冒号
            let ar_style = dims_fn(rel_path)
                .map(|(w, h)| format!(" style=\"--ar:{} / {};\"", w, h))
                .unwrap_or_default();

            // alt 转义（src/alt 来自 markdown，pulldown-cmark 已转义过，这里直接用）
            let alt_attr = if alt.is_empty() {
                String::new()
            } else {
                format!(" alt=\"{}\"", alt)
            };

            format!(
                "<span class=\"blur-img\"{ar}><img class=\"blur-img-placeholder\" src=\"{src}?w=20\"{alt_attr}><img class=\"blur-img-full\" data-src=\"{src}?w=800\"{alt_attr}></span>",
                ar = ar_style,
                src = src,
                alt_attr = alt_attr,
            )
        })
        .into_owned()
}

#[cfg(feature = "server")]
/// 把每个 `<table>...</table>` 包进可横向滚动的 `<div class="table-wrap">`。
///
/// 移动端窄屏下宽表格无法横向滚动：pulldown-cmark 产出裸 table，外层 `<main>` 又是
/// `overflow-hidden` 会把超宽内容直接裁掉。这里给 table 套一层 `table-wrap`（CSS 配
/// `overflow-x: auto`），让表格在容器内滚动而非撑破页面。
///
/// 正则匹配 pulldown-cmark 输出的 `<table ...>...</table>`（非贪婪、跨行），包裹整个
/// table 标签。对已包裹的 HTML 幂等（不会二次嵌套）——`table-wrap` div 内的 table
/// 仍会被正则命中并再次包裹，故调用方仅在渲染管线调用一次。
fn wrap_tables(html: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    static TABLE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)<table(\s[^>]*)?>.*?</table>")
            .expect("TABLE_RE 正则模式应在编译期通过校验")
    });

    TABLE_RE
        .replace_all(html, |caps: &regex::Captures| {
            format!("<div class=\"table-wrap\">{}</div>", &caps[0])
        })
        .into_owned()
}

#[cfg(feature = "server")]
/// 根据标题层级生成嵌套目录 HTML。
fn generate_toc_html(headings: &[(u8, String, String)]) -> String {
    use std::fmt::Write as _;

    if headings.is_empty() {
        return String::new();
    }

    // TOC 大小按标题数估算（每个 li+a 约 64 字节起），避免 String::new 的多次 realloc。
    let mut html = String::with_capacity(headings.len() * 64 + 16);
    html.push_str("<ul>");
    let mut stack: Vec<u8> = vec![headings[0].0];

    for (i, (level, text, id)) in headings.iter().enumerate() {
        let level = *level;

        if i > 0 {
            let prev_level = headings[i - 1].0;
            if level > prev_level {
                // 标题层级升高：打开新的嵌套列表。
                for _ in prev_level..level {
                    html.push_str("<ul>");
                    stack.push(level);
                }
            } else if level < prev_level {
                // 标题层级降低：关闭多余的嵌套列表。
                while let Some(top) = stack.last() {
                    if *top > level {
                        html.push_str("</li></ul>");
                        stack.pop();
                    } else {
                        break;
                    }
                }
                html.push_str("</li>");
            } else {
                html.push_str("</li>");
            }
        }

        // 标题 text 是 pulldown-cmark 收集的纯文本（Text/Code 字面字符），不是 HTML 片段，
        // 因此正文与属性两处都走 escape_heading_text（转义 & < > " '）。原先用 clean_html
        // 处理属性上下文会漏掉 `"`，标题中的双引号会越出 aria-label 边界。
        let escaped_text = escape_heading_text(text);
        let _ = write!(
            html,
            "<li><a href=\"#{id}\" aria-label=\"{escaped_text}\">{escaped_text}</a>"
        );
    }

    // Close remaining lists
    // 闭合所有残留的嵌套列表。
    while stack.len() > 1 {
        html.push_str("</li></ul>");
        stack.pop();
    }
    html.push_str("</li></ul>");

    html
}

#[cfg(feature = "server")]
/// 将标题文本转换为可用于锚点的 slug。
///
/// 汉字转无声调拼音（每字成词，用 `-` 分隔），与 [`crate::api::slug::slugify`]
/// 保持一致；ASCII 字母数字保留，其余字符作为词分隔。结果为空时回退 `heading`。
fn slugify_heading(text: &str) -> String {
    use pinyin::ToPinyin;

    let mut slug = String::new();
    let mut prev_dash = true;

    for c in text.to_lowercase().chars() {
        // 汉字优先转拼音；非汉字（含 ascii）返回 None。
        // 每个汉字成词，拼音后补 `-` 与后续内容分隔（连续汉字也会被分开）。
        if let Some(py) = c.to_pinyin() {
            slug.push_str(py.plain());
            slug.push('-');
            prev_dash = true;
        } else if c.is_alphanumeric() {
            slug.push(c);
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }

    if slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        slug.push_str("heading");
    }

    slug
}

#[cfg(feature = "server")]
/// 将脚注 label 转换为可用于 HTML id / href 锚点的安全标识符。
///
/// 与 `slugify_heading` 不同：脚注 label 是用户自定义的稳定引用键（`[^key]`），
/// **不应**转拼音或回退通用词——需保留 label 原文形态以保证 ref↔def 双向一致、可读。
/// 策略：ASCII 字母数字与 `-`/`_` 保留；其余 ASCII 字符（空格、标点、引号）转 `-`；
/// 非 ASCII（中文、emoji 等）保留原样。合并连续 `-`、去首尾 `-`。
///
/// GFM 模式下 label 单行不含换行，但仍可能含空格/标点，此函数确保产出的 id：
/// (1) 不含 `"`/`<`/`>`/`&`/空格等破坏 HTML 属性或 URL 的字符；(2) 同一 label 必产出同一 id。
fn footnote_id(label: &str) -> String {
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

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn wrap_images_with_blur_wraps_uploads_image() {
        // 注入返回 None 的 dims_fn,验证 --ar 缺省时的结构正确性。
        // 不依赖 uploads/ 文件系统。
        let html = r#"<p><img src="/uploads/nonexistent/test.webp" alt="test"></p>"#;
        let result = wrap_images_with_blur_with(html, |_| None);
        assert!(
            result.contains("blur-img-placeholder"),
            "should have placeholder"
        );
        assert!(result.contains("blur-img-full"), "should have full layer");
        assert!(result.contains("?w=20"), "placeholder should use ?w=20");
        assert!(result.contains("?w=800"), "full should use ?w=800");
        assert!(result.contains("data-src"), "full should use data-src");
    }

    #[test]
    fn wrap_images_with_blur_skips_external_image() {
        // 外链图不进入 dims_fn,保持原样。
        let html = r#"<img src="https://example.com/img.png" alt="ext">"#;
        let result = wrap_images_with_blur_with(html, |_| None);
        assert!(
            !result.contains("blur-img"),
            "external image should not be wrapped"
        );
    }

    #[test]
    fn wrap_images_with_blur_uses_slash_in_aspect_ratio() {
        // 注入已知 dimensions,验证 --ar 用斜杠分隔(如 "--ar:800 / 600;")。
        // 此前依赖 uploads/ 真实文件,现已解耦。
        let html = r#"<img src="/uploads/2026/06/18/abc.webp" alt="t">"#;
        let result = wrap_images_with_blur_with(html, |_| Some((800, 600)));
        assert!(result.contains("--ar:"), "should have --ar");
        assert!(
            result.contains(" / "),
            "aspect-ratio must use slash separator, got: {}",
            result
        );
        assert!(result.contains("--ar:800 / 600;"), "应含精确宽高");
    }

    #[test]
    fn wrap_images_with_blur_omits_ar_when_no_dimensions() {
        // dims_fn 返回 None 时不输出 --ar,避免空 style 属性。
        let html = r#"<img src="/uploads/x.webp" alt="t">"#;
        let result = wrap_images_with_blur_with(html, |_| None);
        assert!(!result.contains("--ar"), "无 dimensions 不应有 --ar");
        // 但包裹结构仍应生成
        assert!(result.contains("blur-img"));
    }

    #[test]
    fn wrap_images_with_blur_strips_query_from_rel_path() {
        // rel_path 提取应去 query 后缀,dims_fn 收到的是去 query 的路径。
        // dims_fn 是 Fn(replace_all 要求),用 RefCell 捕获传入值。
        use std::cell::RefCell;
        let html = r#"<img src="/uploads/2026/x.webp?w=100" alt="t">"#;
        let received = RefCell::new(String::new());
        let _ = wrap_images_with_blur_with(html, |p| {
            *received.borrow_mut() = p.to_string();
            Some((100, 100))
        });
        assert_eq!(
            received.borrow().as_str(),
            "2026/x.webp",
            "rel_path 应去 query"
        );
    }

    #[test]
    fn wrap_images_with_blur_preserves_alt() {
        let html = r#"<img src="/uploads/x.webp" alt="描述">"#;
        let result = wrap_images_with_blur_with(html, |_| Some((10, 10)));
        assert!(result.contains(r#"alt="描述""#), "placeholder 应保留 alt");
        assert!(
            result.matches(r#"alt="描述""#).count() == 2,
            "两层 img 都应带 alt"
        );
    }

    #[test]
    fn wrap_images_with_blur_omits_alt_attr_when_empty() {
        let html = r#"<img src="/uploads/x.webp">"#;
        let result = wrap_images_with_blur_with(html, |_| None);
        assert!(!result.contains("alt=\""), "无 alt 时不应生成空 alt 属性");
    }

    #[test]
    fn full_pipeline_wrap_then_clean_preserves_slash() {
        // 模拟完整渲染管线:wrap → clean_html,验证 sanitizer 不破坏斜杠。
        // 注入确定 dimensions,脱离文件系统依赖。
        let html = r#"<img src="/uploads/2026/06/18/abc.webp" alt="t">"#;
        let wrapped = wrap_images_with_blur_with(html, |_| Some((800, 600)));
        let cleaned = clean_html(&wrapped);
        assert!(
            cleaned.contains(" / "),
            "clean_html must preserve slash in --ar, got: {}",
            cleaned
        );
    }

    // ---- wrap_tables 单元测试 ----

    #[test]
    fn wrap_tables_wraps_bare_table() {
        let html =
            "<table><thead><tr><th>A</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>";
        let result = wrap_tables(html);
        assert!(
            result.starts_with("<div class=\"table-wrap\"><table>")
                && result.ends_with("</table></div>"),
            "应整体包裹一层 table-wrap, got: {}",
            result
        );
    }

    #[test]
    fn wrap_tables_wraps_table_with_attributes() {
        // pulldown-cmark 不会给 table 加属性,但正则需兼容带属性/自闭合起始的 table。
        let html = r#"<table class="x"><tr><td>1</td></tr></table>"#;
        let result = wrap_tables(html);
        assert!(
            result.contains("<div class=\"table-wrap\"><table"),
            "应从 table 起始标签整体包裹, got: {}",
            result
        );
        assert!(result.ends_with("</table></div>"));
    }

    #[test]
    fn wrap_tables_wraps_multiple_tables() {
        let html =
            "<table><tr><td>1</td></tr></table>\n<p>间隔</p>\n<table><tr><td>2</td></tr></table>";
        let result = wrap_tables(html);
        let wrap_count = result.matches("<div class=\"table-wrap\">").count();
        assert_eq!(wrap_count, 2, "两个 table 应各自包裹, got: {}", result);
        // 中间段落不被误包
        assert!(result.contains("\n<p>间隔</p>\n"));
    }

    #[test]
    fn wrap_tables_handles_multiline_table() {
        // pulldown-cmark 产出的 table HTML 是单行无换行的,但正则用 (?s) 跨行兼容手写 HTML。
        let html = "<table>\n  <tr>\n    <td>1</td>\n  </tr>\n</table>";
        let result = wrap_tables(html);
        assert!(
            result.starts_with("<div class=\"table-wrap\"><table>")
                && result.ends_with("</table></div>"),
            "跨行 table 应整体包裹, got: {}",
            result
        );
    }

    #[test]
    fn wrap_tables_does_not_touch_non_table_html() {
        let html = "<p>段落</p><ul><li>项</li></ul>";
        let result = wrap_tables(html);
        assert_eq!(result, html, "无 table 时应原样返回");
    }

    #[test]
    fn wrap_tables_then_clean_preserves_div_and_table() {
        // 端到端:wrap → clean_html,确认 sanitizer 放行 div.table-wrap 与内部 table 结构。
        let html =
            "<table><thead><tr><th>H</th></tr></thead><tbody><tr><td>v</td></tr></tbody></table>";
        let wrapped = wrap_tables(html);
        let cleaned = clean_html(&wrapped);
        assert!(
            cleaned.contains(r#"<div class="table-wrap">"#),
            "clean_html 应保留 table-wrap div, got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("<table>") && cleaned.contains("</table>"),
            "clean_html 应保留 table 标签, got: {}",
            cleaned
        );
    }

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
    fn slugify_heading_simple() {
        assert_eq!(slugify_heading("Hello World"), "hello-world");
    }

    #[test]
    fn slugify_heading_special_chars() {
        assert_eq!(slugify_heading("What's new? (2024)"), "what-s-new-2024");
    }

    #[test]
    fn slugify_heading_chinese() {
        // 汉字逐字转拼音，用 `-` 分隔。
        assert_eq!(slugify_heading("你好世界"), "ni-hao-shi-jie");
    }

    #[test]
    fn slugify_heading_mixed_chinese_ascii() {
        // Rust 入门指南 → rust + ru-men-zhi-nan，词之间用 `-` 分隔。
        assert_eq!(slugify_heading("Rust 入门指南"), "rust-ru-men-zhi-nan");
    }

    #[test]
    fn slugify_heading_collapses_dashes() {
        assert_eq!(slugify_heading("a--b"), "a-b");
    }

    #[test]
    fn slugify_heading_strips_trailing_dash() {
        assert_eq!(slugify_heading("hello!"), "hello");
    }

    #[test]
    fn slugify_heading_empty_returns_heading() {
        assert_eq!(slugify_heading(""), "heading");
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
    fn generate_toc_html_empty() {
        assert_eq!(generate_toc_html(&[]), "");
    }

    #[test]
    fn generate_toc_html_single_heading() {
        let headings = vec![(2u8, "Title".to_string(), "title".to_string())];
        let html = generate_toc_html(&headings);
        assert!(html.contains("href=\"#title\""));
        assert!(html.contains("<ul>"));
        assert!(html.contains("</ul>"));
    }

    #[test]
    fn generate_toc_html_nested() {
        let headings = vec![
            (2u8, "Chapter".to_string(), "chapter".to_string()),
            (3u8, "Section".to_string(), "section".to_string()),
        ];
        let html = generate_toc_html(&headings);
        assert!(html.contains("href=\"#chapter\""));
        assert!(html.contains("href=\"#section\""));
        let ul_count = html.matches("<ul>").count();
        assert_eq!(ul_count, 2);
    }

    #[test]
    fn generate_toc_html_escapes_quote_in_attr() {
        // 标题中的双引号不得越出 aria-label 属性边界。
        let headings = vec![(
            2u8,
            "\" onmouseover=\"alert(1)".to_string(),
            "heading".to_string(),
        )];
        let html = generate_toc_html(&headings);
        // aria-label 中的双引号被转义为 &quot;，无法越出属性边界注入新属性。
        assert!(
            html.contains("aria-label=\"&quot; onmouseover=&quot;alert(1)\""),
            "aria-label 应转义内部双引号，got: {html}"
        );
        // 关键：不得出现「未被引号包裹、可被解析为真实属性」的 onmouseover= 片段。
        // 正文中作为纯文本出现 "onmouseover" 字符串是安全的（无 < 或属性结构）。
        let attr_injection = "\" onmouseover=\"";
        let injected = html.matches(attr_injection).count();
        // 原始输入里有 1 个裸双引号起头；转义后该模式不应再作为属性边界出现。
        // 注意 aria-label 内部的双引号已变成 &quot;，因此裸的 `" onmouseover="` 不应存在。
        assert_eq!(
            injected, 0,
            "不应存在未转义的属性边界 `\" onmouseover=\"`，got: {html}"
        );
    }

    #[test]
    fn generate_toc_html_escapes_ampersand_in_attr() {
        let headings = vec![(2u8, "A & B".to_string(), "heading".to_string())];
        let html = generate_toc_html(&headings);
        assert!(
            html.contains("aria-label=\"A &amp; B\""),
            "& 应在属性中转义，got: {html}"
        );
    }

    #[test]
    fn generate_toc_html_escapes_less_than_in_attr() {
        // `<` 在属性与正文中都应被转义，避免被误解析为标签起始。
        let headings = vec![(2u8, "a < b".to_string(), "heading".to_string())];
        let html = generate_toc_html(&headings);
        assert!(
            html.contains("aria-label=\"a &lt; b\""),
            "< 应在属性中转义，got: {html}"
        );
        assert!(!html.contains("a < b"));
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
            result.html.contains(r##"href="#fn:a""##)
                && result.html.contains(r#"role="doc-noteref""#),
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
            result.html.contains(r#"id="fnref:b-1""#)
                && result.html.contains(r#"aria-label="脚注 1""#),
            "b(先出现)引用编号应为 1, got: {}",
            result.html
        );
        // a 的引用：id=fnref:a-1，aria-label=脚注 2
        assert!(
            result.html.contains(r#"id="fnref:a-1""#)
                && result.html.contains(r#"aria-label="脚注 2""#),
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

    #[test]
    fn render_markdown_mermaid_block_not_highlighted() {
        // mermaid 块跳过 syntect 高亮：源码应是转义纯文本，不被 <span> 包裹。
        // 前端 mermaid.ts 用 textContent 无损提取渲染成 SVG。
        let result = render_markdown_enhanced("```mermaid\ngraph LR\n    A --> B\n```");
        assert!(
            result.html.contains(r#"class="language-mermaid""#),
            "应保留 language-mermaid class, got: {}",
            result.html
        );
        // 不应被 syntect 高亮（无 text plain span）。
        assert!(
            !result.html.contains("text plain"),
            "mermaid 源码不应被 syntect 高亮, got: {}",
            result.html
        );
        // 源码内容保留（HTML 转义后）。
        assert!(result.html.contains("graph LR"));
    }
}
