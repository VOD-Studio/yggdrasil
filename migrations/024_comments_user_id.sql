-- 评论关联登录用户：登录用户发表的评论记录 user_id，读取时 JOIN users
-- 实时解析显示名与头像；匿名评论 user_id 为 NULL（原行为不变）。
-- ON DELETE SET NULL：用户被删除时评论保留并退化为匿名展示。
ALTER TABLE comments ADD COLUMN IF NOT EXISTS user_id INT REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_comments_user ON comments(user_id) WHERE user_id IS NOT NULL;

COMMENT ON COLUMN comments.user_id IS '登录用户评论的作者用户 id；NULL 表示匿名评论';
