# View Transitions 浏览器回归

`test-view-transitions.ts` 使用真实 Chromium 和已启动的开发站点，覆盖标题双向匹配、列表状态与滚动恢复、目录 hash、连续导航、主题竞争、减少动态效果、不支持 API、移动端，以及 300ms 等待后不重播动画。

栏目动效另在正常浏览器窗口检查：

- 首页 → 归档 → 友链 → 关于 → 搜索，正文从右侧 16px 淡入，反向切换从左侧淡入；进入用 snappy 弹簧（约 410ms）、退出 140ms。分页属于首页、标签详情属于归档，同栏目和文章导航保留原有过渡。
- 桌面当前栏目的浮起胶囊（连同文字）以 snappy 弹簧在栏目之间滑动，新旧文字交叉淡入淡出，不遮挡其余导航文字；进入搜索页时胶囊在原栏目处淡出，从搜索页回到栏目时胶囊原地淡入。页头保持实时可点击，页脚位置稳定，长短页面切换时滚动条预留空间不变；滚动后切换或后退，正文快照不应跳位或覆盖页头。
- 在 390px 宽度通过底部抽屉互切，正文位移缩为 8px，抽屉收起，不捕获隐藏的桌面胶囊。检查浅色、深色、键盘 Enter、直接刷新、前进后退和连续导航。
- 减少动态效果或缺少原生 API 时应正常切换页面，不保留方向标记或临时快照名。主题切换打断导航后，最后一次路由仍须正确，文章标题共享过渡仍须双向匹配。

方向、路由范围、历史记录与中断清理的单元回归：`cd libs && bun run --filter '@yggdrasil/core' test`。

首页“开始阅读”用 `scripts/test-home-reading.ts` 检查：首次直接进入首页、同一 hash
重复点击、键盘 Enter、SPA 返回、直接 hash 加载，以及桌面／390px 手机的明暗主题和
减少动态效果。脚本采样实际滚动帧，确认平滑滚动没有被路由恢复打断，列表停在固定
页头下方，点击不刷新文档或新增历史记录。截图默认写到 `/tmp/home-reading-*.png`，
可通过 `HOME_READING_SCREENSHOTS` 指定已有输出目录。

```bash
PLAYWRIGHT_MODULE=/path/to/playwright \
CHROMIUM_PATH=/path/to/chromium \
VT_BASE=http://localhost:8080 \
bun scripts/test-home-reading.ts
```

更新日志版本目录另用 `scripts/test-changelog-nav.ts` 检查，覆盖桌面明暗主题、680px 高度窗口、390px 小屏和减少动态效果：目录高度、独立滚动及边缘渐隐、选中底色的实际动画、锚点遮挡、阅读位置跟随、直接 hash 加载和 SPA 卸载／重挂载。截图默认写到 `/tmp/changelog-*.png`，可通过 `CHANGELOG_SCREENSHOTS` 指定已有输出目录。

```bash
PLAYWRIGHT_MODULE=/path/to/playwright \
CHROMIUM_PATH=/path/to/chromium \
CHANGELOG_BASE=http://localhost:8080 \
bun scripts/test-changelog-nav.ts
```

```bash
PLAYWRIGHT_MODULE=/path/to/playwright \
CHROMIUM_PATH=/usr/bin/chromium \
bun scripts/test-view-transitions.ts
```

如果项目外已经安装了可被 Bun 解析的 `playwright`，可以省略 `PLAYWRIGHT_MODULE`；使用 Playwright 自带 Chromium 时可省略 `CHROMIUM_PATH`。脚本不安装依赖，也不会启动或停止开发服务器。

