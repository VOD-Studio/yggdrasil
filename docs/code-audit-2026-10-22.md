# 代码质量审计（2026-10-22）

- 版本：v0.24.2（master @ `d97e013`，工作树干净）
- 范围：`src/**`（336 个 Rust 文件 / 87k 行）、`libs/**`（7 个 Bun 包）、`styles/**`、`scripts/**`、`docs/**`、`Cargo.toml`
- 方法：7 路并行只读审计（`src/api`、UI、infra/db/mcp、libs/styles、冗余、授权、测试）+ 主审对关键结论逐行复核
- 性质：本文件是审计记录，不是待办清单；每条的修复建议均为最小改动

## 0. 基线（实测通过项）

| 检查 | 命令 | 结果 |
|---|---|---|
| Clippy | `cargo clippy --all-features --all-targets --locked -- -D warnings` | exit 0，0 warning（无 `[lints]` / crate 级 `allow` 屏蔽） |
| 格式 | `cargo fmt --all -- --check` | 0 diff |
| 前端 lint / 类型 | `bun run lint`、`bun run typecheck`（`libs/`） | Biome 141 文件通过；7 个包 tsc 通过 |
| Rust 测试 | `cargo test --locked --all-features` | 992 通过，11 ignored，0 失败 |
| 前端测试 | `bun run test`（`libs/`） | 全包通过（core 189 / tiptap 203 / lightbox 109 …） |
| 遗留标记 | `grep -rn "TODO\|FIXME\|XXX\|todo!\|unimplemented!" src` | 0 |
| panic 面 | 非测试路径 `unwrap/expect` 146 处 / 87k 行 | 抽查均为 LazyLock 正则、`NonZeroU32` 不变量、静态 HeaderValue 等有注释的不可失败点 |
| 依赖卫生 | 逐个 direct/optional dep 在 `src/**` + `build.rs` grep | 0 个未引用 crate；重名 crate（`fancy-regex`、`moka`+`dashmap`、`zenwebp`+`image`、`resvg`+`skrifa`）均为有意选择 |
| 分层 | `grep "use crate::db::" src/pages src/components` | 0 处；`mcp` 复用 `api`，无反向依赖 |
| 迁移 | `migrations/*.sql` vs `src/db/migrate.rs` `MIGRATIONS` | 29 个全部注册、升序、无重复，另有 3 条测试守护 |

结论：底子好于同规模项目，问题集中在**授权/只读判定的边界条件**、**复制粘贴样板**、**自证式测试**三类。

## 1. P0 安全与正确性

### 1.1 导出接口"只读"校验可绕过，跨站 GET 可触发任意写

- `src/api/database/export.rs:116`：`matches!(stmt, Statement::Query(_) | Statement::Explain { .. })` —— 放行任意 `EXPLAIN`。sqlparser 的 `parse_explain` 对内部语句只做泛化解析，因此 `source=query:EXPLAIN ANALYZE DELETE FROM posts WHERE id=1` 通过 `parse_source`（`:90-100`）。
- `export_sql` 在 `:151` 原样 `client.query(&source_sql, &[])` 执行，Postgres 下 `EXPLAIN ANALYZE` **确实执行**内部 DML。
- 路由注册为 GET：`src/startup.rs:529`；而 `csrf_middleware` 仅拦 POST/PUT/PATCH/DELETE（`src/api/csrf.rs:15-20`，`:131`），session cookie 为 `SameSite=Lax`（`src/auth/session.rs:40`）→ 管理员浏览器一次跨站顶层 GET 即完成写入（含删表级 DML）。
- 同源缺陷（潜在）：`src/api/database/sql_console.rs:191-198` 的 `is_read_only` 同样放行 `Statement::Explain { .. }`，导致 `writes_affect_cache`（`:224`）判错、缓存失效被跳过。当前因其外包 `SELECT * FROM (…)` / `EXPLAIN EXPLAIN …` 而报错未触发，不可依赖。
- 修复：`is_read_only_ast` 拒绝 `Explain { analyze: true, .. }` 与非 `Query` 内层节点；导出接口改 POST 或对该路由单独做 origin 校验；`sql_console` 同法收紧分类。

### 1.2 MCP bearer token 不参与封禁/降级吊销

