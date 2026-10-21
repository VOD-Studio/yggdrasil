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

### 双客户端模板与 WASM preload

```sh
make test-release-bootstrap
make build
bun scripts/test-dual-flavor.cjs
make test-wasm-preload
```

`test-release-bootstrap` 使用临时 fixture，覆盖准确入口解析、reader/full 隔离、陈旧哈希剔除、preload 的 URL/CORS/位置、最终模板与发布清单一致性，以及缺失资源时不覆盖旧发布目录；也包含在 `make test` 中。

`test-dual-flavor.cjs` 使用一次性 PostgreSQL 容器与真实 release 布局，检查 reader/full/404、HTML 缓存与 ETag/304、带 hash 资源 immutable 与可变 URL 缓存边界、SPA 与跨入口导航、每次文档加载只请求一次对应 WASM、「首次阅读→进入后台」累计传输量报告、`YGG_PREVIOUS_PUBLIC_DIR` 旧发布资源保留窗口（回退字节、MIME/`.br` 协商、标记、不遮蔽当前发布），以及移除 reader 产物后的完整应用回退模式（全路径 full 模板、hydration、守卫不变）。`make test-wasm-preload` 复用同一隔离环境并追加 P5 专项 A/B；不会启动开发构建，也不会使用现有业务数据库。运行前需要 Docker、PostgreSQL 客户端及 `libs/` 中的 Playwright。

专项脚本通过两个本地代理访问同一 release 服务：对照组只移除 HTML head 的 WASM preload，实验组保留；两组采用相同的 HTML 处理，资产原始字节及 Brotli/缓存头透传。不使用会禁用浏览器缓存的 Playwright route。首页、固定文章、full 图鉴分别至少测 5 次，区分冷/热缓存与无交互的首屏指标、真实菜单交互就绪代理指标。保存原始样本、中位数、网络/CPU/浏览器与服务端缓存配置、请求及缓存证据、LCP/CLS、控制台警告和截图。报告写入 `target/regressions/`；没有充分净收益证据时不能将结果称为通过性能验收。

也可对已启动的**本地隔离 release 服务**测量（文章必须是固定 fixture，`SSR_CACHE_SECS` 应与服务端实际配置一致）：

```sh
BASE_URL=http://127.0.0.1:8080 SSR_CACHE_SECS=60 \
  POST_PATH=/post/dual-flavor-post make test-wasm-preload
```

release 装配会在 head 开头注入 `as="fetch" type="application/wasm" crossorigin="anonymous"`，WASM URL 来自对应 module 入口的实际初始化参数，不硬编码 hash。清单从最终模板生成；`build-release.cjs --verify` 会核对 preload 和当前入口、flavor 资产闭包及清单。仅带内容哈希的资产沿用 immutable，固定路径资源的缓存策略不变。

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

## 公开阅读性能与双机缓存

```sh
make test-public-performance
```

脚本创建自己的 PostgreSQL 16 容器和 `ygg_mcp_test` 数据库，启动两个本地应用实例，验证 HTML 缓存命中、ETag/304、TTL、认证与笔记绕过缓存、事务回滚、提交后的双机失效及通知断线恢复。浏览器覆盖桌面/手机、明暗主题、151 篇归档分页、文章 SPA 跳转，以及评论区在接近视口前不加载 Tiptap。截图、结果和服务器日志保存到 `target/regressions/*-public-performance/`，结束后清理本次创建的容器和应用进程。

生产匿名公开页缓存默认 60 秒、最多 32 MiB；不依赖 `/app/static`。每次内容提交通过 PostgreSQL 通知失效，应用每 5 秒核对持久化版本兜底。浏览器 HTML 使用 `no-cache` 配合 ETag 校验；带哈希的静态资源保留 immutable。`SSR_CACHE_SECS=0` 禁用 HTML 缓存。
