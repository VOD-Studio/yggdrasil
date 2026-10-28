# v1.0 收口：门禁、原语统一、设计细节与 1.0.0 发布

## Context

v1.0 重设计分支（`v1.0`）有四类未收口问题：`cargo fmt --check` 在 `src/pages/components_showcase/comment_previews.rs:181` 失败；`Cargo.toml` 仍是 `0.24.2`、`CHANGELOG.md` 没有 1.0 发布条目（关于页经 `latest_release()` 读 CHANGELOG、仪表盘读 `CARGO_PKG_VERSION`，都显示旧版本）；方案 `docs/v1.0-重设计方案.md` §组件原语要求的 `SegmentedControl`（胶囊轨道 + 滑块，`@container` 窄宽只留图标）与 `Composer`（`field-sizing: content`、IME 守卫、Clay 双环焦点）没有统一成原语和图鉴条目；以及四处细节偏差：手机搜索面板显示 `Esc` 而非「取消」、面板关闭直接卸载没有 120ms 淡出、`styles/pages/showcase.css:286` 的 `transition: all`、高分屏 0.5px 发丝线 token 未实现。终态：上述全部落地，`make lint`/`make test`/两个 WASM clippy 全绿，版本 1.0.0。

## 前置条件（执行前检查）

工作树里有进行中的 ⌘K 面板去重（`docs/plan/PALETTE_DEDUP_PLAN.md`）：未跟踪的 `src/components/palette.rs`，以及已修改的 `search_palette.rs`、`admin_palette.rs`、`admin_layout.rs`、`frontend_layout.rs`、`header.rs`、`mod.rs`、`components_showcase.rs`、`search.rs`。本计划改动的文件与它重叠，提交不能混入别人的改动。

- 执行第一步前跑 `git status --short`。若这些文件已提交（`src/components/palette.rs` 已被跟踪且工作树干净），直接开始。
- 若仍未提交：先用 `ask` 向用户确认去重是否已完成、可否由本次执行代为提交（提交信息 `refactor(palette): 前台搜索与后台命令面板共用外壳`）。用户未答复前不得开始任何会改到上述文件的步骤。
- 下文第 5 步按去重后的结构写（`PaletteShell`/`PaletteState` 在 `src/components/palette.rs`）。若用户回复去重已放弃并回退，把第 5 步对 `palette.rs` 的改动原样落到 `search_palette.rs` 与 `admin_palette.rs` 各自的外壳与 `close` 里。

## Approach

每步完成、验证后单独提交（`type(scope): 中文摘要`），只含本步改动。步骤顺序：1 → 2 → 3 → 4 → 5 → 6；3、4、5 互不依赖，但都要在 2 之后（新写 CSS 一律用 `var(--hairline)` 表示发丝线宽度，不写 `1px`）。

### 1. rustfmt 门禁 — `style(showcase): 修正 rustfmt 格式`

- 运行 `cargo fmt`。预期只改 `src/pages/components_showcase/comment_previews.rs:181`（去掉 `.to_string(),` 的尾逗号）。若 `cargo fmt` 还改了别的文件，那是去重工作的残留，按「前置条件」处理，不并入本提交。
- 检查：`cargo fmt --check` 无输出。

### 2. 高分屏发丝线 token — `feat(ui): 发丝线高分屏 0.5px`

1. `styles/theme.css`：在 `@theme static { … }` 之后、现有 `@layer base { .dark { … } }` 之前加：
   ```css
   /* 发丝线宽度：普通屏 1px，2 倍及以上高分屏 0.5px（一个物理像素）。颜色用 --color-hairline。 */
   @layer base {
     :root {
       --hairline: 1px;
     }

     @media (min-resolution: 2dppx) {
       :root {
         --hairline: 0.5px;
       }
     }
   }
   ```
2. 批量替换：只改「同一行含 `var(--color-hairline)` 或 `var(--sc-line)`」的行，把其中独立的 `-1px` 换成 `calc(-1 * var(--hairline))`、独立的 `1px` 换成 `var(--hairline)`。范围是 `styles/` 下全部 `.css`（含 `theme.css` 里的 `--shadow-popover`）、`libs/tiptap-editor/src/style.css`、`libs/codemirror-editor/src/themes.ts`。可直接执行：
   ```sh
   perl -pi -e 'if (/var\(--color-hairline\)|var\(--sc-line\)/) { s/(?<![\w.-])-1px\b/calc(-1 * var(--hairline))/g; s/(?<![\w.-])1px\b/var(--hairline)/g }' $(git ls-files 'styles/*.css' 'styles/**/*.css') libs/tiptap-editor/src/style.css libs/codemirror-editor/src/themes.ts
   ```
   已核对：含发丝线颜色、又有多个 `1px` 的行只有 `article.css:613` 和 `showcase.css:690`，每一层都是发丝线，可以一起替换。
