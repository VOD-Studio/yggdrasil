-- 保留全局版本与原通知格式，旧进程在滚动更新期间仍能监听同一张表。
ALTER TABLE content_cache_revision
    ADD COLUMN posts_revision BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN comments_revision BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN friends_revision BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN settings_revision BIGINT NOT NULL DEFAULT 0;

CREATE OR REPLACE FUNCTION notify_content_cache_change() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    comments_changed BOOLEAN := TG_TABLE_NAME = 'comments';
BEGIN
    IF TG_TABLE_NAME = 'posts' THEN
        -- 公开评论读取依赖文章可见性，正文、标题等更新可以保留评论缓存。
        IF TG_OP = 'UPDATE' THEN
            SELECT EXISTS (
                SELECT 1 FROM old_posts o FULL JOIN new_posts n USING (id)
                 WHERE COALESCE(o.status = 'published' AND o.deleted_at IS NULL, FALSE)
                       IS DISTINCT FROM
                       COALESCE(n.status = 'published' AND n.deleted_at IS NULL, FALSE)
            ) INTO comments_changed;
        ELSE
            comments_changed := TRUE;
        END IF;
    END IF;
    UPDATE content_cache_revision
       SET revision = revision + 1,
           posts_revision = posts_revision + (TG_TABLE_NAME IN ('posts', 'post_tags', 'tags'))::int,
           comments_revision = comments_revision + comments_changed::int,
           friends_revision = friends_revision + (TG_TABLE_NAME = 'friend_links')::int,
           settings_revision = settings_revision + (TG_TABLE_NAME = 'settings')::int
     WHERE singleton;
    PERFORM pg_notify('ygg_content_changed', 'changed');
    RETURN NULL;
END;
$$;

DROP TRIGGER content_cache_changed ON posts;
CREATE TRIGGER content_cache_changed
    AFTER INSERT OR DELETE OR TRUNCATE ON posts FOR EACH STATEMENT
    EXECUTE FUNCTION notify_content_cache_change();
CREATE TRIGGER content_cache_updated
    AFTER UPDATE ON posts REFERENCING OLD TABLE AS old_posts NEW TABLE AS new_posts
    FOR EACH STATEMENT EXECUTE FUNCTION notify_content_cache_change();
