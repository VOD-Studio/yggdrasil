# 两个 ⌘K 面板去重

## Context

前台搜索面板 `SearchPalette`（`src/components/search_palette.rs:438-931`）和后台命令面板 `AdminPalette`（`src/components/admin_palette.rs:177-545`）的大部分代码逐行重复：signal 集合、打开时记录焦点并聚焦输入框、关闭复位、防抖 + seq 请求、scrim/dialog/`data-ygg-modal-panel`/`trap_tab`、输入行键位、进度点、分组列表渲染、行组件、空结果和页脚结构（`docs/code-audit-2026-10-22.md:99` 记录了这一问题）。两边真正不同的只有数据源、条目、动作和文案。

目标：把公共部分抽到新模块 `src/components/palette.rs`，两个面板只保留各自的数据、条目映射、选中动作和插槽内容。**用户可见行为、DOM id、class、ARIA 和文案保持不变。**

## Approach

所有步骤只涉及一个功能点。先完成第 1 步（新模块和调用方迁移），再做第 2、3 步；第 2、3 步互不依赖。每一步完成后都应能编译。

### 1. 新建 `src/components/palette.rs`，迁入共享原语并迁移调用方

1. 从 `search_palette.rs` **原样剪切**以下内容到 `palette.rs`，文档注释一并迁入，名字不变：
   - `SearchPaletteHandle` 结构体及其 `impl`（第 15-32 行，包括 `__skipViewTransition` 调用）
   - `use_search_palette_provider`、`use_search_palette`（第 34-43 行）
   - `contains_ci`、`highlight`、`snippet`（第 128-192 行），保持 `pub(crate)`
   - `viewport_center`、`focus_by_id`（第 350-381 行），保持 `pub(crate)`
   - `use_palette_shortcut`（第 383-431 行），**改为私有 `fn`**，因为以后只有本模块的 `use_palette_state` 调用它
   - `DEBOUNCE_MS = 150`、`PROGRESS_DELAY_MS = 300`，改为本模块私有常量；同时删除 `admin_palette.rs:20-22` 中重复的两份常量
2. 将 `search_palette.rs` 测试中的 `highlight_marks_every_case_insensitive_hit` 和 `snippet_centers_on_the_first_hit_with_ellipses`（第 947-971 行）原样移到 `palette.rs` 末尾的 `#[cfg(test)] mod tests { use super::*; … }`。
3. 在 `src/components/mod.rs` 的布局层中，于 `pub mod nav;` 之后、`/// 搜索面板（⌘K）` 之前加入以下内容。这里**不加** `cfg` 门控，因为 reader 客户端的 `SearchPalette` 也会用到：
   ```rust
   /// ⌘K 面板公共部分：打开句柄、全局快捷键、文本匹配与面板外壳（前台搜索与后台命令面板共用）。
   pub mod palette;
   ```
4. 迁移调用方路径，属于干净切换，不做 re-export：
   - `src/components/admin_layout.rs:14`：`search_palette::{use_search_palette, use_search_palette_provider}` 改为 `palette::{…}`
   - `src/components/frontend_layout.rs:11`：拆成 `use crate::components::palette::use_search_palette_provider;` 和 `use crate::components::search_palette::SearchPalette;`
   - `src/components/header.rs:50`：`crate::components::search_palette::use_search_palette()` 改为 `crate::components::palette::use_search_palette()`
   - `src/pages/components_showcase.rs:1539` 和 `:1631`：`search_palette::use_search_palette_provider()` 改为 `palette::use_search_palette_provider()`。`:1549` 的 `search_palette::SearchPalette` 保持不变
   - `src/pages/search.rs:11`：`search_palette::{contains_ci, highlight, snippet}` 改为 `palette::{…}`；`:392` 中的 `search_palette::focus_by_id` 改为 `palette::focus_by_id`
   - `admin_palette.rs:10-12`、`:184`、`:219` 的旧路径会在第 3 步重写时一起处理
   - 验收：`grep -rn "search_palette::" src` 只命中 `SearchPalette` 组件的两处用法，即 `frontend_layout.rs` 和 `components_showcase.rs:1549`

### 2. 在 `palette.rs` 中新增面板状态、外壳和行组件

