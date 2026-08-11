---
name: yggdrasil-material-icons
description: |
  需要在 Yggdrasil UI 任何位置加图标时使用：去 Google Material Symbols
  (fonts.google.com/icons) 找合适的图标，下载 SVG 放到 public/icons/ 留档，
  再把 path 内联进 Dioxus 组件（fill: currentColor 适配明暗主题）。强制遵循
  本仓库「内联 SVG path + currentColor、绝不走 <img src>」的约定。
  触发关键词："icon"、"图标"、"material symbol"、"glyph"。
allowed-tools:
  - Read
  - Edit
  - Write
  - Grep
  - Glob
  - Browser
metadata:
  trigger: 在 src/components 或 src/pages 的 rsx! 里加 UI 图标 / Material Symbol
  source: 提炼自 src/theme.rs (ThemeToggle)、src/components/header.rs (SearchIconLink)、footer.rs、post/post_header.rs 的统一图标内联约定
---

# Yggdrasil 图标工作流（Material Symbols）

需要 UI 图标时，统一走 Google Material Symbols。**不要**手画 SVG、不要用 emoji 替代、不要引入
第三方图标字体（Font Awesome / Heroicons / lucide 等）。

## 关键约定（务必先读）

本仓库图标有**三种用法，都把 SVG 内联，从不走 `<img src="/icons/...">`**：

1. **内联进组件**（绝大多数情况）—— 把图标 path 直接写进 `rsx!` 的 `svg { path { d: "..." } }`，
   `fill: "currentColor"`，由外层 Tailwind 文字色 / `color` CSS 控制明暗主题适配。
   例：`src/theme.rs` 的 `ThemeToggle`（bedtime / wb_sunny / computer）、
   `src/components/header.rs` 的 `SearchIconLink`、`src/components/footer.rs`、
   `src/components/post/post_header.rs`。
2. **内联成 `&'static str` 常量** —— 复杂 SVG（带渐变/keyframes，如 `SPINNER_SVG`），
   通过 `dangerous_inner_html` 注入。源文件仍存一份在 `public/icons/`。
3. **JS 库 `innerHTML` 字符串注入** —— `libs/` 工作区（如 `libs/tiptap-editor/src/index.ts` 的
   源码/富文本切换按钮）用 `document.createElement` 动态建 DOM，图标通过
   `element.innerHTML = '<svg fill="currentColor" viewBox="0 -960 960 960" ...><path d="..."/></svg>'`
   注入。`fill: currentColor` + 标准 960 视口 + `public/icons/` 留档约定**与 rsx! 完全一致**，
   只是经 DOM API。建议把 SVG 串抽成模块级 `const FOO_ICON_SVG = '...';` 常量复用。

> **例外**：`libs/tiptap-editor/src/slash-command.ts` 的 `/命令` 临时菜单**刻意**用文本字形/emoji
> （`▶`/`<>`/`📤`/`🔗`）做轻量图标——那是瞬态命令面板的有意选择，不是约定违例，**不要**去"修"成 SVG。

`public/icons/` 里的 `.svg` 文件是**源档案（source of record）**，保留 Google 默认下载文件名，
仅用于：追溯图标出处、后续复用 path、替换图标时比对。运行时**不被任何 `<img>` 引用**。

## 标准内联模板（24px Material Symbols，照抄）

所有现有图标都用这同一套属性。新增图标时**完全对齐**：

```rust
svg {
    xmlns: "http://www.w3.org/2000/svg",
    height: "24px",
    view_box: "0 -960 960 960",
    width: "24px",
    fill: "currentColor",
    path { d: "<把下载的 SVG 里的 path d 值原样粘进来>" }
}
```

- `view_box` 固定 `"0 -960 960 960"`（Google Material Symbols 24px 的标准视口，**不是** `0 0 24 24`）。
- `fill` 必须 `"currentColor"` —— 下载下来的 SVG 是 `fill="#e3e3e3"`，**一定要改**，否则图标在浅色
  主题下会是浅灰色看不见。尺寸不要在 svg 上写死，由外层 Tailwind 类（`w-5 h-5` 等）或
  `height/width: "24px"` 控制（现有代码用后者，保持一致）。
- 只有一个 `<path>` 时用单行 `path { d: "..." }`；多个 path 各写一行。

