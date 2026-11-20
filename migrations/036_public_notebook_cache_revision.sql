ALTER TABLE content_cache_revision ADD COLUMN notebooks_revision BIGINT NOT NULL DEFAULT 0;

CREATE FUNCTION notify_public_notebook_cache_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    UPDATE content_cache_revision
       SET revision = revision + 1, notebooks_revision = notebooks_revision + 1
     WHERE singleton;
    PERFORM pg_notify('ygg_content_changed', 'changed');
    RETURN NULL;
END;
$$;

CREATE TRIGGER public_notebooks_cache_changed
    AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON notebooks
    FOR EACH STATEMENT EXECUTE FUNCTION notify_public_notebook_cache_change();

CREATE TRIGGER public_notebook_membership_cache_changed
    AFTER INSERT OR UPDATE OF notebook_id, note_id OR DELETE OR TRUNCATE ON notebook_notes
    FOR EACH STATEMENT EXECUTE FUNCTION notify_public_notebook_cache_change();