`palette.rs` 引入 `use dioxus::prelude::*;` 和 `use crate::components::ui::{Icon, IconName};`。新增以下内容。行为按代码实现，注释写中文：

```rust
/// 面板的公共状态：打开、输入、当前项、请求进度与焦点归还。由面板组件经
/// [`use_palette_state`] 创建，交给 [`PaletteShell`] 渲染。
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct PaletteState {
    pub(crate) open: Signal<bool>,
    pub(crate) query: Signal<String>,
    pub(crate) active: Signal<usize>,
    loading: Signal<bool>,
    show_progress: Signal<bool>,
    composing: Signal<bool>,
    seq: Signal<u64>,
    /// DOM id 前缀：输入框 `{prefix}-input`、列表 `{prefix}-list`、选项 `{prefix}-opt-{i}`。
    id_prefix: &'static str,
    /// 打开前的焦点，关闭后还回去。
    #[cfg(target_arch = "wasm32")]
    previous_focus: Signal<Option<web_sys::HtmlElement>>,
}
```

- `pub(crate) fn use_palette_state(id_prefix: &'static str, shortcut: bool) -> PaletteState`
  - 依次调用 `use_context::<SearchPaletteHandle>()`。wasm 下调用 `use_palette_shortcut(handle, shortcut)`；非 wasm 下写 `let _ = shortcut;`。然后用 `use_signal` 逐个创建各个字段，`open` 取 `handle.open`。
  - 定义一个 `use_effect`：如果 `(state.open)()` 为 false，直接 return。在 wasm 下，把 `document.active_element()` 转成 `HtmlElement` 后存入 `previous_focus`，写法与 `search_palette.rs:469-478` 相同。随后 `spawn` 一个任务，先执行 `crate::utils::time::sleep_ms(0).await`，再调用 `state.focus_input()`。
- `impl PaletteState`：
  - `pub(crate) fn focus_input(self)`：只在 wasm 下执行 `focus_by_id(&format!("{}-input", self.id_prefix))`。
  - `pub(crate) fn close(mut self) -> bool`：如果 `!*self.open.peek()`，返回 `false`。否则执行 `self.seq += 1`，把 `loading` 和 `show_progress` 设为 false，`query` 设为空串，`active` 设为 0，`open` 设为 false；在 wasm 下取出 `previous_focus` 并调用 `focus()`；最后返回 `true`。面板根据返回值决定是否继续复位自己的状态。
  - 以下方法的行为与 `search_palette.rs:520-555` 完全一致：
    ```rust
    pub(crate) fn schedule_fetch<T, F, Fut>(
        self,
        results: Signal<Vec<T>>,
        results_query: Signal<String>,
        fetch: F,
    ) where
        T: 'static,
        F: FnOnce(String) -> Fut + 'static,
        Fut: std::future::Future<Output = Option<Vec<T>>> + 'static,
    ```
    执行顺序如下：
    1. seq 加 1，并保存为 id。
    2. 取 `query.peek().trim()`。如果为空，清空 `results` 和 `results_query`，把 `loading` 和 `show_progress` 设为 false，然后 return。
    3. 否则 `spawn`：先 `sleep_ms(DEBOUNCE_MS)`，如果 seq 已经变化，或者 `composing` 为 true，就 return。
    4. 把 `loading` 设为 true；另起一个 `spawn` 等待 `PROGRESS_DELAY_MS`，如果到时 seq 未变且 `loading` 仍为 true，就把 `show_progress` 设为 true。
    5. 执行 `let res = fetch(q.clone()).await`；如果 seq 已经变化，return。
    6. `Some(items)` 时设置 `results = items`、`results_query = q`。`None` 表示失败或被限流，此时保留上次结果。
    7. 把 `loading` 和 `show_progress` 设为 false。

    异步块内部用 `let mut state = self;` 这样的 Copy 副本修改 signal。
- 展示数据：
  ```rust
  /// 面板一行的展示内容。
  #[derive(Clone, Debug, PartialEq)]
  pub(crate) struct PaletteEntry {
      pub(crate) icon: IconName,
      pub(crate) title: String,
      /// 标题下的摘要片段（只有前台文章有）。
      pub(crate) summary: Option<String>,
      /// 右侧元信息，空串不显示。
      pub(crate) meta: String,
      pub(crate) mono_meta: bool,
      /// 是否用 `<mark>` 标出命中（最近搜索、外观切换不标）。
      pub(crate) highlight_hits: bool,
  }

  #[derive(Clone, Debug, PartialEq)]
  pub(crate) struct PaletteGroup {
      pub(crate) label: &'static str,
      pub(crate) entries: Vec<PaletteEntry>,
  }
  ```
