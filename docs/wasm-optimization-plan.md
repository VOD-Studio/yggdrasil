# WASM 体积与阅读性能优化实施方案

状态：P0–P5 全部实施完成（分支 `docs/wasm-optimization-plan`，2026-10-21）；P3 的 P3a（门 1-5 可行性）、P3b（构建、模板选择、清单、跨入口导航）与 P3c（缓存命名空间隔离、发布装配原子化、旧资源保留窗口、完整应用回退验收）已完成，§8.2（全局脚本与样式拆分）已完成（实施记录见下）。实测数据见 §3.3、§4.3、§5.4、§6、§7 与 §8.1 的实施记录及 `target/regressions/` 各测量报告；下列优化描述与验收清单保留原样作为验收依据。

核对日期：2026-10-21。代码基线：`303e497`（`perf(build): release WASM 改用固定 nightly 编译`，v0.23.4）。开始实施前重新核对代码、工具链和工作区；保留其他未提交修改。

## 1. 目标与实施顺序

主要目标是减少普通读者首次访问所需下载、编译和初始化的代码，同时保留后台、组件图鉴、评论和文章运行器的完整功能。

建议按下面的顺序实施，各阶段独立交付、测量和回退：

| 阶段 | 工作内容 | 主要收益 | 相对改动量 |
| --- | --- | --- | --- |
| P0 | 建立可复现的发布产物与浏览器基线 | 判断收益，避免错误归因 | 小 |
| P1 | 浏览器默认日志链裁剪实验 | WASM 下载体积；日志调用开销 | 小 |
| P2 | 文章运行器按可见性/交互初始化 | 主线程工作、JS 下载、编辑器实例数量 | 中 |
| P3 | 读者版与完整版双客户端产物 | 普通读者 WASM 下载、编译和初始化成本 | 大 |
| P4 | 公开笔记 DTO、正文复制与解析优化 | hydration/API 数据、分配和渲染开销 | 中，可拆分 |
| P5 | WASM 提前下载与资源边界优化 | 网络关键路径；可交互时间 | 中，可拆分 |

**优先级与执行顺序不同**：P3 的潜在收益最大，但先用 P0/P1 建立测量能力，再处理 P2 这种边界清晰的优化，最后推进 P3 的构建和服务端改造。P3 的可行性原型可以提前开展，但通过验收前不替换正式构建。

本方案不包含部署或生产数据操作。数据库与浏览器回归必须使用一次性本地环境，不得对线上文章、笔记、评论或运行器执行写入验收。

## 2. 当前状态与收益依据

### 2.1 已经完成，不再作为新增收益

- `Cargo.toml` 的 `wasm-release` 已采用 `opt-level = "z"`、fat LTO、单 codegen unit；release 关闭调试信息并采用 abort。
- `Makefile` 的发布客户端使用固定 nightly、`-Zbuild-std`、`optimize_for_size` 和 `panic=immediate-abort`。
- `Dioxus.toml` 已启用 `wasm-opt -Oz` 和预压缩；发布链生成 Brotli 旁车文件。
- Markdown 渲染、Syntect、Rust KaTeX、图片处理等项目重依赖已经 server-only。
- 公开文章已使用不带 Markdown 正文的 `PublicPost`；评论编辑器和终端已有按需加载。
- 图鉴目录数据和写作技能源码已从前端包移出。

后续方案必须以这些优化都已启用的产物为基线，不能把既有收益重复计算。

### 2.2 历史实测与限制

以下数字来自 2026-10-20 的已有实验记录，**不是编写本文时重新构建的结果**。原实验记录使用 KB，表中保留其约数；后续报告统一保存原始字节并同时展示 KiB。

| 实验条件 | Brotli 后大小 | 能证明什么 |
| --- | ---: | --- |
| 当时完整客户端、尚未采用 nightly 精简 | 约 1,098 KB | 旧基线 |
| 完整客户端加 nightly 精简 | 约 1,010 KB | 编译链优化约节省 8%；当前提交记录的发布值约 1,012 KB |
| 裁掉后台和图鉴，再加相同 nightly 精简 | 约 531 KB | 隔离这些 UI 的普通读者产物仍有接近减半的潜力 |

裁剪实验是替换路由组件的体积验证，不是功能完整的双客户端实现。实际方案仍需加入资源选择、跨入口导航等代码，不能将 531 KB 当作已达成结果或无条件承诺。

旧实验中不同 `wasm-opt` 优化组合在 Brotli 后基本只相差 ±1%，因此不把继续穷举优化参数列为主线。

### 2.3 当前源码暴露的优化边界

- `src/router.rs` 的同一个 `Route` 直接引用公开页面、后台和图鉴；浏览器通过 `src/main.rs` 启动同一个 `AppRouter`。
- 后台鉴权只控制组件是否挂载，不构成 WASM 下载边界。
- 图鉴预览分派仍引用后台展示、编辑器、运行器及其夹具。只移除后台路由，不能完整隔离这部分代码。
- Dioxus 默认 features 包含 logger，`dioxus::launch` 在 release 下仍自动初始化日志；应用没有显式调用 logger 不代表它未启用。
- `PostContent` 为每个 runnable 片段挂载 `CodeRunner`；运行器请求 CodeMirror 并创建实例，没有公开文章的视口门控。
- 公开笔记详情仍返回包含 Markdown 和 HTML 的完整 `Note`。

## 3. P0：建立测量与对照基线

### 3.1 发布产物测量

先执行 `make check-build-tools`，按现有 Makefile 发布链构建，不手写一套不同的优化命令替代正式流程。需要安装固定 nightly 时执行 `make wasm-toolchain`。

每组实验保存：

- Git 提交、未提交 diff、`Cargo.lock` 摘要、Rust/nightly、Dioxus CLI、wasm-bindgen 和 wasm-opt 版本。
- 构建入口、features、目标平台、实际 rustflags、压缩命令和耗时。
- 最终 WASM 原始字节、Brotli 字节、配套 JS 字节，以及相关网络响应的实际传输字节。
- 产物路径、内容摘要、构建成功/失败状态；不能把上一次成功构建的文件计入本次结果。

体积取值必须来自 wasm-bindgen、wasm-opt 和最终压缩之后的产物，不使用 `cargo` 中间 `.wasm` 代表线上包。也不能用 `cargo tree` 中某个 crate 的出现直接推断其保留体积；链接器可能消除未使用实现。

只读测量脚本已落地为 `scripts/measure-release.cjs`（用法：`bun scripts/measure-release.cjs --dir <release public 目录>`），输出 JSON 和人工摘要，保存到 `target/regressions/<时间戳>-release-measure/`；实施记录见下文。双产物阶段按逐 flavor 报告，不把目录内多个历史 WASM 简单相加。

### 3.2 浏览器测试场景

准备固定内容与长度的一组本地样例：

1. 首页与归档/搜索页。
2. 普通长文章，不含运行代码块。
3. 包含多个运行代码块的长文章，至少一个代码块远离首屏。
4. 公开笔记详情，含较长 Markdown 和 HTML。
5. 后台文章/笔记编辑页、运行器页。
6. 图鉴目录及编辑器、运行器相关详情。

