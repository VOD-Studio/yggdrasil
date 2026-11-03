-- mcp_tokens 记录签发时的 users.session_generation 快照，与 cookie 会话同一套语义：
-- 改密 / ADMIN_PASSWORD 轮换 / 角色变更 bump 世代号后，名下旧 MCP 令牌立即失效。
-- 存量令牌按持有人当前世代回填（无法追溯真实签发世代），保持现有令牌可用。
ALTER TABLE mcp_tokens ADD COLUMN IF NOT EXISTS generation INT NOT NULL DEFAULT 0;

UPDATE mcp_tokens t
SET generation = u.session_generation
FROM users u
WHERE u.id = t.user_id;

COMMENT ON COLUMN mcp_tokens.generation IS '签发时 users.session_generation 的快照，与当前值不一致则令牌失效';