3. 手工改发丝线分隔条的尺寸（这些规则的颜色和尺寸不在同一行）：同一规则里的 `height: 1px` / `width: 1px` / `gap: 1px` 改为 `var(--hairline)`：
   - `styles/components/ui.css`：`.section-label::after`（约 969 行）、`.menu-separator`（约 1226 行）
   - `styles/components/article.css`：`.md-content hr`（约 285 行）
   - `styles/components/admin.css`：折叠侧栏的 `.admin-nav-label` 分隔（约 410-417 行，`:is(…collapsed…) .admin-nav-label` 规则里的 `height: 1px`；它上方 sr-only 规则里的 `width/height: 1px` 不改）、`.batch-bar-sep`（约 842 行，只改 `width`）
   - `styles/pages/about.css`：`.about-facts` 的 `gap: 1px`（约 138 行）
   - `styles/pages/home.css`：`.home-month-label::after`（约 240 行）
   - `styles/pages/writing-guide.css`：`.writing-step-line`（约 68 行，`width`）
   - `styles/pages/admin-dashboard.css`：`.dash-chart-grid > span`（约 168 行）
   - `styles/pages/admin-preview.css`：`.preview-bar-sep`（约 56 行，`width`）
   - `styles/pages/admin-write.css`：`.write-rule`（约 244 行）
   - `styles/pages/notes-public.css`：`.notes-day-head::after`（约 175 行）
   - `libs/tiptap-editor/src/style.css`：`.tiptap-editor hr`（约 183 行）、`.footnote-definition::before`（约 1282 行）
4. `src/components/header.rs:154` 的 `div { class: "my-3 mx-1 h-px bg-hairline" }` 改为 `h-(--hairline)`（Tailwind v4 变量简写）。`src/theme.rs` 的 `THEME_SEGMENT_STYLE` 会在第 3 步整体重写，这里不改。
5. 检查：`grep -rnP "(?<![\w.-])-?1px\b[^;]*var\(--(color-hairline|sc-line)\)" styles libs/tiptap-editor/src libs/codemirror-editor/src` 无命中（`src/theme.rs` 的内联样式在第 3 步改完后才纳入同一 grep）；`make css` 成功；`cd libs && bun run biome check . && bun run typecheck` 通过。

### 3. 统一 `SegmentedControl` — `feat(ui): 统一分段控件原语`

**3.1 新原语** `src/components/ui/segmented.rs`（`src/components/ui.rs` 加 `mod segmented;` 与 `pub use segmented::*;`）。`FilterTabs`、`neighbor_tab`、`TAB_GROUP_ID` 从 `src/components/ui/navigation.rs:104-239` 删除，不保留别名。

```rust
/// 分段控件的一项：值、文字，可选图标与数量。
#[derive(Clone, PartialEq)]
pub struct SegmentedItem<T> {
    pub value: T,
    pub label: String,
    pub icon: Option<IconName>,
    pub count: Option<i64>,
}
impl<T> SegmentedItem<T> {
    pub fn new(value: T, label: impl Into<String>) -> Self; // icon、count 为 None
    pub fn icon(self, icon: IconName) -> Self;
    pub fn count(self, count: i64) -> Self;
}

/// 语义：Tabs = tablist/tab + aria-selected（切换视图、筛选）；Radio = radiogroup/radio + aria-checked（设置取值）。
#[derive(Clone, Copy, PartialEq, Default)]
pub enum SegmentedKind { #[default] Tabs, Radio }

#[derive(Clone, Copy, PartialEq, Default)]
pub enum SegmentedSize { #[default] Md, Sm }

#[component]
pub fn SegmentedControl<T: Clone + PartialEq + 'static>(
    items: Vec<SegmentedItem<T>>,
    value: T,
    on_change: EventHandler<T>,
    aria_label: &'static str,
    #[props(default)] kind: SegmentedKind,
    #[props(default)] size: SegmentedSize,
    /// 只显示图标，文字作为 aria-label 与 title（页头这类固定窄位）。
    #[props(default)] icon_only: bool,
    /// 外层容器附加类（间距、页面变体）。
    #[props(default)] class: &'static str,
) -> Element
```

行为（照搬 `FilterTabs` 的键盘与 roving tabindex，改测量方式）：
- DOM：`div.segmented-wrap` 加修饰类（`segmented-sm`（size Sm）、`segmented-icon-only`（icon_only）、`segmented-adaptive`（`!icon_only` 且至少一项有图标））再加 `class`；内层 `div.segmented[role=tablist|radiogroup][aria-label]["data-ready"="true" 仅在已测量时]`；首个子元素 `span.segmented-thumb[aria-hidden=true][style]`；每项 `button#seg-{group}-{index}.segmented-item[type=button][role=tab|radio]`。Tabs 写 `aria-selected`，Radio 写 `aria-checked`，另一个属性不输出；当前项另加 `data-active="true"`；`tabindex` 当前项 `0`、其余 `-1`；`icon_only` 时加 `aria-label` 与 `title`（值为 label）。内容依次为：有图标时 `Icon { name, size: 14 }`；`!icon_only` 时 `span.segmented-label { label }`；有 count 时 `span.segmented-count { n }`。`group` 来自模块内 `static SEGMENTED_ID: AtomicUsize`，用 `use_hook` 取一次。
- 匹配：当前项为第一个满足 `item.value == value` 的下标；没有匹配时没有当前项，键盘从第 0 项起算。
- 键盘（在 `.segmented` 上）：←/→ 循环到相邻项、Home/End 跳到首尾；`prevent_default`，`on_change.call(新项 value.clone())`，并聚焦 `#seg-{group}-{新下标}`（wasm）。点击项调用 `on_change`。
- 滑块测量（仅 wasm）：`.segmented` 的 `onmounted` 把元素存进 `use_hook` 持有的 `Rc<RefCell<…>>`，同时创建 `web_sys::ResizeObserver` 观察它，回调里重新测量；`use_drop` 里 `disconnect()`，写法照 `src/components/palette.rs` 的 `use_palette_shortcut`（`use_hook` 持 Closure，`use_drop` 清理）。另用 `use_effect(use_reactive((&active_index, &count_key), …))`（`count_key` 是各项 count 拼成的字符串）在 `sleep_ms(30)` 后测量。测量：在轨道里 `query_selector("[data-active]")` 转成 `HtmlElement`，宽度为 0（控件被隐藏）或找不到时把 thumb 状态设为 `None`；否则写 `transform: translate({offset_left}px, {offset_top}px); width: {offset_width}px; height: {offset_height}px; opacity: 1;`。状态为 `None` 时 style 为 `opacity: 0;`，且不输出 `data-ready`。`Cargo.toml` 的 web-sys features 里加 `"ResizeObserver"`。
- 服务端渲染始终为未测量态，与 hydration 首帧一致。