## 完整步骤

### 1. 找图标
- 打开 https://fonts.google.com/icons （Material Symbols 图库）。
- 用英文关键词搜索（如 "search" / "moon" / "sun" / "menu" / "close" / "arrow up"）。
- 选定图标，**记下它的官方名**（如 `search`、`bedtime`、`wb_sunny`、`computer`）。
- 这是一个 JS 单页应用，用 `browser` 工具（`xd://browser`）打开并交互，或让用户在浏览器里搜好
  告诉你图标名。不要尝试 `read` 该 URL —— 返回的是空壳 HTML。

### 1b. 程序化获取（browser 工具不可用时的回退）

- 若 `xd://browser` 打不开 Google Fonts（如报 `open_split did not return a surface_id`），
  可用 `read` 工具直读 jsDelivr 上的 raw SVG：
  `https://cdn.jsdelivr.net/npm/@material-symbols/svg-400@latest/outlined/<name>.svg`
  （把 `<name>` 换成图标名，如 `close.svg`、`code.svg`、`article.svg`）。
- 返回 `<svg viewBox="0 -960 960 960"><path d="..."/></svg>`：标准 960 网格、单 path，提取 `d` 值即可，
  跳过 browser 下载那步。
- ⚠️ **provenance**：`@material-symbols/svg-400` 是**社区包**（marella 维护），**不是 Google 官方 npm**，
  但从 Google 主仓库自动同步，path 忠实。**黄金标准仍是 Google Fonts 网站下载**（保留 Google 原文件名留档）；
  CDN 仅作 headless / 无 browser 环境的回退，且 commit/文档里要标明来源。
- 重量变体：`svg-300`(light) / `svg-400`(regular，= Google 默认 wght400 GRAD0) / `svg-500`(medium) /
  `svg-700`(bold)；`/outlined/` 子目录 = 默认样式。

### 2. 下载 SVG
- 在图标的右侧面板点 **"SVG" 下载按钮**（默认 24px、Weight 400、Grade 0、Optical size 24、
  黑色填充）。用 browser 工具点击下载，或让用户提供文件。
- 下载文件名形如 `<name>_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg` —— **保留这个原名**。

### 3. 放到 public/icons/
- 把原始 SVG（**未改 fill 的原版**）存到 `public/icons/<原名>.svg`。
- 这一步是为留档，不要在这里改成 currentColor（要让存档忠实反映 Google 原版，方便日后比对/替换）。

### 4. 内联进组件
- 打开下载的 SVG，复制 `<path d="..."/>` 里的 `d` 值。
- 在目标组件的 `rsx!` 里用上面的标准模板，`fill` 写 `"currentColor"`，把 `d` 粘进去。
- 复杂/带动画的图标（如 spinner）改用 `&'static str` 常量 + `dangerous_inner_html`，参照
  `src/components/ui.rs` 的 `SPINNER_SVG`。

### 5. 复用优先 —— 不要新建重复图标
- 加图标前先 `grep` 现有代码：搜 `path { d:` 或图标名，确认是否已内联过同一图标。
- 已存在的图标直接复用那个组件/常量，不要复制第二份 path 进新文件。

## 常见错误

| 错误 | 后果 | 正确 |
|---|---|---|
| 用 `<img src="/icons/x.svg">` | 浪费一次请求、无法随主题变色、偏离约定 | 内联 path + `fill: currentColor` |
| 忘了把 `fill="#e3e3e3"` 改成 `currentColor` | 浅色主题下图标隐形 | 一律 `currentColor` |
| `view_box: "0 0 24 24"` | 图标偏移/裁切 | Material Symbols 固定 `"0 -960 960 960"` |
| `public/icons/` 里存了改过 fill 的版本 | 留档失真，日后比对困难 | 存 Google 原版，currentColor 只在组件里设 |
| 用 emoji 代替图标 | 各平台渲染不一致、无法着色 | 走 Material Symbols |

## 备选：其他尺寸/样式

默认下载 24px Filled。如需 Outlined / Rounded / Sharp 变体，在 Google 图库右侧面板切换样式集再下载
（不同样式集的 path 不同，视口仍是 `0 -960 960 960`）。文件名里的样式后缀（如 `GRAD0`/`FILL0`）
会被 Google 自动带出，保留即可。
