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
make test-browser BROWSER_SUITE=lightbox
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