**3.2 CSS**（`styles/components/ui.css` 现有 `.segmented*` 段，约 885-951 行与 1110-1126 行）：
- `.segmented-thumb`：去掉 `top/bottom: 2px`，改 `top: 0; left: 0;`；`transition` 增加 `height`（与 `width` 同一条弹簧曲线）。
- `.segmented-item[aria-selected="true"]` 改为 `.segmented-item[data-active]`；未测量兜底 `.segmented:not([data-ready]) > .segmented-item[data-active]`（浮起底色不变）。
- 新增：
  ```css
  .segmented-sm .segmented-item { height: 1.75rem; padding: 0 0.75rem; font-size: 0.78125rem; }
  .segmented-icon-only .segmented-item { justify-content: center; min-width: 1.75rem; padding: 0 0.4375rem; }
  /* 带图标的分段控件：容器窄于 16rem 时只留图标，文字仍供读屏。 */
  .segmented-adaptive { container: segmented / inline-size; }
  @container segmented (max-width: 16rem) {
    .segmented-item:has(.segmented-label) { justify-content: center; min-width: 1.75rem; padding-inline: 0.4375rem; }
    .segmented-label { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  }
  ```
  `segmented-adaptive` 对宽度有尺寸包含，放进 flex 行里时，调用方必须给容器宽度（见 3.4 的 `.sheet-row`）。

**3.3 迁移 `FilterTabs` 调用点（15 处，全部 `kind` 默认 Tabs，保留 `role=tab`，现有浏览器脚本的 `getByRole('tab')` 不受影响）。** 统一改写：`FilterTabs` → `SegmentedControl`；`items: vec![("v", "文字"), …]` → `vec![SegmentedItem::new("v".to_string(), "文字"), …]`；`active_value` → `value`；`aria_label: Some("x")` → `aria_label: "x"`；`class: ""` 删除；`on_change` 不变（参数为 `String`）。`counts` 改成逐项 `.count(n)`。逐点说明：
- `src/pages/search.rs:331`、`src/pages/admin/posts.rs:365`、`admin/notes.rs:188`、`admin/note_editor.rs:284`、`admin/system/mod.rs:171`、`src/pages/components_showcase/business_previews.rs:190 / 383 / 635`：机械改写。
- `src/pages/admin/comments.rs:197` 与 `admin/assets.rs:277`：原 `counts` 只在数据加载后传，改为加载后给每项链 `.count(n)`，数量顺序与原 `vec![…]` 一致。
- `src/pages/admin/runner.rs:78`：`SUPPORTED_LANGS.iter().map(|(v, l)| SegmentedItem::new(v.to_string(), *l)).collect()`。
- `src/pages/writing_guide.rs:179`：值仍用原来的 `["0","1","2"][i]` 字符串，只换类型构造。
- `src/pages/components_showcase.rs:803/814`（详情页「预览状态」工具栏）：机械改写，`aria_label: "预览状态"`。
- `src/pages/components_showcase.rs:1317` 的 `"FilterTabs"` 预览分支：见 3.6。
- 删掉 `FilterTabs` 的 import（`components_showcase.rs:21`、`business_previews.rs:14` 等），改为导入 `SegmentedControl, SegmentedItem`。验收：`grep -rn "FilterTabs\|filter-tabs" src scripts` 无命中。

**3.4 主题切换改用原语**（`src/theme.rs:342-391`）：
- `ThemeToggle` 的 prop 由 `labels: bool` 改为 `#[props(default)] icon_only: bool`。实现：外层 `div.theme-seg`，带 `onpointerdown`（wasm 下记录 `e.client_coordinates()` 到 `use_signal(|| None::<(f64, f64)>)`）；里面是 `SegmentedControl<Theme>`，`kind: Radio`、`size: Sm`、`aria_label: "外观"`、`icon_only`，items 依次为 Light（`IconName::Sun`）、Dark（`Moon`）、System（`Monitor`），文字用 `mode_label`。`on_change` 时取出记录的坐标，没有（键盘操作）就用 `crate::components::palette::viewport_center()`，然后调用 `choose_theme(theme, mode, origin)`，最后把坐标信号清空。
- 调用点：`frontend_layout.rs:102` 的 `ThemeToggle {}` → `ThemeToggle { icon_only: true }`；`header.rs:157` 的 `ThemeToggle { labels: true }` → `ThemeToggle {}`；`admin_layout.rs:792` 的 `ThemeToggle { labels: drawer }` → `ThemeToggle {}`（侧栏 200–320px，由 `@container` 决定是否显示文字；`drawer` prop 其他地方仍在用，保留）。
- `THEME_SEGMENT_STYLE` 整体替换。它是 SSR 内联、不进 layer 的样式，所以优先级高于 `ui.css` 的兜底规则。新规则：先在未测量时取消 SSR 默认选中项（System）的浮起；再按 `data-theme-mode` 浮起对应位置的按钮。按钮顺序与 items 顺序耦合，注释要写明：
  ```css
  .theme-seg .segmented:not([data-ready]) > .segmented-item[data-active] {
      background-color: transparent; box-shadow: none; color: var(--color-fg-2); font-weight: 400;
  }
  html[data-theme-mode="light"] .theme-seg .segmented:not([data-ready]) > button:nth-of-type(1),
  html[data-theme-mode="dark"] .theme-seg .segmented:not([data-ready]) > button:nth-of-type(2),
  html[data-theme-mode="system"] .theme-seg .segmented:not([data-ready]) > button:nth-of-type(3),
  html:not([data-theme-mode]) .theme-seg .segmented:not([data-ready]) > button:nth-of-type(3) {
      background-color: var(--color-surface-2); color: var(--color-fg); font-weight: 500;
      box-shadow: 0 0 0 var(--hairline) var(--color-hairline), var(--shadow-sm);
  }
  ```
  另有一组 `html.dark[...]` 选择器，同样四条，只把 `background-color` 换成 `var(--color-surface-3)`。同步改 `ThemeToggle`/`ThemePreload` 的文档注释。
