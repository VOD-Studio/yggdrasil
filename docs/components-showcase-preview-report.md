# 组件图鉴真实预览实施记录

日期：2026-09-23。基线为 `d9fc761`；实施分支为 `feat/components-showcase`。本记录对应本地开发服务 `http://localhost:8080`，不代表生产部署验收。

## 交付范围

19 个目标条目都从通用示意卡切换为真实组件或正式页面共用的展示实现：AdminLayout、AssetPickerModal、AssetUploadModal、CodeRunner、CommentForm、CommentItem、CommentList、PendingCommentItem、CommentSection、Footer、FrontendLayout、Header、PostContent、PostFooter、PostToc、TiptapEditor、CodeMirrorEditor、XtermTerminal、Lightbox。目录每个条目都有显式 `preview_mode`、独立的 `usage_kind` 和稳定 slug 分派；缺少渲染分派会使检查失败。

共用展示提取在素材选择网格与选择栏、上传面板与状态行、后台侧栏与布局壳、前台头部/页脚/布局、评论表单与区段、文章正文/页脚/目录以及运行器的编辑器与输出面板。图鉴传入仓库图片、固定文本和局部操作；正式入口保留认证、请求、队列、SSE、路由和持久化。Tiptap、CodeMirror、Xterm 与 Lightbox 挂载真实浏览器实例，按预览作用域清理。

## 检查结果

| 检查 | 结果 |
| --- | --- |
| `make check-dev-tools`、`make build-libs`、`make css` | 通过；JS/CSS 生成资源已重建 |
| `make test` | 通过：Rust 859 项、集成 2 项、工具 1 项；6 项原有测试忽略。前端各包共 487 项通过 |
| `make lint` | 通过：Biome、TypeScript、Clippy、rustfmt |
| `cargo check --locked --target wasm32-unknown-unknown --no-default-features --features web` | 通过 |
| `scripts/test-components-showcase.cjs` | 26 项通过、48 张截图；83 张卡片结构与模式、19 个目标详情桌面浅色与 390px 深色/减少动态效果、交互、重置、重试、重复进出 5 次均通过。原始报告：`/tmp/yggdrasil-showcase-20260923/report.json` |
| 弹窗重置与重试竞态补测 | 素材选择/上传弹窗内重置 4 项通过；上传重试后 200ms 重置、待旧任务结束仍保持失败样例 4 项通过。报告分别在 `/tmp/yggdrasil-showcase-modal-reset-20260923/` 与 `/tmp/yggdrasil-showcase-modal-race-20260923/` |
| `scripts/test-view-transitions.cjs all` | 匿名导航 19 项通过。使用 `VT_TOC_POST_PATH=/post/violet-architecture-and-engineering-philosophy` 指向本地有目录的文章；后台用例因无凭据跳过 |
| 正式文章与图鉴双目录补测 | 正式文章的 17 个目录锚点可更新 hash、滚动窗口并标记当前章节；SPA 导航到图鉴后，图鉴目录只滚动演示容器且不修改页面 hash |

浏览器验收同时记录了 `/about` 基线和图鉴访问。两者均未出现 `/api/` 请求；图鉴无新增请求、拦截的业务请求或页面错误，评论相关 localStorage 项前后一致。脚本监测认证（`get_current_user`、`logout`）、素材（`list_assets`、`get_upload_settings`、`/api/upload`）、评论（`get_comments`、`create_comment`、`check_pending_status`）和运行器（`start_exec`、`start_exec_stream`、`get_exec_result`、`/api/exec/`）等路径，并将新增读取接口与 `/about` 基线比较。业务请求若被拦截也会令验收失败。

## 截图证据

首批四张卡片，实施前取自隔离的 `d9fc761` 工作树，实施后为当前卡片元素截图：

| 组件 | 实施前 | 实施后 |
| --- | --- | --- |
| AssetPickerModal | [前](components-showcase-evidence/before-asset-picker-modal.png) | [后](components-showcase-evidence/after-asset-picker-modal.png) |
| AssetUploadModal | [前](components-showcase-evidence/before-asset-upload-modal.png) | [后](components-showcase-evidence/after-asset-upload-modal.png) |
| AdminLayout | [前](components-showcase-evidence/before-admin-layout.png) | [后](components-showcase-evidence/after-admin-layout.png) |
| CodeRunner | [前](components-showcase-evidence/before-code-runner.png) | [后](components-showcase-evidence/after-code-runner.png) |

代表性详情（桌面浅色 / 手机深色）：[后台布局](components-showcase-evidence/admin-layout-desktop-light.png)、[后台布局手机](components-showcase-evidence/admin-layout-mobile-dark.png)；[评论区](components-showcase-evidence/comment-section-desktop-light.png)、[评论区手机](components-showcase-evidence/comment-section-mobile-dark.png)；[文章目录](components-showcase-evidence/post-toc-desktop-light.png)、[文章目录手机](components-showcase-evidence/post-toc-mobile-dark.png)；[编辑器](components-showcase-evidence/tiptap-editor-desktop-light.png)、[编辑器手机](components-showcase-evidence/tiptap-editor-mobile-dark.png)、[Markdown 源码模式](components-showcase-evidence/tiptap-markdown-source-desktop-light.png)；[灯箱图集](components-showcase-evidence/lightbox-desktop-light.png)、[灯箱手机图集](components-showcase-evidence/lightbox-mobile-dark.png)、[打开的桌面灯箱](components-showcase-evidence/lightbox-overlay-desktop-light.png)、[打开的手机灯箱](components-showcase-evidence/lightbox-overlay-mobile-dark.png)。其余详情截图保留在原始报告的截图目录。

## 尚未完成的外部验证

没有可安全使用的认证业务测试资料，因此未执行正式后台里的真实素材上传、真实评论提交或真实代码执行。当前本地服务还提示运行器镜像未构建。上述生产路径完成了调用方代码复核、相关单元检查及匿名导航回归；图鉴演示通过不能代替这些路径的真实业务验收。
