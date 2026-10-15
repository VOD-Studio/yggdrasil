# 分阶段维护回归

本轮按回归入口、笔记编辑、备份管理、代码运行器、斜杠菜单、样式模块六阶段进行。
每阶段先完成对应验证再提交；远程 CI 的执行结果需在后续推送后单独确认。

## 一次性环境

宿主需要项目开发工具、Docker、与 PostgreSQL 18 兼容的 `psql` / `pg_dump` / `createdb` / `pg_isready`。
浏览器依赖固定在 `libs/package.json` 与锁文件中：

```sh
cd libs
BUN_CONFIG_REGISTRY=https://registry.npmjs.org/ bun install --frozen-lockfile
bun run playwright install chromium
cd ..
make test-databases
make regression-build
make test-browser
make test-browser BROWSER_SUITE=admin
make test-browser BROWSER_SUITE=notes
make test-browser BROWSER_SUITE=backup
make test-browser BROWSER_SUITE=runner
make test-browser BROWSER_SUITE=slash
make test-browser BROWSER_SUITE=lightbox
make test-browser BROWSER_SUITE=showcase
```

`test-databases` 创建独立 PostgreSQL 容器并运行文章、笔记、备份的真实数据库回归。
`test-browser` 创建独立容器、`ygg_browser_test`、管理员会话、文章和图片夹具，使用临时工作目录启动已构建应用。
它不读取开发数据库配置；应用上传和备份文件也只写入临时目录。
每次退出（包括失败、SIGINT、SIGTERM）关闭测试进程、移除本次容器和临时目录；截图与日志保留在
`target/regressions/<时间>-<套件>/`。可通过 `REGRESSION_ARTIFACT_DIR` 指定证据目录。
已有验收脚本仍支持连接手工准备的站点；不要对真实内容运行写入型验收。

`make regression-build` 在源代码变化后重新生成实际使用的前后端；仅运行 `test-browser` 不会隐式重建。
CI 执行同一批脚本，上传失败或成功的证据。快照和故障注入仅验证对应界面行为；数据库和容器执行结果分别报告。

## 本地验证记录

| 阶段 | 范围 | 验证 |
| --- | --- | --- |
| 0 | 隔离回归、固定浏览器依赖、CI 入口 | 5 个数据库回归；10 个页面/视口基线；后台 12 场景；灯箱 4 组场景；前端 lint、YAML 和 diff 检查通过 |
| 1 | 笔记保存、历史恢复、编辑器、元信息与笔记本模块 | 3 个相关单元测试；9 个真实浏览器场景；原生编译/Clippy、WASM Clippy、格式和 diff 检查通过 |
| 2 | 备份任务、导入、列表行与自动设置 | 原生编译/Clippy、WASM Clippy；7 个浏览器场景（真实 pg_dump、下载、恢复、导入、删除及故障/卸载）；截图、格式和 diff 检查通过 |
| 3 | 运行状态、编辑器/终端 hook 与 SSE 资源所有权 | 状态单测、原生/WASM Clippy；6 个真实 Python 容器浏览器场景；完整组件图鉴验收（无新增业务请求）；格式和 diff 检查通过 |
| 4 | 斜杠菜单、运行弹窗及其生命周期 | 508 个前端测试、Biome、类型检查、IIFE 构建；5 个真实编辑器浏览器场景；源码 diff 检查通过 |
| 5 | 有序样式模块、Compose watch 与资源构建 | 压缩 CSS 拆分前后逐字节一致；实际 css-watch 导入文件检测、dev-assets 回归、发布资源构建通过；22 个页面/视口及后台/图鉴浏览器复验通过 |

## 最终验收

- Rust 全量：877 个单元测试、2 个集成测试通过；默认跳过 8 个环境依赖测试。
  其中本次相关的 5 个文章/笔记/备份数据库回归已在独立 PostgreSQL 中实际运行并通过。
- 前端：508 个测试、Biome、全部包的 TypeScript 检查通过；原生和 WASM Clippy、rustfmt 通过。
- 浏览器：22 个页面/视口，后台编辑器 12 场景，笔记 9 场景，备份 8 场景，运行器 6 场景，斜杠菜单 5 场景；完整组件图鉴与灯箱 4 组回归通过。
- 样式迁移的压缩产物 SHA-256：`80acc88e61917536d4cbba86936b6478e3885fda98bc60dfaa32329e29ad852e`（迁移前后相同）。
- 最终检查修正了备份卡片源码回归的 include 路径，并补验刷新成功后清除旧错误提示。
- 临时站点、数据库容器与上传/备份夹具按运行清理；截图和报告在 `target/regressions/`。
- CI 与 Compose 配置已作本地语法检查。没有推送、触发远程 CI 或执行部署；发布资源构建不等同于完整发布打包。