- `styles/components/shell.css:186-223`：删除 `.theme-seg`、`.theme-seg-item*` 规则；保留 `@media (max-width: 767px)` 里的 `.site-actions > .theme-seg { display: none }`。在抽屉段（`.sheet-row` 附近，约 373 行）加 `.sheet-row > .theme-seg { flex: 1; min-width: 0; }` 和 `.sheet-row .segmented-wrap { display: flex; justify-content: flex-end; }`。
- `scripts/test-view-transitions.ts:215`：选择器改为 ``[data-vt-shell="frontend-header"] .theme-seg [aria-label="${target === 'dark' ? '深色' : '浅色'}"]``。

**3.5 手写的 `.segmented` radiogroup 改用原语**（全部 `kind: Radio, size: Sm`）：
- `src/pages/admin/settings/image_section.rs:79`（0–6，`u32`）：`class: "settings-segmented-square"`、`aria_label: "编码方法"`，`on_change: move |n| draft.write().webp.method = n`。
- `settings/security_section.rs:59`（0–3，`aria_label: "可信代理段数"`）、`settings/upload_section.rs:30`（1–8，`aria_label: "上传并发数"`）：值类型与原循环变量一致，文字为 `n.to_string()`。
- `src/pages/admin/system/export.rs:41-58`：删除 `segmented` 闭包，`:68`、`:104` 两处直接写 `SegmentedControl`（值为 `String`，与原 `mode`/`format` 信号一致）。
- `src/pages/admin/mcp/token_editor.rs:269-282`：`SegmentedControl<TokenLifetime>`，items 来自 `LIFETIMES`，`aria_label: "有效期"`。外层 `.mcp-lifetime` 保留；`admin-mcp.css:307` 的 `.mcp-lifetime .segmented { flex-wrap: wrap; }` 保留，滑块按 `offset_top` 定位，换行也对得上。
- `src/pages/admin/preview.rs:183-194`：`SegmentedControl<bool>`，`(false, "桌面")`、`(true, "手机")`，`aria_label: "预览宽度"`。
- CSS 清理：`styles/pages/admin-settings.css:192-216` 删除 `.settings-segmented` 的 `aria-checked` 规则和尺寸规则，`.settings-segmented-square` 改为：
  ```css
  .settings-segmented-square .segmented { border-radius: 8px; }
  .settings-segmented-square .segmented-thumb { border-radius: 6px; }
  .settings-segmented-square .segmented-item { justify-content: center; min-width: 1.875rem; padding: 0 0.5rem; border-radius: 6px; font-family: var(--font-mono); }
  ```
  `styles/pages/admin-preview.css:70-85` 删除 `.preview-width` 规则。验收：`grep -rn 'class: "segmented\|settings-segmented"\|preview-width\|theme-seg-item' src styles` 无命中。
- 图鉴提示卡片的状态切换器 `src/pages/components_showcase.rs:312-345`（`.showcase-alert-switcher`，语义是 `tablist`，却用了 `aria-pressed`）：换成 `SegmentedControl`（Tabs，items 成功/错误/普通，值为 `"success"/"error"/"default"` 字符串，`aria_label: "切换提示状态"`），`on_change` 同时设置 `alert_variant` 与 `alert_box_preset_message(&v)`。删除 `styles/pages/showcase.css:285-299` 的 `.showcase-alert-switcher*` 规则（`transition: all` 随之消失）。
- 范围边界：`palette-scopes`（可横向滚动的范围标签）、`mcp-clients`、`admin-notebooks-tabs`（下划线标签）、`logs-levels`（多选）、`mcp-scopes`（卡片）、`notes-side-item`（竖排筛选）和 `comment_previews.rs` 的模式 chip 样式不同，不并入。

