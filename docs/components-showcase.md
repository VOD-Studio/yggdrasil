# 组件图鉴

入口在 `/about` 页尾；目录为 `/about/components`，详情为 `/about/components/:component`。目录和详情调用项目里的真实组件或与正式页面共用的展示实现，素材、评论、运行器等业务演示使用固定资料和当前预览内的状态。

## 数据与预览

`src/pages/components_showcase_data.json` 为每个条目记录稳定 `slug`、分类、说明、状态、属性、源码和 `code` 用法。两个字段分别控制详情行为与代码区标题：

| 字段 | 取值 | 含义 |
| --- | --- | --- |
| `preview_mode` | `interactive`、`static` | 是否提供可操作的详情预览和“恢复默认”。静态条目只展示组件，不提示无效交互。 |
| `usage_kind` | `rust`、`api`、`router`、`browser` | `code` 分别是 Dioxus 调用示例、Rust 接口说明、路由配置、浏览器调用示例；不决定预览能否交互。 |

这两个字段都必填；原来的 `kind: reference` 不再控制预览或文案。

`src/pages/components_showcase.rs` 按 `slug` 分派预览。简单组件、骨架和固定场景在主文件中；业务、布局与文章、评论、浏览器实例分别在 `src/pages/components_showcase/` 下。目录有条目而没有分派会使检查失败，不会退回通用示意卡。样式在 `input.css` 的 `showcase-*` 区块。

## 增加组件

1. 在 JSON 中添加完整条目，填写 `preview_mode` 与独立的 `usage_kind`。`code` 应展示实际调用；需要上下文的组件要说明上下文由谁提供。
2. 在 `preview_route` 为新 `slug` 增加分派，并在对应预览模块渲染真实组件。需要权限、请求或持久化时，让正式入口保留业务控制，让图鉴传入固定数据及本地回调；两处共用展示代码。内部展示函数使用满足调用关系的最小可见性。
3. 给目录卡片提供可辨认的初始结构；详情补齐代表状态和必要操作。使用确定的 DOM ID 与仓库内的静态素材，限定 DOM 查询和浮层作用域；目录中的重型浏览器实例进入可视范围后再挂载，并随预览卸载清理。
4. 更新属性、说明与源码路径，运行下方检查。`catalog_covers_public_components_and_has_unique_routes` 会检查公开组件遗漏、重复路由和缺少预览分派。

## 业务边界与验证

图鉴的素材选择、上传状态、评论提交与回复、运行器输出都属于本地演示，不会写入正式业务。正式文章评论区继续负责取数、认证探测、提交、待审核轮询和本地存储；图鉴评论区只使用局部上下文。编辑器、终端和灯箱使用真实浏览器实例，演示输出不代表执行了用户代码。“恢复默认”只重建当前预览，不清除目录筛选或全站主题。

优先使用本地工具。修改 `libs/` 时重建 bundle；修改 `input.css` 或新增 Tailwind 类时重建 CSS，并检查生成资源差异。

```bash
make check-dev-tools
make build-libs  # 修改 libs/ 后
make css         # 修改 input.css 或新增 Tailwind 类后
cargo fmt        # 修改 Rust 后
make test
make lint
cargo check --locked --target wasm32-unknown-unknown --no-default-features --features web
```

浏览器脚本连接已经运行的本地站点，不会替你启动或停止服务；将地址改为实际监听地址：

```bash
SHOWCASE_BASE=http://localhost:8080 node scripts/test-components-showcase.cjs
```

`SHOWCASE_BASE` 应与本地 `APP_BASE_URL` 同源。需要指定本机 Playwright 或 Chromium 时设置 `PLAYWRIGHT_MODULE`、`CHROMIUM_PATH`；用 `SHOWCASE_ARTIFACT_DIR` 指定截图与报告目录，默认写入系统临时目录。

浏览器验收还要核对目录与详情的直接访问、返回后筛选和滚动恢复、手机宽度、明暗主题、减少动态效果、重复进入后的实例清理，以及演示期间是否出现新增的认证、上传、评论或执行请求。修改导航或目录时按 `scripts/README-view-transitions.md` 回归正式页面。