- `src/mcp/auth.rs:163-168`：`resolve_principal` 只查 `mcp_tokens`（`token_hash` / `revoked_at` / `expires_at`），不 JOIN `users`；`require_admin`（`src/mcp/tools/common.rs:38-48`）随后信任行内 scope 字符串。
- 对照：cookie 路径每次都校验 `users.role` 与 `users.session_generation`（`src/api/auth.rs:405-439`），migration 018 触发器（`migrations/018_session_generation_trigger.sql:10-21`）专为此设计；017 只有 `REFERENCES users(id) ON DELETE CASCADE`（`migrations/017_mcp_tokens.sql:9`），无级联吊销。
- 影响：账号被封禁/降级后，其 `admin` scope token 仍可执行 `run_code`（容器内代码执行）、`purge_orphan_assets`、`update_settings` 及全部 write scope 操作；`/api/mcp/upload` 走同一函数（`src/mcp/auth.rs:204-211` → `src/api/upload.rs:250`）。唯一止损手段是逐 token 手工吊销。
- 修复：`resolve_principal` 中 `JOIN users u ON u.id = mcp_tokens.user_id` 并要求角色有效（或在签发时写入 `session_generation` 并比对）。

### 1.3 `admin_guard` 对无效会话 fail-open

- `src/middleware.rs:226-234`：`Ok(None)`（token 失效/过期/generation 不匹配）与 `Err(_)`（DB 抖动）一起落到 `_ => true`，注释也如此声明；但同函数文档只承诺"DB 抖动 fail-open、无 token 拦截"。
- 影响：携带陈旧/伪造 cookie 的请求跳过 302 登录跳转，拿到 admin SSR 外壳（无数据泄漏，server fn 会二次校验，但守卫本身失效）。
- 修复：`Ok(None) => false`，仅 `Err(_) => true`。

### 1.4 头像校验两份实现，写库那份更弱

- `src/api/profile.rs:77-92`：`normalize_avatar_url` 注释称"与友链头像同一规则"，实际只判 `starts_with("/uploads/")` + 长度。
- `src/api/friends.rs:208-213`：`is_local_asset_url` 另拒空路径、二次 `/`、`..`、`\0`。
- 影响：`/uploads/../../x`、`/uploads/`、`/uploads//x` 可写入 `users.avatar_url`，后续经 `src/api/comments/helpers.rs:57-63` 渲染为 `<img src>`。
- 修复：`is_local_asset_url` 提升为 `pub(crate)` 共用谓词，删除分叉副本。

### 1.5 运行器非流式路径可永久占用信号量

- `src/infra/docker.rs:293`：`Ok(_) => {} // wait error` 既不 kill 也不重新 await；随后 `:305` 起 `while let Some(item) = stream.next().await` 无超时。
- 影响：容器持续运行且不产出日志时 `run_in_container` 永不返回，调用方的 `RUNNER_SEMAPHORE` 许可（`src/api/code_runner/execute.rs:50,128-131`；MCP `run_code` 同路径）与 `ContainerGuard` 一并泄漏。流式路径在 `:637-681` 已修同类问题。
- 修复：`Ok(_)` 与超时分支同处理 —— kill + 有界重新等待。

### 1.6 素材引用计数：每行 5 个关联子查询

- `src/api/assets/mod.rs:36-41` 的 `ASSET_REF_COUNT_EXPR` 注入分页 SELECT（`src/api/assets/list.rs:99`）：60 行 × (含 `comments.content_html LIKE '%' || a.path || '%'` —— 列上带表达式，索引不可用；`users.avatar_url` / `friend_links.avatar_url` 无索引)。
- `total`（`list.rs:97-108`）与 summary（`:112-128`，同一谓词每行计算 4 次）再各乘全表，管理员每次切 tab 重复。
- 修复：本页引用明细查询（`list.rs:138-230`）已取回同一批引用，`ref_count` 应在 Rust 侧派生；summary 改为 `LEFT JOIN LATERAL` 或落一张 `asset_refs` 汇总。

## 2. 性能与一致性（中优先）

