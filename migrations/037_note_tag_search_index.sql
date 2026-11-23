-- text[] 的元素已是文本，固定换行分隔符使结果只取决于标签；可用于表达式索引。
CREATE FUNCTION note_tags_search_text(text[]) RETURNS text
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    RETURN array_to_string($1, E'\n');

CREATE INDEX note_revisions_tag_search
    ON note_revisions USING GIN (note_tags_search_text(tags) gin_trgm_ops);
