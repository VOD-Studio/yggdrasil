-- 归档兼容缺少发布时间的已发布文章，索引沿用归档查询的回退字段与排序。
CREATE INDEX idx_posts_archive_order
    ON posts ((COALESCE(published_at, created_at)) DESC, id DESC)
    WHERE status = 'published' AND deleted_at IS NULL;
