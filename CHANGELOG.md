# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **后台个人信息页（/admin/profile）**：单列居中卡片流——身份卡（大头像 + 显示名 + 管理员徽章 + 注册时间）、「基本资料」卡（显示名称 / 邮箱，用户名只读）、「安全」卡（修改密码）。users 表新增 `display_name` / `avatar_url` 列（迁移 023）；头像复用素材选择弹窗（上传新图 / 素材库挑选 / 移除），无头像时回退展示名首字符。修改密码校验当前密码，成功后 bump 会话世代踢掉其他设备、本端保持登录；保存资料后全局用户上下文即时刷新。检测到 `ADMIN_*` 环境变量时显示「重启后以环境变量为准」提示。侧栏底部新增用户卡片入口（头像 + 显示名）。
- **备份导入恢复**：备份恢复 tab 新增「导入备份」——本机 `.sql` 备份经 `POST /api/database/backups/import`（multipart 流式落盘，独立 600s 超时）回灌至 `backups/` 并出现在列表，恢复仍走既有管线。导入时即签名校验（外来 SQL 拒收不留盘）、同名冲突拒绝、Content-Length 磁盘空间预检、tmp + 原子 rename 入库；单文件上限 `BACKUP_IMPORT_MAX_MB`（默认 512MB，生产反代 body 限制需同步放大）。

## [0.11.0] - 2026-08-14

### Added

- **站点配置页分区化重构**：设置页重制为「左侧导航 + 右侧独立滚动内容区」分区化布局；安全配置与图片缓存配置改为 DB 驱动即时生效层（改完即生效）；限流 / WebP / 图片 / 运行器等 Tier B 配置提供可编辑 UI（重启后生效）。
- **定时自动备份**：新增备份后端——设置模型 / API、定时调度任务、uploads 打包、自动轮转保留；备份恢复 tab 顶部新增自动备份设置卡片与备份列表配对展示；封装 `TimePicker` 替换原生时间输入。可通过 `BACKUP_*` 环境变量在首次部署时播种。
- **`ADMIN_*` 环境变量启动同步初始管理员**：启动时自动创建或覆盖密码并确保管理员角色，每次启动均同步（env 优先于 DB），免手动首注册流程。
- **后台全页面过渡动画**：仪表盘、文章列表、回收站、评论、写作、素材、友链、MCP、代码试运行、系统、设置等页面统一增加进场动画（页头 / 行 / 卡片 stagger），分区切换与切语言时重播。
- **编辑器接入素材库多选插图**：写作页正文编辑器新增 slash 命令「素材库」与 `insertImagesFromLibrary` 实例方法；`AssetPickerModal` 多选模式，一次插入多张站内图片。
- **素材上传弹窗增强**：改为 worker 池并发上传，并发数（1–8，默认 3）可在站点配置或 `UPLOAD_CONCURRENCY` 环境变量调整；新增上传状态展示与弹窗内分页。
- **友链头像素材库选择**：后台友链头像支持从素材库挑选，统一素材弹窗样式。
- **灯箱查看器操控**：图片灯箱新增底部毛玻璃工具栏（缩小/放大/顺时针旋转 90°/重置/下载原图/关闭），支持滚轮锚定光标缩放、双击切换 2.5×、放大后拖拽平移、移动端双指捏合缩放与竖直拖拽关闭，快捷键 `+`/`-` 缩放、`R` 旋转、`0` 重置；按钮/旋转/重置带 180ms 过渡动画，缩放时浮现倍率徽标，工具栏空闲 2.5s 自动隐藏。
- **灯箱图片加载失败错误态**：图片加载失败时退避重试、显示 `is-error` 占位、灯箱内给出提示而非闪退。
- **仪表盘 30 日发文 sparkline**：总文章数卡内嵌近 30 个自然日每日新建文章数迷你折线（真实数据，`get_post_stats` 附带 `activity_30d` 序列，随统计缓存），折线 + 淡色面积填充，全零时如实呈现平线。
- **仪表盘接口失败重试**：统计、近期文章、待审评论三路加载各自带失败态，接口失败显示「加载失败 + 重试」，不再骨架屏永转，也不回退成 0。
- **写作页封面图增强**：封面图点击放大灯箱、拖拽替换、hover 工具栏，加载期间显示骨架占位。
- **页脚可配置 GitHub 图标**：页脚宽度对齐 header，新增可配置 GitHub 图标。
- **`x-yggdrasil-hash` 响应头**：服务端新增暴露 git commit hash 的响应头（`EXPOSE_VERSION_HEADERS` 控制）。
- **图标系统化**：搜索框清除按钮、编辑器切换按钮等改用 Material Symbols 图标，并新增图标工作流 skill 规范化内联 SVG 流程。
- **双机部署脚本**：新增 `both.fish` 双机部署脚本（xun Primary + rua Replica）。

### Changed

- **WASM 体积优化**：声明 `wasm-release` profile（`opt-level="z"` / `lto="fat"`，夺回 dx 默认注入的 `opt-level="s"`，WASM 产物 ~2.7MB vs 原 3.47MB）；全部静态文本资源（WASM / JS / CSS）构建期预压缩为 `.br` 旁车，服务端经 `precompressed_br()` 直发，零运行时 CPU，wire size 降至 ~732KB。
- **部署改为 pull-based**：xun 从 GHCR 拉取镜像部署（替代 scp 推送），并接入 rua DR 副本可选自动部署（best-effort）；runtime 镜像由 scratch 换为 alpine + postgresql16-client，支持 `pg_dump`/`psql` 备份。
- **图片限流按缓存 miss 计费**：图片处理限流改为按缓存 miss 计费 + 并发处理上限 + 429 响应携带 `Retry-After`，避免对缓存命中请求误限。
- **仪表盘趋势徽章真实化**：总文章数卡写死的「+12%」改为真实的近 30 天新增篇数（`PostStats.recent_30d`），>0 时绿色徽章 + ↑ 箭头双编码（WCAG 2.2 SC 1.4.1），+0 走中性描边；「活跃」「待处理」占位徽章移除。
- **仪表盘动效预算收紧**：卡片 hover 过渡 300→200ms、入场去掉内联 600ms 回退到 400ms 类默认、CountUp 数字滚动 900→450ms；数值补 `tabular-nums` 防滚动宽度跳动；待审卡 amber 文字提亮至 `amber-600/dark:amber-400` 达 AA 对比；近期文章行 hover 浮现 → 箭头。
- **灯箱滚轮行为**：桌面端滚轮由「滚动关闭页面」改为缩放图片（2026 图片查看器惯例）；关闭手势在移动端由滚动驱动改为竖直拖拽直接驱动，行程阈值保持 120px。
- **Checkbox 组件统一**：封装带描边勾画动画的 `CHECKBOX_CLASS`，迁移全项目调用方。
- **后台公共组件收敛**：统一复用 `FormInput`/`FormSelect`/`Popover` 等公共组件，消除内联重复样式与信息泄漏；`components` 模块按可复用层级重组并加导航文档。

### Fixed