- `#[component] fn PaletteRow(id: String, entry: PaletteEntry, query: String, selected: bool, on_hover: EventHandler<()>, on_pick: EventHandler<bool>) -> Element`，保持私有。
  - 以 `search_palette.rs:884-930` 为模板。高亮查询在 `entry.highlight_hits` 为 false 时使用空串。
  - meta 的 class 在 rsx 之前绑定到局部变量：`let meta_class = if entry.mono_meta { "palette-option-meta font-mono" } else { "palette-option-meta" };`
  - `onclick` 中调用 `on_pick.call(modifiers.meta() || modifiers.ctrl())`。
  - `onmousemove`、`onmousedown` 及其注释保持原样。
- `#[component] pub(crate) fn PaletteShell(...) -> Element` 必须写成 `pub(crate)`，**不能写成 `pub fn`**。否则 `components_showcase.rs` 的 `catalog_covers_public_components_and_has_unique_routes` 会要求把它收录进图鉴。props 如下：
  ```rust
  state: PaletteState,
  /// 对话框 aria-label，同时作为 trap_tab 的名称（"搜索" / "命令面板"）
  label: &'static str,
  input_label: &'static str,
  placeholder: &'static str,
  list_label: &'static str,
  /// 页脚 ↵ 的说明（"打开" / "执行"）
  enter_hint: &'static str,
  empty_hint: &'static str,
  groups: Vec<PaletteGroup>,
  /// 远端结果对应的查询与当前输入不同（与 loading 同时为真时不显示空结果）
  waiting: bool,
  #[props(default)] clearable: bool,
  /// 输入行与列表之间的插槽（前台范围标签）
  toolbar: Element,
  /// 空结果提示下方的插槽（前台「在全文中搜索」）
  empty_action: Element,
  /// 页脚快捷键说明右侧的插槽（前台「全文搜索」）
  footer_action: Element,
  on_query_change: EventHandler<()>,
  /// (扁平序号, 是否按住 ⌘/Ctrl)
  on_pick: EventHandler<(usize, bool)>,
  on_close: EventHandler<()>,
  ```
  - 计算：`q = (state.query)()`；`has_query = !q.trim().is_empty()`；`total` 为所有组 `entries.len()` 之和；`active_idx` 在 `total == 0` 时为 0，否则为 `active().min(total - 1)`；`no_results = has_query && total == 0 && !(waiting && (state.loading)())`。
  - 外壳不做 `open` 判断。只有面板处于打开状态时才会渲染外壳。
  - rsx 以 `search_palette.rs:624-823` 为模板，并做以下替换：
    - 文案和 id 字面量换成对应 props，以及 `"{prefix}-input"`、`"{prefix}-list"`、`format!("{prefix}-opt-{active_idx}")`。
    - scrim 的 `onclick`、Esc 按钮，以及 Escape 键在查询为空时，都调用 `on_close.call(())`。
    - 以下三处都执行 `query.set(String::new()); active.set(0); on_query_change.call(())`：Escape 键在查询非空时、`oninput`（保留 `!composing()` 判断，并先设置 query 和 active）、`oncompositionend`。清除按钮也执行这三步，然后再调用 `state.focus_input()`。
    - `Enter` 键：仅在 `total > 0` 时执行 `on_pick.call((active_idx, modifiers.meta() || modifiers.ctrl()))`。
    - 清除按钮只在 `clearable && has_query` 时渲染。
    - 在输入行之后渲染 `{toolbar}`。
    - 列表中，每一行渲染为 `PaletteRow { key: "{i}", id: format!("{prefix}-opt-{i}"), entry, query: q.clone(), selected: i == active_idx, on_hover: move |_| active.set(i), on_pick: move |new_tab: bool| on_pick.call((i, new_tab)) }`。
    - 空结果块固定渲染图标 `SearchX`、`"没有找到「{q.trim()}」"`、`"{empty_hint}"`，然后渲染 `{empty_action}`。
    - 页脚依次渲染 `↑↓ 选择`、`↵ {enter_hint}`、`Esc 关闭`，然后渲染 `{footer_action}`。
    - IME 判断、`Key::Process` 及所有原注释保留。

