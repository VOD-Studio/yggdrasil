-- 归档只收起目录，保留笔记关联、版本与授权。
ALTER TABLE notebooks ADD COLUMN archived_at TIMESTAMPTZ;
ALTER TABLE notebooks ADD CONSTRAINT archived_notebook_is_private
    CHECK (archived_at IS NULL OR NOT is_public);
