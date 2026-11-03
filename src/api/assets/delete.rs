//! 素材删除与孤儿清理接口。
//!
//! 删除保护：被任何文章（含回收站文章）或存活评论引用的素材禁删；
//! 孤儿素材硬删除（文件 + DB 行 + 派生缓存）。一键清理仅作用于
//! 无引用且超过 7 天保护窗的素材（保护未保存草稿的引用）。
//! Leptos server function，注册在 `/api` 路径下，仅 admin 可用。

use crate::api::error::ApiError;
use leptos::prelude::*;

use super::types::{AssetOpResponse, BatchDeleteAssetsResponse, PurgeOrphansResponse};

/// 更新素材 alt（管理性备注，不回写已有文章 HTML）。
#[server(UpdateAssetAlt, endpoint = "update_asset_alt", input = leptos::server_fn::codec::Json)]
pub async fn update_asset_alt(id: String, alt: String) -> Result<AssetOpResponse, ApiError> {
    crate::api::auth::get_current_admin_user().await?;
    update_asset_alt_impl(id, alt).await
}

/// 共享素材操作；调用方须先完成管理员鉴权。
#[cfg(feature = "server")]
pub(crate) async fn update_asset_alt_impl(
    id: String,
    alt: String,
) -> Result<AssetOpResponse, ApiError> {
    use crate::api::error::AppError;
    use crate::db::pool::get_conn;

    let client = get_conn().await.map_err(AppError::db_conn)?;

    // id 在边界处从 String 解析为 Uuid（非法 id 属业务错误，不走 500）。
    let asset_uuid = match uuid::Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return Ok(AssetOpResponse::err("素材 id 非法".to_string())),
    };

    let alt = alt.trim().to_string();
    let updated = client
        .execute(
            "UPDATE assets SET alt = NULLIF($2, ''), updated_at = NOW() WHERE id = $1",
            &[&asset_uuid, &alt],
        )
        .await
        .map_err(AppError::query)?;

    if updated == 0 {
        return Ok(AssetOpResponse::err("素材不存在".to_string()));
    }
    Ok(AssetOpResponse::ok("alt 已更新".to_string()))
}

/// 删除单张素材。
///
/// 引用中 → `Ok(success:false)` + 引用文章列表（业务拒绝不走 Err）；
/// 孤儿 → 物理删除文件、DB 行（refs 级联）与派生缓存。
#[server(DeleteAsset, endpoint = "delete_asset", input = leptos::server_fn::codec::Json)]
pub async fn delete_asset(id: String) -> Result<AssetOpResponse, ApiError> {
    crate::api::auth::get_current_admin_user().await?;
    delete_asset_impl(id).await
}

