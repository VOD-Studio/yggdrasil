# 测试与回归

`make test` 运行 Rust 与前端单元测试；`make lint` 运行 Biome、TypeScript、Clippy 与 rustfmt 检查。CI 在每次 push 和 PR 上严格执行 Rust 格式、原生/WASM Clippy、WASM 编译、Rust 测试及前端 Biome、TypeScript、Vitest。文章数据库回归使用工作流创建的一次性 PostgreSQL 服务，覆盖后台与 MCP 两个入口。

## 开发资源缓存回归

`make dev` 只重建变化的前端库、高亮 CSS 与 KaTeX 资源，成功构建记录在 `target/dev-assets/`。缓存失效与失败重试回归：

```sh
bun scripts/test-dev-assets.cjs
```

测试使用临时目录和模拟编译器，不需要数据库或下载依赖。

## WASM 发布构建回归

升级 Rust／LLVM 或调整 release profile 后，检查最小 WASM 导出在剥离阶段仍保留 wasm-bindgen 元数据。需要已安装 WASM target、`llvm-tools` 及锁文件中的依赖：

```sh
make wasm-bindgen-cache
bun scripts/test-wasm-bindgen-metadata.cjs
```

脚本使用临时项目，模拟 Dioxus 按 `wasm-release` 配置执行 LLVM 后处理，再调用与 `Cargo.lock` 一致的 wasm-bindgen；结束时清理临时项目。

## 数据库回归测试

常规 `make test` 会跳过标记为 `#[ignore]` 的数据库回归。下列文章和上传回归仅允许使用已创建、名为 `ygg_mcp_test` 的一次性 PostgreSQL 数据库；按本机连接信息调整示例 URL：

```sh
# 文章私有工作流
DATABASE_URL=postgresql://postgres@127.0.0.1:55439/ygg_mcp_test \
  cargo test --locked --features server private_post_workflow_database -- --ignored

# 浏览器与 MCP 共同写入规则：校验、PATCH、素材引用、发布状态、事务回滚与权限
DATABASE_URL=postgresql://postgres@127.0.0.1:55439/ygg_mcp_test \
  cargo test --locked --all-features post_write_service_database -- --ignored

# 上传元信息及 HTTP 接口
DATABASE_URL=postgresql://postgres@127.0.0.1:55439/ygg_mcp_test \
  cargo test --locked --features server upload_metadata_database_and_http -- --ignored

# 搜索面板文章建议：标题 > 摘要 > 标签排序、草稿排除、通配符转义
DATABASE_URL=postgresql://postgres@127.0.0.1:55439/ygg_mcp_test \
  cargo test --locked --features server suggestions_rank_and_filter_database -- --ignored
```

上述测试均会自动迁移；文章回归会重置用户及关联内容，上传回归会验证真实 HTTP 上传接口。

笔记数据库回归使用独立的 `ygg_notes_test`，命令见[笔记回归说明](notes.md#回归与待补验收)。备份恢复的独立数据库验收见[备份回归说明](backup-regressions.md)。

## 后台浏览器回归

使用已有 Playwright 安装及已启动的开发站点，会保存指定的测试文章。先运行 `post_write_service_database` 在一次性数据库中创建会话和文章，再以该数据库启动 `make dev`。在该开发站点的素材库上传测试图片，并给测试文章设置有效封面；从文章列表获取其 ID 后执行：

```sh
ADMIN_BASE=http://127.0.0.1:8080 ADMIN_POST_ID=1 \
ADMIN_SESSION=post-write-regression bun scripts/test-admin-editor.cjs
```

脚本验证编辑回填、保存失败重试、素材选择、剪贴板、日志 SSE 及组件卸载，检查桌面明暗主题、窄屏和减少动态效果，并输出截图与 JSON 报告。素材预览需提供对应的测试图片文件；`PLAYWRIGHT_MODULE`、`CHROMIUM_PATH` 可指定已有浏览器工具的位置。灯箱动画和触摸手势另用 `bun scripts/test-lightbox.cjs` 验证。