**3.6 图鉴**：
- `src/pages/components_showcase_data.json:406-464` 的 `FilterTabs` 条目替换为：`name: "SegmentedControl"`、`slug: "segmented-control"`、`label: "分段控件"`、`group: "action"`、`states: "当前 · 切换 · 窄宽只留图标"`、`source: "src/components/ui/segmented.rs"`、`preview_mode: "interactive"`、`usage_kind: "rust"`。`description` 写明：胶囊轨道 + 弹簧滑块；Tabs/Radio 两种语义；带图标时容器窄于 16rem 只留图标；`icon_only` 固定只显示图标。`props` 列出 3.1 的全部 8 个 prop；`keys` 为 `←/→`、`Home/End`、`Tab`；`code` 为 `SegmentedItem::new(...).icon(...)` 的用法示例（字符串值，带一项图标）。
- `preview_route`（`components_showcase.rs:1065`）：`"filter-tabs"` 换成 `"segmented-control"`。
- `LiveComponentPreview` 的 `"FilterTabs"` 分支换成 `"SegmentedControl"`。保留现有的文字示例（全部/已发布/草稿，用 `tab_value`）。`detail` 时下面再放两份带图标的 Radio 示例（浅色 Sun / 深色 Moon / 跟随系统 Monitor，用新的局部信号 `seg_mode: Signal<String>`）：一份放在 `div { class: "w-full" }` 里，一份放在 `div { style: "width: 11rem" }` 里，用来展示窄宽只留图标。两份都只改局部信号，不调用 `choose_theme`。

### 4. 统一 `Composer` — `feat(ui): 统一 Composer 输入栏原语`

**4.1 新原语** `src/components/ui/composer.rs`（`ui.rs` 加 `mod composer;`、`pub use composer::*;`）：

```rust
#[derive(Clone, Copy, PartialEq, Default)]
pub enum ComposerSize { #[default] Md, Sm }

/// 输入栏外壳：20px（Sm 为 14px）圆角、surface-3、发丝线 + composer 阴影，聚焦时 Clay 双环。
#[component]
pub fn Composer(
    #[props(default)] size: ComposerSize,
    /// 块级排列（评论框、快速记录）：不做横排，也不加内边距。
    #[props(default)] stacked: bool,
    #[props(default)] class: &'static str,
    children: Element,
) -> Element   // div.composer[.composer-sm][.composer-stacked] {class}

/// 输入栏里的输入框：单行 input 或多行 textarea（field-sizing: content），自带输入法守卫。
#[component]
pub fn ComposerInput(
    value: String,
    oninput: EventHandler<String>,
    #[props(into)] placeholder: String,
    #[props(default)] id: Option<String>,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] multiline: bool,
    /// 单行搜索框：type=search、name=q、enterkeyhint=search（无脚本时随表单 GET 提交）。
    #[props(default)] search: bool,
    #[props(default)] maxlength: Option<u32>,
    #[props(default)] autofocus: bool,
    #[props(default)] disabled: bool,
    /// 多行：⌘/Ctrl+Enter 调用；单行：给了就由 Enter 调用并阻止默认，否则交给所在表单原生提交。
    #[props(default)] on_submit: Option<EventHandler<()>>,
    /// 调用方的其余按键（Escape、↓ 等）；输入法组合期间不转发。
    #[props(default)] onkeydown: Option<EventHandler<KeyboardEvent>>,
    /// 组合输入开始（true）/ 结束（false）。
    #[props(default)] oncomposition: Option<EventHandler<bool>>,
    #[props(default)] onmounted: Option<EventHandler<MountedEvent>>,
) -> Element

/// 陶土发送键。不给 onclick 时为 type=submit。
#[component]
pub fn ComposerSend(
    label: &'static str,
    #[props(default = IconName::ArrowRight)] icon: IconName,
    #[props(default)] disabled: bool,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
) -> Element   // button.composer-send[aria-label=label] { Icon { name: icon, size: 16 } }

/// 输入法组合中：composition 状态、isComposing、Process 键，或 keyCode 229（Safari 确认候选的回车）。
pub(crate) fn ime_composing(composing: bool, e: &KeyboardEvent) -> bool
```
- `ime_composing` 实现：`composing || e.data().is_composing() || e.key() == Key::Process`；wasm 下再加 `{ use dioxus::web::WebEventExt; e.as_web_event().key_code() == 229 }`。
- `ComposerInput` 内部维护 `composing` 信号：`oncompositionstart`/`oncompositionend` 置位，并转发给 `oncomposition`。`onkeydown`：`ime_composing` 为真就直接返回（不阻止默认）；多行且按下 meta/ctrl+Enter，或单行、按下 Enter、给了 `on_submit`，则 `prevent_default` 并调用 `on_submit`；其余转发给 `onkeydown`。多行渲染 `textarea.composer-input[rows=2]`，单行渲染 `input.composer-input[autocomplete=off]`；`maxlength` 存在时输出字符串形式。
- `src/components/palette.rs` 的 `PaletteShell` 输入框守卫（约 518 行 `composing() || e.data().is_composing() || e.key() == Key::Process`）改为调用 `crate::components::ui::ime_composing(composing(), &e)`，全仓只留这一种写法。

**4.2 CSS**（`styles/components/ui.css` 的 Composer 段，约 986-1050 行）：
- `.composer:focus-within` 改为 `box-shadow: var(--shadow-focus), var(--shadow-composer);`。
- 新增：
  ```css
  textarea.composer-input { height: auto; min-height: 2.5rem; max-height: 20rem; padding: 0.5rem 0; resize: none; field-sizing: content; font-size: 0.875rem; line-height: 1.6; }
  .composer-input:disabled { opacity: 0.5; }
  .composer-send:disabled { background-color: var(--color-ink-2); color: var(--color-fg-3); cursor: default; }
  .composer-sm { align-items: flex-end; padding: 0.375rem 0.375rem 0.375rem 0.875rem; border-radius: 14px; }
  .composer-sm .composer-send { width: 2.25rem; height: 2.25rem; border-radius: 10px; }
  .composer-stacked { display: block; padding: 0; }   /* 必须写在 .composer-sm 之后 */
  ```