| 位置 | 问题 |
|---|---|
| `src/api/markdown.rs:50-414` | `render_markdown_enhanced` 单函数 365 行、约 7 项职责（两步事件收集、TOC、标题/代码块/数学/脚注分支、runnable fence 的 `data-*`、mermaid 特例、表格 flush、后处理）。建议拆 `MarkdownRenderer` 结构体，runnable fence 移入 `code_runner` |
| `src/api/sanitizer.rs:227-268` | `sanitize()` 每次调用重建 `HashSet<&str>` + 18 项 tag→attrs `HashMap`；该路径是 markdown/评论渲染热路径。基表应提为 `static LazyLock<...>`，仅每次合并 `extra_tag_attrs` |
| `src/api/posts/trash.rs:139,224` | `batch_restore_posts` / `batch_purge_posts` 在事务内逐 id 调用 `fetch_post_tags`（最多 50 次往返）；`helpers::fetch_post_tags_batch`（`posts/helpers.rs:226`）已存在且被 `empty_trash`（`trash.rs:296`）使用 |
| `src/mcp/tools/tags.rs:59,115` | `create_tag` / `rename_tag` 只 `invalidate_all_tags()`，漏 `POST_LIST_CACHE` / `TAG_POSTS_CACHE` 与 SSR 层；对照 `cache.rs:820-832` 的 `invalidate_for_post_write`。改标签名后 `/archives` 与标签页最长 60/120s 仍出旧 HTML |
| `src/api/image.rs:828-842`（调用点 `src/components/post/post_cover.rs:29`） | 同步 `std::fs` 读在 SSR 渲染任务内直接调用；其他 markdown 调用点都包了 `spawn_blocking`（`posts/helpers.rs:331`、`comments/create.rs:184`） |
| `src/cache.rs:874-876` | `normalize_search_key` 截断到 200 字符，长查询前缀相同即串缓存（低频，注明即可） |
| `src/api/auth.rs:30-33`、`src/api/comments/helpers.rs:145-152,189-195` | 用字节长度校验却报"字符"：纯中文昵称实际只允许 ~16 字符，提示写 50。`friends.rs:238-242`、`profile.rs:87-90`、`notes/store.rs:321,510` 用的是 `chars().count()`，应统一 |
| `src/api/posts/service.rs:193-197` | 两处 `expect()` 依赖非局部不变量（`replacement()` 总填 content/status），release 下 `panic=abort` 会整进程退出；应让 `replacement()` 一并返回已校验字段 |

## 3. 冗余与死代码（可安全清理）

| 项 | 证据 | 规模 |
|---|---|---|
| admin 前置校验（cookie→session→role→401/403） | `api/database/export.rs:32-49` 与 `api/logs/sse.rs:41-56` 字节级相同（后者注释自承"镜像 export.rs"）；另 `database/backup/transfer.rs:64-79,229-244`、`api/upload.rs:84-102`、`api/notes/attachments.rs:12-16` | 6 份 |
| `INSERT INTO settings … ON CONFLICT (key) DO UPDATE` | `api/settings/{asset_purge.rs:112,backup.rs:115,238,image_cache.rs:158,image_limit.rs:150,rate_limit.rs:366,runner.rs:269,security.rs:201,site.rs:136,trash.rs:56,65,upload.rs:83,webp.rs:119}` + `api/logs/mod.rs:336`；`settings/mod.rs:68-72` 已有批量 `UNNEST` 版本 | 15 处 |
| 手写转义 | `api/feed.rs:51`、`og/mod.rs:410` vs 权威 `utils/html.rs:16`（该模块注释自称"已统一两份 `escape_html`"） | 3 份 |
| `slugify`（拼音 + 连字符合并） | `api/markdown.rs:570` 与 `api/slug.rs:24`，单测亦成对重复（`markdown.rs:899-933` ≈ `slug.rs:162-262`） | 2 份 |
| `group_digits` 逐字相同 | `components/post/post_meta.rs:33`（已 `pub(crate)`，被 `pages/admin/write/metadata.rs:3` 复用）与 `pages/home.rs:526` | 2 份 |
| `is_simple_ident`（**语义不同**：export 版接受 `9db`） | `db/pool.rs:354`（要求首字符 `[A-Za-z_]`）vs `api/database/export.rs:109`；两套单测 | 2 份 |
| `compute_content_hash` 内联 sha256 | `api/comments/helpers.rs:217-231` vs `utils/server.rs:12 sha256_hex` | — |
| 仅 showcase 可达的组件 | `ui/controls.rs:159 LoadingButton`、`:230 Radio`、`ui/surfaces.rs:44 Popover`、`ui/display.rs:15 StatusBadge`、`forms.rs:814 FormLabel` 只被 `components_showcase.rs` 引用；`skeletons/atoms.rs:38 SkeletonCard`、`pages/notes.rs:1018 NotesSkeleton` 零引用 | 7 个 |
| 死常量 / 死导出 | `forms.rs:14 TEXTAREA_CLASS`（全仓 1 命中=定义行）、`libs/shared/src/index.ts:122,139 MERMAID_{LIGHT,DARK}_VARS`、`styles/pages/reading-responsive.css:2 @keyframes fadeIn`、`libs/music-player/`（空目录，不在 `mk/assets.mk:54 DEV_LIBS`） | 4 项 |
| 陈旧 `#[allow(dead_code)]` | `ui/feedback.rs:82,87,119` 注释"随页面重写逐步启用"，实际 `use_toast()` 在 23 个文件使用；`ui/styles.rs:14 BADGE_BASE` 被 `display.rs:3,17` 使用；`models/mod.rs:18` 的 `mcp_token` 被 13 文件引用 | 5 处 |
| 单用户常量 | `ui/styles.rs:24 BTN_SECONDARY` 仅 `empty_state.rs:34` 使用；另有 ~18 处硬编码 `"btn btn-*"` 绕过 `ui/controls.rs:67 button_class()` | — |

