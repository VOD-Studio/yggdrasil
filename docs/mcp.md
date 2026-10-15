# MCP 服务器

内置 MCP 服务器位于 `POST /mcp`（Streamable HTTP，bearer token 鉴权）。AI 客户端（Claude Code / Cursor / Cline 等）可把已发布文章当知识库检索，并按令牌作用域（`read` / `write` / `admin`）执行文章、评论、标签、媒体、设置与代码运行等后台操作。

## 文章

`search_posts` / `get_post` 只返回公开已发布文章。私有文章查询需要 `write` 或 `admin` 令牌，且只访问令牌用户自己的文章：

- `list_posts({"status":"draft","page":1,"per_page":20})`：查询草稿，支持 `query` 标题搜索。
- `get_post_by_id({"post_id":123})`：读取完整编辑内容及 `updated_at`。
- `list_trashed_posts({"page":1})` / `restore_post({"post_id":123})`：查询回收站、恢复文章。恢复保留原发布状态，slug 被占用时自动追加后缀，返回最终 slug。
- `update_post`：建议携带读取结果中的 `expected_updated_at`，例如 `update_post({"post_id":123,"title":"新标题","expected_updated_at":"2026-09-16T00:00:00.123456Z"})`。版本不匹配返回 `conflict` 及当前版本，不写入更改；成功返回新的 `updated_at`。不传版本保留原有更新行为。

## 素材

管理操作均需要 `admin` 令牌。

| 工具 | 用途 |
| --- | --- |
| `list_assets` | 分页搜索素材及引用明细；默认每页 60 张，页码从 1 开始 |
| `update_asset_alt` | 修改或清除 alt，不回写已有文章 |
| `delete_asset` | 永久删除单张无引用素材 |
| `batch_delete_assets` | 批量删除，每次 1–100 个 id，被引用的跳过 |
| `purge_orphan_assets` | 清理无引用且上传超过 7 天的素材 |
| `rebuild_assets_index` | 扫描磁盘重建素材及文章引用索引 |

例如 `list_assets({"filter":"Orphan","query":"截图","sort":"CreatedDesc","page":1})`：`filter` 可选 `All` / `Used` / `Orphan`，`sort` 可选 `CreatedDesc` / `SizeDesc`。返回的 `path` 加上 `/uploads/` 前缀即可作为图片地址；`refs` 包含文章、评论和头像引用。删除与后台共用引用保护，草稿及回收站文章引用同样受保护；删除为永久操作。

## 上传

需要 `write` 或 `admin` 令牌，支持 JPEG/PNG/GIF/WebP，最大 5 MiB。

- 远程图片：`upload_media({"url":"https://example.com/image.png","alt":"图片说明"})`，服务端抓取并入库。
- 本地图片：通过 HTTP multipart 接口上传，二进制不经过 MCP JSON-RPC：

```sh
curl "$APP_BASE_URL/api/mcp/upload" \
  -H "Authorization: Bearer $YGG_MCP_TOKEN" \
  -F 'file=@/path/to/image.png;type=image/png' \
  -F 'alt=图片说明'
```

两条通道都返回 `asset_id`、`alt`、`url`、尺寸、最终 MIME 和 `reused`。`alt` 会保存到素材库；重复上传返回相同素材 ID，不传 alt 保留旧值，传空白清除。这些修改不回写已有文章，Markdown 的图片替代文本仍需单独填写。
