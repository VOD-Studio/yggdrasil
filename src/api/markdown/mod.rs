//! Markdown 渲染与目录生成。
//!
//! 使用 pulldown-cmark 解析 Markdown，两遍遍历事件：第一遍收集标题与脚注引用，
//! 第二遍分派输出。标题/TOC 见 `heading`，脚注见 `footnote`，围栏代码块见
//! `code_block`，图片与表格后处理见 `post_process`；最终经 sanitizer 清理 HTML。
//! 仅在 `feature = "server"` 时执行实际渲染。

#![allow(clippy::unused_unit, deprecated)]

#[cfg(feature = "server")]
mod code_block;
#[cfg(feature = "server")]
mod footnote;
#[cfg(feature = "server")]
mod heading;
#[cfg(feature = "server")]
mod post_process;
#[cfg(all(test, feature = "server"))]
mod tests;

#[cfg(feature = "server")]
/// 对外暴露的 HTML 清理函数，委托给 sanitizer 模块。
pub(crate) fn clean_html(input: &str) -> String {
    crate::api::sanitizer::clean_html(input)
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
/// 把累积的普通事件交给 pulldown-cmark 默认渲染器输出并清空批次（保留容量）。
fn flush_events(html: &mut String, events: &mut Vec<pulldown_cmark::Event<'_>>) {
    if !events.is_empty() {
        pulldown_cmark::html::push_html(html, events.drain(..));
    }
}

#[cfg(feature = "server")]
/// 增强版 Markdown 渲染：生成 TOC、标题锚点与语法高亮代码块。
pub fn render_markdown_enhanced(md: &str) -> RenderedContent {
    use pulldown_cmark::{Event, Options, Tag, TagEnd};
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
                current_heading = Some((heading::heading_level(*level), String::new()));
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
                    let id = heading::slugify_heading(&text);
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
    let toc_html = heading::generate_toc_html(&headings);

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
                flush_events(&mut html, &mut non_heading_events);
                in_heading = true;
                if heading_idx < headings.len() {
                    let (_, _, ref id) = headings[heading_idx];
                    let tag = heading::heading_tag(level);
                    // write! 直写目标 String，零中间分配（format! 会先分配临时 String 再 push_str）。
                    let _ = write!(html, "<{tag} id=\"{id}\">");
                }
            }
            Event::End(TagEnd::Heading(level)) => {
                if heading_idx < headings.len() {
                    let (_, _, ref id) = headings[heading_idx];
                    let tag = heading::heading_tag(level);
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
                flush_events(&mut html, &mut non_heading_events);
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
                // 解析围栏 info：仅「标记为 runnable 且语言受支持」时挂 data-*。
                code_runnable = code_lang.as_deref().and_then(code_block::runnable_attrs);
                code_buffer.clear();
            }
            Event::Text(text) if in_codeblock => {
                code_buffer.push_str(&text);
            }
            Event::End(TagEnd::CodeBlock) => {
                code_block::push_code_block(
                    &mut html,
                    code_lang.as_deref(),
                    code_runnable.take(),
                    &code_buffer,
                );
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
                let id = footnote::footnote_id(&label);
                // display_num：label 首次出现顺序编号；悬空引用（无 def）查不到时回退到引用序号 n。
                let num = fn_num.get(&label).copied().unwrap_or(n);
                // 同 InlineMath：并入本批，避免表格中途 flush。
                let rendered = footnote::reference_html(&id, n, num);
                if in_heading {
                    html.push_str(&rendered);
                } else {
                    non_heading_events.push(Event::InlineHtml(rendered.into()));
                }
            }
            Event::Start(Tag::FootnoteDefinition(name)) => {
                // 脚注定义开始：先刷出累积的普通事件，再用 <aside> 开启语义化容器。
                flush_events(&mut html, &mut non_heading_events);
                let label = name.to_string();
                let id = footnote::footnote_id(&label);
                let num = fn_num.get(&label).copied().unwrap_or_else(|| {
                    // 定义未被任何引用提及（悬空定义）：用 fn_order 长度+1 兜底编号。
                    fn_order.len() + 1
                });
                footnote::push_definition_open(&mut html, &id, num);
                fn_def_stack.push(label);
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                // 脚注定义结束：先刷出累积的脚注正文事件（定义内部的段落/列表等），
                // 再按引用次数输出 N 个 back-link（↩、↩²、↩³…），最后闭合 <aside>。
                flush_events(&mut html, &mut non_heading_events);
                // 弹栈取当前定义的 label，配对 Start。
                if let Some(label) = fn_def_stack.pop() {
                    let id = footnote::footnote_id(&label);
                    let ref_count = fn_refs.get(&label).copied().unwrap_or(0);
                    footnote::push_backlinks(&mut html, &id, ref_count);
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
    flush_events(&mut html, &mut non_heading_events);

    let html = post_process::wrap_images_with_blur(&html);
    // 表格外层套可滚动容器，移动端窄屏可横向滚动而不被外层 overflow-hidden 裁切。
    // sanitizer 不放行 table 的 class/style，但 div 在白名单、class 属全局属性，故包裹 div。
    let html = post_process::wrap_tables(&html);
    RenderedContent {
        html: clean_html(&html),
        toc_html,
    }
}