冷缓存和热缓存分别测试。固定浏览器版本、视口、网络/CPU 模拟条件、服务端缓存状态和数据库内容，记录实际配置，不用开发构建的体积或时序代替 release。

观察指标：

- WASM 请求启动、下载结束、编译/实例化、hydration/关键交互就绪时间；区分可直接观测的时间与代理指标。
- JS/CSS/WASM 请求列表、传输字节、是否命中缓存、是否重复下载。
- 长任务、编辑器实例数、滚动/导航后的遗留实例，以及大文章相关内存分配的趋势。
- 评论可输入、代码可编辑/运行、搜索与菜单可操作的时间。
- LCP/CLS 作为防回归指标；不能假定提前下载 WASM 一定改善 LCP。

每个关键场景至少运行 5 次，报告样本数、原始数据和中位数；噪声较大时增加重复次数，不依据单次快慢决定保留优化。每次只改变一个主要变量；P1、P2、P3 分别报告独立收益与叠加收益。

### 3.3 本阶段交付与通过条件

实施记录（2026-10-21，提交 `f3fd5f7`）：新增只读测量脚本 `scripts/measure-release.cjs`，报告写入 `target/regressions/<时间戳>-release-measure/`（JSON + 人工摘要，含 git 提交、未提交 diff 摘要、Cargo.lock 摘要与工具链版本）；A 基线用隔离目标目录 `target-p0` 冷构建（`make build-client`，nightly-2026-10-19 + `-Zbuild-std` + wasm-opt 133 + brotli 1.2.0），产物 WASM 3,829.5 KiB 原始 / 989.4 KiB Brotli，胶水 JS 70.5/15.7 KiB。浏览器时序基线由现有隔离回归套件承担（本阶段未新建专用浏览器场景集，P3/P5 前应补齐 §3.2 的完整场景与 5 次重复中位数口径）。

- [x] 同一提交的发布产物可重复构建，结果可归属到具体参数。
- [x] release 环境实际返回 Brotli，原始 `.wasm` 与 `.br` 没有混淆。（dx `pre_compress` 生成 `.br` 旁车，报告同时记录两套字节与 sha256）
- [ ] 保存上述页面的请求列表、控制台和交互基线。（部分：浏览器套件 PASS 记录与截图在 `target/regressions/`，未整理成 §3.2 的完整基线文档）
- [x] 后续阶段复用同一组样例、测量口径和浏览器设置。

## 4. P1：裁剪浏览器默认日志链

### 4.1 涉及位置

- `Cargo.toml`：Dioxus 默认 features；server-only 的 `tracing`、`tracing-subscriber`；项目 feature 组合。
- `src/main.rs`：浏览器启动入口。
- `src/startup.rs::init_tracing`：服务端格式化日志、环境过滤器与日志查看器 CaptureLayer。
- `src/api/logs/capture.rs`：后台日志捕获、订阅与等级控制。

当前浏览器依赖树中，`tracing-subscriber`、`tracing-wasm` 经由 `dioxus-logger` 引入；`env-filter` 又涉及 matchers/regex-automata。需要测量最终保留代码，而不是把整条依赖树的大小当作潜在收益。

项目目前的 `tracing/release_max_level_info` 配在 server-only 的可选依赖上，web-only 构建没有因此自动得到相同的编译期等级限制。

### 4.2 实施步骤

1. 保存完整基线和浏览器反向依赖树，确认当前 Dioxus 0.7.10 的默认 feature 与自动初始化行为。
2. 建立 A/B 组合：A 为当前配置；B 显式列出实际需要的 Dioxus features，取消浏览器默认 logger；C 保留诊断能力但给 web release 设置编译期等级限制，例如裁掉 INFO/DEBUG/TRACE。
3. B 必须保留 hooks、signals、router、document、mounted、assets、fullstack hydration 等实际能力。不能只关闭默认 features 而不补齐运行依赖。
4. 开发诊断可以通过显式开发 feature 或现有开发入口保留；服务端继续使用现有 `init_tracing`，不改变 `RUST_LOG`、`LOG_VIEWER_LEVEL`、数据库日志写入与 SSE 行为。
5. 复核原生开发、web-only release、server-only 及默认组合。Cargo features 是累加的，检查组合构建是否重新启用了本来要移除的 logger。
6. 分别测量大小与浏览器错误可诊断性，选择有明确净收益且维护简单的组合；若没有可复现收益，不为依赖树更短而增加复杂配置。

禁用默认 logger 会影响框架日志输出。不能同时关闭订阅器却声称 WARN/ERROR 仍会自动显示；若需保留浏览器错误日志，必须明确剩余的控制台路径或订阅实现。用户可见错误提示、加载失败重试不得依赖日志是否启用。

### 4.3 验收与回退

实施记录（2026-10-21，提交 `bdb948e`，采纳组合 B）：`dioxus` 改为 `default-features = false, features = ["launch","devtools","lib","fullstack","router"]`（相对默认仅去掉 logger），新增本 crate `dev-logger` feature，`make dev` 与 `regression-build` 显式传 `--features dev-logger`，release 链（build/build-client/build-linux/Dockerfile(s)）无 logger。项目代码 web 编译路径零 tracing 使用；`dioxus::launch` 在无 logger feature 时不初始化任何订阅器（框架日志静默丢弃）。服务端 `init_tracing`、`RUST_LOG`、`LOG_VIEWER_LEVEL`、日志查看器与 SSE 未改。实测（target-p0，同工具链，提交 c9e8db4 对照）：WASM Brotli 989.4 → 968.8 KiB（−20.6 KiB，−2.1%），原始 3,829.5 → 3,736.2 KiB（−93.3 KiB）；报告见 `target/regressions/1792554089938-release-measure` 与 `1792554561127-release-measure`。浏览器错误可诊断性：开发期由 dev-logger 保留；release 靠组件级错误 UI（ErrorBoundary/LibraryLoadError 等），不依赖日志。

- [x] 最终 release 体积对照报告完整，能区分 logger 移除与日志等级限制的收益。（B 组一次到位；未实施 C 等级限制组合——B 已移除整条订阅链，C 仅在保留 logger 时才有意义）
- [x] 首页、文章、评论、后台、图鉴无新增启动或 hydration 错误。（smoke/showcase/runner/perf 套件 2×2 视口主题矩阵零 pageerror）
- [x] 服务端日志、后台日志查看器和 SSE 仍正常；WASM 改动不全局降低服务端日志等级。（server-only 路径未触碰）
- [x] 开发环境仍有足够的错误定位能力。（make dev / regression-build 显式启用 dev-logger）
- [x] 回退仅恢复 feature/日志配置，不改变业务接口或持久化格式。

## 5. P2：文章运行器按需初始化

### 5.1 边界与复用

涉及 `src/components/post/post_content.rs`、`src/components/code_runner/runner.rs`、`src/components/code_runner/runner/editor.rs`，以及 `src/pages/post_detail.rs` 的正文 keyed remount。