- `VT_BASE` 默认 `http://127.0.0.1:8080`，必须与站点配置的 `APP_BASE_URL` 同源；用 `localhost` 替代 `127.0.0.1` 可能触发 CSRF 403。
- 公开页面检查需要已有已发布文章、搜索结果，以及至少两篇文章的标签。`VT_SEARCH_QUERY` 默认 `Rust`，`VT_TAG_PATH` 默认 `/tags/Rust`，可按当前数据调整。目录 hash 检查默认使用归档首篇文章；若该文章没有目录，可用 `VT_TOC_POST_PATH=/post/<slug>` 指向一篇有目录的已发布文章。
- 可传单个场景参数：`public`、`reduced`、`unsupported`、`mobile`、`share`、`slow`、`chunk`、`admin`、`prefetch`；不传参数时运行 `all`，未知场景会报错退出。`VT_DEBUG=1` 输出原生过渡的快照名称与时间。
- `share` 场景比较首页直达、首页→文章→首页的 SPA 往返、文章直达时的 description 与 Open Graph 描述、类型、地址和图片；检查每类标签恰好一个、往返不刷新文档，且没有 `pageerror`。
- `chunk` 场景把 `/pkg/**` 的 `.wasm` 请求延迟 1200ms，模拟目标页分包超出 300ms 预算：协调器应跳过空过渡（`readyError === 'AbortError'`），旧正文子元素淡出（`html.is-route-pending`，期间 `pointer-events: none`），同一位置淡入目标页的路由骨架（`main > .route-pending`），分包到达后无空白帧地接上页面骨架或正文，随后新页进场且不重播动画；槽位在新页提交后移除。`chunk-late-skeleton-in-view` 先把旧页面滚到底部再触发，断言骨架槽位贴在视口顶部附近且骨架可见。「无空白帧」断言依赖 `DelayedSkeleton` 在 `is-route-pending` 仍存在时立即显示：若在页面挂载后出现空白帧且该段 `pending` 为假，说明协调器先于组件 Effect 撤了类，应改协调器而不是加定时器。中断场景使用请求屏障阻塞更新日志视图分包，确认首页仍在显示后，分别后退或点击归档；恢复目标路由后再释放迟到分包，核对它没有覆盖当前正文、骨架槽位已移除，随后继续真实点击导航。通过条件包含 URL、DOM 路由标记、正文标题、pending 清除、正文可见、文档未刷新，以及整个场景无 `pageerror` 和控制台 error；不再豁免 WASM panic。分包冷加载时接口慢同样会挂起路由（路由器等到数据到达才换页），因此 `slow` 场景在冷分包下走的也是这条路径；标题共享过渡的断言只在目标页按时渲染时要求 `readyError` 为空。
- 可选的 `VT_ADMIN_USERNAME`、`VT_ADMIN_PASSWORD` 启用登录、后台子页面分包迟到骨架（`admin-chunk-late-skeleton`：延迟日志页分包，断言 `main > .admin-route-pending` 在顶栏下方可见且提交后移除）、后台分页／筛选／内层滚动恢复、预览返回与退出检查。凭据只从环境读取，不提交注册表单。退出接口固定为 `/api/logout`，结束时直接检查并撤销测试 Cookie 对应的会话；即使登录后的页面导航失败，也会请求退出并验证成功，清理失败会明确报错。

后台分页与预览使用浏览器请求拦截生成临时响应，预览正文替换为简短占位，以单独验证动画和网络等待。测试不创建、修改或删除文章，不把临时数据写入数据库。搜索、标签和归档的主流程使用真实站点响应。标签与归档列表已不显示封面，因此不再运行旧的列表封面成功／失败／超时场景。

800ms 的接口延迟用例要求 VT 更新回调在 450ms 内结束，为 300ms 预算保留浏览器调度余量，并验证迟到内容不触发第二次动画。该预算不包含无法被计时器打断的同步布局与样式计算；大型文档的主线程渲染耗时应单独分析。

后台已发布文章标题也在 `/admin/preview/...` 中打开，保证列表→预览→返回保持同一布局；真实已发布文章的预览往返已纳入可选后台回归。预览面包屑 Home 继续使用原整页跳转。

本次额外浏览器验证使用当前开发数据，已确认以下行为：

- 真实文章上下篇切换后，标题与正文对应正确；两篇文章内的 CodeMirror 输入和撤销、后台运行器编辑器的输入和撤销正常，未执行代码。
- 共享过渡及普通文章导航后，封面有正确尺寸，灯箱可以打开、加载图片并用 Escape 关闭。
- 真实已发布文章从后台列表进入预览并明确返回列表正常；设置页实际滚动容器在离开并后退后恢复同一历史记录及原来的 900px 滚动位置，包含容器启用 CSS 平滑滚动的情况。
- 预览面包屑 Home 触发一次整页文档请求，新文档显示并水合为首页。原生跨文档动画本身未能在本机无头 Chromium 稳定验证：新文档可见，但不产生动画帧或 `pagereveal`，浏览器跳过过渡；只有 `@view-transition` 的独立 HTML 对照也出现相同行为。跨文档动画仍需在正常窗口中人工确认，不能由整页跳转成功推断动画成功。

清理路径另用“登录成功后转到 `about:blank`”验证，直接退出请求成功、会话 Cookie 消失且当前用户接口返回空用户。上述手工场景不等同于脚本每次都会运行的自动化覆盖。