- **数据库备份恢复可移植性与原子性**：新备份不再写入数据库属主/ACL 元数据；恢复旧备份时安全移除 pg_dump 属主语句，并用单事务执行，目标环境缺少源库角色或任一 SQL 失败时不再留下半恢复数据库。
- **素材重建索引误删大图片 (#30)**：重建索引此前对大体积 WebP/JPEG 重新转码并误删，现保留原图、保留原始文件名；动图（GIF/WebP animation）保留动画绕过处理流水线。
- **灯箱操控动画**：修复打开灯箱后首下缩放/旋转出现非均匀缩放回弹与起始帧跳变（强制 reflow 提交归一化基态矩阵后启动过渡）；旋转改为绕照片中心原地进行；关闭飞行不再反向空转；打开时背景遮罩不再闪烁；加载失败时不再遗留不可见遮罩层。
- **admin 卡片 hover 无过渡动画**：`ADMIN_CARD_CLASS`/`ADMIN_TABLE_CLASS` 尾部的 `transition-colors` 在 Tailwind v4 编译产物中排在 `.transition-all` 之后，覆盖了组件追加的 `transition-all`，导致 hover 位移/阴影瞬时跳变；改为裸 `transition`。
- **仪表盘近期文章不可点击**：行容器由 `div` + `cursor-pointer` 改为 `Link`，整行跳转 `/admin/preview/:slug` 只读预览，草稿亦可预览。
- **migration 005 幂等**：005 迁移的 comments 触发器改为幂等，重复执行不再报错。
- **限流日志与错误**：无法获取客户端 IP 的 WARN 改为进程级 warn-once，避免日志刷屏；图片限流响应错误装箱修复。
- **构建链修复**：空 `RUSTC_WRAPPER` 规避 sccache 与 dx 的 rustc 包装链冲突；清理双目标构建 warning；发布构建前检查 brotli 可用性；`make fix` / docker-fix 移除 `dx fmt`。
- **友链管理**：修复后台友链头像不显示、默认排序值缺失、素材库头像路径、删除确认改用 Tooltip、素材弹窗定位偏移等问题。
- **素材弹窗与卡片**：弹窗关闭动画失效、提示框消失缺退出动画、媒体卡片圆角不统一等细节修复。

## [0.10.1] - 2026-08-06

### Added

- **草稿预览页**：登录后可在 `/admin/preview/:slug` 预览草稿文章，并配置专属骨架屏。
- **生产镜像版本 tag**：生产 Docker 镜像支持按版本号打 tag。

### Changed

- **镜像构建加速**：预缓存 wasm-bindgen-cli 与 cargo buildx cache mount，缩短镜像构建时间。

### Fixed

- **预览页空白闪烁**：修复预览页骨架屏结束后主内容区出现空白闪烁的问题。
- **骨架屏宽度坍缩**：修复预览骨架屏在登录校验期间宽度坍缩的问题。

## [0.10.0] - 2026-08-05

### Added

- **WGSL 代码高亮**：新增 WGSL 语言语法高亮支持。
- **文章外链图片灯箱与混合图集**：Lightbox 支持直接放大文章外链图片，并与站内图片无缝组合为混合图集。
- **素材管理与上传弹窗**：素材管理页新增内嵌上传弹窗，支持开合/状态切换过渡动画与素材网格入场阶梯动画。
- **后台导航菜单分组**：侧边栏引入「内容管理」与「工具」可折叠子菜单组，整合回收站、试运行、MCP 与系统设置页面。
- **自定义 Checkbox 与操作栏动画**：统一后台 Checkbox 样式为主题色自定义方框，增加批量操作栏展开/收起过渡效果。
- **Code Runner 就绪探测与优化**：增加 Docker 启动期就绪探测与运行时错误分类；Go 语言代码块实现跨运行共享卷构建缓存提升执行效率；跨语言切换时自动重载编辑组件。

### Changed

- **性能与内存优化**：sanitizer 热路径消除每元素 HashSet 分配，借用替代 clone 减少冗余分配。
- **Dioxus 依赖升级**：升级 Dioxus 核心框架依赖至 0.7.10。

### Fixed

- **Markdown 复杂表格与公式**：修复 Markdown 表格内公式导致单元格错渲染为 `<th>` 的问题，优化复杂表格样式。
- **KaTeX · 符号渲染**：修复 `\text{}` 内 `·` (U+00B7) 渲染为红字 `\cdotp` 的问题 (#13)。
- **切页评论与选择器残留**：切换文章时强制重载评论组件防止历史评论残留 (#10)；修复批量操作栏收起后残存间距与重建按钮 Tooltip 裁切问题。
- **素材粘帖上传与弹窗动画**：修复原生监听器内 spawn 空 scope 栈 panic 以及上传弹窗关闭动画失效问题。
- **开发与容器环境**：修复 Docker 开发容器下挂载 uploads 目录导致的素材图 404 及 pnpm 旧模块清理。
## [0.9.0] - 2026-08-03

### Added

- **桌面端悬浮目录（TOC）**：文章页新增少数派风格悬浮目录，支持滚动监听（scroll-spy）、悬浮展开与固定（pin）模式。
- **移动端导航动画**：为移动端顶部导航菜单添加双向平滑展开/折叠与图标微动动画。
- **友链管理功能**：新增后台友链 CRUD 管理、前台 `/friends` 列表展示及 moka 高效缓存。
- **订阅 Feed 端点**：新增 RSS 2.0 (`/feed.xml` / `/rss.xml`) 与 JSON Feed 1.1 (`/feed.json`) 订阅生成与输出端点。
- **Changelog 结构化时间线**：版本变更日志升级为结构化解析 + 时间线卡片 UI。

### Fixed

- **Changelog 导航高亮**：修复 CHANGELOG 侧栏版本导航高亮未能精准跟随当前阅读位置的问题。
- **友链删除回调**：修复后台友链删除确认框弹窗回调语法错误。
- **CI 多架构构建**：修复 GHCR 镜像构建脚本中 `docker image inspect` 获取 Architecture 标识的问题。

## [0.8.3] - 2026-08-03

### Added

- **Mermaid 流程图全屏放大浮层**：复杂/宽流程图支持全屏点击放大查看，离散缩放支持 200ms 平滑过渡，交互与图片灯箱统一（支持 FLIP 飞行开关动画、滚动驱动关闭、拖拽平移、滚轮/捏合/双击缩放与 Esc/✕ 关闭）。
- **CI GHCR 镜像归档**：GitHub Actions 新增主应用多架构（amd64/arm64）Manifest 镜像及 6 个 Code Runner 沙箱镜像向 GHCR (`ghcr.io`) 自动推送与归档。

### Fixed

- **Mermaid 异常容器清理**：修复 `mermaid.render` 解析失败时在 `document.body` 遗留临时 `#d${id}` 容器导致页面底部出现错误块的视觉问题。
- **Mermaid 宽图放大居中偏位**：修复 `fitToScreen` 居中计算未乘以 `fitScale` 导致宽图放大浮层整体向左上偏位的问题。

### Changed

- **Mermaid 预加载与渲染体验**：在浏览器空闲期（`requestIdleCallback`）自动预加载 mermaid bundle 模块，并在图像渲染过程中增加转圈角标提示。
- **CI 构建流程优化**：限定 `build-amd64` 触发条件（仅主干分支、tag 及手动触发），增加镜像无 DB 启动冒烟测试与 Runner 目录变更检测，提升 CI 执行效率。
- **后台管理 UI 细节优化**：优化后台管理页面 UI 布局结构与风格样式。

## [0.8.2] - 2026-07-31

### Security

- **session_generation 自动失效旧会话**：`users.session_generation` 版本化机制此前只有读侧（`get_user_by_token` 命中缓存后回查版本号，不匹配即逐出），但写侧无任何自动 bump——降级/封禁用户后已签发会话仍长期有效。新增 BEFORE UPDATE 触发器（migration 018）：仅在 `role` 真正变化（`IS DISTINCT FROM`）时 bump 世代号，其它列更新不误伤，触发器内修改 `NEW` 不递归。端到端语义由既有读侧逐出逻辑承接。

### Fixed

- **评论「审核中」徽章永久残留（issue #9 回归）**：评论审核通过后前端轮询从一次性 `use_future` 改为 `use_resource`（依赖 pending 列表自动重启），并加去重 `use_effect` 兜底，根治 pending 占位项残留。
- **mermaid 多行节点标签 descender 裁切**：`foreignObject` 默认 `overflow:hidden` 在部分浏览器 sub-pixel 渲染下裁切多行节点下沉笔画（g/p/y）。注入 SVG `overflow="visible"` presentation attribute 修复（CSS `overflow` 对 foreignObject 无效，须写属性）。
- **全文搜索 trigram 索引**：恢复 `posts.search_text` 的 trigram GIN 索引（migration 014 以错误理由删除——`gin_trgm_ops` 本就是为 `%pat%` 包含匹配设计的；migration 019 按 004 原定义重建）。`search_posts` 与 MCP `search_published` 的 ILIKE 不再全表扫。
- **后台表格列换行**：日期/状态/操作列加固定宽度与 `whitespace-nowrap`，避免窄屏换行。
- **线上容器无法创建备份**：scratch 镜像缺 `backups/` 目录且属主为 root、进程以 nobody 运行不可写。修复 `.dockerignore` 放行 `backups/.gitkeep`、Dockerfile 预建目录；备份写入失败改为携带真实 `io::Error`。
- **rustdoc 线上 404**：镜像内生成 rustdoc 修复 `/doc` 路由 404；订正 rustdoc intra-doc 断链与无效 HTML 标签。
- **Docker 构建**：cargo-chef tarball 解压去子目录层；docker-tests 缺少 alpine 镜像时跳过而非 panic。
- **release 后 changelog 测试失败**：修复发布后遗留的测试断言失败。

### Changed

- **CI 迁移到 GitHub Actions**：从 Gitea Actions 迁移到 GitHub Actions，新增 SSH 部署到 xun；`test` + `build-amd64` 改为每次推送触发，`build-arm64`/`release`/`deploy` 改为仅 tag 推送（`v*`）或 `workflow_dispatch` 触发。
- **arm64 原生构建 + GitHub Release 自动发布**：新增 arm64 原生构建（`ubuntu-24.04-arm` runner），GitHub Release 自动发布三件产物（amd64 镜像 / arm64 镜像 / x86_64 musl 静态二进制 + public 资源）。
- **CI 缓存优化**：cargo-chef 依赖分层 + BuildKit GHA 层缓存；升级所有 actions 到 node24 兼容版本。
- **lint 增强**：Makefile `lint` 目标增加 `cargo fmt -- --check`；全仓库 cargo fmt 统一格式。

### Internal

- 订正 `.cargo/config.toml` 过时的 cross build 叙事；修正 CHANGELOG 代码围栏 Markdown 转义。

## [0.8.1] - 2026-07-30

### Added

- **MCP `update_post` 部分更新**：`update_post` 改为 PATCH 语义，`UpdatePostParams` 所有可改字段（title / content_md / summary / slug / tags / status / cover_image）改为 `Option`，仅 `post_id` 必填；服务端按提供的字段动态构建 `UPDATE SET` 子句，未提供 `content_md` 时跳过 Markdown 重新渲染，省去 `spawn_blocking` 开销。语义要点：`summary` 随正文联动（正文变且未给则自动提取）；`slug` 仅显式提供才改动；`tags` 为 `None` 不改、`Some` 替换（空列表清空）；`cover_image` 为 `None` 不改、空串清空、URL 设置；`status` 变化时首发自动填 `published_at`。旧的全量调用（同时传 title+content_md）行为不变，向后兼容。

## [0.8.0] - 2026-07-30

### Added

- **TOML 语法高亮**：`toml` 代码块此前回退为纯文本（syntect 默认精选集不含 TOML）；现已引入官方 sublimehq TOML v2 语法，支持日期时间、数组表、内联表与多行字符串。
- **Docker 开发环境**：新增 `Dockerfile.dev` + `docker-compose.dev.yml`，内置 dx 0.7.9、tailwindcss v4、Node 22/pnpm 与 wasm32 target；源码 bind mount 实现热重载，cargo/pnpm 缓存跨重启持久化，直连宿主原生 PostgreSQL。新增 `make docker-dev` / `docker-dev-down` / `docker-dev-shell` 便利 target。
- **x86_64 镜像零 QEMU 交叉编译**：改用 `Dockerfile.cross` 三阶段构建（Trixie 编前端 + Alpine/zig 交叉编 server + scratch 合并产物），arm64 Mac 上不再经 QEMU/Rosetta 翻译，告别 cross 工具链容器在 Rosetta 下的 SIGSEGV 崩溃。

### Changed

- **MCP 媒体上传改为带外传输，彻底移除 base64**：`upload_media` 工具改为接收 URL，由服务端经 SSRF 防护抓取（强制 https、DNS 解析即锁 IP 杜绝 rebinding、禁重定向、流式体积上限、超时）；新增 `POST /api/mcp/upload` bearer multipart 端点，供 host/shell 直接 POST 二进制。二进制不再进 JSON-RPC，不再受 4MiB 请求体上限（原 base64 路径实际仅约 2.8MiB 原始图）约束。新增 server-only 依赖 `reqwest`（rustls-tls，musl 静态链接友好），移除 `base64` 依赖。

### Fixed

- **交叉编译部署后登录 405**：Dioxus 0.7.9 的 server-fn URL 末尾去冲突后缀为 `xxh64(CARGO_MANIFEST_DIR + module_path!, 0)`；host 交叉编译与容器内编译的 `CARGO_MANIFEST_DIR` 不同，导致前后端算出不同 URL 后缀，POST 落入 SSR 兜底 GET 路由返回 405。改由 `.cargo/config.toml` 固定 `SERVER_FN_OVERRIDE_KEY`，两次编译读到同一哈希 key。
- **Dockerfile 漏拷 workspace 包**：两个 Dockerfile 的 pnpm manifest COPY 此前只拷了 4/7 个包，补齐 `shared` / `xterm-terminal` / `mermaid-renderer`。

## [0.7.0] - 2026-07-29

### Added

- **MCP 服务器**：博客现在是一个 Model Context Protocol 服务器（`POST /mcp`，Streamable HTTP，无状态），管理员的 AI 客户端（Claude Code / Cursor / Cline）可经 bearer token 连接，把已发布文章当知识库检索，并执行几乎所有后台操作。
  - **认证**：管理员在 `/admin/mcp` 签发令牌，三档作用域（read / write / admin），可选有效期（1/7/30/90 天 / 永不过期）。令牌明文经 AES-GCM-256 静态加密存储（可重查），SHA-256 哈希做每请求查找。
  - **知识库（read）**：`search_posts` / `get_post` / `list_tags` 工具 + 已发布文章作为可枚举的 MCP Resources（`post://{slug}`，游标分页）。
  - **写操作（write）**：文章 CRUD（含草稿）、评论审核、标签管理、媒体上传（base64 → WebP 转码去重）。
  - **管理（admin）**：站点设置读写、代码运行器（沙箱执行）。
  - **加固**：token-keyed 限流（默认 10/s burst 30，超限 429）、`last_used_at` 节流刷新、鉴权审计日志、Origin→403（spec 强制，rmcp 内置）、协议版本头校验、4MiB 请求体上限。
  - **配置生成**：后台一键复制 4 种客户端配置（Claude Code / Cursor / Cline / 通用 JSON + CLI）。
  - 传输用官方 `rmcp` crate（`=3.0.0-beta.3`，3.x 才有 Origin 校验等 spec 强制项）挂载于 axum。新增依赖 `rmcp`、`aes-gcm`、`base64`（均 server-only）。
  - 新增环境变量 `MCP_TOKEN_ENC_KEY`（hex 编码 32 字节 AES-256 密钥，`openssl rand -hex 32` 生成）、`RATE_LIMIT_MCP_PER_SEC` / `RATE_LIMIT_MCP_BURST`。
- **素材管理（媒体库）**：新增 `media_assets` 注册表数据层与 `/admin/assets` 管理页，把上传的图片当作一等公民管理。
  - **只读列表页**：素材网格 + 引用/未引用（孤儿）筛选；引用徽标展示被哪些文章引用。
  - **删除保护、孤儿清理与 alt 编辑**：被引用素材不可删除；可一键清理无引用的孤儿素材；就地编辑 alt 文本。
  - **全量索引重建**：扫描 `posts.content_html` 重建每张图片的引用关系（批量 500）。
  - **封面「从素材库选择」联动**：写文章页封面图改为从媒体库挑选，封面与素材库打通。
  - **灯箱预览**：素材网格接入 Lightbox，支持图集切换浏览。
  - **搜索与防抖**：素材页搜索框 300ms 防抖。
  - **多选与批量删除**。
  - **上传去重**：上传图片按内容 SHA-256 计算指纹，重复上传直接复用已登记素材，不再产生冗余文件。
  - **Pagination 页码跳转**：素材列表分页支持页码直跳（`Pagination` 组件新增跳转能力）。
- **关于页重设计**：关于页重制为「世界树与遗忘」主题叙事，新增年轮式链接区。
- **`/changelog` 页面**：新增更新日志页，内嵌渲染 `CHANGELOG.md`，并在年轮区接入入口。
- **后台仪表盘动画**：仪表盘分块进场动画 + 数字滚动，节奏可调。
- **`FormSelect` 主题化下拉组件**：封装自定义主题化下拉弹层，替代原生 `<select>`（OS 弹窗无法跟随主题）；全项目原生 `<select>` 统一替换为 `FormSelect`。
- **后台文章搜索**：文章列表支持按标题搜索。
- **MCP 配置生成 UI**：客户端配置代码块加语法高亮、复制按钮就地反馈；令牌列表与配置生成 loading 替换为骨架屏；新增 OpenCode 客户端配置片段。

### Changed

- **重构系列 R2–R6**：抽取 `tools/common.rs` 消除 MCP helper 四重拷贝（R2）；放宽 posts helpers 可见性并删除 MCP 内的平行拷贝（R3）；抽取 `render_post_fields` 消除文章渲染 + 字数/阅读时间度量的重复（R4）；抽取 `fetch_post_tags` 消除 9+ 处标签查询重复（R5）；抽取 `invalidate_for_post_write` 统一写后缓存失效序列（R6）。
- **utils 常量集中**：集中 `hash_token` / `EMAIL_REGEX` / `MAX_FILE_SIZE` 的重复定义；集中 `MIGRATE_STARTUP_TIMEOUT_SECS` 解析；提取 `formatted_date` 公共实现；统一 LIKE 模式转义（`consts`）。
- **输入框统一 `FormInput`**：后台文章搜索框、SQL 控制台输入框等统一改用 `FormInput` 公共组件。
- **代码运行器执行层**：抽取 `spawn_exec_task` 消除 `start_exec` / `stream` 重复（M10）。

### Fixed

- **KaTeX `\pu` 转译**：修复 `\pu` 预转译的 off-by-one 与 UTF-8 破坏（C1+C2）。
- **代码运行器**：流式执行超限时回收挂起的容器，避免泄漏（C3）；移除 `source_prop_signal` 的渲染纯净性违规（C4）。
- **图片缓存清理任务**：防溢出/下溢 panic（C5）。
- **MCP 工具归属与缓存**：trash/delete 补归属校验 + `publish_post` 标签缓存失效（M1+M6）；标签文章数计数 + 批量删除 SSR 失效（M2+M5）。
- **备份/恢复**：`pg_dump`/`psql` 移入 `spawn_blocking` 并按行读取，避免阻塞异步线程（M3）。
- **评论**：评论 markdown 渲染移入 `spawn_blocking`（M4）。
- **标签页缓存**：移除未使用且永不失效的 `PostsByTagPage` 缓存键（M7）。
- **限流**：键控限流器定期 GC + XFF 伪造风险文档化（M8），GC 任务派生守卫运行时可用性。
- **数据导出**：不再向 HTTP 响应泄露原始 DB 错误（M9）。
- **SQL 控制台**：读路径追加 `LIMIT` 避免全表物化（M11）；`SHOW` 只读分类修正。
- **后台文章列表**：渲染拉取失败的错误态（M12）。
- **素材管理**：uuid 参数序列化失败导致上传 500；`SUM(size_bytes)` 显式转 bigint 修复列表 500；引用徽标加 `z-10` 修复被 blur-img 遮盖；灯箱图集切换不重算几何导致图片压扁；删除返回「素材不存在」时刷新网格自愈过期数据；工具栏右侧按钮沉入 `FilterTabs` 区域紧贴横幅；操作横幅上下间距不对称（40/8 → 24/24）。
- **MCP**：reveal/revoke 令牌时将字符串 id 解析为 `Uuid`；`FormSelect` 面板水平居中修复双倍位移；客户端配置格式修正（Claude Code / Cursor / Cline，联网核实 2026）；配置生成 `use_effect` 死循环；开发期放行 `0.0.0.0` Host 让 dx 代理可用、Host 白名单加入 `APP_BASE_URL` 域名。
- **文档**：修正文档漂移（AGENTS.md / DEPLOYMENT / .env.example 等）。

### Removed

- **死代码清理 D1–D12**：删除死模块 `resources.rs`、死 `User` 结构体、未用参数、mhchem 冗余分支、死导出与文档化死分支、未使用且永不失效的缓存键等；收窄模块级 allow。
- 迁移后补回/移除测试中失效的 `sha2::Digest` 导入。

## [0.6.2] - 2026-07-24

### Fixed

- **web-only 构建失败**：`sleep_ms` 原用 `#[cfg(not(target_arch = "wasm32"))]` guard tokio 分支，但 tokio 是 server-only optional 依赖。该 guard 在「非 wasm32 主机 + 仅 web feature」组合下误激活导致编译失败（长期被 dev-dependencies 中的 tokio 掩盖，只有排除 dev-deps 的生产构建才暴露）。修复为 `#[cfg(all(feature = "server", not(target_arch = "wasm32")))]`，符合 dual-target gating 规范。
- **404 页「返回首页」卡死**：文章详情页对不存在的 slug 抛出 404 错误后，Dioxus `ErrorBoundary` 捕获错误渲染 fallback，但点击「返回首页」仅更新 URL 不切换页面（`ErrorBoundary` 需显式 `clear_errors()` 才能恢复渲染 children）。修复：返回首页改用 button，onclick 内先 `clear_errors()` 再导航。

## [0.6.1] - 2026-07-23

### Changed

- **消除非测试代码中的裸 `unwrap()`**：在 `panic = "abort"` 全局下，任何裸 `unwrap()` 都会直接崩溃整个进程且无法恢复。将所有非测试代码中的 `unwrap()` 改写为带不变量说明的 `expect()`（消息需解释*为何*不可能失败），并在 AGENTS.md 规范 #16 中固化该约束。

### Fixed

- **mhchem 转译器三处 panic 修复**：修复 `panic = "abort"` 下化学公式转译器的三处崩溃：`". __* "` 正则转录错误、`find_observe_end` 逐字节扫描多字节字符越界、以及 `re!` 宏编译失败时未降级为不匹配导致直接 panic。前两处为输入触发的运行时 panic，第三处补上缺失的防御性边界。
- **Docker 构建缺 `patches/` 目录**：构建镜像时未复制 `patches/` 目录导致 `pnpm install` 报 ENOENT，补充复制修复。

## [0.6.0] - 2026-07-23

### Added

- **编辑器 mermaid 实时预览**：Tiptap 代码块在编辑器内实时渲染 mermaid 流程图，所见即所得。
- **编辑器脚注所见即所得**：Tiptap 富文本编辑器内脚注直接可见，不再依赖 Markdown 源码。
- **mhchem 化学公式**：移植 mhchem 转译器，支持 `\ce`/`\pu` 化学方程式语法转 LaTeX 渲染。
- **KaTeX 物理学宏表**：注册 16 个物理学宏（如 `\d`、`\od`、`\textsubscript` 等），适配物理学公式写作习惯。
- **新建文章默认直接发布**：`/admin/write` 新建文章的默认发布状态改为直接发布。

### Changed

- **mermaid 主题变量下沉**：将 Catppuccin 主题变量从 tiptap-editor 下沉到 `@yggdrasil/shared` 共享包，统一管理。
- **中间件抽取**：将 `ssr_generation`/`version_headers` 中间件从 `main.rs` 抽出至 `src/middleware.rs`。

### Fixed

- **窄表格边框裁切**：修复窄表格在 `table-wrap` 容器内边框被裁切的问题。
- **移动端表格滚动**：修复移动端表格无法横向滚动的问题。
- **空工具栏顶栏**：隐藏无语言标识代码块的空工具栏顶栏。
- **KaTeX sanitizer 拦截**：允许 KaTeX 渲染所需的 `svg`、`path` 标签及绘图属性通过 sanitizer。
- **tiptap-markdown 过度转义**：修复 tiptap-markdown 过度转义导致内容显示损坏。
- **脚注序列化转义失效**：修复 tiptap 序列化转义脚注语法 `[^id]` → `\[^id\]` 导致脚注失效。
- **SQL 控制台写后缓存失效**：SQL 控制台执行写操作后全量失效相关缓存。

## [0.5.0] - 2026-07-22

### Added

- **文章数学公式 SSR 渲染(KaTeX)**：引入 `katex-rs` 在服务端把 `$...$`/`$$...$$` TeX 渲染成视觉层 HTML span(`OutputFormat::Html`,不含 MathML,XSS 面最小;`throw_on_error=false` 坏公式渲染成红色错误而非中断);自托管 KaTeX CSS + woff2 字体到 `public/katex/`(`make katex-css` 从 npm `katex` dist 拷贝)。
- **评论数学公式渲染**：评论路径同步开启 `ENABLE_MATH`,span 白名单加 `style` 保留 KaTeX 内联定位样式。
- **编辑器数学公式节点**：Tiptap 数学公式节点带 KaTeX 预览,根治编辑器序列化破坏 LaTeX 的问题。
- **mermaid 流程图懒加载渲染**：文章页 `language-mermaid` 代码块经 IntersectionObserver 视口可见时动态 `import('/mermaid/mermaid.js')`(独立 IIFE bundle,~3.4MB / gzip ~900KB,非全局注入),`mermaid.render` 产 SVG 注入;主题经 `__initMermaid` 传入,`securityLevel: 'strict'`,幂等守卫防重复渲染;渲染失败保留源码并加 `.mermaid-error` class。
- **流程图配色对齐 Catppuccin**：mermaid 配色对齐 Catppuccin 主题,容器美化。
- **流程图主题切换动画**：流程图主题切换跟随 View Transitions 圆形扩散动画,主题切换时重渲染已渲染的流程图。
- **bun 代码运行器**：新增 `yggdrasil-runner-bun` 沙箱镜像(官方 `bun.sh/install` 脚本 + musl 变体 + `libstdc++`/`libgcc` C++ 运行时);admin 代码试运行沙箱加 bun 语言按钮;CodeMirror 加 TypeScript 模式;语言别名归一化(`ts`/`typescript`→`bun`,在 `parse_fence_info` 统一,`LANGUAGES.get` 只见规范化 key)。
- **文章页脚注完整支持**：语义化 + back-link + 样式。
- **Vue SFC 语法高亮**：文章页代码块支持 Vue SFC 语法高亮。
- **搜索入口改为图标按钮**：header 搜索入口从文字改为图标按钮。
- **正文折叠块卡片化**：`<details>` 折叠块卡片化,自绘 chevron + hover/focus 态。
- **task-list checkbox 自绘**：文章页 task-list checkbox 改用 `appearance:none` 自绘圆角方框。
- **代码块字号调整**：文章页代码块字号从 13.6px 调整为 16px。
- **响应头暴露版本信息**：server 通过 `Server`/`X-Yggdrasil-Version`/`X-Yggdrasil-Git` 响应头主动暴露版本与 git 描述信息(`EXPOSE_VERSION_HEADERS` 可关)。
- **Docker multi-arch 构建目标**：`make docker-amd64` 与 `make docker-apple` 构建 x86_64 镜像。
- **服务器端口占用优雅退出**：端口被占用时优雅退出而非 panic。
- **压缩算法默认 off**：`COMPRESSION_ALGORITHMS` 中间件默认值改为 off。

### Changed

- **mimalloc 全局分配器**：用 mimalloc 替换系统全局分配器(`#[global_allocator]`,双 cfg 门控:server feature + 非 wasm32;musl 静态链接友好)。
- **性能优化系列**：`escape_html` 链式 5 次 replace 改单遍扫描;`slugify` 单遍状态机重写(分配 4→1);Markdown 渲染消除双解析 + `format!` 改 `write!` 直写;upload 消除 `data.to_vec()` 多次全文件深拷贝;`cache_key` 单次拼接 + `detect_format` 零分配后缀匹配;posts list/search 零 capacity Vec 改 collect 预分配 + helpers retain。
- **重构系列**：admin `/admin/posts` 与 `/admin/posts/trash` 合并为单路由 + tab 切换;`Pagination` 支持可选 `on_prev`/`on_next` 回调;抽取 `@yggdrasil/shared` 内部源共享包消除跨 IIFE 库的类型/常量重复;抽 `main.rs` 中间件到 `src/middleware.rs`;为 `Response` 类型添加构造器消除 51 处样板;抽 `invalidate_post_metadata()`/`upload_error()` 等消除样板;统一 WASM sleep 到 `utils::time::sleep_ms`;删除死代码(`delayed_loading.rs`/`ui.rs EmptyState`/`CommentActions`/未用 re-export);拆分 `system.rs` 为 `system/` 目录(按 tab 分文件);图片处理合并维度读取函数共享 `image_reader_limits`。
- **依赖升级**：TypeScript 升级至 7.0.2(Go 原生编译器)。

### Fixed

- **SSR 缓存失效根治**：文章写入后物理删除 SSR 磁盘缓存目录,根治「重建后内容不更新」(Dioxus 0.7 增量渲染器只暴露 TTL 失效手段,通过删文件绕过限制);build 前清除 `static/` SSR 缓存目录。
- **Docker 构建/部署**：Docker 构建透传 git 信息修复 `x-yggdrasil-git` 恒为 unknown;预装 binaryen 避免 dx 运行时下载 wasm-opt 失败;补齐 Dockerfile 缺失的 katex-css 与 restore-webp 步骤;升级 builder 至 trixie 满足 dx 对 GLIBC_2.39 的需求;用 tmpfs `mode=1777` 替换 uid/gid 选项兼容 Podman;GitHub Releases 下载最终改用直连(移除 gh-proxy)。
- **线上代码高亮缺失**：编译期内嵌自定义语法,修复线上 Docker 镜像代码高亮缺失(原先运行时加载语法文件在打包镜像中找不到)。
- **mermaid 渲染**：改用 script 标签加载 IIFE bundle 修正全局变量取值;修复 tsc 类型错误;主题切换时重渲染已渲染的流程图。
- **文章锚点导航**：`scrollToHash` 增加一次性守卫,切主题不再跳回 URL hash;增加 ResizeObserver 布局稳定期,修正 mermaid 异步渲染导致的锚点落点偏移。
- **WASM 双端编译**：hooks 模块移出 server gate 双端可见(原先 WASM 编译失败)。
- **评论代码块转义**：统一 `escape_html`,修复代码块单引号未转义。
- **Docker daemon 容错**：Docker daemon 不可用/断连时不再 panic(集成测试在无 daemon 环境优雅跳过)。
- **后台布局**：重建结果消息改绝对定位,避免撑高容器顶起按钮。
- **图片 cfg 门控**：为 `ImageFmt` 别名补上 `#[cfg(feature = "server")]` 门控。
- **clippy/lint**：修复 rust-1.97 clippy `useless_borrows_in_formatting` 告警、Biome 告警。
- **安全测试**：补齐安全关键路径的单元测试盲区。

### Internal

- **skills 体系**：新增 `optimizing-rust-performance` 与 `rust-advanced-performance` 性能优化技能;清理已卸载的第三方 skills 及 lock 注册;`deploy-to-linux` 添加手动部署模式。
- **部署脚本**：新增 xun 服务器全量部署脚本。
- **文档**：AGENTS.md 补充别名归一化与 bun 镜像说明、数学公式与流程图架构说明、xterm-terminal/shared 库说明;补全 `.env.example` 缺失的 5 个环境变量。

## [0.4.0] - 2026-07-13

### Added

- **代码运行器（Code Runner）**：读者可在文章页直接运行 ` `lang runnable ``` 代码块，在隔离的 Docker 容器中执行，支持 Python / Node.js / Go / Rust 四种语言；admin 侧 `/admin/runner` 试验沙箱页支持任意代码试跑（跳过速率限制）。三层架构：`src/infra/docker.rs`（bollard 执行层，只读 rootfs + tmpfs + 资源/能力限制 + `ContainerGuard` Drop 强制清理）、`src/api/code_runner/`（任务注册表、语言注册表、双速率限制 + 白名单 + 大小检查）、Markdown 渲染层（`PostContent` 拆分 `Html`/`Runnable` 片段，每块渲染为真实 `<CodeRunner>` vdom 元素）。所有 `CODE_RUNNER_*` 环境变量可调，支持 per-IP 速率与每日上限。
- **流式代码执行（SSE + xterm.js）**：CodeRunner 切换为 SSE + xterm.js 流式输出方案。新增 `xterm-terminal` IIFE 子工程（xterm.js 6.0）、`xterm_bridge.rs` wasm-bindgen 绑定、`/api/exec/stream` SSE 端点、Docker 执行层流式路径（wait 与 log 读取并发）；支持无缓冲 stdout（`python -u`）、SSE done 事件回传 `duration_ms`、运行前隐藏输出区 + skeleton 占位。
- **`/admin/system` 管理后台**：全新管理区，5 个 tab —— 数据库状态（表统计/活跃连接/迁移版本）、服务器状态（sysinfo 主机指标 + moka 缓存命中率轮询）、SQL 控制台（全读写，4 道护栏：sqlparser AST 门 + WHERE 缺失拒绝 + 查询超时 + 前端确认；单元格类型化渲染 NULL/布尔/数字、表头 sticky、行截断展开）、数据导出（Axum 流式 SQL/CSV）、备份恢复（`pg_dump` 优先 + COPY 回退、DashMap 任务进度表 + 轮询、备份文件签名校验 + 路径白名单）。
- **UI 重新设计（工业极简 + Catppuccin）**：全站配色迁移到 Catppuccin（Latte/Mocha），移除 Rust 中硬编码颜色，统一语义色阶；后台重设计为现代极简侧边栏布局（写文章页改为左右两栏、编辑器自适应高度）；圆角 token 化为三档梯度（32/16/8）并统一所有组件；Markdown 表格重设计 + 表格单元格圆角防背景溢出；全局路由切换平滑挂载动画 + View Transitions 圆形展开主题切换动画；编辑器背景图（线条小狗）有内容时自动调淡透明度。
- **编辑器可运行代码块 NodeView**：CodeBlock 改用 `CodeBlockLowlight` + Catppuccin 高亮配色；新增 CodeBlockNodeView（语言标签 + 运行按钮 + 结果区），点语言标签可编辑语言与配置；斜杠菜单新增「可运行代码块」条目 + 模态框配置 runnable fence info；`make_run_code_closure` 桥接编辑器内运行代码。
- **编辑器任务列表手动输入**：支持 `- [ ]` 逐字符输入创建任务列表（appendTransaction，非 InputRule 全量替换）；前台与编辑器 checkbox 垂直中线对齐。
- **SQL 控制台 Ctrl+Enter**：`/admin/system` SQL 控制台接通 Ctrl/Cmd+Enter 运行快捷键。
- **CodeMirror Vim 模式**：admin CodeRunner 沙箱编辑器支持 Vim 模式开关，默认开启；CodeMirror 编辑器高度自适应与滚动限制。
- **文章重建内容按钮**：文章列表操作列新增「重建内容」按钮，重建支持并发 loading（spinner 覆盖文字）。
- **中文 slug 自动转拼音**：中文标题自动转拼音生成 URL slug。
- **FreeBSD x86_64 交叉编译**：`make build-freebsd` + `make freebsd-sysroot`，clang + lld + sysroot。
- **构建信息注入**：启动时打印 git/rustc/构建时间信息。
- **404 页面提交 HTTP 404 状态码**（SSR 层）；`ErrorBoundary` 包裹公开路由，文章详情页 404 等错误上抛至 `ErrorLayout`。
- **SSR 层 admin 认证守卫**：未登录访问 admin 直接在 SSR 跳转登录页，避免闪烁。
- **登录表单回车提交**；网站 favicon；评论背景图自动调淡。

### Changed

- **pnpm workspace 重构**：JS 子项目从 npm 迁移到 pnpm workspace，根工作区在 `libs/`，单一 `libs/pnpm-lock.yaml` + 共享 `libs/tsconfig.base.json`；引入 Biome v2.5 monorepo 配置并全量格式化；Makefile 整合 `lint`/`fix`/`test` 目标。
- **消除 `js_sys::eval`**：DOM 互操作全面从字符串求值迁移到 wasm-bindgen 绑定层（`tiptap_bridge`、`codemirror_bridge`、`xterm_bridge`），清理 wasm32 target 残留 clippy lint。
- **通用 hooks 抽取**：新增 `use_paginated`（分页加载）、`use_event_listener`（通用事件监听，解决 `use_hook` + `use_effect` + `use_drop` 资源所有权陷阱）。
- **SQL 控制台组件化**：`SqlConsoleTab` 改用独立 `SqlResultTable` 组件。
- **按钮 token 化**：新增 `BTN_PRIMARY`/`BTN_OUTLINE` 等按钮令牌与 `LoadingButton` 组件，消除样式散落。
- **骨架屏统一延迟**：统一骨架屏延迟机制，200ms 内加载完成不显示骨架，避免快网络闪烁。
- **后台菜单边距收紧**：后台所有页面左右边距统一（`px-10 → px-6`），写文章页移除页头条。
- **回收站合并入文章列表**：`/admin/trash` 合并为 `/admin/posts` 的 URL 驱动 tab，`PostStats` 新增回收站计数 badge。
- **依赖升级**：cargo 与 pnpm 依赖全量升级到最新版本；新增 `tokio-stream`（SSE）、`bollard`（Docker）。
- **Runner 配置**：`CODE_RUNNER_LANGUAGES` 默认开放全部语言；admin runner 页展示 Go/Rust。
- **Tooltip 组件抽取**：文章列表操作按钮用 `Tooltip` 包裹。

### Fixed

- **反应式 hook 不追踪普通 prop 的陷阱**：修复同一路由变体间导航（如 `/post/a → /post/b`）后文章正文/列表/标签不更新的严重 bug —— 根因是 `use_server_future`/`use_memo`/`use_resource` 不追踪非 signal 依赖。改为在闭包内读 `router.current()` signal 或直接内联计算。同理修复评论区 `CommentSection` 依赖追踪与 SSR hydration 不匹配。
- **主题切换动画**：修复 VT 动画期间 CodeMirror/xterm 主题同步（避免圆形展开动画期间直接跳变）、清理 VT 期间的 `animate-page-enter` transform（修复代码块被覆盖）、跟随系统模式系统偏好变化时同步 dark class（改回瞬切避免动画冲突）、`prefers-reduced-motion` 降级。
- **备份恢复假成功**：修复备份恢复实际不写入数据却返回成功（`psql` 未加 `ON_ERROR_STOP=1` 导致语句全错仍 exit 0）；修复备份/恢复任务轮询永不启动导致按钮卡在 loading。
- **可运行代码块容器清理**：修复容器清理失败静默泄漏（重试 + 日志告警）、编译型语言需 `/tmp` tmpfs 执行权、去掉 `nproc` ulimit 修复容器启动 EAGAIN。
- **文章锚点导航**：修复直接访问 `#hash` 时标题被 sticky header 遮住、hydration 后点击标题锚点触发整页刷新、hash 锚点跳转失效。
- **Tiptap 编辑器**：修复斜杠命令创建可运行代码块时模态框被立即关闭、斜杠命令文本残留进新节点（`/code` 带入 codeBlock）、代码块内 Backspace 删整块（`ignoreMutation` 误忽略 contentDOM 编辑）、Backspace 在 lowlight decoration 重建后失效、runnable 块 `classList.add` 抛 `InvalidCharacterError`、语言下拉展开时 Enter 误触发插入、空项 Enter 不退出列表（畸形文档根因）、TaskInputRule 升级后光标被甩到下一行、升级后折叠行被撑高 + 折叠图标垂直不居中、CodeMirror 折叠图标垂直不居中。
- **CodeMirror 编辑器**：修复编辑回退反馈循环（editing reversion loop）、编辑器塌缩导致上下背景割裂（`height:100%` 失效）、行号区背景与代码区割裂、未撑满容器、SQL 编辑器 gutter 与 content 背景割裂。
- **后台骨架屏**：修复后台骨架屏不可见、骨架屏→认证→正常页面布局闪烁、写文章页骨架屏高度不撑满（多次迭代）。
- **路由与分页**：修复 SQL 控制台 Ctrl+Enter 触发 panic（无 dioxus scope）、上下篇切换后可运行代码块消失。
- **UI 细节**：修复 markdown 表格水平填充渲染、admin 布局滚动条位置、写文章页滚动性、`--font-sans` 补齐 CJK sans 字体栈、暗色 type 色值。
- **数据库错误日志**：展开迁移错误的 source chain 全链路。
- 其他构建、CI、Docker 镜像（multi-arch buildx、HTTPS Debian 镜像绕过 HTTP 透明拦截、apk 清华镜像）、格式化与测试修复。

### Security

- **SQL 控制台护栏加固**：`DROP DATABASE`/`DROP SCHEMA`/`CREATE DATABASE` 绝对禁止（字符串预检 + AST 门）；`DROP`/`TRUNCATE`/`ALTER` 需确认；`UPDATE`/`DELETE` 无 `WHERE` 拒绝；多语句默认禁用；结果上限 500 行。
- **备份文件校验**：备份文件携带签名头，restore 拒绝非系统文件；`backup_path` 路径穿越漏洞修复（补单测）；`pg_dump --clean --if-exists` 使 restore 幂等（drop+recreate 而非 relation 已存在报错）。
- **备份恢复补单测**：`backup_path` 路径穿越漏洞补表驱动单测。

### Internal

- **新子工程 `xterm-terminal`**：xterm.js 6.0 IIFE 库 + smoke 测试。
- **新子工程 `codemirror-editor`**：CodeMirror 编辑器 + Rust bridge，Ctrl/Cmd+Enter 运行快捷键。
- **Docker runner 镜像**：`docker/build-runners.sh` 构建 base → python → node → go → rust 链；Go 镜像重定向 `GOCACHE`/`GOPATH` 到 `/tmp`，Rust 镜像封装 `run-rust.sh` 两步编译+运行 wrapper。
- **新增 Rust 单测**：SQL 控制台护栏表驱动单测、备份恢复单测、markdown `wrap_images_with_blur` 解耦文件系统依赖。
- **codemirror-editor smoke 测试**、`xterm-terminal` smoke 测试。
- **AGENTS.md 文档扩充**：Code Runner 架构说明、双 target 验证陷阱、custom hook 资源所有权陷阱、反应式 hook 不追踪普通 prop 的踩坑记录、Tiptap 交互 bug Playwright 调试方法论。
- **matt-pocock engineering skills** 引入。

## [0.3.0] - 2026-06-29

### Added

- **评论系统**：完整的访客评论功能，包含昵称/邮箱/URL、嵌套回复、管理后台审核（通过/标垃圾/删除）、待审评论 localStorage 持久化与 pending 状态轮询、相对时间显示。
- **文章回收站**：删除文章进入回收站，支持恢复、彻底删除、批量操作与清空；新增 `/admin/trash` 管理页面、`settings` 键值表与可配置的自动清理后台任务（保留天数、上限数量）。
- **主题切换动画**：基于 View Transitions API 的圆形展开动画（纯 CSS 实现，从点击点扩散），支持 `prefers-reduced-motion` 降级。
- **编辑器图片上传协调器**：Tiptap 自定义 Image 扩展，带上传中占位图（模糊 + spinner + 错误态）、失败重试、保存拦截、上传计数；支持 slash 命令、粘贴、拖拽。
- **封面图上传**：文章编辑器支持封面图上传（拖拽/点击/粘贴），封面区空态矮横条 + 可滚动主体布局。
- **Blur-up 渐进式图片加载**：Markdown 图片包裹双层结构（低清模糊 + 高清淡入），基于图片尺寸缓存的宽高比占位。
- **Lightbox 子工程**：`libs/lightbox/` TypeScript 项目，图库导航（淡入、箭头、键盘）、原点感知缩放、滚动关闭、防止背景滚动与闪烁。
- **yggdrasil-core 子工程**：核心 JS bundle 子工程，迁移 `post-content` 复制按钮与主题切换逻辑，删除 `public/js/`。
- **新增语法高亮**：TypeScript、JSX、TSX、Zig；补全 Swift 高亮；大小写不敏感的语法名匹配。
- **Markdown 源码视图切换**：编辑器可在富文本与源码视图间切换，按滚动比例同步位置。
- **健康检查端点**：新增 `healthz` 与 `readyz`。
- **内嵌数据库迁移**：启动时自动运行迁移（advisory lock + 逐迁移事务），迁移失败以友好退出 + 可配置重试窗口（`MIGRATE_STARTUP_TIMEOUT_SECS`）替代 panic；启动期自动创建目标数据库。
- **会话安全**：Session token 以 SHA-256 哈希存储（不再存明文）；可配置的单用户 session 数量上限（行锁串行化）；角色/状态变更通过 generation 失效所有会话。
- **CSRF 防护**：基于 Origin 的写接口 CSRF 检查，`APP_BASE_URL` 未设置时启动告警。
- **Cookie 安全**：`COOKIE_SECURE` 环境变量控制 session cookie 的 Secure 标志。
- **真实客户端 IP**：从 `X-Forwarded-For` 按 `TRUSTED_PROXY_COUNT` 提取真实 IP；未知 IP 时使用宽松限流桶。
- **图片响应缓存**：`Cache-Control` 与 `ETag` 头，支持 `If-None-Match` 304（RFC 7232 合规）；图片内存缓存改用 `bytes::Bytes`，新增按文件年龄与总大小淘汰的磁盘缓存定时清理任务。
- **图片上限可配置**：`MAX_IMAGE_DIMENSION`、`MAX_IMAGE_PIXELS`、`WEBP_QUALITY`、`WEBP_METHOD` 等环境变量，默认值翻倍；统一各格式大小上限。
- **`posts` 表字数与阅读时间**：新增 `word_count`/`reading_time` 列并在写入时维护；列表/搜索接口不再返回正文，读取预存的字数与阅读时间。
- **`PostListItem` 轻量 DTO**：列表/标签/搜索接口不再返回完整正文，显著降低缓存与序列化体积。
- **会话与搜索缓存**：基于 moka 的会话内存缓存（缓存对象不含密码哈希）；搜索结果短 TTL 缓存（10 秒，key 规范化）。
- **空状态与配图**：首页、归档、标签、搜索、后台文章/评论/回收站均接入 `EmptyState` 组件与装饰性配图（线条小狗等），配图圆角 + 暗色模式降亮。
- **UI 重新设计**：温暖色调 + 鼠尾草绿主色，统一 `paper-*` 主题变量；后台对齐前台主题变量，新增次要按钮冷调玫瑰色。
- **后台交互改进**：文章管理分页、重建内容按钮（带 tooltip）、重建缓存栏；评论状态筛选 `FilterTabs` 组件带滑动指示动画；回收站自动清理面板带滑入动画。
- **HTTP 压缩**：默认启用所有压缩算法，可通过 `COMPRESSION_ALGORITHMS` 配置；公共页面与静态资源 `Cache-Control`。
- **Dockerfile**：静态 musl 镜像构建（release 二进制 strip 符号）。
- **Gitea Actions CI**：CI 工作流。

### Changed

- 写路径缓存失效从「全量清空」改为「精确到 slug / tag / 列表页」，并在读取 slug/tag 元数据时使用事务 + `FOR UPDATE` 避免并发竞态；批量恢复、清空、重建等路径均采用精确失效。
- `get_post_stats` 将 3 次独立 `COUNT(*)` 合并为单次条件聚合查询。
- 图片解码、缩放、编码逻辑通过 `tokio::task::spawn_blocking` 移至阻塞线程池；Argon2 hash/verify、Markdown 渲染、GIF/WebP 原始校验同样 offload 到阻塞线程池。
- 为非图片路由启用 `CompressionLayer` 与 `TimeoutLayer`，`/uploads/*` 图片路由跳过压缩与全局超时。
- 连接池回收方法从 `Verified` 改为 `Fast`；重试从固定 2s 改为指数退避 + 抖动；新增 `statement_timeout`（`STATEMENT_TIMEOUT_SECS`）。
- 为 `deadpool-postgres` 显式指定 Tokio1 runtime；删除无效的 trgm GIN 索引。
- Tiptap 编辑器升级到 Vite 8 / Vitest 4 / TypeScript 6 / Tiptap 3.27（Rolldown），并从 `js_sys::eval` 迁移到 wasm-bindgen 绑定层（`tiptap_bridge`），EditorOptions/onReady/onUploadEvent 回调替代轮询。
- Lightbox 与 post-content 从 `include_str!` 内联改为配置驱动初始化；移除旧 `ImageViewer` 组件。
- JS 子项目从 npm 迁移到 pnpm。
- 封面图比例统一为 21:9；卡片重构使用原生 `blur-img` 结构。
- 邮箱正则、sanitizer allowlist 用 `LazyLock` 静态化以避免每次调用分配。
- Markdown 渲染读取 DB 中预渲染的 `content_html`/`toc_html`，写入时存储 `toc_html`。
- 大量「上帝组件」拆分为子组件：`CoverUploader`、`FilterTabs`、`AutoPurgeSettings`、`RebuildCacheBar`，共享 `EmptyState` 组件。

### Fixed

- 修复 `/uploads/{*path}` 路径因缺少 `ConnectInfo` 扩展而返回 HTTP 500 的问题。
- 修复并发重复评论提交（advisory lock）；重复检查改为事务内原子操作并对 `content_hash` 建索引。
- 修复评论表单：服务端 honeypot、a11y label、回复布局；待审评论嵌套显示于父评论下；评论项添加 `md-content` 类修复高亮与空行。
- 修复时序枚举攻击：不存在用户执行 dummy Argon2 verify；对 `check_pending_status` 限流防止状态枚举。
- 修复图片磁盘缓存清理跳过符号链接，防止遍历到缓存目录外部；过期 session 清理同时失效会话内存缓存。
- 修复 SSR hydration 不匹配、暗色模式 FOUC、ThemeToggle 状态同步。
- 修复 Tiptap 二次导航空白、链接命令顺序与 URL scheme 校验、blob 泄漏（节点销毁时 revoke）。
- 修复 404 页面、首页卡片嵌套锚点、封面图高度塌陷、暗色模式封面灰背景。
- 修复 WASM 构建假阳性 warning（27 处 `cfg` gate）、Tailwind v4 与 Tiptap 构建问题。
- 修复 dev 模式 SSR 缓存导致渲染陈旧 HTML；`/doc` 路由与静态托管冲突的 panic。
- 修复 `b7afd12` 起的 highlight 大小写匹配导致 Haskell 等高亮失效。
- 其他构建、CI、格式化与测试修复。

### Security

- Session token 以 SHA-256 哈希存储；可配置单用户 session 上限并以行锁串行化。
- 基于 Origin 的写接口 CSRF 防护；`COOKIE_SECURE` 控制 Secure 标志。
- 不存在用户执行 dummy Argon2 verify 防时序枚举；`check_pending_status` 限流。
- 图片路径 `canonicalize` 前缀检查（纵深防御）；拒绝不可解码图片返回 422，原始文件上限 20MB；所有图片响应加 `X-Content-Type-Options: nosniff`。
- 分页参数 `per_page` 与 `page` 上限钳制，消除公开接口 DoS 与无界 OFFSET / 缓存键扇出。
- 磁盘缓存写入改为 temp-file + rename 原子操作。

### Internal

- 新增 vitest 测试套件：`tiptap-editor`（UploadCoordinator、UploadImageNodeView、isValidUrl）、`lightbox`（geometry、lightbox 生命周期）、`yggdrasil-core`（post-content、theme-transition 降级）。
- 扩充 Rust 单元测试覆盖：AppError、sanitizer、WebP、theme、cache、db（迁移注册校验）、PostListItem 等。
- 补齐大量中文文档注释；更新 AGENTS.md、新增 DEVELOPMENT.md 与生产部署指南。
- Makefile 完善 `test`（cargo test + vitest）、`clippy`、`fix`、`doc`（ayu 主题）、`build-lightbox` 等目标。
- 仓库安全审查（`449a545`）修复多项关键问题。

## [0.2.0] - 2026-06-10

### Added

- 404 Not Found 页面
- 基于 moka 的内存缓存模块（文章、标签、统计查询缓存）
- 读写操作缓存支持：读操作命中缓存，写操作自动失效相关缓存
- 文章列表返回准确的总条目数，前台首页/归档/标签页分页使用准确总数
- WebP 图片编码支持（zenwebp），上传图片自动转换为 WebP 以节省存储空间
- 图片变体磁盘级缓存

### Changed

- `posts.rs` API 拆分为模块目录结构
- 统一错误处理为 `AppError` 枚举
- 缓存层优化：直接返回命中结果，COUNT(*) 结果单独缓存
- 提取 WebP 编码辅助函数减少重复代码
- 简化 TraceLayer 配置并重新格式化路由定义

### Fixed

- 修复多处 `#[cfg(feature = "server")]` 门控缺失导致的编译问题
- 缓存正确失效旧 slug 和新 slug
- WebP 解码缓冲区大小限制，防止恶意大分配
- 代码块复制按钮点击处理
- 其他细节修复

### Internal

- 新增缓存、WebP 配置测试用例
- 添加 release 自动化技能

## [0.1.0] - 2026-06-09

### Added

- Dioxus 0.7 全栈项目脚手架
- PostgreSQL 数据库建表（用户、文章、标签、会话）
- 用户认证系统：注册、登录、Session 管理
- 首个注册用户自动成为 admin，后续注册关闭
- HttpOnly cookie 会话机制
- 后台管理页面与路由
- Tiptap 富文本编辑器集成（Markdown 模式）
  - Slash 命令、表格、任务列表、图片和链接扩展
  - 图片粘贴/拖拽上传
- 文章 CRUD：创建、编辑（含数据回填）、列表、删除
- 文章封面图支持
- Markdown 渲染：TOC 目录、锚点链接、字数统计、预计阅读时间
- 代码高亮（syntect + catppuccin 主题），支持 Swift/Kotlin 自定义语法
- XSS 防护（ammonia 清洗 HTML）
- 前台博客页面（PaperMod 风格）
  - 首页（个人简介 + 文章列表 + 分页）
  - 归档页（按年月分组）
  - 标签页（标签云 + 标签详情）
  - 文章详情页（目录、上下篇导航）
  - 搜索页
  - 关于页
- 暗色模式（系统偏好检测 + 手动切换，SSR 安全）
- SSR 预渲染（首页、文章、归档、标签）+ 增量缓存
- 骨架屏加载动画（各页面独立骨架，防闪烁）
- 图片处理：缩放、缩略图、旋转、格式转换（moka 缓存）
- 图片灯箱查看器
- pg_trgm 全文搜索
- Rate limiting（注册、登录、上传接口）
- 数据库连接池重试逻辑
- Session 过期自动清理（每小时）
- 数据库性能索引（posts/tags/sessions）
- 数据库迁移脚本（migrate.sh）
- 122 个单元测试覆盖 12 个模块
- 项目开发指南（AGENTS.md）

### Changed

- Tailwind CSS v4 + 独立 CLI 构建
- admin 模块重构为共享组件 + card 布局
- 全局使用 Dioxus 客户端路由替代原生导航
- 提取公共组件：FormInput/FormLabel/AlertBox、SkeletonLine/SkeletonBox/SkeletonCard
- 提取工具模块：slug、markdown、tags、text、time、session
- API 层 DRY 重构（错误处理、N+1 查询修复 via JOIN+array_agg）
- 文章 slug ASCII 化 + 时间戳回退
- Tiptap 编辑器 Vite 构建输出固定文件名
- 首页 HomeInfo 个人简介替代原始首区

### Fixed

- 修复 admin 路由切换闪烁
- 修复编辑器暗色主题和列表样式
- 修复 Footer 滚动监听器未清理
- 修复 CJK 字数统计
- 修复代码块 Tailwind `.block` 类冲突
- 修复 SSR 水合不匹配（ThemeToggle）
- 修复 WASM 生产环境 404（symlink 修复）
- 修复图片上传 500 错误
- 修复 Markdown 渲染中 data URI 丢失
- 修复暗色模式 FOUC 和状态同步
- 修复登录后 UserContext 未重置
- 修复文章 slug 唯一性检查（含已删除文章）
- 修复 Tiptap 编辑器二次导航空白问题
- 修复模板 hydration 不匹配警告
- 修复 Clippy 和编译器警告
