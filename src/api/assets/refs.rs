//! 评论与头像的素材引用登记。
//!
//! 写入评论、用户头像、友链头像时，先在同一事务内锁定并核对引用的素材，
//! 再把引用写进对应的外键引用表；素材删除与孤儿清理只查引用表判定。

use crate::api::error::AppError;

/// 评论正文引用了未登记或已删除的素材。
pub(crate) const MISSING_COMMENT_ASSET: &str = "评论中的图片不存在或已被删除，请重新上传";
/// 头像 URL 指向未登记或已删除的素材。
pub(crate) const MISSING_AVATAR_ASSET: &str = "头像素材不存在或已被删除，请重新上传";

/// 一组素材路径在本事务内的锁定结果。
pub(crate) struct LockedAssets {
    /// 已登记且已加 FOR KEY SHARE 锁的素材 id（按 id 升序）。
    pub ids: Vec<uuid::Uuid>,
    /// 至少一个路径未登记，或在等锁期间被删除。
    pub missing: bool,
}

/// 按相对路径锁定素材行，供随后在同一事务内写入引用。
///
/// `paths` 须已去重。`FOR KEY SHARE` 与素材删除/清理的 `FOR UPDATE` 互斥：
/// READ COMMITTED 下若等锁期间删除已提交，本语句重读后跳过该行，计入 `missing`；
/// 反之拿到锁后，删除方须等本事务提交，其引用复核是加锁之后的另一条语句、
/// 取新快照，能看到本事务刚提交的引用行。按 id 升序加锁，与删除方加锁顺序一致，
/// 避免多图评论与批量删除互相死锁。
pub(crate) async fn lock_assets_by_path(
    tx: &deadpool_postgres::Transaction<'_>,
    paths: &[String],
) -> Result<LockedAssets, AppError> {
    if paths.is_empty() {
        return Ok(LockedAssets {
            ids: Vec::new(),
            missing: false,
        });
    }
    let rows = tx
        .query(
            "SELECT id FROM assets WHERE path = ANY($1) ORDER BY id FOR KEY SHARE",
            &[&paths],
        )
        .await
        .map_err(AppError::query)?;
    Ok(LockedAssets {
        missing: rows.len() < paths.len(),
        ids: rows.iter().map(|r| r.get(0)).collect(),
    })
}

/// 本地头像 URL（`/uploads/<path>`）→ 素材相对路径；外链或空返回 None。
pub(crate) fn avatar_asset_path(avatar_url: Option<&str>) -> Option<String> {
    avatar_url?.strip_prefix("/uploads/").map(str::to_string)
}

/// 登记新评论引用的素材（须已由 [`lock_assets_by_path`] 锁定）。
pub(crate) async fn insert_comment_refs(
    tx: &deadpool_postgres::Transaction<'_>,
    comment_id: i64,
    asset_ids: &[uuid::Uuid],
) -> Result<(), AppError> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    tx.execute(
        "INSERT INTO comment_asset_refs (comment_id, asset_id) SELECT $1, unnest($2::uuid[])",
        &[&comment_id, &asset_ids],
    )
    .await
    .map_err(AppError::tx)?;
    Ok(())
}

/// 用户头像引用改为 `asset_id`（None 表示无本地头像引用）。
pub(crate) async fn sync_user_avatar_ref(
    tx: &deadpool_postgres::Transaction<'_>,
    user_id: i32,
    asset_id: Option<uuid::Uuid>,
) -> Result<(), AppError> {
    tx.execute(
        "DELETE FROM user_avatar_asset_refs WHERE user_id = $1",
        &[&user_id],
    )
    .await
    .map_err(AppError::tx)?;
    if let Some(asset_id) = asset_id {
        tx.execute(
            "INSERT INTO user_avatar_asset_refs (user_id, asset_id) VALUES ($1, $2)",
            &[&user_id, &asset_id],
        )
        .await
        .map_err(AppError::tx)?;
    }
    Ok(())
}