### 3. 改写两个面板，让它们只保留各自的数据、条目映射和动作

**`search_palette.rs`**：保留 `Scope`、`Item`、`Group`、`PAGES`、`RECENT_*`、`page_target`、`group_limit`、`build_groups`、`scope_count`、`read_recents`、`remember`、`write_recents` 及其余测试。删除 `PaletteRow`，删除已经迁走的项。

- imports 改为：`crate::components::palette::{contains_ci, snippet, use_palette_state, viewport_center, PaletteEntry, PaletteGroup, PaletteShell}`。原有的 `Icon`、`IconName`、`Route`、`theme`、`api` 导入保留。
- 新增 `fn entry(item: &Item, query: &str) -> PaletteEntry`，把原 `PaletteRow` 的 `match` 搬过来：
  - Post：图标 `FileText`；如果摘要非空，`summary` 为 `snippet(s, query, 18)`；meta 为 `%Y.%m.%d`；`mono_meta: true`。
  - Tag：图标 `Tag`；meta 为 `"{count} 篇"`；`mono_meta: false`。
  - Page：图标 `CornerDownLeft`；meta 为路径；`mono_meta: true`。
  - Recent：图标 `RotateCcwClock`；meta 为空串；`mono_meta: false`。
  - Theme：图标 `Moon`；meta 为 `"外观"`；`mono_meta: false`。
  - `highlight_hits` 只在 Recent 和 Theme 上为 false。除 Post 外，`summary` 都是 `None`。
- `SearchPalette`：
  - 函数签名不变。
  - 先写 `let palette = use_palette_state("palette", shortcut);`，再按需绑定 `palette.open`、`palette.query`、`palette.active`。只给确实需要 `.set` 的 signal 加 `mut`，否则 clippy 会报 `unused_mut`。
  - 保留 `scope`、`posts`、`posts_query`、`tags`、`recents`、`theme`、`resolved`、`navigator`。
  - 保留一个 `use_effect`，只负责 `recents.set(read_recents())`，以及在 tags 为空时首次加载标签（`list_tags`，并过滤掉 `post_count > 0` 以外的标签）。焦点相关逻辑已经移到 `use_palette_state`。
  - `close` 写成 `move || { if palette.close() { scope.set(Scope::All); posts.set(Vec::new()); posts_query.set(String::new()); } }`。
  - `schedule_fetch` 写成 `move || palette.schedule_fetch(posts, posts_query, |q| async move { suggest_posts(q).await.ok().map(|res| res.posts) })`，并保留“失败或被限流时保留上一次结果”的注释。
  - `if !open() { return rsx! {}; }` 保留。
  - `build_groups` 调用保持不变。接着生成 `flat: Vec<Item>`，再生成 `view: Vec<PaletteGroup>`，其中每个 entry 由 `entry(item, &q)` 构造。
  - `activate(item, new_tab)` 的分支全部保持原样。
  - 在 rsx 之前分别绑定 `let empty_query = q.trim().to_string(); let footer_query = empty_query.clone();`，供两个按钮的闭包各自 move。
  - 渲染以下组件：
    ```
    PaletteShell { state: palette, label: "搜索", input_label: "搜索文章、标签或页面", placeholder: "搜索文章、标签，或跳转到页面…", list_label: "搜索结果", enter_hint: "打开", empty_hint: "标题、摘要与标签里都没有命中，试试更短的关键词，或去全文搜索。", groups: view, waiting: posts_query() != q.trim(), clearable: true, on_query_change: move |_| schedule_fetch(), on_pick: move |(index, new_tab): (usize, bool)| { if let Some(item) = flat.get(index).cloned() { activate(item, new_tab); } }, on_close: move |_| close(), toolbar: rsx!{ <原 palette-scopes div，点击时调用 palette.focus_input()，不再用 focus_by_id> }, empty_action: rsx!{ <原「在全文中搜索」按钮，使用 empty_query> }, footer_action: rsx!{ if has_query { <原 palette-full-text 按钮，使用 footer_query> } } }
    ```
- 模块文档第 6 行后补一句：`公共状态、输入与对话框结构见 [crate::components::palette]。`

