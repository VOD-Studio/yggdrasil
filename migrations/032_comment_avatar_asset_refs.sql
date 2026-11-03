-- 评论、用户头像、友链头像的素材引用改为外键引用表，并从现有内容回填。
CREATE TABLE comment_asset_refs (
    comment_id BIGINT NOT NULL REFERENCES comments(id) ON DELETE CASCADE,
    asset_id   UUID   NOT NULL REFERENCES assets(id)   ON DELETE CASCADE,
    PRIMARY KEY (comment_id, asset_id)
);
CREATE INDEX idx_comment_asset_refs_asset ON comment_asset_refs (asset_id);

CREATE TABLE user_avatar_asset_refs (
    user_id  INT  PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    asset_id UUID NOT NULL REFERENCES assets(id) ON DELETE CASCADE
);
CREATE INDEX idx_user_avatar_asset_refs_asset ON user_avatar_asset_refs (asset_id);

CREATE TABLE friend_avatar_asset_refs (
    friend_id INT  PRIMARY KEY REFERENCES friend_links(id) ON DELETE CASCADE,
    asset_id  UUID NOT NULL REFERENCES assets(id) ON DELETE CASCADE
);
CREATE INDEX idx_friend_avatar_asset_refs_asset ON friend_avatar_asset_refs (asset_id);

-- 回填：评论正文（含回收站评论）按与 extract_asset_paths 相同的路径规则提取。
INSERT INTO comment_asset_refs (comment_id, asset_id)
SELECT DISTINCT c.id, a.id
FROM comments c
CROSS JOIN LATERAL regexp_matches(
    c.content_html,
    '/uploads/([0-9]{4}/[0-9]{2}/[0-9]{2}/[^"''[:space:]?#)]+)',
    'g'
) AS m(captures)
JOIN assets a ON a.path = m.captures[1];

INSERT INTO user_avatar_asset_refs (user_id, asset_id)
SELECT u.id, a.id FROM users u JOIN assets a ON u.avatar_url = '/uploads/' || a.path;

INSERT INTO friend_avatar_asset_refs (friend_id, asset_id)
SELECT f.id, a.id FROM friend_links f JOIN assets a ON f.avatar_url = '/uploads/' || a.path;
