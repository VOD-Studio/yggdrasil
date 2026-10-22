-- 素材引用判定按 asset_id 查笔记引用；主键以 note_id 开头无法命中，且表含全部历史版本。
CREATE INDEX IF NOT EXISTS idx_note_asset_refs_asset ON note_asset_refs (asset_id);