**`admin_palette.rs`**：保留 `POST_LIMIT`、`Action`、`jump_targets`、`AdminPaletteDemo`、`Item`、`Group`、`Item::title`、`build_groups` 及测试。删除 `AdminPaletteRow` 和重复常量。

- imports 改为：`crate::components::palette::{contains_ci, use_palette_state, viewport_center, PaletteEntry, PaletteGroup, PaletteShell}`、`crate::components::ui::IconName`。`Icon` 组件已经不用，删除它的导入。
- 新增 `fn entry(item: &Item) -> PaletteEntry`，搬运 `AdminPaletteRow` 的 `match`：
  - Post：图标 `FileText`；meta 在草稿状态下为 `"草稿"`，否则为 `%Y.%m.%d`。
  - Action：`icon()` 和 `label()`；meta 为空串。
  - Jump：使用自带的 icon 和 label；meta 为 `"跳转"`。
  - 所有条目都使用 `summary: None, mono_meta: false, highlight_hits: true`。
- `AdminPalette`：
  - 函数签名不变。
  - 写 `let palette = use_palette_state("admin-palette", demo.is_none());`。
  - 保留 `demo_pick`、`demo_posts`、`posts`、`posts_query`、`theme`、`resolved`、`navigator`。
  - `close` 写成 `move || { if palette.close() { posts.set(Vec::new()); posts_query.set(String::new()); } }`。
  - `schedule_fetch` 写成 `move || palette.schedule_fetch(posts, posts_query, move |q| async move { … })`。在 async 块里，先单独写一条语句计算 `let demo_hits = demo_posts.peek().as_ref().map(|all| all.iter().filter(|p| contains_ci(&p.title, &q)).take(POST_LIMIT as usize).cloned().collect::<Vec<_>>());`，这样 peek guard 不会跨过 `.await`。如果 `demo_hits.is_some()`，返回 `demo_hits`；否则执行 `list_posts(1, POST_LIMIT, AdminPostQuery::search(q)).await.ok().map(|res| res.posts)`。
  - 图鉴演示改走同一条请求路径后，`loading` 会在同一次 poll 内先置 true、再置 false，不会产生可见差异。这一点可以接受。
  - `if !open() { return rsx! {}; }` 保留。生成 `groups`、`flat` 和 `view` 的方式与前台相同。
  - `activate(item)` 保持原样，包括开头的 `close()` 和 demo 分支。
  - 渲染以下组件：
    ```
    PaletteShell { state: palette, label: "命令面板", input_label: "搜索文章或输入命令", placeholder: "搜索文章，或输入命令…", list_label: "命令与搜索结果", enter_hint: "执行", empty_hint: "文章标题、命令与栏目里都没有命中。", groups: view, waiting: posts_query() != q.trim(), on_query_change: move |_| schedule_fetch(), on_pick: move |(index, _): (usize, bool)| { if let Some(item) = flat.get(index).cloned() { activate(item); } }, on_close: move |_| close(), toolbar: rsx! {}, empty_action: rsx! {}, footer_action: rsx! {} }
    ```
    `clearable` 使用默认值 false，与后台当前没有清除按钮的行为一致。
- 模块文档第 3 行改为：`输入、防抖请求、键盘选择与对话框结构由 [crate::components::palette::PaletteShell] 提供；数据与`，第 4 行开头保持“动作是后台自己的。”，语义上续接上一行。

### 4. 提交

验证通过后，只暂存本次改动的文件。工作区里已有用户未提交的改动，不能用 `-A`：

```
git add src/components/palette.rs src/components/search_palette.rs src/components/admin_palette.rs src/components/mod.rs src/components/admin_layout.rs src/components/frontend_layout.rs src/components/header.rs src/pages/components_showcase.rs src/pages/search.rs
git commit -m "refactor(palette): 前台搜索与后台命令面板共用面板外壳"
```

## Critical files & anchors

- `src/components/search_palette.rs:438-931`：前台面板主体和 `PaletteRow`。外壳 rsx 以这里为模板，因为它比后台多出清除按钮和插槽位置。
- `src/components/admin_palette.rs:177-545`：后台面板主体。需要特别注意的差异包括 demo 分支、Enter 不区分 new_tab、没有清除按钮，以及 id 前缀为 `admin-palette`。
- `src/pages/components_showcase.rs:1725-1778`：图鉴覆盖测试会扫描所有 `pub fn 大写名`。新组件必须写成 `pub(crate)`。
- `src/components/mod.rs:34-54`：布局层模块声明。`palette` 不能加 reader 门控。

