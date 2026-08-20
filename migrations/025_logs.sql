-- 运行日志查看器：进程内 tracing Layer 捕获 info+ 日志，经 mpsc 批量落库，
-- 后台任务按保留期（logs_retention_days）与行数上限（logs_max_rows）裁剪。
-- ts 由 writer 显式传入事件捕获时刻；DEFAULT now() 仅兜底手动直插场景。
CREATE TABLE IF NOT EXISTS logs (
    id BIGSERIAL PRIMARY KEY,
    ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    level TEXT NOT NULL,
    target TEXT NOT NULL,
    message TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_logs_level ON logs(level);
CREATE INDEX IF NOT EXISTS idx_logs_target ON logs(target);

COMMENT ON TABLE logs IS '运行日志查看器的落库日志（tracing capture → 批量 INSERT → 保留期裁剪）';