/// 共享素材操作；调用方须先完成管理员鉴权。
#[cfg(feature = "server")]
pub(crate) async fn delete_asset_impl(id: String) -> Result<AssetOpResponse, ApiError> {
    use crate::api::error::AppError;
    use crate::db::pool::get_conn;
    use crate::models::asset::{AssetRef, AssetRefPostStatus};

    let mut client = get_conn().await.map_err(AppError::db_conn)?;
    // 锁住素材行，令并发文章保存的 asset_refs 外键检查在删除提交前排队，
    // 避免引用检查后插入新引用再被 ON DELETE CASCADE 静默移除。
    let tx = client.transaction().await.map_err(AppError::tx)?;

    let asset_uuid = match uuid::Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return Ok(AssetOpResponse::err("素材 id 非法".to_string())),
    };

    let row = tx
        .query_opt(
            "SELECT id AS id, path, filename FROM assets WHERE id = $1 FOR UPDATE",
            &[&asset_uuid],
        )
        .await
        .map_err(AppError::query)?;
    let Some(row) = row else {
        return Ok(AssetOpResponse::err("素材不存在".to_string()));
    };
    let path: String = row.get("path");

    let note_reference: bool = tx
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM note_asset_refs WHERE asset_id=$1)",
            &[&asset_uuid],
        )
        .await
        .map_err(AppError::query)?
        .get(0);
    if note_reference {
        return Ok(AssetOpResponse::err(
            "该素材被笔记或历史版本引用，无法删除".to_string(),
        ));
    }

    // 引用检查：含回收站文章（其 purge 时 refs 级联删，图自然变孤儿）。
    let ref_rows = tx
        .query(
            "SELECT p.id, p.title, p.slug, p.status, p.deleted_at \
             FROM asset_refs r JOIN posts p ON p.id = r.post_id \
             WHERE r.asset_id = $1 ORDER BY p.id",
            &[&asset_uuid],
        )
        .await
        .map_err(AppError::query)?;
    if !ref_rows.is_empty() {
        let refs: Vec<AssetRef> = ref_rows
            .iter()
            .map(|r| {
                let status = AssetRefPostStatus::resolve(
                    r.get::<_, &str>(3),
                    r.get::<_, Option<chrono::DateTime<chrono::Utc>>>(4),
                );
                AssetRef::Post {
                    post_id: r.get(0),
                    title: r.get(1),
                    slug: r.get(2),
                    status,
                }
            })
            .collect();
        return Ok(AssetOpResponse {
            success: false,
            message: format!("该素材正被 {} 篇文章引用，无法删除", refs.len()),
            refs,
        });
    }

    // 评论与头像引用共用一次查询，避免单删路径额外往返数据库。
    let reference_row = tx
        .query_one(
            &format!(
                "SELECT {comment_ref} AS comment_referenced, \
                        {avatar_ref} AS avatar_referenced \
                 FROM assets a WHERE a.id = $1",
                comment_ref = super::COMMENT_REF_CLAUSE,
                avatar_ref = super::AVATAR_REF_CLAUSE,
            ),
            &[&asset_uuid],
        )
        .await
        .map_err(AppError::query)?;
    if reference_row.get::<_, bool>("comment_referenced") {
        return Ok(AssetOpResponse {
            success: false,
            message: "该素材正被评论引用，无法删除".to_string(),
            refs: Vec::new(),
        });
    }

    if reference_row.get::<_, bool>("avatar_referenced") {
        return Ok(AssetOpResponse {
            success: false,
            message: "该素材正被头像引用，无法删除".to_string(),
            refs: Vec::new(),
        });
    }

    // 先提交 DB 删除，再清理文件；文件删除失败只会留下可由重建索引清理的孤儿文件，
    // 不会在事务失败时丢失仍被引用的实体文件。
    let file_path = format!("uploads/{}", path);
    tx.execute("DELETE FROM assets WHERE id = $1", &[&asset_uuid])
        .await
        .map_err(AppError::tx)?;
    tx.commit().await.map_err(AppError::tx)?;
    if let Err(e) = tokio::fs::remove_file(&file_path).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!("Remove asset file failed ({}): {}", file_path, e);
        }
    }
    crate::api::image::invalidate_asset_caches(&path).await;

    Ok(AssetOpResponse::ok("已删除".to_string()))
}

/// 批量删除素材。
///
/// 逐项做引用检查：被引用的跳过（保护语义与单删一致），无引用的硬删除
/// （文件 + DB 行 + 派生缓存）。非法 id 计入失败，不存在的 id 静默忽略。
/// 返回删除/跳过/失败统计，供批量操作条展示。
#[server(BatchDeleteAssets, endpoint = "batch_delete_assets", input = leptos::server_fn::codec::Json)]
pub async fn batch_delete_assets(ids: Vec<String>) -> Result<BatchDeleteAssetsResponse, ApiError> {
    crate::api::auth::get_current_admin_user().await?;
    batch_delete_assets_impl(ids).await
}