单路审计（未逐条复核）补充：

- `src/pages/admin/system/export.rs:41-56` 的 `segmented` radiogroup 闭包复制 6 处（`settings/image_section.rs:79`、`security_section.rs:59`、`upload_section.rs:30`、`mcp/token_editor.rs:271`、`admin/preview.rs:183`），而 `Radio` 正是为此存在；`CHANGELOG.md:170` 声称导出页已改用 `Radio`，实际没有。
- `src/components/search_palette.rs:438-780` 与 `src/components/admin_palette.rs:177-453` 近乎逐行重复（signal 集合、防抖 + seq 生成、scrim/dialog/`data-ygg-modal-panel`/`trap_tab`、输入行键位、进度点、分组渲染），仅数据源与文案不同。
- `admin-page-head` 结构复制 13×（`dashboard.rs:209`、`posts.rs:110`、`notes.rs:158`、`comments.rs:184`、`assets.rs:250`、`friends.rs:139`、`posts_trash.rs:243`、`profile.rs:77`、`runner.rs:68`、`mcp.rs:126`、`notes/notebooks.rs:87`、`settings/mod.rs:322`、`system/mod.rs:165`）+ 12 处骨架副本；副标题占位写法已分叉（`"\u{00a0}"` vs 占位串）。
- `forms.rs:632-647` 的 `FormInput` 缺 `maxlength`/`rows`/`required`/`readonly`，导致 40 处裸 `<input class: "field …">`（`grep -rn 'class: "field' src | grep -v forms.rs`）。
- `forms.rs:105-112,151-152,205-208,340`、`admin_layout.rs:499-500,730-731`、`admin_palette.rs:179`、`header.rs:45-46`、`navigation.rs:42`：showcase 专用分支（`id_scope`、`CommentSource::Local`、`preview`/`on_demo_navigate`/`demo_active`）编进了生产组件。
- `libs/codemirror-editor/src/index.ts:20-70` 与 `libs/xterm-terminal/src/index.ts:19-65` 是同一套 IIFE 实例注册表 + 主题热切换样板；`libs/yggdrasil-core/src/changelog-nav.ts:89-133` 与 `toc-sidebar.ts:73-110` 是两套滚动监听实现（阈值不同，同文档会给出不同"当前节"）；`libs/lightbox/src/view.ts:18` 与 `mermaid-overlay.ts:96,101` 的 `rectOf`/`clamp` 跨包逐字重复。
- `libs/codemirror-editor/package.json:29` 的 `"codemirror": "^6.0.2"` 无任何 import，也不是 peer 需求。
- `libs/yggdrasil-core/src/theme-preload.test.js:7-12` 正则抽取 `src/theme.rs` 常量并 `new Function` 求值 —— Rust 侧重命名即打断 JS 套件，应移到 `scripts/*.cjs` 或改为断言 SSR HTML。

