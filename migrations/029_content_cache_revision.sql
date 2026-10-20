-- 持久化版本用于断线补偿；通知只在事务提交后投递，回滚不会失效其它节点。
CREATE TABLE content_cache_revision (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    revision BIGINT NOT NULL DEFAULT 0
);
INSERT INTO content_cache_revision (singleton) VALUES (TRUE);

CREATE FUNCTION notify_content_cache_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    UPDATE content_cache_revision SET revision = revision + 1 WHERE singleton;
    PERFORM pg_notify('ygg_content_changed', 'changed');
    RETURN NULL;
END;
$$;

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'posts', 'post_tags', 'tags', 'comments', 'friend_links', 'settings',
        'notes', 'notebooks', 'notebook_notes'
    ] LOOP
        EXECUTE format(
            'CREATE TRIGGER content_cache_changed AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION notify_content_cache_change()',
            table_name
        );
    END LOOP;
END;
$$;