/// 共享素材操作；调用方须先完成管理员鉴权。
#[cfg(feature = "server")]
pub(crate) async fn batch_delete_assets_impl(
    ids: Vec<String>,
) -> Result<BatchDeleteAssetsResponse, ApiError> {
    use crate::api::error::AppError;
    use crate::db::pool::get_conn;

    // id 在边界处从 String 解析为 Uuid；非法 id 属业务错误，计入 failures 不走 500。
    let mut uuids: Vec<uuid::Uuid> = Vec::with_capacity(ids.len());
    let mut failures: i64 = 0;
    for id in &ids {
        match uuid::Uuid::parse_str(id) {
            Ok(u) => uuids.push(u),
            Err(_) => failures += 1,
        }
    }
    if uuids.is_empty() {
        return Ok(BatchDeleteAssetsResponse {
            success: false,
            message: "没有可删除的素材".to_string(),
            deleted_count: 0,
            skipped_referenced: 0,
            freed_bytes: 0,
            failures,
        });
    }

    let mut client = get_conn().await.map_err(AppError::db_conn)?;
    let tx = client.transaction().await.map_err(AppError::tx)?;

    let locked = tx
        .query(
            "SELECT id FROM assets WHERE id = ANY($1) FOR UPDATE",
            &[&uuids],
        )
        .await
        .map_err(AppError::query)?;
    let locked: Vec<uuid::Uuid> = locked.iter().map(|r| r.get(0)).collect();
    let deleted = delete_unreferenced_locked(&tx, &locked).await?;
    tx.commit().await.map_err(AppError::tx)?;

    let skipped = (locked.len() - deleted.len()) as i64;
    let freed_bytes = deleted.iter().map(|(_, size)| size).sum();
    let deleted_count = deleted.len();
    failures += remove_asset_files(deleted).await;

    let mut message = format!("已删除 {} 张素材", deleted_count);
    if skipped > 0 {
        message.push_str(&format!("，跳过 {} 张被引用", skipped));
    }
    Ok(BatchDeleteAssetsResponse {
        success: true,
        message,
        deleted_count: deleted_count as i64,
        skipped_referenced: skipped,
        freed_bytes,
        failures,
    })
}

/// 删除已被本事务 `FOR UPDATE` 锁住、且此刻仍无引用的素材，返回 (path, size_bytes)。
///
/// 必须与加锁分成两条语句：READ COMMITTED 下，若加锁语句等待的是并发文章保存
/// 插入 asset_refs 时持有的外键锁，拿到锁后 Postgres 不会重新求值 WHERE，
/// 加锁语句里的 `NOT {ASSET_REF_CLAUSE}` 仍基于旧快照，会把刚提交的引用随
/// ON DELETE CASCADE 一起删掉。本语句取新快照复核；锁已持有，之后的新引用
/// 只能在删除提交后因外键失败而报错，不会被静默删除。
#[cfg(feature = "server")]
async fn delete_unreferenced_locked(
    tx: &deadpool_postgres::Transaction<'_>,
    locked_ids: &[uuid::Uuid],
) -> Result<Vec<(String, i64)>, crate::api::error::AppError> {
    use crate::api::error::AppError;

    if locked_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = tx
        .query(
            &format!(
                "DELETE FROM assets a WHERE a.id = ANY($1) AND NOT {asset_ref} \
                 RETURNING a.path, a.size_bytes",
                asset_ref = super::ASSET_REF_CLAUSE
            ),
            &[&locked_ids],
        )
        .await
        .map_err(AppError::tx)?;
    Ok(rows.iter().map(|r| (r.get(0), r.get(1))).collect())
}

/// DB 删除提交后清理文件与派生缓存，返回非 NotFound 的文件删除失败数；
/// 残留文件由重建索引兜底。
#[cfg(feature = "server")]
async fn remove_asset_files(deleted: Vec<(String, i64)>) -> i64 {
    let mut failures = 0;
    for (path, _) in deleted {
        let file_path = format!("uploads/{}", path);
        if let Err(e) = tokio::fs::remove_file(&file_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("Remove asset file failed ({}): {}", file_path, e);
                failures += 1;
            }
        }
        crate::api::image::invalidate_asset_caches(&path).await;
    }
    failures
}

/// 一键清理孤儿素材：无引用且 created_at 早于 7 天保护窗。
///
/// 先提交 DB 删除，再逐项删文件（容忍单项失败）。
/// 返回清理数量、释放字节数与文件删除失败数。
#[server(PurgeOrphanAssets, endpoint = "purge_orphan_assets", input = leptos::server_fn::codec::Json)]
pub async fn purge_orphan_assets() -> Result<PurgeOrphansResponse, ApiError> {
    crate::api::auth::get_current_admin_user().await?;
    purge_orphan_assets_impl(super::list::PURGE_GRACE_DAYS).await
}

