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

- [笔记与 AI 知识库](docs/notes.md)：随记 / 主题笔记、有序笔记本、私密图片，以及独立的工作稿、公开版、知识库收录版。前台 `/notes`，后台 `/admin/notes`。

- 内置 MCP 服务器（`POST /mcp`，Streamable HTTP，bearer token 鉴权）。
- AI 客户端（Claude Code / Cursor / Cline 等）可把已发布文章当知识库检索，并按作用域（read / write / admin）执行文章、评论、标签、媒体、设置与代码运行等后台操作。

MCP 私有文章查询需要 `write` 或 `admin` 令牌，且只访问令牌用户自己的文章：
`list_posts({"status":"draft","page":1,"per_page":20})` 查询草稿，支持 `query` 标题搜索；
`get_post_by_id({"post_id":123})` 读取完整编辑内容及 `updated_at`。
`list_trashed_posts({"page":1})` 查询回收站，`restore_post({"post_id":123})` 恢复文章；
恢复保留原发布状态，slug 被占用时自动追加后缀，返回最终 slug。
更新时建议携带读取结果中的 `expected_updated_at`，例如
`update_post({"post_id":123,"title":"新标题","expected_updated_at":"2026-09-16T00:00:00.123456Z"})`。
版本不匹配返回 `conflict` 及当前版本，不写入更改；成功返回新的 `updated_at`。
不传版本保留原有更新行为。
原有 `search_posts` / `get_post` 仍只返回公开已发布文章。

MCP 素材工具（管理操作均需要 `admin` 令牌）：

| 工具 | 用途 |
| --- | --- |
| `list_assets` | 分页搜索素材及引用明细；默认每页 60 张，页码从 1 开始 |
| `update_asset_alt` | 修改或清除 alt，不回写已有文章 |
| `delete_asset` | 永久删除单张无引用素材 |
| `batch_delete_assets` | 批量删除，每次 1–100 个 id，被引用的跳过 |
| `purge_orphan_assets` | 清理无引用且上传超过 7 天的素材 |
| `rebuild_assets_index` | 扫描磁盘重建素材及文章引用索引 |

例如 `list_assets({"filter":"Orphan","query":"截图","sort":"CreatedDesc","page":1})`；
`filter` 可选 `All` / `Used` / `Orphan`，`sort` 可选 `CreatedDesc` / `SizeDesc`。
返回的 `path` 加上 `/uploads/` 前缀即可作为图片地址；`refs` 包含文章、评论和头像引用。
删除与后台共用引用保护，草稿及回收站文章引用同样受保护；删除为永久操作。

上传需要 `write` 或 `admin` 令牌，支持 JPEG/PNG/GIF/WebP，最大 5 MiB：

- 远程图片：`upload_media({"url":"https://example.com/image.png","alt":"图片说明"})`，服务端抓取并入库。
- 本地图片：通过 HTTP multipart 接口上传，二进制不经过 MCP JSON-RPC：

```sh
curl "$APP_BASE_URL/api/mcp/upload" \
  -H "Authorization: Bearer $YGG_MCP_TOKEN" \
  -F 'file=@/path/to/image.png;type=image/png' \
  -F 'alt=图片说明'
```

两条通道都返回 `asset_id`、`alt`、`url`、尺寸、最终 MIME 和 `reused`。
`alt` 会保存到素材库；重复上传返回相同素材 ID，不传 alt 保留旧值，传空白清除。
这些修改不回写已有文章，Markdown 的图片替代文本仍需单独填写。

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

## 数据库回归测试

MCP 数据库回归测试需使用名为 `ygg_mcp_test` 的一次性 PostgreSQL 数据库：

```sh
DATABASE_URL=postgresql://postgres@127.0.0.1:55439/ygg_mcp_test \
YGGDRASIL_TEST_DATABASE_URL=postgresql://postgres@127.0.0.1:55439/ygg_mcp_test \
  cargo test --locked --features server database -- --ignored
```

测试会自动迁移并重置该测试库的用户和文章夹具，同时验证上传 HTTP 接口。

## 文档

- [更新日志](CHANGELOG.md)（亦可在 `/changelog` 查看）
- [贡献者约定](AGENTS.md)
