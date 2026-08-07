-- 站点公开配置键：页脚 GitHub 链接（空值表示不展示图标）。
-- 复用 007 建立的 settings 键值表；ON CONFLICT 保证重复执行迁移安全。
INSERT INTO settings (key, value) VALUES
    ('site_github_url', '')
ON CONFLICT (key) DO NOTHING;
