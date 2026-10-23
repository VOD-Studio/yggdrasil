-- sessions 记录签发时的世代号快照。
-- 此前只有 users.session_generation 一个值，缓存未命中路径无从比较，
-- 改密 / ADMIN_PASSWORD 轮换 / 角色变更后，旧 cookie 在缓存逐出后会重新通过校验。
-- 现在两条校验路径都要求 sessions.generation = users.session_generation。
-- 存量会话按用户当前世代回填（无法追溯其真实签发世代），保持现有登录态不变。
ALTER TABLE sessions ADD COLUMN IF NOT EXISTS generation INT NOT NULL DEFAULT 0;

UPDATE sessions s
SET generation = u.session_generation
FROM users u
WHERE u.id = s.user_id;

COMMENT ON COLUMN sessions.generation IS '签发时 users.session_generation 的快照，与当前值不一致则会话失效';
