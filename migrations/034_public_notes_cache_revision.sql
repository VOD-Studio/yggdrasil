-- 首页只读取公开笔记的当前发布快照。工作稿和笔记本改动不影响任何共享缓存。
ALTER TABLE content_cache_revision ADD COLUMN public_notes_revision BIGINT NOT NULL DEFAULT 0;

DROP TRIGGER content_cache_changed ON notes;
DROP TRIGGER content_cache_changed ON notebooks;
DROP TRIGGER content_cache_changed ON notebook_notes;

CREATE FUNCTION notify_public_notes_cache_change() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    public_changed BOOLEAN := TRUE;
BEGIN
    IF TG_OP = 'UPDATE' THEN
        SELECT EXISTS (
            SELECT 1
              FROM (SELECT id, slug, published_version, published_at FROM old_notes
                     WHERE deleted_at IS NULL AND published_version IS NOT NULL) o
              FULL JOIN (SELECT id, slug, published_version, published_at FROM new_notes
                          WHERE deleted_at IS NULL AND published_version IS NOT NULL) n USING (id)
             WHERE o.id IS NULL OR n.id IS NULL
                OR (o.slug, o.published_version, o.published_at)
                    IS DISTINCT FROM (n.slug, n.published_version, n.published_at)
        ) INTO public_changed;
    ELSIF TG_OP = 'INSERT' THEN
        SELECT EXISTS (
            SELECT 1 FROM new_notes WHERE deleted_at IS NULL AND published_version IS NOT NULL
        ) INTO public_changed;
    ELSIF TG_OP = 'DELETE' THEN
        SELECT EXISTS (
            SELECT 1 FROM old_notes WHERE deleted_at IS NULL AND published_version IS NOT NULL
        ) INTO public_changed;
    END IF;
    IF public_changed THEN
        -- 旧实例在滚动更新期间仍用全局版本失效。
        UPDATE content_cache_revision
           SET revision = revision + 1, public_notes_revision = public_notes_revision + 1
         WHERE singleton;
        PERFORM pg_notify('ygg_content_changed', 'changed');
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER public_notes_cache_inserted
    AFTER INSERT ON notes REFERENCING NEW TABLE AS new_notes
    FOR EACH STATEMENT EXECUTE FUNCTION notify_public_notes_cache_change();
CREATE TRIGGER public_notes_cache_updated
    AFTER UPDATE ON notes REFERENCING OLD TABLE AS old_notes NEW TABLE AS new_notes
    FOR EACH STATEMENT EXECUTE FUNCTION notify_public_notes_cache_change();
CREATE TRIGGER public_notes_cache_deleted
    AFTER DELETE ON notes REFERENCING OLD TABLE AS old_notes
    FOR EACH STATEMENT EXECUTE FUNCTION notify_public_notes_cache_change();
CREATE TRIGGER public_notes_cache_truncated
    AFTER TRUNCATE ON notes
    FOR EACH STATEMENT EXECUTE FUNCTION notify_public_notes_cache_change();
