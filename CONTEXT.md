# Yggdrasil

全栈博客/CMS（Dioxus 0.7 单代码库双目标：WASM 前端 + Axum 服务端）。本文件只收项目专属领域词条。

## Language

### 备份与恢复

**备份 (Backup)**:
`backups/` 目录下由本系统生成的 `.sql` 文件，首行带签名 `-- YGGDRASIL BACKUP v1`。前缀 `backup_` 为手动，`auto_` 为定时任务产物（仅 auto_ 参与保留份数轮转）。
_Avoid_: dump、快照

**配对素材包 (Paired uploads archive)**:
与备份同名、后缀 `_uploads.tar.gz` 的 uploads/ 打包（排除可重建的 `.cache/`）。只随备份展示/下载/删除，恢复永远手动。
_Avoid_: 附件包、媒体备份

**恢复 (Restore)**:
仅接受本系统生成的备份（首行签名校验）+ 二次确认，经 psql 重放 `.sql` 重建数据库；uploads 不在恢复范围内。
_Avoid_: 导入（见下）、回滚

**导入 (Import)**:
把一份**本系统生成**的备份 `.sql` 从本机上传进 `backups/`，使其出现在备份列表中。导入只入库，不触发恢复；恢复仍是列表上的独立动作。
_Avoid_: 上传备份（口语可用，但代码与文档统一用 import）