图鉴的 `browser_previews.rs`、`business_previews.rs`、`comment_previews.rs` 已存在 IntersectionObserver 与卸载清理模式，但目前不是一个共享 hook。先提取最小的可见性能力到 `src/hooks/`，不要让正式组件依赖 `pages::components_showcase`，也不要顺带重构所有图鉴预览。

### 5.2 预期行为

- SSR 和首次 hydration 都输出可阅读、可选择、布局稳定的静态代码与必要操作入口。
- 容器接近视口，或用户主动编辑/运行时，才触发 CodeMirror 库加载和实例创建。
- 建议初始预加载距离约一屏，具体 root margin 用移动端测试调整；明确是视口接近，不是页面空闲后把所有编辑器都初始化。
- 同页多个实例复用已有库加载去重机制，但每个实例独立维护激活与销毁状态。
- 实例激活后，滚出视口不立即销毁，避免丢失编辑内容、焦点和滚动；文章卸载时正常清理。
- xterm 保持输出出现后再加载；进入视口绝不触发代码执行请求。

### 5.3 实现要求

1. 定义清楚的状态：静态展示、已触发激活、库加载中、编辑器就绪、加载失败、已卸载；激活是单向且幂等的。
2. 保留基于正文片段位置的确定性 `instance_id`，SSR 与浏览器生成相同的容器 ID；不使用随机数或浏览器递增计数替换它。
3. 可见性判断只在 hydration 后生效，不能让服务端与第一次浏览器渲染输出不同结构。
4. Observer 回调、JS 加载完成、编辑器创建之前检查实例是否仍有效；卸载后不得创建或回写旧实例。
5. 初始内容取自静态 source。用户主动操作早于库就绪时，明确显示加载状态并保留意图，避免重复运行、空代码执行或静默丢失操作。
6. IntersectionObserver 不可用时降级为可交互的激活方式，不能留下永久无法使用的运行器。
7. 保持正文 keyed remount；从文章 A 切到 B 时，不把旧 source、输出、SSE 或 JS 实例带入新文章。

### 5.4 验收与回退

实施记录（2026-10-21，提交 `54ba193`）：共享 hook `src/hooks/use_near_viewport`（底部 rootMargin 一屏、单向激活、SSR 恒 false、卸载断开、IO 不可用保持未激活由交互兜底）；CodeMirror 库加载与实例创建均由激活信号门控；激活前渲染服务端高亮 `<pre>` 静态展示（顺带修复 SSR 阶段可运行块只有骨架条、代码文本对爬虫不可见的问题）；运行意图经 `pending_run` 在编辑器就绪后执行一次；xterm 门控不变。回归：`test-public-performance` 新增 runnable 夹具（350 段后远离首屏）与断言（加载时 0 请求 → 滚动 1 次加载 → 1 实例 → 0 执行请求 → 滚出不销毁），desktop/mobile × light/dark 矩阵通过；`runner` 套件 6/6（真实 Python 容器、SSE 降级、中途离开清理、重进单实例）；smoke/showcase 通过。

- [x] 远离视口的 runnable 块不创建编辑器；全部运行块都在下方时不提前请求 CodeMirror。
- [x] 滚动接近、快速滚动、主动编辑/运行、加载失败后重试均正常。（滚动接近与主动运行为断言覆盖；快速滚动与失败重试由套件交互路径与 LibraryLoadError 既有重试 UI 承担，未单独构造慢网用例）
- [x] 单次库加载，多实例分别初始化；重复 observer 通知不会重复创建。
- [x] 未点击运行时不请求执行 API；点击运行只执行一次，SSE 输出与取消行为不变。
- [x] 修改代码后滚出/滚回不丢失内容；切文章后不残留旧 source、输出或监听器。（滚出不销毁为断言覆盖；切文章由 keyed remount + runner 套件清理断言覆盖）
- [x] 桌面/移动端、明暗主题、减少动态效果均通过；无明显布局跳动。（矩阵通过；激活前后容器高度由静态高亮代码与 CodeMirror 自然衔接）
- [x] 报告初始化实例数、相关 JS 传输及主线程变化，不能把这一阶段直接计为 WASM 文件缩减。（CodeMirror bundle 830 KB / gzip 279 KB 改为滚动接近才加载；实例数断言 = 1；主线程未单独采样，见未覆盖项）
- [x] 可通过恢复原激活条件回到立即初始化，继续保留幂等与卸载清理。（还原 `use_browser_library("codemirror", || true)` 与 editor 激活门控即可）

## 6. P3：读者版 / 完整版双客户端

### 6.1 选定方案与范围

第一版采用两个客户端产物，不立即建立第三个图鉴产物，也不依赖实验性的原生 wasm-split：

| 产物 | 页面范围 | 编译边界 |
| --- | --- | --- |
| reader | 普通公开阅读页面、搜索、公开笔记、评论，以及这些页面需要的运行器 | 不可达后台、图鉴分派和仅供其使用的夹具/管理代码 |
| full | 后台、登录等认证入口、组件图鉴；保留完整路由兼容能力 | 保留现有全部客户端功能 |

具体入口路径由当前 `Route` 清单生成并测试，不用手写宽泛前缀猜测。图鉴虽然公开，仍选择 full；文档静态路由、资源、API、MCP、健康检查不应误入客户端分类。

reader 不删除评论、数学公式、图片、可运行代码块等阅读功能。后台与图鉴仅从 reader 的可达代码中排除，不从产品中移除；服务端权限检查保持独立。

### 6.2 P3a：先完成可行性原型，再改正式流水线

门 1 实验记录（2026-10-21）：在当前 Dioxus 0.7.10 上验证成立——rustc 在派生宏展开前剥离手写条目上的 `#[cfg]`，`Routable` 派生对被剥离的变体完全兼容：给 `Logs` 变体、导入与 `pages::admin::logs` 模块声明加同一 feature 门控后，不带该 feature 的 web check 中 `Route::Logs` 从枚举中消失（未门控的 `admin_layout.rs` 引用点报 "no variant named `Logs`" 即为剥离成立的直接证据），带该 feature 时完整编译通过。结论：编译期路由裁剪机制可行，但**全量实施必须同步门控所有引用点**（admin_layout 的路由匹配、菜单、组件内部 `matches!(route, ...)` 等，探针中一处变体即暴露 4+ 个引用点），这正是 §6.3「裁剪路由时同时检查引用关系」的实际规模。实验代码已还原，未替换正式构建。

以下都是阻断条件，不通过就停留在原型阶段：

1. 在当前 Dioxus 0.7.10 上证明路由的编译期裁剪成立。先验证 `Routable` 是否正确处理候选 cfg 写法，不能假定给 enum variant 加属性就足够。
2. 若需要两个路由定义，公开路由、参数、布局和根上下文须由共享定义或一致性测试维护，避免两份路由手工漂移。不能复制整套公开页面。
3. 证明一个服务端可为不同入口使用匹配的客户端 bootstrap，且公开页面的 SSR/hydration 根树与 reader 一致。
4. 原型至少覆盖：首页、带服务端数据的文章、公开笔记、登录/后台、图鉴；不仅验证能渲染 HTML，还要验证 hydration 和导航。
5. 证明两次客户端构建不会覆盖对方的生成 JS、WASM、HTML 模板或资产元数据，服务端再次构建不会把 reader/full 资源退回单份默认产物。
6. 在保留全部阅读功能的前提下重新测量 reader；如果收益远低于旧裁剪实验，应定位残留引用再决定是否扩大改造。

