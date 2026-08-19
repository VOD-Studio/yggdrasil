-- 用户资料字段：显示名称与头像（/admin/profile 个人信息页）。
-- display_name 为对外展示名，未设置时 UI 回退展示 username；
-- username 仍是唯一登录凭据，不可修改。
ALTER TABLE users ADD COLUMN IF NOT EXISTS display_name VARCHAR(50);
ALTER TABLE users ADD COLUMN IF NOT EXISTS avatar_url VARCHAR(512);

COMMENT ON COLUMN users.display_name IS '对外展示名称；为空时回退 username';
COMMENT ON COLUMN users.avatar_url IS '头像 URL（/uploads/ 素材路径或 http(s) 外链）';
