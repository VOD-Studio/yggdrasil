-- 工作稿、公开版本与知识库版本分别指向不可变快照。
CREATE TABLE notes (
    id SERIAL PRIMARY KEY,
    owner_id INTEGER NOT NULL REFERENCES users(id),
    slug TEXT NOT NULL UNIQUE,
    version INTEGER NOT NULL DEFAULT 0,
    published_version INTEGER,
    knowledge_version INTEGER,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);

CREATE TABLE note_revisions (
    note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('moment', 'topic')),
    title TEXT NOT NULL DEFAULT '',
    content_md TEXT NOT NULL,
    content_html TEXT NOT NULL,
    toc_html TEXT NOT NULL DEFAULT '',
    summary TEXT NOT NULL DEFAULT '',
    tags TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    search_text TEXT GENERATED ALWAYS AS (title || ' ' || content_md) STORED,
    PRIMARY KEY (note_id, version)
);
ALTER TABLE notes ADD CONSTRAINT notes_current_revision
    FOREIGN KEY (id, version) REFERENCES note_revisions(note_id, version) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE notes ADD CONSTRAINT notes_public_revision
    FOREIGN KEY (id, published_version) REFERENCES note_revisions(note_id, version) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE notes ADD CONSTRAINT notes_knowledge_revision
    FOREIGN KEY (id, knowledge_version) REFERENCES note_revisions(note_id, version) DEFERRABLE INITIALLY DEFERRED;
CREATE INDEX notes_owner_updated ON notes(owner_id, updated_at DESC);
CREATE INDEX note_revisions_search ON note_revisions USING GIN(search_text gin_trgm_ops);

CREATE TABLE notebooks (
    id SERIAL PRIMARY KEY,
    owner_id INTEGER NOT NULL REFERENCES users(id),
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TABLE notebook_notes (
    notebook_id INTEGER NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
    note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (notebook_id, note_id)
);
CREATE INDEX notebook_notes_note ON notebook_notes(note_id);

-- 独立的私密图片存储：不进入公开 uploads 路径，数据库备份包含原图。
CREATE TABLE note_attachments (
    id UUID PRIMARY KEY,
    owner_id INTEGER NOT NULL REFERENCES users(id),
    mime TEXT NOT NULL,
    data BYTEA NOT NULL CHECK (octet_length(data) <= 5242880),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TABLE note_revision_attachments (
    note_id INTEGER NOT NULL,
    version INTEGER NOT NULL,
    attachment_id UUID NOT NULL REFERENCES note_attachments(id),
    PRIMARY KEY (note_id, version, attachment_id),
    FOREIGN KEY (note_id, version) REFERENCES note_revisions(note_id, version) ON DELETE CASCADE
);
-- 素材库图片仍为公开素材；保留所有版本引用，防止孤儿清理误删。
CREATE TABLE note_asset_refs (
    note_id INTEGER NOT NULL,
    version INTEGER NOT NULL,
    asset_id UUID NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    PRIMARY KEY (note_id, version, asset_id),
    FOREIGN KEY (note_id, version) REFERENCES note_revisions(note_id, version) ON DELETE CASCADE
);

-- 新能力显式授权，已有令牌不获得私人知识权限。
ALTER TABLE mcp_tokens ADD COLUMN notes_read BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE mcp_tokens ADD COLUMN notes_write BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE mcp_tokens ADD COLUMN notebook_ids INTEGER[];