- 删除重复：`styles/components/comments.css:47-59`（`.composer.comment-composer` 的三条规则）；`styles/pages/admin-comments.css:145-172` 只留 `.admin-comment-reply { margin-top: 0.625rem; }`；`styles/pages/admin-notes.css:8-45`：`.admin-notes-quick` 只留 `padding: 0.875rem 0.875rem 0.625rem 1.125rem;`，删掉其外观、`:focus-within` 和全部 `.admin-notes-quick-text*` 规则，新增 `.admin-notes-quick .composer-input { display: block; width: 100%; min-height: 3.5rem; padding: 0; font-size: 0.9375rem; line-height: 1.7; }`，占位色规则只保留 `.admin-notes-quick-tags::placeholder`。

**4.3 迁移五处**：
- 首页 `src/pages/home.rs:231-266`（`HomeSearch`）：`form` 保留 `role/action/method/onsubmit`，`class` 改为 `"home-search"`；里面放 `Composer { … }`，依次是原 `label.sr-only`、`Icon Search 18`、`ComposerInput { id: "home-search-input", search: true, placeholder, value: query(), oninput: query.set }`、原 `kbd.home-search-kbd`、`ComposerSend { label: "搜索" }`。
- 全文搜索 `src/pages/search.rs:224-301`：`form` 保留 `class: "search-composer page-enter"`、`style`、`role/action/method/onsubmit`，里面放 `Composer`。`ComposerInput` 的参数：`id: "article-search"`、`search: true`、`maxlength: 200`、`onmounted` 写 `input_element`、`oninput` 沿用原逻辑（空串调用 `clear_search()`，否则 `query.set` 后 `schedule_search()`）、`oncomposition: move |active| { is_composing.set(active); schedule_search(); }`、`onkeydown` 沿用原来的 Escape/ArrowDown 分支（删掉自写的组合判断）。清除按钮原样保留；发送键为 `ComposerSend { label: "搜索", disabled: query().trim().is_empty() }`。
- 评论框 `src/components/comments/form.rs:375`：`div.composer.comment-composer` 改为 `Composer { stacked: true, size: if is_reply { ComposerSize::Sm } else { ComposerSize::Md }, class: "comment-composer", … }`，子内容不变。表单根上的 Mod+Enter（约 365 行）改为 `if !ime_composing(false, &e) && (ctrl || meta) && Enter { do_submit() }`。
- 后台行内回复 `src/pages/admin/comments.rs:518-530`：`Composer { size: ComposerSize::Sm, class: "admin-comment-reply" }`，里面放 `ComposerInput { multiline: true, autofocus: true, aria_label: format!("回复 {author}"), placeholder: format!("回复 {author}…（⌘↵ 发送）"), on_submit: move |_| send(), … }` 和 `ComposerSend { label: "发送回复", icon: IconName::Send, disabled: sending() || text().trim().is_empty(), onclick: move |_| send() }`。
- 快速记录 `src/pages/admin/notes.rs:381-482`：把 `submit` 的主体抽成 `let mut save = move || { … }`（去掉 `FormEvent`）；`form` 不加 class，写 `onsubmit: move |ev| { ev.prevent_default(); save(); }`；里面放 `Composer { stacked: true, class: "admin-notes-quick" }`，内容是 `ComposerInput { multiline: true, aria_label: "快速记录", disabled: recovering, placeholder, value, oninput, on_submit: move |_| save() }` 和原 `div.admin-notes-quick-foot`（不变）。删除原来按 `query_selector(".admin-notes-quick")` 调 `request_submit` 的 wasm 代码。
- 验收：`grep -rn '"composer \|composer-input\|composer-send' src --include=*.rs` 只命中 `src/components/ui/composer.rs`。

**4.4 图鉴**：三个都是 `pub` 组件，覆盖测试 `catalog_covers_public_components_and_has_unique_routes` 要求三者都要有条目。在 `components_showcase_data.json` 加三条（`group: "form"`、`preview_mode: "interactive"`、`usage_kind: "rust"`、`source: "src/components/ui/composer.rs"`）：
- `Composer` / `composer` / 「输入栏」：states「单行 · 多行 · 聚焦」；props 为 `size`、`stacked`、`class`、`children`；code 展示首页式组合（`form` > `Composer` > `Icon` + `ComposerInput { search: true }` + `ComposerSend`）。
- `ComposerInput` / `composer-input` / 「输入栏输入框」：states「单行 · 多行 · 输入法组合」；props 为 4.1 的全部字段；keys 为 `Enter`（单行提交）、`⌘/Ctrl + Enter`（多行提交）、「输入法组合中」（不提交）。
- `ComposerSend` / `composer-send` / 「发送键」：states「可用 · 不可用」；props 为 `label`、`icon`、`disabled`、`onclick`。
- 三个 slug 都加到 `preview_route` 的 `PreviewRoute::Live` 分支。`LiveComponentPreview` 新增分支，只用局部信号、不发请求：
  - `Composer`：单行搜索式，提交后在下方显示「已提交：{值}」。
  - `ComposerInput`：`Composer { size: Sm }` 里放多行输入，`on_submit` 把内容追加到一个本地列表并清空输入框，列表显示在下方。
  - `ComposerSend`：并排放一个可用、一个 `disabled` 的发送键。

### 5. 搜索面板关闭淡出与手机「取消」，去掉 `transition-all` — `fix(palette): 关闭淡出与手机取消入口`

