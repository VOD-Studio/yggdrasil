---
name: yggdrasil-ui-design
description: 在 Yggdrasil 仓库中新增或修改前台、后台界面，调整配色、排版、组件样式或动效时使用；按现行设计 token、组件原语与弹簧动效规范实现并验证。不用于撰写博客文章。
---

# Yggdrasil 界面设计与动效实现

完整规范见 `docs/界面设计规范.md`。本技能给出动手顺序和容易出错的地方；数值以规范和源码为准，不要凭记忆填。

## 1. 先查再写

1. 读 `docs/界面设计规范.md` 中与本次改动相关的章节（颜色第 3 节、排版第 4 节、布局第 6 节、组件第 7 节、动效第 8 节）。
2. 在 `src/components/ui/`、`src/components/forms.rs`、`src/components/empty_state.rs` 里找可复用的组件，优先使用它们：`Button`、`IconButton`、`FormInput`、`FormSelect`、`TimePicker`、`NumberInput`、`ToggleSwitch`、`Composer`、`SegmentedControl`、`Menu`、`Popover`、`ModalShell`、`ConfirmDialog`、`BottomSheet`、`SideSheet`、`Tooltip`、`TagChip`、`StatusDot`、`EmptyState`、`LoadError`、`Toast`。
3. 在 `styles/components/` 或 `styles/pages/` 里找同类页面的现有写法，沿用相同的类名风格与层级。新规则放进所属模块；不要为了文件名排序去改 `input.css` 的导入顺序，它就是层叠顺序。

## 2. 用 token，不写裸值

- 表面 `surface-0…3`、`popover`；文本 `fg`、`fg-body`、`fg-2`、`fg-3`；透明度 `ink-1…4`；边框 `hairline`；圆角 `rounded-panel/code/dialog/composer`；阴影 `shadow-sm/md/lg/popover/composer/focus`。
- 品牌色分工：`clay` 只做装饰（点、环、进度、竖线）；文字用 `clay-text`；按钮底用 `clay-fill` 配 `on-clay`；徽标底用 `clay-soft`。不要让 clay 大面积铺色。
- 危险、成功、警告用 `danger`、`success`、`warning` 及其 `-soft`，不与 clay 混用。
- 边框写成发丝线环：`inset 0 0 0 var(--hairline) var(--color-hairline)` 或 `ring-hairline`。
- 需要新 token 时只改 `styles/theme.css`：浅色写在 `@theme static`，深色写在 `.dark` 覆盖块，两套都要给。深色不是反色，保持暖调。
- 不要手改生成文件：`styles/motion.css`（改 `libs/shared/src/spring.ts` 后 `bun run motion-css`）、`src/components/ui/icons_data.rs`（改 `libs/shared/src/icons.ts` 后 `bun run icons`）、代码配色（改 `libs/shared/src/code-palette.ts` 后 `bun run code-themes` 与 `make highlight-css`）。生成命令都在 `libs/shared` 下运行。

## 3. 排版

- 字体只用 `font-sans` / `font-serif` / `font-mono`，不加载网页字体。大标题、导语、引用、日期用衬线；界面和正文用黑体。
- 中文不用斜体；标题不收字距；会变化的数字加 `tabular-nums`；文章与归档日期用衬线加 `oldstyle-nums`。
- 内容中不手动在中英文之间补空格（全站已开启 `text-autospace`）。
- 栏目小标签复用 `.section-label`；行内图标加 `icon-inline`。

## 4. 交互

- 可聚焦元素一律 `outline: none` + `box-shadow: var(--shadow-focus)`；无效输入用 danger 环。
- 图标按钮必须有 `label`（同时是读屏名称和悬浮提示）。
- 对话框用 `ModalShell`：它负责锁定焦点、Esc 关闭和焦点归还。危险确认用 `ConfirmDialog`（`alertdialog`）。
- 任何文本输入的键盘处理都先过 `composer.rs::ime_composing`，组合输入期间不响应回车、方向键和 Esc。
- 抽屉复用 `BottomSheet` / `SideSheet`（4px 死区、80px 关闭阈值、heavy 弹簧），不要另写拖拽逻辑。
- 手机触控目标不小于 44px；手机布局统一在 767px 切换（与 Tailwind `md:` 一致）。
- 用户可见的错误必须是可读中文，不能直接拼接 server function 错误。
- 加载态：用骨架时结构要与真实布局对齐，并经过 `DelayedSkeleton`（200ms 内不显示）。

