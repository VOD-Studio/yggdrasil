# Yggdrasil

![CI](https://github.com/DefectingCat/yggdrasil/actions/workflows/ci.yml/badge.svg)

基于 Dioxus 0.7 的全栈博客与内容管理系统。一套 Rust 代码同时编译为浏览器端 WASM 前端与原生 Axum 服务端——服务端渲染（SSR）配合多级缓存兼顾首屏速度与 SEO，写作侧集成富文本编辑、数学公式与流程图，运维侧提供数据库管理、SQL 控制台与备份恢复。

在线实例：<https://rua.plus>

## 特性

**内容与写作**

- Tiptap 富文本编辑器，所见即所得：脚注、任务列表、数学公式节点、Mermaid 流程图实时预览。
- 服务端 Markdown 渲染：pulldown-cmark + syntect 代码高亮 + KaTeX 数学公式 + Mermaid 流程图，渲染结果固化入库。
- 中文标题自动转拼音 slug，悬浮目录（TOC）滚动监听，图片灯箱预览。
- 评论系统（嵌套回复、审核、防滥用），友链页，RSS 2.0 与 JSON Feed 订阅源。

**代码沙箱**

- 文章内 ` ```lang runnable ``` 代码块与后台 `/admin/runner` 在隔离 Docker 容器中执行，支持 Python / Node / Go / Rust / Bun。
- SSE 流式输出经 xterm.js 实时回显；只读 rootfs + tmpfs + 资源/能力限制 + 容器强制清理。

**AI 集成**

- 内置 MCP 服务器（`POST /mcp`，Streamable HTTP，bearer token 鉴权）。
- AI 客户端（Claude Code / Cursor / Cline 等）可把已发布文章当知识库检索，并按作用域（read / write / admin）执行文章、评论、标签、媒体、设置与代码运行等后台操作。

**媒体与素材**

- 素材库：按内容 SHA-256 去重、引用追踪、孤儿清理、就地编辑 alt，WebP 转码与图片尺寸/像素校验。

**运维后台**（`/admin/system`）

- 仪表盘、数据库状态与连接指标、服务器资源监控、SQL 控制台（四道护栏）、流式数据导出、`pg_dump` 备份恢复。

**性能与安全**

- Dioxus 增量 SSR 渲染 + moka 多级缓存（写路径物理失效），mimalloc 全局分配器。
- Argon2 密码哈希、cookie 会话（世代号失效）、CSRF 防护、按 IP 限流、会话数上限与 LRU 淘汰。

**外观**

- Catppuccin Latte / Mocha 双主题，View Transitions 圆形展开切换动画，响应式移动端布局。

## 技术栈

- **框架**：Dioxus 0.7（fullstack + router，单代码库双目标）
- **服务端**：Axum、tokio、tokio-postgres + deadpool 连接池、moka 缓存、mimalloc
- **数据库**：PostgreSQL
- **前端**：Tailwind CSS v4 + Catppuccin 双主题；JS 子工程以 pnpm workspace 组织（Tiptap / CodeMirror / xterm.js / Mermaid / Lightbox），构建为 IIFE bundle 注入 `public/`
- **安全**：Argon2、AES-GCM-256（MCP 令牌静态加密）、governor 限流
- **沙箱**：bollard（Docker 执行层）

**生产部署必须前置反向代理**（nginx / Caddy）做 TLS 终结，并设置：

- `APP_BASE_URL`（CSRF 可信来源）
- `COOKIE_SECURE=true`
- `TRUSTED_PROXY_COUNT`（精确反代跳数，错误值会被 XFF 伪造绕过限流）

容器监听 `127.0.0.1:3000`，健康探针：`/healthz`（存活）、`/readyz`（就绪，`SELECT 1`）。更多细节见 [贡献者约定](AGENTS.md) 的生产部署一节。

## 项目结构

```
src/          Rust 源码（前端 + 服务端，feature 门控双目标）
  api/        端点：auth / posts / comments / settings / code_runner / mcp ...
  mcp/        MCP 服务器（rmcp，bearer 鉴权 + 作用域）
  db/         连接池、迁移、重试
libs/         pnpm workspace 前端 JS 库（构建产物写入 public/）
migrations/   编号 SQL 迁移（启动时自动运行）
syntaxes/     syntect 代码高亮语法定义（Sublime 格式）
themes/       Catppuccin Latte / Mocha 高亮主题
docker/       Dockerfile 与代码运行沙箱镜像
public/       静态资源（构建期生成）
```

## 文档

- [更新日志](CHANGELOG.md)（亦可在 `/changelog` 查看）
- [贡献者约定](AGENTS.md)