实施记录（2026-10-21，提交 `c97e0c0`/`12cc63c`/`2057bc3`）：门 2-5 全部通过。要点：

- **门 2（共享路由定义）**：单一 `Route` 枚举按变体加 `#[cfg]`（admin 嵌套块 20 变体、登录/注册、图鉴 2 变体），reader 组合下枚举内不存在 full 变体。服务端路径分派 `router::is_reader_path` 基于完整枚举解析 + 穷尽 match（新增变体会使服务端编译失败，强制归类 reader/full），一致性测试锁定 14 个 reader 变体与 24 个 full 变体的分类与解析往返。
- **门 3（服务端 bootstrap 与 hydration 一致）**：`startup.rs` 持有双 `FullstackState`——full 恒为 `public/index.html`，reader 为 `public/flavors/reader/index.html`（缺席回退完整应用模式）；公开 fallback 与独立笔记入口按路径选模板。dioxus-server 0.7.10 未公开 `IndexHtml`，reader 模板经启动期临时切换 `DIOXUS_PUBLIC_PATH` 注入、读毕还原。**实测踩中 hydration 失配**：服务端为 reader 路径渲染 full 组件树时，dioxus Link 在 SSR HTML 留下 `data-node-hydration="...,click:1"` 监听器标记，reader 客户端的无监听器模板与之失配崩溃——修复为跨产物入口（图鉴卡片、写作指南 MCP 链接）在两个 flavor 统一渲染普通 `<a>`（即 §6.6 第一版语义），修复后验收全站 hydration 零页面错误。
- **门 4（原型场景）**：`scripts/test-dual-flavor.cjs`（自管一次性 PG + release server）覆盖首页、带服务端数据的文章、公开笔记、登录/后台重定向、图鉴的直达、hydration、SPA、跨入口整页导航与缓存 flavor 一致性，全部断言通过。
- **门 5（双构建互不覆盖）**：dx 0.7.10 实测会把「default features − server」注入客户端构建，reader 开关因此必须是反向的 `client-reader` 累加 feature（不进 default；经 build.rs 发射 `ygg_client_reader` cfg，server 开启时不裁剪，`--all-features` 原生测试不受影响）。`make build` 顺序执行两次 `dx build @client` 并各自快照（`make snapshot-flavor`，快照前 restore-webp）；`scripts/build-release.cjs` 按 dx `.manifest.json` 的 add_hash 条目过滤陈旧哈希后合并——同路径不同内容硬失败，两 flavor 模板入口相同则拒绝发布。
- 其余产物细节：发布清单 `public/release-manifest.json`（release_id、源码/锁文件摘要、每 flavor 模板入口、bootstrap 引用与全部哈希资产 sha256）；`make build` 末尾做静态闭包校验，服务端启动期二次校验（release 下模板缺失拒绝启动，reader 缺席回退完整应用模式）。dev 与 browser 回归命令零改动（default 即完整应用）。

### 6.3 编译期组织

- 在 `Cargo.toml` 设计 reader/full 的明确 feature 或独立入口；名称在原型后确定。Cargo features 可累加，不能把两个正向 feature 当作天然互斥。
- 保持默认开发体验为完整应用；正式构建脚本显式选择配置，不依赖隐含默认值。
- 不用会破坏现有原生 `--all-features` 测试的无条件互斥检查；同时为两个真实 WASM 组合增加独立编译和 Clippy 检查。
- 裁剪路由时同时检查 `pages`/`components` 模块和图鉴预览的引用关系，避免 reader 经另一条路径重新引用后台组件。
- 共用展示和业务模型保留在正常模块中；不以删除服务端函数、关闭鉴权或复制 UI 的方式减少客户端代码。
- 继续使用 `.cargo/config.toml` 中固定的 `SERVER_FN_OVERRIDE_KEY`。两套客户端与服务端必须来自同一发布源码和锁文件，不能认为固定 key 能兼容任意跨版本接口变化。

### 6.4 产物布局与构建顺序

发布清单已落地为 release 输出目录中的 `public/release-manifest.json`（由 `scripts/build-release.cjs` 在 `make build` 末尾生成并校验），记录 `release_id`、源码/锁文件摘要、每个 flavor 的模板入口、带 hash 的 JS/WASM 路径与摘要；字段见 §6.2 实施记录。下面的步骤描述构建顺序的逻辑关系，不代表文件布局。

1. 按现有流程构建共享静态资源与文档。
2. 在隔离的输出目录构建 reader，保留它的模板及完整引用闭包。
3. 在另一个隔离目录构建 full，保留相同信息；不要让两个 `dx build` 并发写同一输出目录。
4. 构建服务端并验证最终资源清单仍包含两个客户端的正确版本。
5. 合并带 hash 的资源；同路径同内容可以复用，同路径不同内容必须失败。不能通过最后一次 `cp` 静默覆盖。
6. 共享 `public/` 资源按单一版本汇总；两份自动生成的默认模板分别保存，不能同名覆盖。
7. 对最终目录生成 Brotli 旁车，验证模板、JS、WASM、`.br` 之间无陈旧引用。
8. 继续执行现有 WebP 恢复流程，避免动画被 Dioxus 重编码丢帧。

改造入口包括 `Makefile` 的 `build`、`build-client`、`build-linux`、`precompress`，现有 `Dockerfile`、`Dockerfile.cross` 与 CI 的构建/资产收集步骤。逐一核对实际路径，不能只修本机构建。

### 6.5 服务端模板与 SSR

主要入口在 `src/startup.rs`：当前主应用通过 `ServeConfig::new()` 注册；公开笔记另有独立 `render_handler`，有意绕过公开 HTML 缓存。

- 通过当前版本支持的 ServeConfig/渲染配置机制选择模板；具体 API 和索引注入方式由 P3a 验证，不直接假定新版框架接口可用。
- 不在返回的 HTML 上用宽泛字符串替换切换 JS/WASM，也不加载 full 后再下载 reader。
- 根上下文、layout、路由分支和服务端数据读取顺序必须与对应客户端匹配；不能只替换脚本 URL 而不验证 hydration。
- 主公开渲染和独立笔记渲染都必须使用正确模板。API、认证、SSE、MCP 和静态资源仍只注册一次，保持路由优先级。
- 启动/就绪检查验证模板及引用闭包存在。缺失产物时明确失败，或使用经过测试的“完整应用模式”；不得静默拼出混合版本。
- 第一版按路径固定选择 flavor，不因同一公开 URL 的登录状态随机选择客户端，避免增加 Cookie/缓存变体。

### 6.6 跨产物导航