文档类：

- `docs/10-设计风格与动效体系总结.md`：第 5 行自述"适用仓库：`open-claude-ui`"，全部令牌（`--cds-*`、`--df-*`）在本仓库零命中，且把 `open-claude-ui` 的 `AGENTS.md`/`code-examples/*` 当"当前仓库"。放在 `docs/` 会被当成项目设计规范引用 —— 删除或移出。
- `docs/components-showcase-preview-plan.md:265`、`docs/wasm-optimization-plan.md:77,232`：状态行已写"完成"，正文仍在"建议新增"已存在的产物（`scripts/test-components-showcase.cjs` 已被 `scripts/regression.cjs:12` 与实际文件采纳；`scripts/measure-release.cjs`、`public/release-manifest.json` 已存在）。活文档是 `docs/components-showcase.md`；这两份应标注"历史记录，勿按此文实施"。
- `scripts/test-writing-guide.cjs`：唯一引用是自身头注释，不在 `scripts/regression.cjs` 套件表、不在 Makefile、不在 CI，而其保护的 `/writing-guide` 路由已被 smoke 覆盖。
- `.gitignore:39-40`、`.dockerignore:43,48`、`Cargo.toml:62`：FreeBSD 交叉编译已移除（`CHANGELOG.md:191`），残留忽略规则与注释。

## 4. 测试质量（假绿优先）

- `src/api/health.rs:106-131`：两个"探针测试"从不调用 `healthz()` / `readyz()`，只 `json!` 造值再断言自己造的值（注释自承"用字面量模拟字段值"）。`/healthz`、`/readyz` 是发布/容器健康契约，目前**零真实覆盖**。改：直接调用并断言返回结构。
- `src/api/upload.rs:845-875`：`filename_format_no_spaces`、`should_use_webp_ext_for_non_gif`、`should_preserve_gif_ext` 各自在测试内重写文件名拼接与扩展名规则，再断言本地变量；真实逻辑在 `:475`（文件名）与 `:592-660`（`transcode`）。且镜像的规则与生产**不一致**（生产保留 `image/webp`、WebP 更大时保留原格式）。改：删除，改测 `transcode_image_blocking`（gif/webp/jpeg 输入）。
- `tests/post_detail_slug_rerun.rs:11,31`：`include_str!("../src/pages/post_detail.rs")` + 字符串断言（`!src.contains("slug_signal")`、`src.contains("std::iter::once(post.id)")`）。组件重命名即真空通过，无关重构即误报；同时 `:18` 的失败指引指向不存在的 `.agents/skills/dioxus-render-purity/SKILL.md`（全仓仅此一处出现）。
- 11 个 `#[ignore]` DB 回归中 2 个无法按文档运行：`src/api/settings/trash.rs:138` 与 `src/api/mcp_tokens.rs:206` 读 `YGGDRASIL_TEST_DATABASE_URL`，而该变量全仓库只出现在这两行 —— 不在 `docs/testing.md`、不在 `scripts/test-databases.sh`（该脚本只设 `DATABASE_URL` → `ygg_mcp_test`/`ygg_notes_test` 与 `YGG_BACKUP_TEST_DATABASE_URL`）、无 Makefile target。二者只用 TEMP 表，无破坏风险，纯粹是死测试。改：换用 `DATABASE_URL`，或补进文档与脚本。

## 5. 工程卫生

