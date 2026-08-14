# 2026 内容管理后台仪表盘设计调研

> **用途**：为 Yggdrasil `/admin/` 仪表盘页（`src/pages/admin/dashboard.rs`）的改版提供设计输入。
> **调研日期**：2026-08-14。**范围**：个人博客/CMS 站长看到的管理首页，不是企业级 BI。
> **方法**：优先一手高可信来源（Nielsen Norman Group、Material Design 3 / Google Design、Apple HIG、web.dev、WCAG 2.2、Refactoring UI、Tailwind/shadcn/ui、Vercel Geist、Linear、Atlassian Design、Stephen Few）。每条结论标注来源与可见日期。
> **本仓库既有设计体系**（所有建议必须落在其上，而不是另起炉灶）：`paper-*` CSS 变量双主题；卡片圆角 16px（内容）/ 32px（壳）；用 box-shadow 而非 border 分层；`animate-page-enter` / `animate-row-enter` 交错入场；`prefers-reduced-motion` 降级；无渐变、无重阴影。

---

## 1. 统计卡片解剖（Stat-card anatomy）

### 来源共识

- **数值为主、标签为辅。** Refactoring UI 明确指出：仪表盘上同类数据并排、需要可扫读时确实需要标签，但标签是"配角"——应通过更小字号、更低对比度、更轻字重去弱化，"数据本身才是重点"（[Labels are a last resort](https://www.refactoringui.com/previews/labels-are-a-last-resort)，Refactoring UI 免费预览章节，页面未标注日期）。
- **数字排版用等宽数字（tabular-nums）。** shadcn/ui 官方 dashboard-01 区块的统计卡片实现为：`CardTitle` 放数值（`text-2xl font-semibold tabular-nums`），`CardDescription` 放标签，趋势用 `Badge` + 上下箭头图标 + 百分比，卡片底部另有一行上下文文案（如 "Trending up this month"）（[dashboard-01 注册表源码 section-cards.tsx](https://ui.shadcn.com/r/styles/new-york-v4/dashboard-01.json)，2024–2025 现行版本；[区块预览页](https://ui.shadcn.com/blocks)）。
- **趋势指示必须有参照系。** 裸的 "+12%" 没有说明对比窗口就是误导；shadcn 的约定是 delta 徽章之外再在卡片底部写明时间窗（"this month" / "vs last month"）。Apple HIG 同样要求数据描述"避免有歧义的格式与缩写"（[Apple HIG — Charts](https://developer.apple.com/design/human-interface-guidelines/charts)，持续更新）。
- **趋势用"颜色 + 图标/文字"双编码，绝不只用颜色。** WCAG 2.2 SC 1.4.1（Level A）规定颜色不得作为传达信息的唯一视觉手段（[Understanding SC 1.4.1 Use of Color](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html)）；NN/g 的数据可视化学术综述进一步指出约 4.5% 人口有色觉障碍（男性约 8%），颜色应作为"次级分组线索"而非主信号（[Dashboards: Making Charts and Graphs Easier to Understand](https://www.nngroup.com/articles/dashboards-preattentive/)，2017-06-18）。

### Delta 徽章 vs Sparkline vs 纯数值——何时趋势会误导

- NN/g 的结论是：长度与 2D 位置是人脑前注意（preattentive）处理最准的定量通道，因此**折线/迷你 sparkline 比角度（仪表盘 gauge）、面积（饼图/环图）都更易读**；gauge 类"汽车仪表盘"组件既浪费空间又难解读（同上文，2017-06-18）。
- 但趋势数字本身在小样本下会误导：个人博客每周发文 0–2 篇，"环比 +100%"只是噪音。shadcn/Tailwind 模板默认带 delta 徽章，那是因为它们面向 SaaS 营收数据；这是"形式照抄、语境不符"的典型陷阱——Google 在 M3 Expressive 研究里也警告：脱离熟悉范式的"新颖设计"看起来现代，可用性却下降（[Expressive Design: Google's UX Research](https://design.google/library/expressive-material-design-google-research)，2025）。
- **本仓库建议**：
  1. 卡片骨架保持"大数值（`text-4xl`，`tabular-nums`）+ 小标签（`text-sm`，`--color-paper-secondary`）"，数值在上或标签在上均可，但标签必须弱化——现状已大致符合，补上 `tabular-nums` 防止 CountUp 滚动时数字宽度跳动。
  2. **趋势要么真实、要么删除**：可计算且有意义的口径才展示（如"本周新增 3 篇 / 上周 1 篇"，并写明窗口）；算不出来就省略 trend 字段，不要硬编码（详见第 5 节）。
  3. 若未来要加趋势可视化，选 sparkline 折线（位置/长度通道），不要 gauge、不要饼环图（NN/g，2017）。
  4. 涨跌颜色必须配 ↑/↓ 图标或文字（WCAG 1.4.1），绿色表"正向趋势"即可（Refactoring UI 的语义色约定：[Building Your Color Palette](https://www.refactoringui.com/previews/building-your-color-palette)）。

---

## 2. 布局：Bento vs 表格优先 vs 分栏

### 来源共识

- **仪表盘 = 单页、一眼监控、可直接行动。** NN/g 对 dashboard 的定义是"collections of data visualizations, presented in a single-page view that imparts at-a-glance information on which users can act"（[NN/g Dashboards](https://www.nngroup.com/articles/dashboards-preattentive/)，2017-06-18）。Stephen Few 更早厘清：dashboard 的核心是**监控（monitoring）**，把深度分析混进来会造成"dashboard confusion"（[Dashboard Confusion Revisited](https://www.perceptualedge.com/articles/visual_business_intelligence/dboard_confusion_revisited.pdf)，Perceptual Edge 白皮书，2007）。
- **当代主流模板的事实标准是"统计卡片行 → 主图表 → 数据表"三段式。** shadcn/ui dashboard-01 的页面结构就是 `<SectionCards /> → <ChartAreaInteractive /> → <DataTable />`（[页面源码](https://ui.shadcn.com/r/styles/new-york-v4/dashboard-01.json)）；Tailwind 官方的 Catalyst 应用 UI Kit 同样以应用后台组件为主打（[Catalyst](https://catalyst.tailwindui.com)，页面未标注日期）。
- **视觉分组的手段是"容器/包含（containment）+ 尺寸 + 颜色"，而不是装饰。** Google 的 M3 Expressive 研究（46 项研究、18000+ 参与者）发现：用包含、尺寸、颜色把关键元素"推"出来，用户定位关键 UI 元素的速度最高提升 4 倍，且基本消除了年长用户的劣势（[Google Design，2025](https://design.google/library/expressive-material-design-google-research)）。

### 分歧点

- **Bento 网格的适用尺度。** 2025–2026 的模板/营销物料普遍吹捧 bento；但 NN/g 与 Stephen Few 的立场是任务优先、克制密度——bento 一旦塞进过多异质卡片就成"视觉噪音"。Google 自己的研究也强调"Context still matters"：破坏熟悉的纵向列表范式（他们的播放列表实验）会直接拉低可用性。
- **个人博客规模的正确密度远低于 SaaS 模板。** 模板里 4 KPI 卡 + 交互图表 + 大型表格是"演示密度"；单作者博客没有营收图表可放，硬凑图表等于造假数据（第 5 节）。

### 本仓库建议

- **维持当前"4 统计卡（一行 bento 带）→ 近期文章列表"的双段结构**，这是正确尺度；不要为"像 dashboard"而加图表区。博客没有日级指标时，近期文章列表本身就是最诚实的"活动流"。
- 4 张卡是个人博客的上限；如需扩充，优先把"待审评论"这类**可行动**指标留在第一屏（符合 NN/g "at-a-glance information on which users can act"）。
- 列表优先用"行 + 分隔"而非每行一张卡片：本仓库 `ADMIN_TABLE_CLASS` 容器 + `divide-y` 的做法与"减少视觉噪音、保持对齐"的方向一致（Linear 2024 改版核心目标即"reduce visual noise, maintain visual alignment, increase hierarchy and density"，[How we redesigned the Linear UI, part Ⅱ](https://linear.app/now/how-we-redesigned-the-linear-ui)，2024-03-28）。

---

## 3. 微交互：悬浮、时长、缓动与入场预算

### 时长与缓动的共识（多来源互证）

| 场景 | NN/g（2020-02-09） | Atlassian Design（现行） |
| --- | --- | --- |
| 简单反馈（hover/press、开关） | ≈100ms，"感觉即时" | 50–150ms |
| 元素入场/模态/较大位移 | 200–300ms | 150–400ms（modal 250ms、dropdown 150ms） |
| 上限 | 500ms 以上"开始感觉拖" | 400ms 为上限 |

来源：[Executing UX Animations: Duration and Motion Characteristics](https://www.nngroup.com/articles/animation-duration/)（2020-02-09）；[Atlassian Design — Motion](https://atlassian.design/foundations/motion)（页面未标注日期，Early Access 令牌体系）。

- **缓动方向**：入场用 ease-out（先快后慢，让眼睛预测落点），离场用 ease-in（加速离开），纯线性运动显得不自然（NN/g，2020）。Atlassian 给出可直接抄的贝塞尔值：入场 `cubic-bezier(0, 0.4, 0, 1)`，出场 `cubic-bezier(0.6, 0, 0.8, 0.6)`，微妙淡入 `cubic-bezier(0.4, 1, 0.6, 1)`。
- **元素越大，时长越长**；高频出现的动画要更短更收敛（NN/g，2020）。
- **Material 3 已整体迁移到弹簧物理（springs）的 motion token 体系**，令牌分 spatial / effects 两类、各含 default/fast/slow 三档（[M3 Motion overview](https://m3.material.io/styles/motion/overview/how-it-works)；[M3 Easing and duration](https://m3.material.io/styles/motion/easing-and-duration)，2025 Expressive 更新）。

### 何时动画伤害感知性能

- 动画的最大副作用是**劫持注意力**：人眼周边视觉对运动极敏感，无信息量的动效会分散注意力甚至惹恼用户（[The Role of Animation and Motion in UX](https://www.nngroup.com/articles/animation-purpose-ux/)，2020-01-12）。
- Atlassian 给了一个一刀切的检验问题：**"如果把这个动画删掉，用户会丢失信息或上下文吗？"** 不会就删掉或缩短（[Atlassian Motion](https://atlassian.design/foundations/motion)）。
- 性能层面：大量元素同时做 opacity 动画在多平台上计算昂贵，会掉帧（NN/g，2020）；交错入场本质就是"多个元素同时淡入+位移"，要控制同时动画的元素数量。

### 本仓库建议

- **悬浮反馈**：待审评论卡目前是 `hover:-translate-y-1 hover:shadow-md duration-300`——300ms 对 hover 偏慢（两来源都建议 ≤150–200ms）。建议改 `duration-150` 或 `duration-200`，缓动 ease-out；位移 -4px（-translate-y-1）幅度可保留。
- **入场交错预算**：现有 `animate-row-enter` 60/120/200ms 延迟 + `RecentPostItem` 80ms/行 × 5 行 ≈ 400ms 总延迟，加 300ms 左右单元素时长，整页入场 <1s——在预算内，保持。**但**单个卡片入场时长不应超过 400ms；`待审评论` 卡的 600ms `animation-duration` 超出两来源上限，建议压到 ≤400ms。
- 入场动画只跑 `transform` + `opacity`（现状符合），不要动画 `box-shadow` 或布局属性。
- M3 的弹簧物理在 Dioxus/纯 CSS 里成本高，本仓库继续用命名的贝塞尔曲线即可（NN/g 也提醒缓动规格要以工程可翻译的方式交付）。

---

## 4. 加载与空状态：骨架屏、数字滚动、reduced-motion

### 骨架屏共识（NN/g，[Skeleton Screens 101](https://www.nngroup.com/articles/skeleton-screens/)，2023-06-04）

- 骨架屏用于**整页加载**，用线框式占位预告页面结构：降低感知等待、帮用户建立心智模型、降低认知负荷——本仓库 `SkeletonBox` 按"标题条 + 大数字条"仿形，方向正确。
- **脉冲/shimmer 动画可用但需克制**：NN/g 明确提醒这类动画"可能分散注意力、惹恼用户，甚至造成无障碍问题"。
- **反模式**：只显示页面框架（header/footer + 空背景）的 frame-display 骨架等价于 spinner，等待稍长用户就以为页面坏了——不要退化成这样。
- 加载 <1s 时**不要**闪骨架屏（闪烁反而让人跟不上）；>10s 应改用进度条；骨架屏不能替代性能优化本身。

### 数字滚动（Count-up）

- NN/g 认可的先例：Hipmunk 加载机票结果时数字从 0 滚到 754——它传达了"系统正在并发聚合多个搜索源"这一真实过程（[NN/g，2020-01-12](https://www.nngroup.com/articles/animation-purpose-ux/)）。即：**数字滚动的正当用途是传达"数据正在到达/聚合"，而不是装饰**。
- 时长必须服从第 3 节的预算：当前 `CountUp` 约 900ms，超过 NN/g 500ms 上限，建议压到 400–500ms（ease-out 系缓动保留）。

### Reduced-motion

- WCAG 2.2 SC 2.3.3（AAA）要求：交互触发的非必要动画必须可关闭；充分技术即 CSS `prefers-reduced-motion` 查询（C39）与 JS 侧 `matchMedia`（SCR40）（[Understanding SC 2.3.3](https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html)）。前庭障碍用户被触发的后果包括眩晕、恶心、偏头痛。
- web.dev 的工程指引：CSS 与 JS 双侧都要响应；JS 驱动的动画要用 `matchMedia('(prefers-reduced-motion: reduce)')` 监听并中止（[prefers-reduced-motion: Sometimes less movement is more](https://web.dev/articles/prefers-reduced-motion)，Thomas Steiner，页面未标注日期）。
- **现状已符合**：`CountUp` 命中 reduced-motion 直接显示终值；入场动画有降级。改版时保持"reduced-motion = 终值直出 + 无位移动画"即可。

---

## 5. 数据新鲜度与诚实：为什么假指标侵蚀信任

- **把编造的数字呈现为真实数据，属于欺骗性设计的一阶近亲。** NN/g 对 deceptive patterns 的定义是"诱导用户做不符合自身利益之事的设计"，并强调其"不道德且有法律风险"（[Deceptive Patterns in UX](https://www.nngroup.com/articles/deceptive-patterns/)，2023-12-01）。仪表盘上硬编码的 "+12%" 虽然动机是装饰，但效果是让用户基于虚构信息形成判断——一旦被发现，整个页面的数字都会失去可信度。
- **诚实加载 > 装饰性占位**：骨架屏之所以被 NN/g 认可，恰恰因为它是"诚实的等待"——占位结构如实预告内容形态；同理，加载失败/无数据时应该如实呈现（重试入口或空状态），而不是显示一个看似真实的假数字（[NN/g Skeleton Screens 101](https://www.nngroup.com/articles/skeleton-screens/)，2023-06-04）。
- **监控类指标必须可行动**（NN/g dashboard 定义，2017）：一个既非真实、也不导向任何动作的指标（如硬编码的"活跃"）两条标准都不满足。
- **现状问题（务必在改版中修复）**：`dashboard.rs` 的 `StatCard` 目前硬编码 `trend: "+12%"` / `"活跃"` / `"待处理"`。
- **诚实 fallback 的形态**：
  1. 有真实口径 → 显示真实 delta + 时间窗文案（"本周新增 N 篇"）；
  2. 没有口径 → 只显示数值与标签，卡片依然成立（Refactoring UI：数值本身即主角）；
  3. 接口失败 → 该卡显示"数据加载失败 + 重试"，或整卡隐藏，而不是显示 0（0 是真实数据，失败不是 0）。

---

## 6. 暗色模式与主题

### 来源共识

- **用语义化的自适应颜色变量，杜绝硬编码色值。** Apple HIG Dark Mode："拥抱随外观自适应的语义色……避免硬编码颜色值或不会自适应的颜色"；暗色不是亮色的简单反色——"背景更暗、前景更亮"，且两种模式都要实测（配合 Increase Contrast / Reduce Transparency 开关）（[Apple HIG — Dark Mode](https://developer.apple.com/design/human-interface-guidelines/dark-mode)，2024-08-06 更新）。
- **web 端的推荐架构就是 CSS 变量双主题**：web.dev 示例用 `--color` / `--background-color` 两套变量 + 通用样式表，避免下载未使用模式的 CSS、避免 FOUC（[prefers-color-scheme: Hello darkness, my old friend](https://web.dev/articles/prefers-color-scheme)，Thomas Steiner，页面未标注日期）。本仓库的 `paper-*` 变量体系与此完全同构。
- **色阶要预定义、按用途映射。** Vercel Geist 的 10 级色阶每级有固定用途（100 默认背景 / 200 hover 背景 / 400 默认边框 / 900 次级文字 / 1000 主文字），页面背景只给两个层级（[Geist Colors](https://vercel.com/geist/colors)，页面未标注日期）；Refactoring UI 同样主张 8–10 级灰阶预先定义、不要运行时 `lighten()/darken()`，并指出**纯黑不自然，用极深灰代替**（[Building Your Color Palette](https://www.refactoringui.com/previews/building-your-color-palette)）。
- **工程化主题生成是可选进阶**：Linear 2024 改版把明暗双主题改为从 3 个变量（基色、强调色、对比度）在 LCH 感知均匀色彩空间生成，对比度变量天然支持高对比无障碍主题（[Linear，2024-03-28](https://linear.app/now/how-we-redesigned-the-linear-ui)）。

### 分歧点

- Apple 建议"避免提供应用内外观开关"（以系统为准）；web.dev 则演示了 dark-mode-toggle 覆盖系统偏好的做法。对博客后台：**跟随系统 + 可选覆盖**是务实的中间态（读者侧已有主题切换的先例时保持一致）。

### 本仓库建议

- 继续 `paper-*` 语义变量路线，不改架构；新增任何颜色（如趋势红/绿、amber 强调）都要在明暗两套变量下各自校准对比度（WCAG 文本 4.5:1；图标/组件 3:1）。
- 暗色模式下阴影几乎不可见——"shadow-not-border"的分层策略在暗色下需依赖 `--color-paper-entry` 等明度差来撑层级（这正是 Geist 两个 background 层级与 Apple "前景更亮"原则的用法）。
- 暗色背景避免纯黑（Refactoring UI）。

---

## 7. 反模式清单（Dashboard anti-patterns）

逐条溯源：

1. **饼图/环图/雷达/汽车仪表盘 gauge**：面积与角度编码定量信息效率低，占空间（[NN/g，2017-06-18](https://www.nngroup.com/articles/dashboards-preattentive/)）。
2. **3D 图表**：透视扭曲面积与对齐，定量关系被系统性误读（同上）。
3. **用颜色表达数值大小**：颜色是前注意特征但无自然顺序，只能做次级分组线索（同上）。
4. **硬编码/编造指标**：欺骗性设计近亲，侵蚀整个页面可信度（[NN/g Deceptive Patterns，2023-12-01](https://www.nngroup.com/articles/deceptive-patterns/)）。
5. **无时间窗的 delta**："+12%" 不写对比期即误导（综合 shadcn footer 约定与 [Apple HIG Charts](https://developer.apple.com/design/human-interface-guidelines/charts) 的"避免歧义格式"）。
6. **frame-display 骨架屏**（只有框架没有内容仿形）（[NN/g Skeleton Screens，2023-06-04](https://www.nngroup.com/articles/skeleton-screens/)）。
7. **<1s 的加载闪骨架/闪 spinner**：闪烁比不显示更糟（同上）。
8. **超时长动画**：>500ms 即"拖"（[NN/g，2020-02-09](https://www.nngroup.com/articles/animation-duration/)）；hover 反馈超过 ~200ms 显钝（[Atlassian Motion](https://atlassian.design/foundations/motion)）。
9. **无信息量的装饰动画 / 注意力劫持**（[NN/g，2020-01-12](https://www.nngroup.com/articles/animation-purpose-ux/)）；检验标准："删掉它用户会丢失信息吗？"（Atlassian）。
10. **忽略 `prefers-reduced-motion`**（[WCAG 2.2 SC 2.3.3](https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html)；[web.dev](https://web.dev/articles/prefers-reduced-motion)）。
11. **只用颜色表达涨跌/状态**（[WCAG 2.2 SC 1.4.1](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html)）。
12. **为"看起来像 dashboard"而破坏熟悉范式**（Google 播放列表实验：新颖但不可用；[Google Design，2025](https://design.google/library/expressive-material-design-google-research)）。
13. **暗色 = 亮色简单反色 / 硬编码色值**（[Apple HIG Dark Mode，2024-08-06 更新](https://developer.apple.com/design/human-interface-guidelines/dark-mode)）。
14. **把深度分析混进监控页**（Stephen Few，[Dashboard Confusion Revisited，2007](https://www.perceptualedge.com/articles/visual_business_intelligence/dboard_confusion_revisited.pdf)）。
15. **渐变堆料**：shadcn dashboard-01 卡片默认用 `bg-gradient-to-t from-primary/5`——与本仓库"无渐变"体系冲突，照抄模板时须剔除。

---

## 落地清单（映射到 `src/pages/admin/dashboard.rs` 组件）

### StatCard（统计卡 × 3）

- [ ] **删除硬编码 trend**（`"+12%"`/`"活跃"`/`"待处理"`）。有真实口径则显示真实 delta + 时间窗文案（如"本周新增 N 篇"），否则省略 trend 字段。（§5；NN/g 2023 / Refactoring UI）
- [ ] 数值加 `tabular-nums`，保持 `text-4xl` 大字号 + 轻字重；标签保持 `text-sm` + `--color-paper-secondary` 弱化。（§1；Refactoring UI / shadcn）
- [ ] 若引入涨跌指示：颜色 + ↑/↓ 图标双编码。（§1；WCAG 1.4.1）
- [ ] 接口失败时显示"加载失败 + 重试"或隐藏该卡，**不要回退成 0**。（§5）

### CountUp（数字滚动）

- [ ] 时长 900ms → **400–500ms**，easeOut 系缓动保留。（§4；NN/g 2020 上限 500ms）
- [ ] 保留"仅在数据首次到达时播放"的语义（数字滚动 = "数据已聚合完成"的信号，而非装饰）。（§4；NN/g Hipmunk 先例）
- [ ] 保留 reduced-motion 终值直出。（§4；WCAG 2.3.3 / C39）

### 待审评论卡（第 4 张行动卡）

- [ ] `duration-300` → `duration-150`/`duration-200`，缓动 ease-out；`-translate-y-1` + `hover:shadow-md` 幅度可保留。（§3；Atlassian 50–150ms / NN/g ~100ms 反馈）
- [ ] `animation-duration: 600ms` 入场 → ≤400ms。（§3）
- [ ] amber 强调 + "去审核 →"文案（颜色与文字双编码）已符合，保留；确认暗色下 amber-500 对背景对比 ≥4.5:1。（§6）
- [ ] 保持"数据就绪后补挂入场类"的挂法（避免骨架截断动画），这是正确模式。（§4；仓库既有约定）

### 近期文章列表（RecentPostItem × 5）

- [ ] 交错 80ms/行、整页入场 <1s 在预算内，保持；单元素入场 ≤400ms、ease-out。（§3）
- [ ] 保持容器 + `divide-y` 的行式列表，不要改成每行一张卡片。（§2；Linear 减噪原则）
- [ ] 空状态 `EmptyState`（含"写文章"主操作）符合"指标须可行动"原则，保持。（§2；NN/g 行动导向定义）

### 页头操作区

- [ ] 保持单主按钮（"发布文章" `BTN_PRIMARY`）+ 次按钮（"全部文章" `BTN_SECONDARY`）：主操作通过尺寸/颜色/容器被首先看到（M3 Expressive：关键元素定位最高快 4 倍）。（§2；Google Design 2025）

### 骨架屏

- [ ] 保持线框仿形 + 克制的 `animate-pulse`；SSR 直出或数据 <1s 到达时避免闪烁骨架。（§4；NN/g 2023）
- [ ] 骨架块使用 `--color-paper-*` 变量而非硬编码灰色，保证暗色模式正确。（§6）

### 全局自查

- [ ] 无渐变（shadcn 卡片的 `from-primary/5` 渐变不引入）、无 gauge/饼环图、无 3D。（§7）
- [ ] 新增颜色全部走 `paper-*` / 语义变量并在明暗双主题下分别验证对比度。（§6）

---

## 参考来源

1. NN/g — Dashboards: Making Charts and Graphs Easier to Understand（2017-06-18）: https://www.nngroup.com/articles/dashboards-preattentive/
2. NN/g — Skeleton Screens 101（2023-06-04）: https://www.nngroup.com/articles/skeleton-screens/
3. NN/g — Executing UX Animations: Duration and Motion Characteristics（2020-02-09）: https://www.nngroup.com/articles/animation-duration/
4. NN/g — The Role of Animation and Motion in UX（2020-01-12）: https://www.nngroup.com/articles/animation-purpose-ux/
5. NN/g — Deceptive Patterns in UX: How to Recognize and Avoid Them（2023-12-01）: https://www.nngroup.com/articles/deceptive-patterns/
6. Material Design 3 — Motion（how it works）: https://m3.material.io/styles/motion/overview/how-it-works
7. Material Design 3 — Easing and duration（Expressive 弹簧物理迁移）: https://m3.material.io/styles/motion/easing-and-duration
8. Google Design — Expressive Design: Google's UX Research（M3 Expressive，2025）: https://design.google/library/expressive-material-design-google-research
9. Apple HIG — Charts: https://developer.apple.com/design/human-interface-guidelines/charts
10. Apple HIG — Dark Mode（2024-08-06 更新）: https://developer.apple.com/design/human-interface-guidelines/dark-mode
11. web.dev — prefers-reduced-motion: Sometimes less movement is more（Thomas Steiner）: https://web.dev/articles/prefers-reduced-motion
12. web.dev — prefers-color-scheme: Hello darkness, my old friend（Thomas Steiner）: https://web.dev/articles/prefers-color-scheme
13. WCAG 2.2 — Understanding SC 2.3.3 Animation from Interactions: https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html
14. WCAG 2.2 — Understanding SC 1.4.1 Use of Color: https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html
15. Refactoring UI — Labels are a last resort: https://www.refactoringui.com/previews/labels-are-a-last-resort
16. Refactoring UI — Building Your Color Palette: https://www.refactoringui.com/previews/building-your-color-palette
17. Atlassian Design — Motion（时长/缓动令牌）: https://atlassian.design/foundations/motion
18. shadcn/ui — Blocks（dashboard-01 预览）: https://ui.shadcn.com/blocks ；注册表源码: https://ui.shadcn.com/r/styles/new-york-v4/dashboard-01.json
19. Vercel — Geist Design System / Colors: https://vercel.com/geist/introduction ，https://vercel.com/geist/colors
20. Linear — How we redesigned the Linear UI (part Ⅱ)（2024-03-28）: https://linear.app/now/how-we-redesigned-the-linear-ui
21. Stephen Few — Dashboard Confusion Revisited（Perceptual Edge 白皮书，2007）: https://www.perceptualedge.com/articles/visual_business_intelligence/dboard_confusion_revisited.pdf
22. Tailwind — Catalyst UI Kit: https://catalyst.tailwindui.com
