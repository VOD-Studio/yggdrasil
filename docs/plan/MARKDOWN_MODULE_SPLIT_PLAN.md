## Context

`src/api/markdown.rs` 有 1642 行：约 1000 行测试，`render_markdown_enhanced`（50–408 行）单函数约 360 行。把它拆成 `src/api/markdown/` 目录，按职责划分子模块；同时把代码块与脚注的 HTML 输出分支提成子模块函数，让主循环只负责事件分派与 flush。状态变量和两遍遍历结构保持不变，不引入 `MarkdownRenderer` 结构体。验收标准：所有输入的 HTML/TOC 输出都与原实现逐字节一致。

外部 API 不变：调用方只用 `crate::api::markdown::render_markdown_enhanced` 和 `crate::api::markdown::RenderedContent`（`changelog.rs:269`、`writing_guide.rs:22,30,35`、`posts/helpers.rs:116,332,374`、`posts/outline.rs:77`、`utils/toc.rs:159`），这些调用点不用改。`src/api/mod.rs:39` 中的 `pub mod markdown;` 不用改。`src/api/comments/markdown.rs` 是独立模块，不在本次范围内。

## Approach

### 0. 先采集基线输出（改代码之前）

新建临时文件 `src/api/markdown_golden.rs`，并在 `src/api/mod.rs` 末尾加入：

```rust
#[cfg(all(test, feature = "server"))]
mod markdown_golden;
```

文件内容如下。基线采集和重构后的对比都使用这个文件，期间不要修改它：

````rust
//! 临时：Markdown 拆分前后的逐字节输出对比，验证完即删除。
use std::fmt::Write as _;

const CORPUS: &str = r##"# 标题 `code` 与 $E=mc^2$

## Tom & Jerry "quoted" <T>

### 第三级

## 脚注

正文[^a]，再次[^a]，另一个[^my note]，第三次[^a]。

[^a]: 定义 A，引用 [^b]

[^b]: 嵌套定义 B

[^my note]: 空格 label

[^unused]: 悬空定义

悬空引用[^missing]。

| 名 | 式 |
|---|---|
| a | $R_s=1$ |
| x[^b] | tail |

$$\frac{a}{b}$$

```mermaid
graph LR
    A --> B
```

```python runnable {"timeout_secs":10,"memory_mb":256}
print('hi')
```

```js runnable
console.log(1)
```

```ruby runnable
puts 'hi'
```

```rust
fn main() {}
```

    indented code

```
plain
```

