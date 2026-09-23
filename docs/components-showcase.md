# 组件图鉴

公开入口在 `/about` 页尾；目录为 `/about/components`，用法页为 `/about/components/:component`。

目录数据位于 `src/pages/components_showcase_data.json`。每个条目有稳定的 `slug`、分类、用途、属性、用法片段和源码位置。新增 `src/components/` 中的公开组件时，需要补上条目；`catalog_covers_public_components_and_has_unique_routes` 测试会检查遗漏和重复路径。浏览器侧的 Tiptap、CodeMirror、Xterm、Lightbox 封装也在目录中。

展示方式按组件依赖划分：

- 24 个通用组件调用真实 Dioxus 实现，可在图鉴或详情页操作。
- 页面骨架直接调用原组件，以缩略窗口呈现。
- 能安全构造静态资料的文章、评论和 SQL 组件使用固定本地样例；样例链接区域带 `inert`，不会跳转到不存在的文章。
- 素材上传、后台布局、编辑器等需要权限、数据或浏览器宿主的组件展示场景、接口和源码位置，不从公开图鉴发起业务请求。

实现位于 `src/pages/components_showcase.rs`，样式在 `input.css` 的 `showcase-*` 区块。目录和详情页共用前台 Header 与主题切换；“关于”导航在这两个路由下保持选中。设计原型保存在本地分支 `design/components-showcase-prototype`，正式页面已经按选定的图鉴方案实现。

修改后运行 `make css`、`make lint`、`make test`，再检查直接访问、站内导航、明暗主题、手机宽度和减少动态效果。