- **`public/` 生成产物半追踪**：9 个 bundle 仍在 git 索引（`git ls-files public/` 可见 `public/{codemirror,lightbox,tiptap,xterm,yggdrasil-core}/*`），而 `.gitignore:15-20` 只挡住更新的 `public/yggdrasil-core/yggdrasil-enhance.js`（`git check-ignore -v` 命中 `.gitignore:18`）。该文件被 `src/components/enhance_assets.rs:20` 与 `src/middleware.rs:495` 引用。后果：新增构建产物 `git add` 被静默拒绝，旧的 9 份产物照旧随仓库发布。改：`git rm --cached` 那 9 份并统一策略。
- **`scripts/test-dev-assets.cjs` 已失效**：mock 输出表只写 `yggdrasil-core.js/.js.map/.css`，`mk/assets.mk:62` 的 `DEV_OUTPUTS_yggdrasil-core` 有 5 项（含 `yggdrasil-enhance.js/.map`），于是 `mk/assets.mk:69` 每次都挂 `dev-assets-force`，`scripts/test-dev-assets.cjs:78` 的 `step([])` 断言失败。实测（`node scripts/test-dev-assets.cjs`）抛 `ERR_ASSERTION`。该脚本只在 `docs/testing.md:10` 手工提及，不在 CI（`.github/workflows/ci.yml` 无引用）。改：补两项 mock 输出或纳入 CI。
- `src/models/settings.rs`（1561 行）：各组 clamp 边界不一致 —— `clamp_max_mb`/`clamp_max_age_hours`/`clamp_per_sec` 无上限，`RunnerSettings::clamp_max_output_bytes`/`clamp_max_source_bytes` 只 `n.max(1)` 无 MAX，面板/环境变量填 `u64::MAX` 会关掉喂给容器的输出/源码上限（`infra/docker.rs` 用 `limits.output_bytes`）；`SystemInfo`（env 快照 DTO）也留在此模型里。改：统一 `clamp_range(v,min,max)` + 各字段 `MAX_*`，`SystemInfo` 迁至 API 层。
- `src/mcp/auth.rs:63-95`：MCP 限流（`RATE_LIMIT_MCP_PER_SEC`/`_BURST`/`UPLOAD_*`）与 `MCP_TOKEN_ENC_KEY` 仅环境变量、面板不可见，而 Web 侧限流是 DB 驱动 + 可编辑（`models/settings.rs:527-531`、`api/settings/rate_limit.rs`）。改：并入 `RateLimitSettings`，统一重启提示。
- `src/api/url_fetch.rs:177-208`：SSRF 黑名单覆盖 IPv4-mapped / ULA / link-local / multicast，但未展开嵌入 IPv4 的过渡前缀（`64:ff9b::/96` NAT64、`2002::/16` 6to4、`2001::/32` Teredo）。在路由这些前缀的主机上，`upload_media(url=https://[64:ff9b::7f00:1]/…)` 会打到 127.0.0.1（记 `write` scope 即可触发）。改：先展开过渡前缀再走 IPv4 分支，或对 IPv6 改用白名单。
- `src/components/code_runner/runner/{editor.rs:92-96,terminal.rs:63-67}`：同一段 `document.documentElement().class_list().contains("is-theme-transitioning")` DOM 探测复制两份；应提取到 `src/utils/js.rs` 旁。
- `src/mcp/tools/posts/tests.rs` 的 76 处 `unwrap/expect` 位于测试辅助文件内，属正常（记录以免误判为非测试路径 panic 面）。

## 6. 建议修复顺序

1. §1.1 导出只读绕过（改判定 + 路由方法，各 ~10 行）—— 唯一具备"跨站触发写"路径的缺陷
2. §1.2 MCP token 吊销缺口 —— 封禁机制对 bearer 通道无效
3. §1.5 信号量泄漏、§1.4 头像校验、§1.3 guard fail-open
4. §3 表内前 6 项（纯删除/收敛，零行为变更，可一次性提交）
5. §4 三个假测试 + 2 个不可运行回归
6. §5 卫生项（`public/` 追踪策略、失效脚本、文档归档）

## 附：审计方法（可复现）

- 并行 7 路只读审计，分工：`src/api/**`；`src/pages+components`；`src/db+cache+middleware+mcp+models+infra`；`libs+styles+public`；仓库冗余与依赖；授权/信任边界（security-reviewer）；测试套件
- 主审复核项：`cargo clippy/fmt/test`、`bun run lint/typecheck/test`、`scripts/test-dev-assets.cjs`、`git ls-files/check-ignore`、以及 §1.1/§1.2/§1.3/§1.4/§1.5/§1.6/§3 表格主要行的逐行 `read`/`grep`
- 标注"单路审计（未逐条复核）"的条目仅来自一个审计源，修复前建议先确认