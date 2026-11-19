# 样式源文件

`input.css` 是全站入口，`input-admin.css` 只为后台和组件图鉴生成附加样式。Bun 合并导入文件，入口中的导入顺序即层叠顺序。

- `theme.css` 和 `motion.css` 提供站点与 rustdoc 共用的主题和动效变量。
- `reset.css` 和 `base.css` 提供站点元素默认值及全局规则。
- `components/` 收录跨页面组件样式，`pages/` 收录页面和路由交互样式。
- `utilities.css` 提供读屏标签和等宽数字两个通用类。

`make css` 生成开发产物，`make css-watch` 持续监听两个入口及其导入文件，`make build-assets` 生成压缩产物。两份输出都在 `public/`，属于忽略的生成文件。