改 `src/components/palette.rs`：
- `PaletteState` 新增私有字段 `closing: Signal<bool>`，新增常量 `const EXIT_MS: u32 = 120;`，注释写明与 `palette.css` 的 `palette-out` 时长一致。`use_palette_state` 里初始化为 false；打开 effect 在 `open()` 为真时先 `closing.set(false)`。
- 新增 `pub(crate) fn visible(self) -> bool { (self.open)() || (self.closing)() }`。
- `close` 改为 `pub(crate) fn close(mut self, reset: impl FnOnce() + 'static)`，不再返回 bool。已关闭时直接返回。否则：`seq += 1`、`loading`/`show_progress` 置 false、`open.set(false)`、`closing.set(true)`、立即归还焦点；然后 `spawn` 等 `EXIT_MS`。等待结束时若 `open.peek()` 为真（淡出期间又被 ⌘K 打开），什么都不做，保留上一次输入；否则清空 `query`、`active` 置 0、调用 `reset()`、`closing.set(false)`。输入与结果在淡出结束前保持原样，列表不跳动。
- 调用方：`search_palette.rs` 的 `close` 闭包改为 `palette.close(move || { scope.set(Scope::All); posts.set(Vec::new()); posts_query.set(String::new()); })`；`admin_palette.rs` 的 `close` 改为 `palette.close(move || { posts.set(Vec::new()); posts_query.set(String::new()); })`。两处 `if !open() { return rsx! {}; }` 都改为 `if !palette.visible() { return rsx! {}; }`。
- `PaletteShell`：`let closing = !(state.open)();`。scrim 和 `.palette` 在 closing 时加类 `is-closing`；面板 `"data-ygg-modal-open"` 在 closing 时为 `"false"`；面板加 `inert: closing.then_some("true")`。
- `Esc` 按钮（约 567-572 行）：去掉文字子节点，改为 `span { class: "palette-esc-key", "Esc" }` 和 `span { class: "palette-esc-cancel", "取消" }`。不加 aria-label：被 `display: none` 隐藏的那个 span 不进入无障碍树，可见文字就是名称。
- `styles/components/palette.css`：
  ```css
  .palette-esc-cancel { display: none; }
  .palette-scrim.is-closing,
  .palette.is-closing { animation: palette-out 120ms var(--ease-out-expo) both; pointer-events: none; }
  @keyframes palette-out { to { opacity: 0; } }
  ```
  在现有 `@media (max-width: 640px)` 里加 `.palette-esc { height: 2.75rem; padding: 0 0.25rem; background: transparent; color: var(--color-clay-text); font-family: inherit; font-size: 0.9375rem; }`、`.palette-esc-key { display: none; }`、`.palette-esc-cancel { display: inline; }`。`is-closing` 选择器的优先级高于减少动态效果块里的 `.palette`，所以减少动态效果时同样只有淡出。
- `src/pages/components_showcase/comment_previews.rs:815`：Tailwind `transition-all` 改为 `transition-colors`（这里只有颜色和底色变化）。验收：`grep -rnE "transition:\s*all|transition-all" styles src libs/*/src` 无命中。

### 6. 发布 1.0.0 — `chore(release): 发布 v1.0.0`

- `Cargo.toml:3` 改为 `version = "1.0.0"`；运行一次 `cargo check`（不加 `--locked`）让 `Cargo.lock` 里 `name = "yggdrasil"` 的版本更新；`git diff Cargo.lock` 只应有这一行。
- `CHANGELOG.md`：`## [Unreleased]` 下保持为空；其后插入 `## [1.0.0] - <执行当天 date +%F>`，现有 Unreleased 下的 Security/Changed/Fixed/Internal 四节原样移到 1.0.0 下。1.0.0 标题下、第一个 `###` 之前写一段导语（解析器会作为 `intro_html` 渲染）：「v1.0 全站重设计：陶土橙单一品牌色、系统字体与衬线标题、Lucide 图标、弹簧动效与暖调代码配色，前台与后台全部页面按新设计重写。」。在已有各节中补充以下条目（与已有条目合并到同名分类，`Added`、`Removed` 为新增分类，放在 `Security` 之后）：
  - `### Added`：**⌘K 面板**（前台搜索：文章建议接口、最近搜索、范围标签；后台命令面板）；**首页书桌**（最新一篇目录预览、按月索引、可配置标语）；**笔记阅读上下文**（笔记本目录、上一篇/下一篇、`?book=`）与笔记本目录页；**分享图与网站图标**（`/og/site.png`、`/og/post/{slug}.png`，明暗 SVG 图标与 manifest）；**404 推荐文章、交换友链信息、关于页统计、仪表盘发布节奏、侧栏数量、回收站剩余天数、设置统一保存条、笔记版本面板、密码强度条、新建文章起步模板、草稿预览宽度切换、灯箱缩略条与缩放档位、MCP 令牌接入配置**；**组件原语** `SegmentedControl`（窄宽只留图标）与 `Composer` 输入栏族，图鉴同步收录。
  - `### Changed`：**全站重设计**（设计 token、系统字体栈、深色模式重新调色、页面转场改用 snappy 弹簧与 out-expo 曲线）；**后台改为桌面应用式外壳**（可折叠、可拖宽的侧栏，面包屑，表格发丝线分隔）；**发丝线**在 2 倍及以上高分屏为 0.5px；**搜索面板**关闭时 120ms 淡出，手机全屏时输入框旁为「取消」；**后台失败提示**改为可读文案，不再拼接 server function 原始错误。
  - `### Removed`：Catppuccin 主题、`paper-*` 样式与桥接层、线条小狗插画、Material 图标、`PostCard`、`FilterTabs`（由 `SegmentedControl` 取代）、`SproutPlaceholder`（更名为 `ImagePlaceholder`）。