/// 友链头像引用改为 `asset_id`（None 表示无本地头像引用）。
pub(crate) async fn sync_friend_avatar_ref(
    tx: &deadpool_postgres::Transaction<'_>,
    friend_id: i32,
    asset_id: Option<uuid::Uuid>,
) -> Result<(), AppError> {
    tx.execute(
        "DELETE FROM friend_avatar_asset_refs WHERE friend_id = $1",
        &[&friend_id],
    )
    .await
    .map_err(AppError::tx)?;
    if let Some(asset_id) = asset_id {
        tx.execute(
            "INSERT INTO friend_avatar_asset_refs (friend_id, asset_id) VALUES ($1, $2)",
            &[&friend_id, &asset_id],
        )
        .await
        .map_err(AppError::tx)?;
    }
    Ok(())
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::lock_assets_by_path;
    use crate::db::pool::get_conn;

    /// 连接一次性测试库、执行迁移并清空素材与用户相关数据。
    async fn fresh_database() -> deadpool_postgres::Object {
        let mut client = get_conn().await.unwrap();
        let database: String = client
            .query_one("SELECT current_database()", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(database, "ygg_mcp_test", "requires isolated test database");
        crate::db::migrate::run_on_conn(&mut client).await.unwrap();
        client
            .batch_execute(
                "TRUNCATE users RESTART IDENTITY CASCADE;
                 TRUNCATE assets CASCADE;
                 INSERT INTO users(id,username,email,password_hash,role)
                   VALUES (1,'refs-owner','refs@test.invalid','unused','admin');",
            )
            .await
            .unwrap();
        client
    }

    #[test]
    #[ignore = "requires disposable DATABASE_URL database ygg_mcp_test"]
    fn asset_refs_database_comment_requires_and_protects_asset() {
        crate::db::TEST_DATABASE_RUNTIME.block_on(comment_requires_and_protects_asset_impl());
    }

    /// 评论引用缺失素材被拒；引用存在的素材登记引用行，评论进回收站后仍禁删、不被清理。
    async fn comment_requires_and_protects_asset_impl() {
        let _guard = crate::db::TEST_DATABASE_LOCK.lock().await;
        let client = fresh_database().await;
        client
            .batch_execute(
                "INSERT INTO posts(id,author_id,title,slug,content_md,status,published_at)
                   VALUES (1,1,'引用','asset-refs','正文','published',NOW());
                 INSERT INTO assets(id,path,filename,mime,size_bytes,width,height,content_hash,created_at)
                   VALUES (gen_random_uuid(),'2026/01/01/120000.kept.webp','kept.webp','image/webp',
                           3,1,1,'kept',NOW()-INTERVAL '30 days');",
            )
            .await
            .unwrap();

        let missing = crate::api::comments::create_comment(
            1,
            None,
            "读者".into(),
            "reader@test.invalid".into(),
            None,
            "![x](/uploads/2026/01/01/090000.missing.webp)".into(),
            String::new(),
        )
        .await
        .unwrap();
        assert_eq!(missing.error_code.as_deref(), Some("missing_asset"));
        let comments: i64 = client
            .query_one("SELECT count(*) FROM comments", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(comments, 0, "缺失素材的评论不得写入");

        let created = crate::api::comments::create_comment(
            1,
            None,
            "读者".into(),
            "reader@test.invalid".into(),
            None,
            "![x](/uploads/2026/01/01/120000.kept.webp)".into(),
            String::new(),
        )
        .await
        .unwrap();
        assert!(created.success, "{}", created.message);
        let asset_id: uuid::Uuid = client
            .query_one("SELECT id FROM assets", &[])
            .await
            .unwrap()
            .get(0);
        let refs: Vec<(i64, uuid::Uuid)> = client
            .query("SELECT comment_id, asset_id FROM comment_asset_refs", &[])
            .await
            .unwrap()
            .iter()
            .map(|r| (r.get(0), r.get(1)))
            .collect();
        assert_eq!(refs, [(created.comment_id.unwrap(), asset_id)]);

        client
            .execute("UPDATE comments SET status='trash', deleted_at=NOW()", &[])
            .await
            .unwrap();
        let deleted = crate::api::assets::delete::delete_asset_impl(asset_id.to_string())
            .await
            .unwrap();
        assert!(!deleted.success);
        assert_eq!(deleted.message, "该素材正被评论引用，无法删除");
        let purged = crate::api::assets::delete::purge_orphan_assets_impl(7)
            .await
            .unwrap();
        assert_eq!(purged.deleted_count, 0);
        let assets: i64 = client
            .query_one("SELECT count(*) FROM assets", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(assets, 1, "回收站评论引用的素材必须保留");
    }

    #[test]
    #[ignore = "requires disposable DATABASE_URL database ygg_mcp_test"]
    fn asset_refs_database_lock_reports_asset_deleted_while_waiting() {
        crate::db::TEST_DATABASE_RUNTIME.block_on(lock_reports_deleted_impl());
    }

    /// 删除方先锁住素材行；写入方等锁期间删除提交，锁定结果必须报告缺失。
    async fn lock_reports_deleted_impl() {
        const PATH: &str = "2026/01/01/120000.race.webp";
        let _guard = crate::db::TEST_DATABASE_LOCK.lock().await;
        let client = fresh_database().await;
        client
            .execute(
                "INSERT INTO assets(id,path,filename,mime,size_bytes,width,height,content_hash,created_at) \
                 VALUES (gen_random_uuid(),$1,'race.webp','image/webp',3,1,1,'race',NOW())",
                &[&PATH],
            )
            .await
            .unwrap();

        let mut deleter = get_conn().await.unwrap();
        let delete = deleter.transaction().await.unwrap();
        delete
            .query_one("SELECT id FROM assets WHERE path = $1 FOR UPDATE", &[&PATH])
            .await
            .unwrap();

        let writer = tokio::spawn(async {
            let mut conn = get_conn().await.unwrap();
            let tx = conn.transaction().await.unwrap();
            let locked = lock_assets_by_path(&tx, &[PATH.to_string()]).await.unwrap();
            (locked.ids.len(), locked.missing)
        });
        let mut waited = false;
        for _ in 0..200 {
            let blocked: i64 = client
                .query_one(
                    "SELECT count(*) FROM pg_stat_activity \
                     WHERE datname = current_database() AND wait_event_type = 'Lock'",
                    &[],
                )
                .await
                .unwrap()
                .get(0);
            if blocked > 0 {
                waited = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(waited, "写入方应在素材行锁上等待");
        delete
            .execute("DELETE FROM assets WHERE path = $1", &[&PATH])
            .await
            .unwrap();
        delete.commit().await.unwrap();

        assert_eq!(writer.await.unwrap(), (0, true));
    }
}