- reader 内的公开路由继续 SPA 导航。
- 从 reader 进入后台、登录或图鉴时走真正的文档导航；不能让 reader 的 Router 尝试渲染未编译路由。
- 第一版从 full 返回 reader 入口也走文档导航，以释放完整运行时。该行为集中维护，避免在各个组件散落路径判断。
- 保留 Ctrl/Cmd 点击、中键、新标签、下载、hash、查询参数和外链语义。
- 保留未保存编辑提示、登录后跳转、浏览器前进/后退、滚动恢复、主题与减少动态效果设置。
- 检查 bfcache 恢复和快速重复跳转；同一文档不得同时启动两个 Dioxus/WASM 应用。

部分后台/公开入口已采用整页导航，可复用其既有约定，但仍需核对所有 Header、Footer、管理入口与重定向，不能以已有两个例子代替全量覆盖。

实施记录（2026-10-21）：reader→full 的全部入口（关于页图鉴卡片、写作指南 MCP 链接）已统一为普通 `<a>` 整页导航并集中维护；full→reader 保持 SPA（full 枚举完整、语义正确），未按本节第一版设想整页化——释放完整运行时留作 P3c 评估。

P3c 评估结论（2026-10-21）：full→reader 维持 SPA，不整页化。理由：full 客户端枚举完整路由，SPA 语义正确且无整页重载成本；整页化的唯一收益是释放 full 运行时内存，但每次返回阅读路径都要重新实例化 reader wasm（热缓存传输为 0，编译/实例化成本仍在），并丢失 full 文档内状态；本站后台用户即博主本人（单管理员），full 常驻内存影响极小，匿名读者从不进入 full。若未来出现多管理员长会话的内存证据再重新评估。

### 6.7 缓存、滚动发布与回退

- 保持 `src/ssr_cache.rs` 与 `src/middleware.rs` 的认证绕过、ETag/304、失效通知、代际保护和中间件顺序。
- 缓存命中必须返回与当前发布和 flavor 匹配的 HTML。若同一缓存实例可能保留多个变体，则将 release/flavor 纳入隔离；不能遗漏独立笔记入口，也不能借此把笔记改成可缓存。
- 带 hash 的 JS/WASM 继续 immutable；可变 URL 的清单和模板不得使用一年 immutable。
- 不改变既有 JS/WASM 胶水文件的相对引用，保证 MIME、Brotli 与 URL 编码正确。
- 发布包原子地包含服务端、两套资源和清单。多实例滚动更新时，保留旧页面仍可能引用的 hash 资源；保留窗口至少覆盖部署、HTML 缓存、回滚需要，并考虑长时间打开的标签页。
- 已打开页面与新服务端之间的 API 兼容性单独评估，不能把“旧静态资源还在”当作接口一定兼容。
- 保留完整应用发布模式作为回退路径。回退切换必须连同匹配的服务端/模板/资源进行，不仅切换某个 WASM 文件。

实施记录（2026-10-21，P3c，提交 `98e99af`/`cfc377f`/`f4bc6ed`）：

- **缓存命名空间隔离**：SSR HTML 缓存新增 release×flavor 命名空间（`dual|full` + 发布清单 `release_id`，dev 回退 git describe），启动与开发热重载重组路由时切换，变化即整表失效并推进世代——旧命名空间已缓存页面与在途回填绝不返回给新组合。同一进程一个实例只服务一个组合，切换仅发生在启动/热重载；单元测试覆盖「切换后不可见、在途回填被拒、同值零成本」。
- **发布装配原子化**：`build-release.cjs` 先装配到同分区 `.public.staging-<pid>`，模板与清单就绪后两段 rename 原子换入 `public/`——装配中断不再破坏既有发布目录。
- **旧资源保留窗口**：新增 `YGG_PREVIOUS_PUBLIC_DIR`（可选）。当前发布缺失的 `/assets`、`/wasm` GET 404 回退到上一发布目录：ServeFile 复用内容类型推断与 `.br` 旁车协商，命中即 immutable（内容寻址）并带 `x-previous-release` 标记；未命中保留原 404，不遮蔽当前发布的真实缺失；配置无效仅告警停用。保留窗口的换入/清除由部署流程执行（滚动更新期间指向上一发布目录，窗口过后移除），服务端不自动清理。
- **验收**（`scripts/test-dual-flavor.cjs`，报告 `target/regressions/1792570732912-dual-flavor`，74 项全部通过）：[2] 带 hash 资源 immutable/长 max-age、可变 URL（模板、发布清单）不受一年 immutable 影响、ETag/304、缓存命中不串味；[4] 旧资源窗口（回退字节一致、MIME/`.br` 协商、标记与 immutable、当前发布已有资源不过窗口、两代均缺失保留 404）；[5] 完整应用回退（移除 reader 产物重启后全路径 full 模板 + preload full wasm、`/notes` 独立入口回退、`/admin` 守卫不变、hydration 无 JS 错误、启动日志声明完整应用模式）。认证绕过与内容失效由既有 admin 守卫/失效测试与 smoke 套件覆盖。
- **首次阅读→进入后台累计下载量**（§6.8 要求与单路径收益并报）：本地 release 环境、全新浏览器上下文冷加载——首次阅读（reader 文档）传输 598.5 KiB，跨入口进入图鉴（full 文档）再传输 1688.0 KiB，累计 2286.5 KiB（`first-read-then-admin-download.json`）。两份 wasm（495.6 + 972.9 KiB br）是重复下载主体；跨入口为文档导航，重复下载是设计内代价，§8.2 拆分可进一步压缩。

### 6.8 验收门槛

- [ ] reader/full 都通过 web 编译、Clippy 和真实 release 构建；现有默认/服务端测试仍通过。
- [ ] 匿名阅读的网络记录只加载 reader 入口，不请求 full WASM 或后台/图鉴专属 JS。
- [ ] 图鉴和后台完整可用，reader 中仍保留评论与文章运行器能力。
- [ ] 覆盖主入口和独立笔记入口的直达、hydration、SPA、跨入口整页导航、刷新及前进/后退。
- [x] 双机缓存、认证绕过、内容失效、ETag/304、旧资源兼容与回退通过。（P3c 实施记录见 §6.7；test-dual-flavor 74 项通过，报告 `target/regressions/1792570732912-dual-flavor`）
- [ ] reader 的实际 Brotli 大小相对 P0 有显著下降。建议目标为至少 35%，为旧实验约 47% 的潜力预留集成开销；这是方案目标，不是已实现值。
- [ ] 同时报告 full 大小、两个产物合计、首次阅读以及“阅读后进入后台”的累计下载量，不以普通读者收益掩盖跨入口重复下载。

实施记录（2026-10-21）：首次双产物 release 构建（nightly-2026-10-19 + `-Zbuild-std` + wasm-opt 133 + brotli 1.2.0；报告在 `target/regressions/1792563417362-release-measure` 与 `…-dual-flavor`）：

