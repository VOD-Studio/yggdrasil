# View Transitions 浏览器回归

`test-view-transitions.cjs` 使用真实 Chromium 和已启动的开发站点，覆盖标题双向匹配、列表状态与滚动恢复、目录 hash、连续导航、主题竞争、减少动态效果、不支持 API、移动端、封面成功／失败／超时，以及 300ms 等待后不重播动画。

```bash
PLAYWRIGHT_MODULE=/path/to/playwright \
CHROMIUM_PATH=/usr/bin/chromium \
node scripts/test-view-transitions.cjs
```

如果项目外已经安装了可被 Node 解析的 `playwright`，可以省略 `PLAYWRIGHT_MODULE`；使用 Playwright 自带 Chromium 时可省略 `CHROMIUM_PATH`。脚本不安装依赖，也不会启动或停止开发服务器。

- `VT_BASE` 默认 `http://127.0.0.1:8080`，必须与站点配置的 `APP_BASE_URL` 同源；用 `localhost` 替代 `127.0.0.1` 可能触发 CSRF 403。
- 公开页面检查需要已有已发布文章、搜索结果，以及至少两篇文章的标签。`VT_SEARCH_QUERY` 默认 `Rust`，`VT_TAG_PATH` 默认 `/tags/Rust`，可按当前数据调整。
- 可传单个场景参数：`public`、`reduced`、`unsupported`、`mobile`、`cover`、`slow`、`admin`。`VT_DEBUG=1` 输出原生过渡的快照名称与时间。
- 可选的 `VT_ADMIN_USERNAME`、`VT_ADMIN_PASSWORD` 启用登录、后台分页／筛选／内层滚动恢复、预览返回与退出检查。凭据只从环境读取；测试创建的登录会话会退出，不提交注册表单。

封面和后台分页用浏览器请求拦截生成临时响应；封面使用仓库图片，正文替换为简短占位，以单独验证动画和网络等待。测试不创建、修改或删除文章，不把临时数据写入数据库。搜索、标签和归档的主流程使用真实站点响应。

800ms 的接口／图片延迟用例要求 VT 更新回调在 450ms 内结束，为 300ms 预算保留浏览器调度余量，并验证迟到内容不触发第二次动画。同步主线程工作无法被该计时器打断：当前开发数据中的大型文章曾测得约 6.8s 的布局／样式长任务；禁用导航协调器、使用原 Dioxus history 的对照约 7.1s。这属于现有大型文档渲染成本，不能将其误判为等待网络或图片超过预算。

后台已发布文章标题也在 `/admin/preview/...` 中打开，保证列表→预览→返回保持同一布局。原 Dioxus 0.7.10 路径“后台→公开详情→首页”在关闭 VT 的对照中也会触发 `listening` 节点回收错误；真实已发布文章的预览往返已纳入可选后台回归。预览面包屑 Home 继续使用原整页跳转。