## Verification

工作目录为 `/Users/issuser/Developer/xfy/yggdrasil`。

1. 单元测试：
   - `cargo test --locked palette`：确认 `palette::tests`（highlight、snippet）、`search_palette::tests` 和 `admin_palette::tests` 全部通过。
   - `cargo test --locked catalog_covers_public_components`：确认图鉴覆盖测试没有被 `PaletteShell` 打破。
2. Lint：
   - `cargo clippy --locked --all-targets --all-features -- -D warnings`
   - `cargo clippy --locked --target wasm32-unknown-unknown --no-default-features --features web -- -D warnings`
   - `cargo clippy --locked --target wasm32-unknown-unknown --no-default-features --features web,client-reader -- -D warnings`

   reader 版本没有 `admin_palette`，这一项用于确认 `palette.rs` 在 reader 中可以独立编译。
3. 引用检查：`grep -rn "search_palette::" src` 只命中 `SearchPalette` 的两处用法。
4. 浏览器冒烟测试：使用用户已经启动的 dx serve（`:8080`，不要自己运行 `make dev`）。先等 `target/dx/yggdrasil/debug/web/server-*` 的 mtime 晚于本次改动；页面使用 `.env` 中 `APP_BASE_URL` 的同源地址 `http://localhost:8080`，不要混用 `127.0.0.1`；禁用缓存后重新加载。
   - 前台 `/`：
     - 按 Meta+K 后出现 `.palette[role=dialog][aria-label=搜索]`，`document.activeElement.id === "palette-input"`。
     - 空查询时显示“跳转”组，最后一项是外观切换。
     - 输入一个站内已有文章标题中的词，出现“文章”组，`#palette-opt-0[aria-selected=true]`；按 ArrowDown 后变为 `#palette-opt-1` 被选中。
     - 存在 `button[aria-label=清除输入]`。
     - 点击“标签”范围后，焦点回到 `#palette-input`。
     - 按 Escape 后输入被清空；再按一次 Escape，面板关闭，焦点回到打开前的元素。
     - 输入 `zzzzqq` 并等待约 500ms，出现“没有找到「zzzzqq」”和“在全文中搜索”按钮；点击后跳到 `/search?q=zzzzqq`。
   - 后台：用 `.env` 中的管理员账号登录，进入 `/admin`。
     - 按 Meta+K 后，焦点在 `#admin-palette-input`，页面上不存在“清除输入”按钮。
     - 输入“写”后，“操作”组包含“写文章”。
     - 按 Enter 后跳到 `/admin/write`，面板关闭。
   - 图鉴 `/about/components/admin-palette`：
     - 点击“打开命令面板”，输入 `Rust` 后出现“用 Rust 写一个 6502 模拟器”。
     - 按 Enter 后出现文本 `on_pick: 用 Rust 写一个 6502 模拟器`。
     - 期间没有发往文章列表 server fn 的网络请求。
   - 用完后关闭测试 tab；如果为了登录创建了临时会话，也一并清理。

## Assumptions & contingencies

- 函数名 `SearchPaletteHandle`、`use_search_palette_provider` 和 `use_search_palette` 保持不变，只更改模块路径。这样可以缩小改动面，图鉴 JSON 里的用法代码也不用修改。
- 如果 Dioxus 不能把 `on_pick: move |(index, new_tab): (usize, bool)| …` 推断成 `EventHandler<(usize, bool)>`，改用 `on_pick: move |pick: (usize, bool)| { let (index, new_tab) = pick; … }`。
- 如果 rsx 中 `toolbar: rsx! { … }` 这类嵌套插槽出现宏级借用或 move 报错，就先在 rsx 之前用 `let toolbar = rsx! { … };` 等绑定好，再以 `toolbar: toolbar` 传入。仓库中已有在 render 区预先绑定的写法。
- 如果第 4 步浏览器冒烟测试时 dx 没有自动重建（server-* 的 mtime 不更新），只汇报静态验证结果，并说明需要用户重启 dx serve。不要自己启动服务器。