/// 删除超过 `retention_days` 的孤儿素材；管理端与定时任务共用。
/// 调用方须先完成管理员鉴权（定时任务除外）。
#[cfg(feature = "server")]
pub(crate) async fn purge_orphan_assets_impl(
    retention_days: i32,
) -> Result<PurgeOrphansResponse, ApiError> {
    use crate::api::error::AppError;
    use crate::db::pool::get_conn;

    let mut client = get_conn().await.map_err(AppError::db_conn)?;
    let tx = client.transaction().await.map_err(AppError::tx)?;

    // 锁住候选行，令并发文章保存的外键检查在删除提交前排队；引用在删除语句里复核。
    let rows = tx
        .query(
            &format!(
                "SELECT a.id FROM assets a \
                 WHERE NOT {asset_ref} \
                   AND a.created_at < NOW() - make_interval(days => $1) \
                 FOR UPDATE",
                asset_ref = super::ASSET_REF_CLAUSE
            ),
            &[&retention_days],
        )
        .await
        .map_err(AppError::query)?;
    let ids: Vec<uuid::Uuid> = rows.iter().map(|r| r.get(0)).collect();
    let deleted = delete_unreferenced_locked(&tx, &ids).await?;
    tx.commit().await.map_err(AppError::tx)?;

    if deleted.is_empty() {
        return Ok(PurgeOrphansResponse {
            success: true,
            message: "没有可清理的未引用素材".to_string(),
            deleted_count: 0,
            freed_bytes: 0,
            failures: 0,
        });
    }

    let freed_bytes = deleted.iter().map(|(_, size)| size).sum();
    let deleted_count = deleted.len() as i64;
    let failures = remove_asset_files(deleted).await;

    Ok(PurgeOrphansResponse {
        success: true,
        message: format!("已清理 {} 张未引用素材", deleted_count),
        deleted_count,
        freed_bytes,
        failures,
    })
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::purge_orphan_assets_impl;
    use crate::db::pool::get_conn;

    #[test]
    #[ignore = "requires disposable DATABASE_URL database ygg_mcp_test"]
    fn purge_keeps_reference_committed_while_waiting_for_lock() {
        crate::db::TEST_DATABASE_RUNTIME.block_on(purge_keeps_reference_impl());
    }

    /// 文章保存先插入引用（持有外键锁未提交），清理随后锁同一行等待；
    /// 保存提交后，清理不得把刚提交的引用随素材级联删除。
    async fn purge_keeps_reference_impl() {
        let _guard = crate::db::TEST_DATABASE_LOCK.lock().await;
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
                   VALUES (1,'purge-owner','purge@test.invalid','unused','admin');
                 INSERT INTO posts(id,author_id,title,slug,content_md,status)
                   VALUES (1,1,'引用','purge-race','正文','draft');
                 INSERT INTO assets(id,path,filename,mime,size_bytes,width,height,content_hash,created_at)
                 VALUES
                   (gen_random_uuid(),'purge-race-kept.webp','kept.webp','image/webp',3,1,1,'kept',NOW()-INTERVAL '30 days'),
                   (gen_random_uuid(),'purge-race-orphan.webp','orphan.webp','image/webp',5,1,1,'orphan',NOW()-INTERVAL '30 days');",
            )
            .await
            .unwrap();

        let mut saver = get_conn().await.unwrap();
        let save = saver.transaction().await.unwrap();
        save.execute(
            "INSERT INTO asset_refs(asset_id,post_id) \
             SELECT id, 1 FROM assets WHERE path = 'purge-race-kept.webp'",
            &[],
        )
        .await
        .unwrap();

        let purge = tokio::spawn(purge_orphan_assets_impl(7));
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
        assert!(waited, "清理应在被引用素材的行锁上等待");
        save.commit().await.unwrap();

        let res = purge.await.unwrap().unwrap();
        assert_eq!((res.deleted_count, res.freed_bytes), (1, 5));
        let remaining: Vec<String> = client
            .query(
                "SELECT a.path FROM assets a JOIN asset_refs r ON r.asset_id = a.id",
                &[],
            )
            .await
            .unwrap()
            .iter()
            .map(|r| r.get(0))
            .collect();
        assert_eq!(remaining, ["purge-race-kept.webp"]);
    }
}