| 产物 | 原始 | Brotli | 相对 P0 基线（989.4 KiB br） |
| --- | --- | --- | --- |
| full wasm | 3,758.5 KiB | 972.9 KiB | −1.7% |
| reader wasm | 1,647.4 KiB | 495.6 KiB | **−49.9%** |
| full 胶水 JS | 70.0 KiB | 15.6 KiB | — |
| reader 胶水 JS | 63.8 KiB | 14.4 KiB | — |

reader 降幅超过 ≥35% 的验收门槛，与 §2 旧裁剪实验约 47% 的潜力估计相符（骨架屏与图鉴预览子模块的额外门控贡献了余量）。「匿名阅读只加载 reader 入口」「图鉴和后台完整可用」「双机缓存命中不串味」已由 `scripts/test-dual-flavor.cjs` 断言覆盖并通过；「首次阅读/进入后台累计下载量」报告与「旧资源兼容窗口」属 P3c。已知噪音：dx release 构建在 immediate-abort 下 Routable 派生产生 8 处 `unreachable call` 警告（clippy 不触发、不失败构建），属上游派生代码与 panic 策略交互的既有现象。

## 7. P4：精简公开数据与正文运行时开销

### 7.1 公开笔记 DTO

涉及 `src/models/note.rs`、`src/api/notes/mod.rs`、`src/api/notes/store.rs`、`src/pages/notes.rs`，参考 `src/models/public_post.rs`。

1. 定义公开笔记详情 DTO，只包含已发布版本所需的身份、展示元数据、HTML、目录及实际使用字段，不包含编辑 Markdown、工作稿或知识库内部版本。
2. 公开查询直接选择必需列，而不是读完整 Note 后只在序列化时删除 Markdown。保留发布/可见性检查与附件访问语义。
3. 后台编辑、知识库/MCP 和其他确实需要完整内容的接口保持原契约；先核对调用方，再决定新接口还是内部公开查询分支。
4. 公开列表已有对 Markdown/TOC 的精简，不反向恢复其完整字段。
5. 测量 SSR hydration 与 SPA API 响应的原始/Brotli 字节和反序列化开销，不将它报告为 WASM 下载收益。