![本地](/uploads/2026/06/18/abc.webp) ![外链](https://example.com/a.png)

- [ ] 未完成
- [x] 已完成

#### 深层 $a^2$
"##;

#[test]
#[ignore = "throwaway golden dump"]
fn dump() {
    let out = std::env::var("MD_GOLDEN_OUT").expect("MD_GOLDEN_OUT");
    let mut inputs: Vec<(String, String)> = vec![("corpus".into(), CORPUS.into())];
    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir("docs")
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    paths.push("CHANGELOG.md".into());
    paths.push("public/skills/yggdrasil-writing/SKILL.md".into());
    paths.sort();
    for p in paths {
        inputs.push((p.display().to_string(), std::fs::read_to_string(&p).unwrap()));
    }
    let mut dump = String::new();
    for (name, md) in inputs {
        let r = crate::api::markdown::render_markdown_enhanced(&md);
        let _ = writeln!(dump, "=== {name}\n{}\n--- toc\n{}", r.html, r.toc_html);
    }
    std::fs::write(out, dump).unwrap();
}
````

运行（工作目录为仓库根目录）：`MD_GOLDEN_OUT=/tmp/md-before.txt cargo test --locked markdown_golden -- --ignored --nocapture`。确认 `/tmp/md-before.txt` 非空，且包含 `fn-backref`、`data-runnable="true"`、`language-mermaid`、`table-wrap`、`blur-img`。

从这一步到第 4 步对比完成之前，不要修改 `CHANGELOG.md`、`docs/*.md` 和 `public/skills/yggdrasil-writing/SKILL.md`（`CHANGELOG.md` 已有用户未提交的改动，读取即可，不要动）。

### 1. 把文件改成目录模块，并移走纯辅助函数

1. 执行 `git mv src/api/markdown.rs src/api/markdown/mod.rs`，保留文件历史。仓库 `src/api` 下的目录模块统一使用 `mod.rs`，例如 `code_runner/`、`posts/`。
2. 在 `mod.rs` 中 `#![allow(clippy::unused_unit, deprecated)]` 的下一行声明子模块。整体 cfg gate 沿用 `src/api/code_runner/mod.rs:79-87` 的写法：纯 server 子模块在声明处 gate，子模块内部的条目不再逐个加 `#[cfg(feature = "server")]`。

   ```rust
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
   ```

3. 搬迁规则：函数体、文档注释和函数内部的 `use` 都原样保留，只删掉条目上的 `#[cfg(feature = "server")]`，可见性按下列说明调整。各子模块的单元测试放在文件末尾的 `#[cfg(test)] mod tests { use super::*; … }` 中，从原测试模块原样搬过来；行号以原文件为准，搬之前先重新读一遍。
   - **`heading.rs`**，模块文档写 `//! 标题：层级映射、锚点 slug 与目录（TOC）HTML。`
     - `escape_heading_text`（原 15–23 行）：保持私有。
     - `heading_tag`（原 35–46 行）：改为 `pub(super)`。
     - `generate_toc_html`（原 503–563 行）：改为 `pub(super)`。
     - `slugify_heading`（原 565–604 行）：改为 `pub(super)`。
     - 新增函数，函数体就是原 90–97 行的 match。用全路径写法，和 `heading_tag` 保持一致：
       ```rust
       /// Markdown 标题层级转数字（H1 → 1 … H6 → 6），供 TOC 层级比较。
       pub(super) fn heading_level(level: pulldown_cmark::HeadingLevel) -> u8
       ```
     - 测试：`slugify_heading_*`（原 901–936 行）、`generate_toc_html_*`（原 966–1039 行）。
   - **`footnote.rs`**，模块文档写 `//! 脚注：label → 锚点 id 与引用/定义/回链 HTML 片段。`
     - `footnote_id`（原 606–646 行）：改为 `pub(super)`。
     - 测试：`footnote_id_*`（原 1457–1520 行，带上 `// ---- footnote_id 单元测试 ----` 注释）。
   - **`post_process.rs`**，模块文档写 `//! 渲染后处理：/uploads/ 图片 blur-up 包裹与表格横向滚动容器。`
     - `wrap_images_with_blur`（原 410–420 行）：改为 `pub(super)`。
     - `wrap_images_with_blur_with`（原 422–475 行）：保持私有，只有本模块和测试用到。
     - `wrap_tables`（原 477–501 行）：改为 `pub(super)`。
     - 测试：原 652–830 行（`wrap_images_with_blur_*`、`full_pipeline_wrap_then_clean_preserves_slash`、`wrap_tables_*`、`wrap_tables_then_clean_preserves_div_and_table`）。测试模块需要多导入一行 `use crate::api::markdown::clean_html;`。
   - **`tests.rs`**，即主流程的端到端测试，顶层第一行为 `use super::*;`。参照 `src/mcp/tools/posts/tests.rs` 的写法，不再套一层 `mod tests {}`，代码整体左移 4 格。依次放入原 832–899 行（`render_markdown_table_*`、`render_markdown_heading_math_stays_inside_heading`）、938–964 行（`clean_html_*`）、1041–1455 行（`render_markdown_*` 中的代码块与脚注部分）、1522–1641 行（数学公式与 mermaid）。
   - 删掉 `mod.rs` 中原来的整个 `#[cfg(all(test, feature = "server"))] mod tests { … }` 块。
4. `mod.rs` 保留以下内容：模块文档（改成下面的文字）、`#![allow(...)]`、子模块声明、`clean_html`（原样，仍为 `pub(crate)`）、`RenderedContent`（原样）、新增的 `flush_events`（见第 2 步），以及 `render_markdown_enhanced`。
   ```rust
   //! Markdown 渲染与目录生成。
   //!
   //! 使用 pulldown-cmark 解析 Markdown，两遍遍历事件：第一遍收集标题与脚注引用，
   //! 第二遍分派输出。标题/TOC 见 `heading`，脚注见 `footnote`，围栏代码块见
   //! `code_block`，图片与表格后处理见 `post_process`；最终经 sanitizer 清理 HTML。
   //! 仅在 `feature = "server"` 时执行实际渲染。
   ```
5. 在 `render_markdown_enhanced` 内把被移走的函数改成带路径的调用：`slugify_heading` → `heading::slugify_heading`，`generate_toc_html` → `heading::generate_toc_html`，`heading_tag`（两处）→ `heading::heading_tag`，`footnote_id`（三处）→ `footnote::footnote_id`，`wrap_images_with_blur` → `post_process::wrap_images_with_blur`，`wrap_tables` → `post_process::wrap_tables`。第一遍里的标题层级 match（原 90–97 行）替换为 `current_heading = Some((heading::heading_level(*level), String::new()));`，再把函数首行的 `use` 改成 `use pulldown_cmark::{Event, Options, Tag, TagEnd};`（去掉 `HeadingLevel`）。`use std::fmt::Write as _;` 保留，标题的 `write!` 还在用。

本步检查：`cargo check --locked`。

### 2. 提取 flush 辅助函数

在 `mod.rs` 中 `render_markdown_enhanced` 之前新增：

```rust
#[cfg(feature = "server")]
/// 把累积的普通事件交给 pulldown-cmark 默认渲染器输出并清空批次（保留容量）。
fn flush_events(html: &mut String, events: &mut Vec<pulldown_cmark::Event<'_>>) {
    if !events.is_empty() {
        pulldown_cmark::html::push_html(html, events.drain(..));
    }
}
```

生命周期用 `'_`，不要写显式 `<'a>`，否则会触发 clippy `needless_lifetimes`。主循环中下面 4 处 `if !non_heading_events.is_empty() { push_html(...); non_heading_events = Vec::new(); }`，以及循环结束后的最终 flush（原 395–398 行），都替换为 `flush_events(&mut html, &mut non_heading_events);`：

- `Start(Tag::Heading)`，原 165–168 行
- `Start(Tag::CodeBlock)`，原 191–194 行
- `Start(Tag::FootnoteDefinition)`，原 327–330 行
- `End(TagEnd::FootnoteDefinition)`，原 348–351 行

每处前面的说明注释保留。`InlineMath`/`DisplayMath`/`FootnoteReference` 刻意不 flush（避免打断表格），保持原样。

### 3. 把代码块与脚注的 HTML 输出提到子模块

新建 **`code_block.rs`**，模块文档写 `//! 围栏代码块：runnable 标记解析与 `<pre><code>` 输出（mermaid / syntect 高亮 / 可运行）。`，文件顶部加 `use std::fmt::Write as _;`。

- `pub(super) fn runnable_attrs(info: &str) -> Option<(String, String)>`：函数体就是原 208–221 行闭包的内容（`parse_fence_info` → `runnable && is_supported_lang` → overrides 序列化为 JSON 并转义 → `Some((lang, overrides_escaped))`，其余情况返回 `None`）。原 206–207、212 行的注释一并移入，作为文档注释或行内注释。
- `pub(super) fn push_code_block(html: &mut String, info: Option<&str>, runnable: Option<(String, String)>, source: &str)`：原 228–272 行的逻辑，按以下方式替换变量：
  - `code_lang.as_deref()` → `info`
  - `&code_buffer` → `source`
  - `code_runnable.take()` → `runnable`
  - `if let Some(lang) = &code_lang` → `if let Some(lang) = info`
  - mermaid 分支在 `html.push_str("</code></pre>");` 之后直接 `return;`，原来的 `in_codeblock = false; continue;` 由调用方负责。
  - 原有注释全部保留。
- 主循环改动：
  - `Start(Tag::CodeBlock)` 中的赋值改为 `code_runnable = code_lang.as_deref().and_then(code_block::runnable_attrs);`，保留原 206–207 行注释的一句概要。
  - `End(TagEnd::CodeBlock)` 分支整体改为：
    ```rust
    Event::End(TagEnd::CodeBlock) => {
        code_block::push_code_block(
            &mut html,
            code_lang.as_deref(),
            code_runnable.take(),
            &code_buffer,
        );
        in_codeblock = false;
    }
    ```
  - 原代码在 mermaid 分支不会 `take()` runnable；现在统一 `take()`，结果等价：下一个代码块开始时 `code_runnable` 会被重新赋值，且 `mermaid` 不在 runner 白名单中。

在 **`footnote.rs`** 中加入 `use std::fmt::Write as _;`，并新增以下函数。格式串从原代码逐字复制，相关注释一并带上：

- `pub(super) fn reference_html(id: &str, n: usize, num: usize) -> String`：新建一个 `String`，用原 315–318 行的 `write!` 写入后返回。
- `pub(super) fn push_definition_open(html: &mut String, id: &str, num: usize)`：原 339–342 行的 `write!`，带上原 337–338 行注释。
- `pub(super) fn push_backlinks(html: &mut String, id: &str, ref_count: usize)`：原 356–372 行，包括 `if ref_count > 0` 判断和 `2..=ref_count` 循环。

主循环改动：

- `FootnoteReference` 分支：原 314–318 行改为 `let rendered = footnote::reference_html(&id, n, num);`，后面按 `in_heading` 分流的代码不变。
- `Start(FootnoteDefinition)` 分支：原 339–342 行改为 `footnote::push_definition_open(&mut html, &id, num);`。
- `End(FootnoteDefinition)` 分支：在 `if let Some(label) = fn_def_stack.pop()` 内保留 `id` 和 `ref_count` 的计算，原 356–372 行改为 `footnote::push_backlinks(&mut html, &id, ref_count);`。`html.push_str("</aside>");` 不变。

以下内容留在主函数中，不要再提取：第一遍的脚注计数状态（`fn_refs`、`fn_order`、`fn_num`、`fn_ref_seen`、`fn_def_stack`）、围栏语言提取（原 196–205 行）、标题内 `Text`/`Code` 的直写（原 376–391 行）。

### 4. 对比基线、清理并更新路径引用

1. 运行 `MD_GOLDEN_OUT=/tmp/md-after.txt cargo test --locked markdown_golden -- --ignored --nocapture`，再执行 `cmp /tmp/md-before.txt /tmp/md-after.txt`，必须无输出、退出码为 0。如有差异，用 `diff` 定位到对应的分支，按原代码修正，直到一致。
2. 删除 `src/api/markdown_golden.rs`，并删掉 `src/api/mod.rs` 中对应的两行声明。
3. 更新引用旧路径的注释和文档。`docs/code-audit-2026-10-22.md` 是带日期的审计快照，不要改。
   - `libs/tiptap-editor/src/footnote.ts:24`：`src/api/markdown.rs:74-116 的 fn_order` → `src/api/markdown/mod.rs 中 render_markdown_enhanced 的 fn_order`
   - `styles/components/article.css:293`：`src/api/markdown.rs::wrap_tables` → `src/api/markdown/post_process.rs::wrap_tables`
   - `public/skills/yggdrasil-writing/SKILL.md:213`：`` `src/api/markdown.rs` `` → `` `src/api/markdown/` ``
4. 只对本模块运行 `rustfmt --edition 2021 src/api/markdown/mod.rs`（会顺着子模块声明一起处理）。不要运行全量 `cargo fmt` 或 `dx fmt`。

## Critical files & anchors

- `src/api/markdown.rs` → `src/api/markdown/mod.rs`：`render_markdown_enhanced` 的第二遍循环（原 161–393 行），第 2、3 步的替换点都在这里。
- `src/api/code_runner/mod.rs:79-87`：在声明处整体 cfg gate 纯 server 子模块的先例。
- `src/mcp/tools/posts/tests.rs`：独立 `tests.rs` 文件的写法（顶层 `use super::*;`，没有外层 `mod tests`）。
- `src/api/mod.rs`：只在第 0、4 步临时增删 `markdown_golden` 声明，第 39 行不变。

## Verification

在仓库根目录执行，不需要数据库：

1. 第 4.1 步的 `cmp` 无差异。这一项证明重构后输出逐字节不变，覆盖脚注多次引用与嵌套、悬空引用与悬空定义、mermaid、runnable（含 overrides 和别名）、白名单外语言、表格内公式与脚注、`/uploads/` 图片和外链图片、TOC 层级跳跃，以及仓库全部 `docs/*.md`、`CHANGELOG.md`、`SKILL.md`。
2. `cargo test --locked api::markdown`：迁移后的测试全部通过，测试数量与拆分前一致（拆分前先跑一遍 `cargo test --locked api::markdown 2>&1 | grep 'test result'` 记录数量）。
3. `cargo test --locked toc` 和 `cargo test --locked outline`：外部调用方 `utils/toc.rs`、`posts/outline.rs` 的测试通过。
4. `cargo clippy --locked --all-targets --all-features -- -D warnings`：没有警告。
5. `cargo check --locked --target wasm32-unknown-unknown --no-default-features --features web`：web 目标下整个模块被 gate 掉，没有新增 dead_code 或 unused 警告。
6. `wc -l src/api/markdown/*.rs`：`mod.rs` 不超过约 330 行，其中 `render_markdown_enhanced` 不超过约 230 行。
7. 提交：`git add -A src/api/markdown.rs src/api/markdown src/api/mod.rs libs/tiptap-editor/src/footnote.ts styles/components/article.css public/skills/yggdrasil-writing/SKILL.md`。提交前用 `git diff --cached --stat` 确认 `src/api/mod.rs` 已无改动；如果仍有改动，说明临时声明没删干净，需要回到第 4.2 步。然后执行 `git commit -m "refactor(markdown): 拆分 Markdown 渲染模块"`。工作区里用户已有的其他改动（`.env.example`、`CHANGELOG.md`、`src/api/rate_limit.rs` 等）不要暂存。
