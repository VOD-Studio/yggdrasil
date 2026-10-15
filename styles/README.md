# 样式源文件

`input.css` 是唯一 Tailwind 入口，导入顺序就是原有层叠顺序。

- `theme.css`：Tailwind 语义 token 和 dark variant。
- `base.css`：原有 base 层规则，包括主题变量覆盖与 MCP 接入卡片。
- `components/`：保留 `@layer components` 的分页、文章卡片、目录、正文、图表、图片、更新日志和控件。
- `pages/`：原有未分层规则，按页面及导航/阅读交互分组。

迁移只调整文件归属。不要为了排序文件名而重排入口，不要随意把未分层规则放进 components 层。
`@source` 留在根入口，避免相对路径变成 `styles/src`。新增规则放进所属模块；跨页面基础规则需检查层级和覆盖顺序。

`make css`、`make css-watch`、`make dev-assets` 和 `make build-assets` 都使用同一入口。
Docker 的完整源码 COPY 包含此目录，Compose watch 对它采用与 `input.css` 相同的 rebuild 策略。
`public/style.css` 仍为忽略的生成产物，不提交。

本次拆分的压缩 CSS 与拆分前逐字节相同；后续功能改动不要求维持该历史字节值。
