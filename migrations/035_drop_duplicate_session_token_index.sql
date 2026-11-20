-- sessions_token_hash_key 是 token_hash 唯一约束自动创建的索引。
DROP INDEX IF EXISTS idx_sessions_token_hash;
