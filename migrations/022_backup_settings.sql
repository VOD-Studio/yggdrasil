-- 自动备份配置键（回收站配置同款 key/value 模式）。
-- 默认关闭自动备份；每天 04:00 UTC；保留最近 30 份；产物含 uploads 打包。
-- 上次执行结果键（backup_last_run_*）不预置，由调度任务首次执行后写入。
-- ON CONFLICT 保证重复执行迁移安全。
INSERT INTO settings (key, value) VALUES
    ('backup_auto_enabled', 'false'),
    ('backup_time_utc', '04:00'),
    ('backup_retention_count', '30'),
    ('backup_include_uploads', 'true')
ON CONFLICT (key) DO NOTHING;
