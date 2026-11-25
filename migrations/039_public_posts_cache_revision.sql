-- posts_revision 继续覆盖后台和旧实例；公开文章、Feed 和 SSR 单独跟踪可见变更。
ALTER TABLE content_cache_revision ADD COLUMN public_posts_revision BIGINT NOT NULL DEFAULT 0;

CREATE OR REPLACE FUNCTION notify_content_cache_change() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    posts_changed BOOLEAN := TG_TABLE_NAME IN ('posts', 'post_tags', 'tags');
    public_posts_changed BOOLEAN := posts_changed;
    comments_changed BOOLEAN := TG_TABLE_NAME = 'comments';
BEGIN
    IF TG_TABLE_NAME = 'posts' THEN
        IF TG_OP = 'UPDATE' THEN
            -- 旧、新可见集合的并集影响文章，只有进出集合才影响公开评论。
            SELECT COUNT(*) > 0, COUNT(*) FILTER (WHERE o.id IS NULL OR n.id IS NULL) > 0
              INTO public_posts_changed, comments_changed
              FROM (SELECT id FROM old_posts WHERE status = 'published' AND deleted_at IS NULL) o
              FULL JOIN (SELECT id FROM new_posts WHERE status = 'published' AND deleted_at IS NULL) n USING (id);
        ELSE
            IF TG_OP <> 'TRUNCATE' THEN
                SELECT EXISTS (SELECT FROM affected_posts WHERE status = 'published' AND deleted_at IS NULL)
                  INTO public_posts_changed;
            END IF;
            comments_changed := public_posts_changed;
        END IF;
    ELSIF TG_TABLE_NAME = 'post_tags' AND TG_OP IN ('INSERT', 'DELETE') THEN
        -- 先与并发发布按同一版本行排序，再读取文章可见性，避免后提交的标签变更漏刷。
        PERFORM FROM content_cache_revision WHERE singleton FOR UPDATE;
        SELECT EXISTS (SELECT FROM affected_links l JOIN posts p ON p.id = l.post_id
                        WHERE p.status = 'published' AND p.deleted_at IS NULL)
          INTO public_posts_changed;
    ELSIF TG_TABLE_NAME = 'tags' AND TG_OP = 'INSERT' THEN
        -- 新标签加入公开文章时由 post_tags 刷新；零篇标签只影响完整标签目录。
        public_posts_changed := FALSE;
    ELSIF TG_TABLE_NAME = 'comments' AND TG_OP = 'DELETE' THEN
        -- 删除无评论的草稿也会执行外键级联 DELETE 语句。
        IF NOT EXISTS (SELECT FROM removed_comments) THEN
            RETURN NULL;
        END IF;
    END IF;
    UPDATE content_cache_revision
       SET revision = revision + 1,
           posts_revision = posts_revision + posts_changed::int,
           public_posts_revision = public_posts_revision + public_posts_changed::int,
           comments_revision = comments_revision + comments_changed::int,
           friends_revision = friends_revision + (TG_TABLE_NAME = 'friend_links')::int,
           settings_revision = settings_revision + (TG_TABLE_NAME = 'settings')::int
     WHERE singleton;
    PERFORM pg_notify('ygg_content_changed', 'changed');
    RETURN NULL;
END;
$$;

-- UPDATE 已有 old_posts / new_posts；插入与删除共用表示受影响行的别名。
DROP TRIGGER content_cache_changed ON posts;
CREATE TRIGGER content_cache_inserted AFTER INSERT ON posts REFERENCING NEW TABLE AS affected_posts
    FOR EACH STATEMENT EXECUTE FUNCTION notify_content_cache_change();
CREATE TRIGGER content_cache_deleted AFTER DELETE ON posts REFERENCING OLD TABLE AS affected_posts
    FOR EACH STATEMENT EXECUTE FUNCTION notify_content_cache_change();
CREATE TRIGGER content_cache_truncated AFTER TRUNCATE ON posts
    FOR EACH STATEMENT EXECUTE FUNCTION notify_content_cache_change();

DROP TRIGGER content_cache_changed ON post_tags;
CREATE TRIGGER content_cache_inserted AFTER INSERT ON post_tags REFERENCING NEW TABLE AS affected_links
    FOR EACH STATEMENT EXECUTE FUNCTION notify_content_cache_change();
CREATE TRIGGER content_cache_deleted AFTER DELETE ON post_tags REFERENCING OLD TABLE AS affected_links
    FOR EACH STATEMENT EXECUTE FUNCTION notify_content_cache_change();
CREATE TRIGGER content_cache_changed AFTER UPDATE OR TRUNCATE ON post_tags
    FOR EACH STATEMENT EXECUTE FUNCTION notify_content_cache_change();

DROP TRIGGER content_cache_changed ON comments;
CREATE TRIGGER content_cache_deleted AFTER DELETE ON comments REFERENCING OLD TABLE AS removed_comments
    FOR EACH STATEMENT EXECUTE FUNCTION notify_content_cache_change();
CREATE TRIGGER content_cache_changed AFTER INSERT OR UPDATE OR TRUNCATE ON comments
    FOR EACH STATEMENT EXECUTE FUNCTION notify_content_cache_change();