## 5. 动效

- 选档位：按压、开关、箭头用 `bouncy`；进场、弹层、菜单、对话框、滑块用 `snappy`；正反馈用 `gentle`；抽屉、侧栏宽度、吸附用 `heavy`。写法：`animation: name var(--spring-snappy-duration) var(--ease-spring-snappy)`。
- 状态变化（颜色、阴影、焦点）用 150–220ms `var(--ease-out-expo)`；退出更短、不回弹；按下用 80ms `var(--ease-press)`。
- 只动 `transform`、`opacity`、`box-shadow`、`clip-path`；不写 `transition: all`。
- 页内入场优先复用 `.animate-page-enter`、`.page-enter`（`style="--enter-index: N"`，N 封顶 5）、`.animate-popover-enter(-edge)`、`.animate-select-enter`、`.animate-modal-*`。
- 填充模式：终态就是元素自然值时用 `backwards`，不要用 `forwards`——残留的单位矩阵 `transform` 会让元素成为 `fixed` 后代的包含块。从非自然状态开始的弹层用 `both`。
- 元素需要居中定位又要做 `transform` 动画时，静态定位写 `[transform:translateX(-50%)]` 或独立的 `translate` 属性，并把同样的值写进关键帧首尾。Tailwind v4 的 `-translate-x-1/2` 走独立 `translate` 属性，会与关键帧的 `transform` 叠加成双倍位移。
- 滚动驱动动画（`animation-timeline: scroll()`）的基础样式必须是「未滚动」态，关键帧只写 `to`；路由替换正文的那一帧时间线可能失效。
- 在 JS 中读弹簧时长要换算单位：`parseFloat(d) * (d.endsWith('ms') ? 1 : 1000)`，因为压缩后的 CSS 可能写成 `.41s`。
- 页面级过渡只改 `libs/yggdrasil-core` 里现有的 View Transitions 协调代码（`route-transitions.ts`、`view-transition-lifecycle.ts`、`theme-transition.ts`），不要另起一套。带 `view-transition-name` 的元素在过渡期间不可点击，需要保持可点的元素（如导航胶囊）在实时 DOM 里用 WAAPI 做动画。
- 每个新动画都要写 `@media (prefers-reduced-motion: reduce)` 降级：去掉位移和缩放，只保留 120–150ms 淡入或直接切换。JS 动画用 `@yggdrasil/shared` 的 `prefersReducedMotion()` 拦截。
- `transitionend` 要配兜底计时器，并在进入下一阶段前清理。

## 6. 组件图鉴

新增或改名 `src/components/**` 下的公开组件（`pub fn` 大写开头，布局组件与 `skeletons/` 除外）时，同步更新 `src/pages/components_showcase_data.json`：slug 唯一、分组已知、`code` 非空，并补上预览分派。预览使用生产组件和本地数据，不发送鉴权、上传、评论或运行请求，卸载时清理浏览器实例。

## 7. 验证

按改动范围选择：

```sh
make css                                   # 改了 input.css 或 styles/**
cd libs/shared && bun run test             # 改了 spring / palette / icons 源
cd libs/yggdrasil-core && bun run test     # 改了转场、目录、滚动相关代码
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo check --locked --target wasm32-unknown-unknown --no-default-features --features web
cargo test --locked components_showcase    # 增删公开组件
bun scripts/test-components-showcase.cjs   # 图鉴预览（SHOWCASE_BASE、SHOWCASE_ONLY）
```

在浏览器里实际查看：桌面与 390px 手机宽度、浅色与深色、减少动态效果。改了导航或转场时，按 `scripts/README-view-transitions.md` 验证直接加载与 SPA 导航，运行 `bun scripts/test-view-transitions.cjs`（`VT_BASE` 必须与 `APP_BASE_URL` 同源）。无头浏览器里导航成功不代表跨文档动画成功，后者需要在普通窗口中人工确认。

改了全局样式或脚本，递增 `src/app.rs` 里 `shell()` 的 `?v=N`。

## 8. 收尾

- 修改了 token、档位或组件规格时，同步更新 `docs/界面设计规范.md` 中对应的表格。
- 修好了规范第 11 节「已知偏差」中的某一项，就把它从表中删去。
- 按功能单元提交：`type(scope): 中文摘要`，显式暂存相关路径。