验收覆盖公开、未公开、工作稿与已发布版不同、撤回、附件权限和前后台回填；响应结构断言中确认不存在不需要的 Markdown/内部字段。数据库验证只使用 `ygg_notes_test`，命令与数据清理风险见 [笔记回归说明](notes.md#回归与待补验收)。

实施记录（2026-10-21，提交 `0cc3d7b`）：`PublicNote` 只含阅读所需字段（version 为页面公开展示的已发布修订号）；`store::get_public` 只选必需列，可见性谓词与 `get(Access::Public)` 一致；`get_note` server fn 返回 DTO，`NoteDocument` 改收 DTO，后台预览经 `From<Note>` 转换；`Note` 的 `content_md` 与内部字段加 `#[serde(default)]`，发布切换前已打开的旧标签页仍可反序列化新响应。`notes_database_workflow` 回归扩展并使用 `ygg_notes_test` 通过（版本锁定、字段缺失、撤回不可见、未知 slug 报错）；notes 编辑器套件通过。公开列表端点与 MCP 完整 Note 契约未动。

### 7.2 正文复制与片段解析

实施记录（2026-10-21，提交 `3804b26`，两项均已实施）：(1) `split_content_fragments` 结果按内容哈希做进程内有界缓存（8 篇、插入序淘汰、条目保留原文做全文核对，碰撞不会错渲染），主题切换等重渲染零重新解析，content_html 变化自动失效，keyed remount 不受影响；(2) 新增 `PostHeaderData`/`PostFooterData`（`From<&Post>`），PostHeader/PostMeta/PostFooter 改收小型字段集，post_detail/admin preview/图鉴夹具调用点同步更新，`Post::formatted_date` 迁至 `PostHeaderData`。perf/smoke/showcase 套件通过；标题/标签/导航组件不再复制或深比较完整正文。

涉及 `src/pages/post_detail.rs`、`src/components/post/post_header.rs`、`src/components/post/post_footer.rs`、`src/components/post/post_content.rs`。

- 先列出每个子组件实际读取字段；标题、标签、导航等改用小型 props，避免为这些组件复制完整正文。
- 在确实共享大正文的位置评估不可变共享数据；仅换成共享指针仍可能保留昂贵的深比较，需一起检查 props 相等判断与更新路径。
- 按内容或明确内容版本缓存 `split_content_fragments` 结果，不在普通重渲染中重复生成全部字符串片段。
- 不恢复无法跟随普通 props 变化的一次性 memo；保留 keyed remount，并测试文章 A/B 连续切换、同路由内容更新、返回缓存页和运行块 source 更新。
- 用长正文与多运行块样例报告复制/解析次数、主线程和内存趋势；复杂度增加却没有可复现收益的方案不保留。

P4 两项独立提交和回退，不要求先合并成统一模型层重构。

## 8. P5：WASM 下载时机与资源边界

### 8.1 提前下载 WASM

历史产物检查发现入口 JS 执行后才开始请求 WASM；本次未生成新生产 HTML，实施前必须先复验这个现象。

1. 从实际生成产物/发布清单获取当前入口引用的准确 WASM URL，不手写 hash。
2. 评估在 HTML head 增加 `rel=preload`、`as=fetch`、`type=application/wasm` 及与真实请求匹配的 CORS/凭据配置。
3. 浏览器网络面板确认预加载被实际实例化请求复用，没有双份下载或 preload 未使用警告。
4. 只预加载本页 flavor，不为公开页预加载 full。验证正确的 WASM MIME 和 Brotli 返回。
5. 对比请求提前量、交互就绪、LCP 和其他关键资源竞争；若只抢占带宽却没有实际收益，撤回或调整优先级。

实施记录（2026-10-21）：preload 改为发布装配期注入——`build-release.cjs` 经共享模块 `scripts/release-bootstrap.cjs` 从每个模板唯一 module 入口的 `module_or_path` 解析 WASM URL（不手写 hash，解析失败或两 flavor 入口重合时在覆盖旧发布前失败），在 head 开头插入 `as="fetch" type="application/wasm" crossorigin="anonymous"`（幂等，CORS 与 wasm-bindgen `fetch` 匹配）；`--verify` 核对 preload 的 URL/head 位置/属性与入口引用、flavor 资产闭包、清单摘要及 `.br` 旁车的一致性。单元测试 `scripts/release-bootstrap.test.cjs`（10 例，并入 `make test`/CI）；`test-dual-flavor.cjs` 增加「每文档仅一次对应 flavor WASM」断言（preload 被实例化请求复用、无 unused-preload 警告）。真实请求链 A/B（`make test-wasm-preload`，复用 `test-dual-flavor.cjs` 的隔离 release 环境；双 loopback 代理仅差 head preload、资产原始 Brotli 透传、不用 Playwright route；冷=全新 context、热=同 context 新文档；5 轮 × 3 页 × A/B × 交互/被动 × 冷/热 = 120 文档，4× CPU / 4 Mbps 限速，声明 `SSR_CACHE_SECS=60`；报告 `target/regressions/1792568229510-dual-flavor/wasm-preload/1792568233860-wasm-preload`）：

- 零违规、零拦截请求；两臂上游 HTML 哈希、SSR 代际与缓存状态逐页一致。
- 冷缓存配对中位数（基线−preload，正值为收益）：reader 首页 WASM 开始提前 342.7 ms（MAD 14.5）、菜单就绪提前 212.7 ms（MAD 12.8）；reader 文章 WASM 开始提前 590.0 ms（MAD 1.6）、菜单就绪中性（−3.8 ms，MAD 7.3，噪声内）；full 图鉴 WASM 开始提前 1 723.4 ms（MAD 26.1）、菜单就绪提前 217.5 ms。
- 三页 LCP 一致回退 48–84 ms（带宽竞争的代价，门槛 max(100 ms, 5%) 内通过）；CLS 全程 0；热缓存 WASM transferSize=0 证实缓存复用、无双份下载。
- 判定 retain-candidate（本地固定限速画像下的保留候选，非生产性能结论；methodology/limitations 见 report.json）。未覆盖：真实源站 TTFB/流式响应（两臂文档均经代理缓冲）、桌面/认证路径与 §8.2 拆分。回退方式：从模板移除注入的 preload link 重新装配，或以装配前快照回退。

### 8.2 全局脚本与样式

`Dioxus.toml` 全局引入 core/lightbox 和多份样式；`src/navigation.rs` 的 history 连接依赖 core 全局对象。

- 先区分导航、主题等启动必需代码与文章、图鉴、changelog 的增强代码，再确定拆分边界。
- 不直接给 core 加 `async`；必须保证 history 初始化拿到正确适配器，而不是因时序变化静默选择 fallback。
- 灯箱可评估按页面/内容加载，但保留点击即用与卸载清理。
- KaTeX/高亮样式条件加载必须保证服务端已经输出的内容在首次绘制时可正确显示，不能以明显闪烁换取请求数减少。
- 可变路径资源继续 revalidate；只有内容寻址/版本不可变的 URL 才使用 immutable。

实施记录（2026-10-21）：拆分边界落定为 core（启动必需：route-transitions、theme-transition、anchor-click、page-entry、hash-scroll、browser-library；保持阻塞执行不加 async，`__routeTransitions` 适配器契约不变）与 `yggdrasil-enhance.js`（post-content、mermaid+overlay、toc-sidebar、changelog-nav、showcase-masonry）。带状态的 core 模块经 window 全局桥接供 enhance 消费（`__subscribeThemeChange` 主题 registry；changelog-nav 走 `__initAnchorClick`/`__scrollToHash`），避免双 IIFE 各持一份模块状态造成双监听或收不到通知。`lightbox.js/css` 移出全局阻塞注入，与增强脚本一起由组件经 `document::Script`/`Stylesheet` 按需声明（`src/components/enhance_assets.rs`：PostContent、PostToc、changelog VersionNav、图鉴 ShowcaseGallery 与 ComponentPreview 分派、评论区、后台素材页与封面预览；`document::Script` 按 src 去重）。SSR 直达烤进 head 带 defer，先于 WASM hydration；SPA 首次注入为 async，`invoke_optional_global` 对缺失函数写入 core 无条件建立的 `__pendingEnhanceCalls` 队列，enhance/lightbox IIFE 尾部 flush 补执行（只执行已就绪项，滞留项等价原静默跳过）。KaTeX/高亮样式保持全局加载：SPA 首跳代码页的条件加载会带来一次 revalidate RTT 的裸样式闪烁，plan 禁止以闪烁换请求数（本节第 4 条）；可变路径缓存维持 `no-cache` revalidate、内容寻址产物 immutable（第 5 条现状已合规，未引入新的内容寻址 URL）。

实测（release，Brotli 字节）：全局阻塞路径 69,168 → 56,968（−12,200，−17.6%），head 阻塞请求 7 → 5；纯导航页（首页/归档/标签/搜索/友链/关于/写作指南）总传输等额下降；增强页（文章/笔记/changelog/图鉴）追加 defer 传输 13,639（非阻塞，不参与首屏竞争），总传输 +1,439（双 IIFE 重复打包共享无状态模块的开销）。core.js 11,118 → 6,155；enhance.js 5,595；lightbox.js 6,997 → 7,113（尾部 flush 钩子）。

验收：`scripts/test-dual-flavor.cjs` 新增 [1b] 资源边界断言（全部通过：正文/笔记/changelog/图鉴页注入增强脚本与灯箱、图鉴 detail 页经预览分派自带声明、changelog 无正文图片不注入灯箱、纯导航页不注入、KaTeX/高亮保持全局；含公开 moment 夹具——`/notes` 有内容才注入正是按需语义；种子需同事务插入以过 `notes_current_revision` DEFERRABLE 外键）。`test-lightbox`、`test-components-showcase`、smoke、`test-changelog-nav`（6 配置）、`test-view-transitions`（19 项含 300ms 预算）全部通过。过程中发现并修复：① 图鉴 detail 页不渲染 ShowcaseGallery，预览统一从 `ComponentPreview` 分派口自带资源声明；② `lightbox` `initLazyLoad` 对已就位 src 重新赋值会重启加载、令路由协调器挂起的 `decode()` 以 EncodingError 拒绝（按需加载后首次 SPA 进文章页可能恰在 VT 窗口内初始化），改为「已有 src 即接管」并加单测钉住；③ `test-view-transitions.cjs` 封面夹具拦截的端点名自 P4.2（`91b08be` 改 `get_public_post_by_slug`）起失配、夹具失效，已跟进修正。未覆盖：真实源站 4 Mbps 下首跳 SPA 注入的排队画像（本地回环脚本下载 <10 ms，生产源站带宽更紧但 wasm preload 优先级更高）；仅 Chromium 回归，无 Safari/Firefox 实机。回退方式：恢复 `Dioxus.toml` 全局 script/style 列表并移除组件内 `enhance_assets` 声明即可回到全局注入，core/enhance 拆分保留无害。

P5 主要改善关键路径，不单独声称 WASM 文件变小。

## 9. 后续候选与明确暂缓的方向

### 9.1 评论编辑器单独构建

当前评论与完整写作编辑器使用同一 Tiptap IIFE，只在运行时选用不同扩展。可在 `libs/tiptap-editor/` 建立静态隔离的评论入口，并更新 `libs/yggdrasil-core/src/browser-library.ts` 的加载映射。

这一项只减少读者到达评论区时的 JS 下载/解析，不直接减 WASM。需要保留评论 Markdown、公式、图片上传占位及清理契约，避免同一文档加载两个编辑器入口时全局对象冲突。单独评估和排期，不与 P3 捆绑才能上线。

### 9.2 减少浏览器 Chrono 职责

可以逐步用数值时间戳处理本地评论过期，用已有 `now_millis()` 处理浏览器时钟，并对只读显示提供确定性的格式化字段。但共享 DTO、RFC3339 解析和其他页面仍使用 Chrono，替换少量调用不意味着能移除整个依赖。

若继续推进，必须保持 SSR/浏览器时区与格式一致，兼容已存在的 localStorage 数据，并证明复杂度和收益匹配。暂不把全量替换 Chrono 作为主线。

### 9.3 原生路由 wasm-split

先前在 Dioxus 0.7.10 的实验出现 splitter/hydration 问题；0.8 alpha 实验依赖本地 splitter 与 router macro 修补，并出现 SPA 回归。它不是当前可以直接开启的正式方案。

重新评估的条件：确认目标稳定版本与上游修复状态，重新验证当前项目的 wasm-bindgen 桥接、带数据页面 hydration、SPA 导航，以及分块共享/重复代码。旧实验还出现总分块下载量增加，未来也必须同时报告首次路由和跨路由累计成本。

在这些条件满足前，P3 以两个独立、完整可验证的客户端产物为路线，不为升级实验版本扩大本方案范围。

### 9.4 笔记列表多 PostContent 的运行器容器 id 冲突（2026-10-21 评审发现的既有问题）

`PostContent` 以片段索引派生容器 id（`code-runner-{i}`），而笔记列表页为每张 moment 卡渲染独立 PostContent——两张含可运行块的 moment 卡会生成相同 id。此前 CodeMirror `create()` 按 `getElementById` 命中第一个同名容器即已存在错挂隐患（P2 未加剧：P2 的观察器复用同一 id 约定）。修复需为列表场景的 PostContent 引入确定性唯一前缀（如 note.id）并核对 `__initPostContent` 等按选择器增强的脚本契约，涉及公开渲染路径与图鉴 `scope_id` 语义，单独排期，不与 P3 捆绑。

## 10. 测试矩阵与执行入口

### 10.1 编译与静态检查

当前已存在的命令：

```sh
make check-dev-tools
make check-build-tools
make test
make lint
cargo check --locked --target wasm32-unknown-unknown --no-default-features --features web
cargo clippy --locked --target wasm32-unknown-unknown --no-default-features --features web -- -D warnings
make wasm-bindgen-cache
bun scripts/test-wasm-bindgen-metadata.cjs
```

P3 确定 feature 命名后，为 reader/full 分别补齐上述 WASM 检查和 release 构建。现有单个 `--features web` 检查不能代替两套客户端矩阵；本文不提前提供尚不存在的 Makefile 目标。

日志 feature 调整也需覆盖实际默认开发与 server-only 构建。仅为生成更短依赖树而通过检查、不启动应用，不构成验收。

### 10.2 浏览器与数据库

复用现有入口，并按阶段扩展断言：

```sh
make test-public-performance
make regression-build
make test-browser BROWSER_SUITE=smoke
make test-browser BROWSER_SUITE=runner
make test-browser BROWSER_SUITE=admin
make test-browser BROWSER_SUITE=notes
make test-browser BROWSER_SUITE=showcase
```

这些隔离回归用于功能正确性；它们当前的开发/回归构建不替代 P0 的真实 release 体积与时序测试。P3 还需让测试工具能够启动最终双产物发布布局并验证资源选择，而不是只验证源码下的默认应用。

导航检查遵循 [视图切换回归说明](../scripts/README-view-transitions.md)，重点复用/扩展 `scripts/test-view-transitions.cjs` 的直达、SPA、主题、移动端与减少动态效果场景。图鉴验证遵循 [图鉴文档](components-showcase.md)，确保没有鉴权、上传、评论提交或真实代码执行越界请求。

`make test-public-performance` 覆盖双机 HTML 缓存、失效、认证绕过和评论懒加载；P3 增加缓存 HTML 对应 flavor 与资源清单一致性断言。P2 增加 CodeMirror 网络请求/实例时机断言，而不仅断言页面上有代码块。

数据库回归按 [测试文档](testing.md#数据库回归测试) 和 [笔记文档](notes.md#回归与待补验收) 执行。普通 `make test` 会跳过相关 `#[ignore]` 测试，不得据此声称已通过数据库验收。

### 10.3 必测故障与边界

- [ ] JS/WASM 加载失败、重试、重复触发、实例已卸载后异步完成。
- [ ] 两次构建输出覆盖、清单缺项、模板引用旧 hash、同路径不同内容冲突。
- [ ] 新 HTML 遇到旧缓存、旧标签页遇到新服务端、双机滚动发布和完整回退。
- [ ] 登录失效、未保存编辑提示、跨入口返回、浏览器前进/后退与 bfcache。
- [ ] 笔记独立 SSR 入口、静态文档、API/MCP/SSE 与静态资源不被误分类。
- [ ] 多代码块、无代码块、无 IntersectionObserver、慢网和用户抢先交互。

## 11. 交付、记录与停止条件

每个阶段的交付包含：代码与相关测试、构建/特性配置说明、可复现的实验命令、产物/浏览器结果、未覆盖项和回退方式。生成结果保存在 `target/regressions/`；需要长期保留时单独整理脱敏报告，不能将认证 Cookie、真实用户内容或生产数据写入文档。

建议的实施检查表：

- [x] P0：建立 release 测量与样例基线。（`scripts/measure-release.cjs` + target-p0 隔离构建；§3.2 完整浏览器场景集仍待补）
- [x] P1：完成 logger/日志等级 A/B，决定保留方案并记录收益。（采纳 B，−20.6 KiB br）
- [x] P2：完成可见性激活、生命周期和运行器回归。
- [x] P3a：证明路由裁剪可行（门 1-5 全部通过，见 §6.2/§6.5 实施记录；reader wasm 495.6 KiB br，较 P0 基线降 49.9%）。
- [x] P3b：完成 reader/full 构建、模板选择、清单和跨入口导航（`make build` 端到端 + `scripts/test-dual-flavor.cjs` 全部断言通过；提交 `c97e0c0`、`12cc63c`、`2057bc3`）。
- [x] P3c：完成双机缓存、最终发布布局、旧资源保留和回退验收。（提交 `98e99af`/`cfc377f`/`f4bc6ed`：缓存 release×flavor 命名空间隔离、发布装配原子换入、`YGG_PREVIOUS_PUBLIC_DIR` 旧资源保留窗口、完整应用回退与累计下载量报告；记录与数据见 §6.6/§6.7/§6.8）
- [x] P4：分别完成公开笔记 DTO 与正文复制/解析优化。
- [x] P5：以真实请求链验证 preload 和资源边界，按净收益保留（§8.1 retain-candidate；§8.2 拆分与按需注入完成，实施记录与数据见 §8.2）。

停止或调整方向的条件：

- 无法保证当前稳定框架的 hydration 正确性：不发布双产物，不以忽略控制台错误作为通过。
- 无法可靠合并/选择产物：保留单 full 发布模式，不做脆弱的 HTML 字符串替换兜底。
- 优化没有可复现收益，或损失现有功能/诊断能力超过收益：不强行保留。
- 测试环境、浏览器或一次性数据库不可用：明确记录未验收，不用编译通过代替行为验收。

本文编写阶段只核对现状与设计，不执行上述构建、数据库、浏览器或部署步骤。正式实施时逐项更新状态，并将实际测量与旧实验/方案目标分开记录。
