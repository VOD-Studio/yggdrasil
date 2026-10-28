# Yggdrasil

![CI](https://github.com/DefectingCat/yggdrasil/actions/workflows/ci.yml/badge.svg)

基于 Leptos 0.8 的全栈博客与内容管理系统。一套 Rust 代码，同时编译为浏览器端 WASM（路由级分包）与原生 Axum 服务端。

在线实例：<https://rua.plus>

![Yggdrasil 首页](docs/images/screenshot-home.png)

## 特性

- **写作**：Tiptap 富文本编辑器，支持脚注、任务列表、数学公式与 Mermaid；服务端以 pulldown-cmark、syntect、KaTeX 渲染。
- **笔记与知识库**：随记、主题笔记与公开版 / 知识库版本，详见 [笔记文档](docs/notes.md)。
- **MCP**：内置 MCP 服务器（`POST /mcp`），AI 客户端可检索文章并按作用域管理内容，详见 [MCP 文档](docs/mcp.md)。
- **代码沙箱**：文章内 `runnable` 代码块与 `/admin/runner` 在隔离 Docker 容器中运行 Python / Node / Go / Rust / Bun，SSE 实时回显。
- **运维**：数据库状态、服务器监控、SQL 控制台、数据导出与 `pg_dump` 备份恢复。
- **性能与安全**：增量 SSR 与 moka 多级缓存；Argon2、CSRF 防护、按 IP 限流。
- **外观**：陶土橙暖调明暗双主题，系统字体与 Lucide 图标，响应式布局。

## 技术栈

Leptos 0.8 · cargo-leptos · Axum · PostgreSQL · Tailwind CSS v4 · Bun workspace（Tiptap / CodeMirror / xterm.js / Mermaid）· bollard

## 快速开始

准备 Rust、cargo-leptos 0.3.11（`cargo install cargo-leptos --version 0.3.11 --locked`）、Tailwind CSS v4 standalone CLI 与 Bun，复制 `.env.example` 为 `.env`，然后：

```sh
make dev     # 增量构建前端资源并启动开发服务
make test    # Rust 与前端测试
make lint    # Biome / TypeScript / Clippy / rustfmt
make build   # 发布构建
```

直接运行 `make` 可列出全部目标；缺少本地工具时可用 `make docker-dev`。

发布前先运行 `make wasm-toolchain` 安装固定的 WASM nightly 工具链；`make -j4 build` 可并行准备资源与文档，完成后再构建前端、服务端并装配校验。发布时按 `libs/bun.lock` 冻结安装前端依赖，依赖变更需先更新锁文件。

CI 的 release-split 阶段通过 `bun scripts/check-build.ts` 执行完整 `make build`：构建退出失败，或标准输出/错误输出出现 warning、WARN、error、`(!)` 等诊断时均阻断发布，即使工具返回成功也不放行；检查前会去除 ANSI 颜色码。可在本地运行同一命令，完整原始日志写入 `target/build.log`，CI 无论成功失败都会上传。

`scripts/` 中的 JavaScript 工具已统一为 Bun TypeScript，直接运行 `bun scripts/<name>.ts`；测试文件使用 `bun test scripts/check-build.test.ts`。先在 `libs/` 执行 `bun install --frozen-lockfile`，然后可用 `cd libs && bun run typecheck:scripts` 单独做严格类型检查；`bun run typecheck`、`make lint` 与 CI 也包含此检查。现有 Bash／Fish 运维脚本保持原运行方式。

`make clean` 会删除编译产物、生成的浏览器资源和前端依赖，保留受版本控制的静态资源与上传原文件。

## 生产部署

必须前置反向代理（nginx / Caddy）终结 TLS，并配置：

- `APP_BASE_URL`：CSRF 可信来源
- `COOKIE_SECURE=true`
- `TRUSTED_PROXY_COUNT`：X-Forwarded-For 中真实客户端 IP 右侧的段数，不等于代理层数。直连填 0；单层反代用 `$remote_addr` 覆盖 XFF 并设置 `X-Real-IP` 时填 1；CDN + 源站反代（都追加 XFF）填 1 而非 2。设大会信任伪造 IP 绕过限流；源站还须只接受上游代理回源，否则直连源站同样可伪造

容器监听 `127.0.0.1:3000`，探针为 `/healthz`（存活）与 `/readyz`（就绪）。

生产环境 xun / rua 双机主备（流复制热备、单向文件同步、EdgeOne 回源鉴权、故障切换与回切）见 [docs/双机主备](docs/双机主备/README.md)。

## 目录

```
src/         Rust 源码（api / mcp / db / pages / components）
libs/        前端 JS 库（Bun workspace）
migrations/  SQL 迁移，启动时自动运行
docker/      Dockerfile 与沙箱镜像
docs/        专题文档
```

## 文档

- [更新日志](CHANGELOG.md)
- [贡献者约定](AGENTS.md)