- 检查：`cargo test changelog` 通过（`changelog_parses_real_file` 要求恰好一个最新版、每个正式版都有内容）。

## Critical files & anchors

- `src/components/ui/navigation.rs:104-239`：`FilterTabs` 的键盘与 roving tabindex 实现，`SegmentedControl` 照搬后整段删除。
- `src/theme.rs:282-391`：`THEME_SEGMENT_STYLE`（SSR 首屏选中态）与 `ThemeToggle`；新样式的按钮位置与 items 顺序耦合。
- `src/components/palette.rs:192-326, 416-626`：`PaletteState`/`close`/`PaletteShell`，第 5 步的落点（去重仍在进行时，见「前置条件」）。
- `src/pages/components_showcase.rs:1062-1097, 1317, 1756-1805`：`preview_route`、预览分支，以及要求每个 `pub fn` 组件都进图鉴的覆盖测试。
- `styles/components/ui.css:885-1050, 1110-1126`：分段控件与 Composer 的 CSS 本体。

## Verification

前置：仓库根目录，本地工具链（`make check-dev-tools`），`.env` 可连开发库；浏览器检查需要 `libs/node_modules/playwright` 和 Chromium。

1. 门禁（全部步骤完成后）：`make lint`；`make test`；`cargo clippy --locked --target wasm32-unknown-unknown --no-default-features --features web -- -D warnings`，以及同一命令换成 `--features web,client-reader`。全部通过。第 3 步后跑一次两个 WASM clippy（`ResizeObserver` 只在 wasm 编译）。
2. 隔离浏览器回归：`make regression-build && make test-browser BROWSER_SUITE=showcase`、`BROWSER_SUITE=admin`、`BROWSER_SUITE=backup` 均通过（覆盖 `getByRole('tab')` 选中与图鉴预览不发请求）。另起 `make dev`，运行 `PLAYWRIGHT_MODULE=$PWD/libs/node_modules/playwright CHROMIUM_PATH=/usr/bin/chromium bun scripts/test-view-transitions.ts` 和 `scripts/test-writing-guide.ts`，均通过（主题选择器已改）。
3. 新行为冒烟（对 `make dev` 的 http://127.0.0.1:8080 跑一次性 Playwright 脚本，放在 `/tmp`，不入库）：
   - 发丝线：`deviceScaleFactor: 2` 打开 `/`，`getComputedStyle(document.documentElement).getPropertyValue('--hairline').trim()` 为 `0.5px`；`deviceScaleFactor: 1` 时为 `1px`。
   - 分段控件：打开 `/about/components/segmented-control`。宽示例里 `.segmented-label` 的 `getBoundingClientRect().width` 大于 1；11rem 示例里等于 1，且按钮 `textContent` 仍含「跟随系统」。点击「草稿」后 `[aria-selected="true"]` 移到该项，`.segmented-thumb` 的 `style.transform` 变化。聚焦后按 ArrowRight，选中与焦点一起右移。
   - 主题首屏：`localStorage['yggdrasil-theme']='dark'` 后重载，hydration 前（`page.route` 拦截 `*.wasm` 让它挂起）页头 `.theme-seg` 里第 2 个按钮的 `getComputedStyle(...).backgroundColor` 不是透明，第 3 个是透明。放行后 `[role=radio][aria-checked=true]` 的 aria-label 为「深色」。视口 390 宽打开菜单抽屉，「浅色」文字可见；`/admin` 侧栏拖到 320px 宽时外观控件显示文字，默认 232px 时只有图标。
   - Composer：在 `/` 的首页搜索框输入 `rust` 并回车，跳到 `/search?q=rust`，`.composer:focus-within` 的 box-shadow 含 Clay 色。在 `/about/components/composer-input` 的多行输入里先 `dispatchEvent(new CompositionEvent('compositionstart'))`，再按 Meta+Enter：列表不增加；`compositionend` 后再按 Meta+Enter：列表加一条，输入框清空。
   - 搜索面板：1440 宽按 Control+K，`.palette-esc` 的 `innerText` 为 `Esc`；390 宽打开后为 `取消`，按钮高度 ≥ 44px。点「取消」后立即能查到 `.palette.is-closing`，且输入框值不变；300ms 后 `.palette` 不存在，焦点回到触发按钮；再次打开时输入框为空。
   - 版本：`curl -s http://127.0.0.1:8080/about | grep -o '1\.0\.0 · [0-9-]*'` 有输出。
4. 静态 grep（各步验收里列过的）全部为零命中：`FilterTabs|filter-tabs`、`theme-seg-item|settings-segmented"|preview-width`、`transition:\s*all|transition-all`、`1px … var(--color-hairline)`，以及 `ui/composer.rs` 以外的 `composer-input|composer-send`。

## Assumptions & contingencies

- 发布日期取执行当天；本计划不打 git tag、不推送（仓库规则：仅在用户要求时推送）。
- 自适应阈值统一为 16rem：目前只有主题控件带图标，三项「浅色 / 深色 / 跟随系统」在 Sm 尺寸下约 236px 宽。如果冒烟时抽屉（390 宽）里文字被隐藏，把阈值改为 15rem 再验证；其余不变。
- 如果 `ResizeObserver` 回调在隐藏转显示时没有触发（例如页头从手机宽拖回桌面宽后滑块宽度为 0），在 `SegmentedControl` 里再加 `window` 的 `resize` 监听调用同一个测量函数，写法同 `use_palette_shortcut` 的监听注册与清理。
