# 备份与恢复回归

`src/api/database/backup.rs` 保留原有 server function 入口、DTO 和 Axum handler 导出路径。服务端实现按职责放在 `backup/` 下：

- `creation.rs`：手动/自动备份执行、pg_dump 与数据抢救回退、uploads 打包。
- `files.rs`：安全文件名、签名和头部解析、SQL/归档配对、元信息与轮转。
- `commands.rs`：通过 PG 环境变量构造 pg_dump/psql，避免把连接 URI 放在参数中。
- `restore.rs`：COPY 感知的旧属主语句过滤、临时 SQL 生命周期、事务恢复及缓存失效。
- `transfer.rs`：管理员鉴权、流式导入、大小限制与备份下载。

普通回归不需要数据库：

```sh
cargo test --locked api::database::backup
```

实际 PostgreSQL 回归会 DROP/CREATE 测试表并重放整个数据库的备份，因此仅允许一次性数据库 `ygg_backup_test`。它使用独立的 `YGG_BACKUP_TEST_DATABASE_URL`，不会复用应用数据库连接池。先创建该数据库并确保本机 `pg_dump`/`psql` 与服务器版本兼容，再执行：

```sh
YGG_BACKUP_TEST_DATABASE_URL=postgresql://postgres@127.0.0.1:55439/ygg_backup_test \
  cargo test --locked \
  api::database::backup::restore::tests::backup_round_trip_and_failed_restore_rolls_back_database \
  -- --ignored --exact --test-threads=1
```

该回归实际执行 pg_dump 和 psql，检查签名、旧备份属主语句兼容、COPY 内的 SQL/元命令文本与多行中文保持原样、恢复结果、错误后的完整事务回滚，以及成功/失败后的临时 SQL 清理。普通文件回归还会检查自动轮转成对删除最旧备份，并保留手动备份和未完成导入文件。

恢复仍只覆盖数据库；uploads 归档需手动还原。`sql-fallback` 是仅数据的抢救产物，不是可以经 psql 恢复的脚本。后台恢复成功后的缓存失效，以及真实管理员 HTTP 导入/下载流程，需要另行进行应用验收。
