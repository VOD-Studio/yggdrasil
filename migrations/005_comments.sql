CREATE TABLE IF NOT EXISTS comments (
    id           BIGSERIAL PRIMARY KEY,
    post_id      INT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    parent_id    BIGINT REFERENCES comments(id) ON DELETE SET NULL,
    depth        INT NOT NULL DEFAULT 0,
    author_name  VARCHAR(50) NOT NULL,
    author_email VARCHAR(255) NOT NULL,
    author_url   VARCHAR(500),
    content_md   TEXT NOT NULL,
    content_html TEXT,
    content_hash VARCHAR(64),
    status       TEXT NOT NULL DEFAULT 'pending',
    ip_address   VARCHAR(45),
    user_agent   VARCHAR(500),
    approved_at  TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at   TIMESTAMPTZ,

    CONSTRAINT comments_status_check
        CHECK (status IN ('pending', 'approved', 'spam', 'trash')),
    CONSTRAINT comments_depth_check
        CHECK (depth >= 0 AND depth <= 20),
    CONSTRAINT comments_content_not_empty
        CHECK (length(trim(content_md)) >= 1),
    CONSTRAINT comments_name_not_empty
        CHECK (length(trim(author_name)) >= 1)
);

CREATE INDEX IF NOT EXISTS idx_comments_post_approved
    ON comments(post_id, created_at) WHERE status = 'approved' AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_comments_top_level
    ON comments(post_id, created_at)
    WHERE parent_id IS NULL AND status = 'approved' AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_comments_pending
    ON comments(created_at DESC) WHERE status = 'pending' AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_comments_admin_list
    ON comments(status, created_at DESC) WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_comments_parent
    ON comments(parent_id) WHERE parent_id IS NOT NULL;

CREATE OR REPLACE FUNCTION update_comments_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_comments_updated_at ON comments;
CREATE TRIGGER trg_comments_updated_at
    BEFORE UPDATE ON comments
    FOR EACH ROW
    EXECUTE FUNCTION update_comments_updated_at();
