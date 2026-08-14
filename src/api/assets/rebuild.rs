//! 素材索引全量重建接口。
//!
//! 以磁盘为准自愈 DB 与文件系统的不一致：
//! 1. 扫 `uploads/`（跳过 `.cache` 等点目录）→ upsert assets（技术字段变化才更新，保留 alt）；
//! 2. 删除文件已消失的 DB 行（refs 级联）；
//! 3. 全表扫 posts（含回收站）重建 asset_refs。
//!
//! 幂等：重跑结果相同（技术字段无变化时 updated 为 0，alt 不被覆盖）。
//! 幂等性由「手动触发」语义承载，非常态路径。Dioxus server function，仅 admin 可用。

use dioxus::prelude::*;

use super::types::RebuildAssetsResponse;

/// 可登记的图片扩展名（与 upload.rs 的 ALLOWED_MIME_TYPES 对应）。
#[cfg(feature = "server")]
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp"];

#[cfg(feature = "server")]
/// 扫描到的磁盘文件信息（spawn_blocking 产物）。
struct ScannedFile {
    /// 相对路径 "2026/07/24/x.webp"。
    rel_path: String,
    filename: String,
    mime: &'static str,
    size_bytes: i64,
    width: i32,
    height: i32,
}

#[cfg(feature = "server")]
/// 递归收集 dir 下的图片文件（跳过以 `.` 开头的目录/文件）。
/// `out` 收集可读文件；`unreadable` 收集磁盘存在但尺寸读取失败的路径
/// （保护其 DB 行不被删除步骤误删——见 issue #30）。
fn walk_images(
    dir: &std::path::Path,
    base: &std::path::Path,
    out: &mut Vec<ScannedFile>,
    unreadable: &mut Vec<String>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if name_str.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            walk_images(&path, base, out, unreadable);
            continue;
        }
        let ext = name_str.rsplit('.').next().unwrap_or("");
        if !IMAGE_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)) {
            continue;
        }
        let Ok(rel) = path.strip_prefix(base) else {
            continue;
        };
        let rel_path = rel.to_string_lossy().replace('\\', "/");
        // 尺寸读 header（命中 IMAGE_DIMENSIONS_CACHE 时零 IO）；读不到说明文件损坏或
        // 格式不支持——记录路径但不删其 DB 行（issue #30：曾因跳过而误删大 WebP）。
        let Some((w, h)) = crate::api::image::get_image_dimensions(&rel_path) else {
            tracing::warn!("Rebuild: skip unreadable image {}", rel_path);
            unreadable.push(rel_path);
            continue;
        };
        let size_bytes = entry.metadata().map(|m| m.len() as i64).unwrap_or(0);
        let mime = match ext.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            _ => "image/webp",
        };
        out.push(ScannedFile {
            rel_path,
            filename: name_str.to_string(),
            mime,
            size_bytes,
            width: w as i32,
            height: h as i32,
        });
    }
}

/// 全量重建素材索引。
#[server(RebuildAssetsIndex, "/api")]
pub async fn rebuild_assets_index() -> Result<RebuildAssetsResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;
        use crate::api::error::AppError;
        use crate::db::pool::get_conn;

        let _admin = get_current_admin_user().await?;

        // 磁盘扫描 + header 尺寸读取是 IO 密集同步操作，移到阻塞线程池。
        let (scanned, unreadable) = tokio::task::spawn_blocking(|| {
            let base = std::path::Path::new("uploads");
            let mut files = Vec::new();
            let mut unreadable = Vec::new();
            walk_images(base, base, &mut files, &mut unreadable);
            (files, unreadable)
        })
        .await
        .map_err(|_| AppError::Internal("素材扫描任务失败"))?;

        let mut client = get_conn().await.map_err(AppError::db_conn)?;
        let tx = client.transaction().await.map_err(AppError::tx)?;

        // 1. upsert assets。xmax = 0 判别新插入（PG 系统列：新行 xmax 为 0）。
        //    ON CONFLICT 仅当技术字段实际变化时才更新（IS DISTINCT FROM），
        //    保证幂等重跑 updated = 0 且不覆盖 alt。
        let mut inserted: i64 = 0;
        let mut updated: i64 = 0;
        for f in &scanned {
            // DO UPDATE 的 WHERE 不满足时不返回行（技术字段无变化），用 query_opt 区分三种结果。
            let asset_id = uuid::Uuid::new_v4();
            let row = tx
                .query_opt(
                    "INSERT INTO assets (id, path, filename, mime, size_bytes, width, height) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7) \
                     ON CONFLICT (path) DO UPDATE SET \
                         filename = EXCLUDED.filename, \
                         mime = EXCLUDED.mime, \
                         size_bytes = EXCLUDED.size_bytes, \
                         width = EXCLUDED.width, \
                         height = EXCLUDED.height, \
                         updated_at = NOW() \
                     WHERE assets.size_bytes IS DISTINCT FROM EXCLUDED.size_bytes \
                        OR assets.width IS DISTINCT FROM EXCLUDED.width \
                        OR assets.height IS DISTINCT FROM EXCLUDED.height \
                        OR assets.mime IS DISTINCT FROM EXCLUDED.mime \
                        OR assets.filename IS DISTINCT FROM EXCLUDED.filename \
                     RETURNING (xmax = 0) AS was_inserted",
                    &[
                        &asset_id,
                        &f.rel_path,
                        &f.filename,
                        &f.mime,
                        &f.size_bytes,
                        &f.width,
                        &f.height,
                    ],
                )
                .await
                .map_err(AppError::tx)?;
            match row {
                Some(r) if r.get::<_, bool>("was_inserted") => inserted += 1,
                Some(_) => updated += 1,
                None => {} // 技术字段无变化，幂等跳过
            }
        }

        // 2. 删除文件已消失的 DB 行（refs 级联删）。
        //    排除集 = 可读文件 + 尺寸读取失败的文件（后者仍在磁盘上，
        //    不应误删——issue #30 的根因就是此处曾遗漏无法读尺寸的大 WebP）。
        let mut keep_paths: Vec<String> = scanned.iter().map(|f| f.rel_path.clone()).collect();
        keep_paths.extend(unreadable.iter().cloned());
        let removed = tx
            .execute(
                "DELETE FROM assets WHERE NOT (path = ANY($1))",
                &[&keep_paths],
            )
            .await
            .map_err(AppError::tx)?;

        // 3. 重建 asset_refs：全表扫 posts（含回收站——回收站文章的引用同样阻止删除）。
        let post_rows = tx
            .query("SELECT id, content_html, cover_image FROM posts", &[])
            .await
            .map_err(AppError::query)?;
        tx.execute("DELETE FROM asset_refs", &[])
            .await
            .map_err(AppError::tx)?;
        let mut ref_count: i64 = 0;
        for pr in &post_rows {
            let post_id: i32 = pr.get("id");
            let content_html: Option<String> = pr.get("content_html");
            let cover_image: Option<String> = pr.get("cover_image");
            let found = crate::api::posts::helpers::extract_asset_paths(
                content_html.as_deref().unwrap_or(""),
                cover_image.as_deref(),
            );
            if found.is_empty() {
                continue;
            }
            let n = tx
                .execute(
                    "INSERT INTO asset_refs (asset_id, post_id) \
                     SELECT id, $1 FROM assets WHERE path = ANY($2) \
                     ON CONFLICT DO NOTHING",
                    &[&post_id, &found],
                )
                .await
                .map_err(AppError::tx)?;
            ref_count += n as i64;
        }

        tx.commit().await.map_err(AppError::tx)?;

        let scanned_count = scanned.len() as i64;
        let skipped_count = unreadable.len() as i64;
        let message = if skipped_count > 0 {
            format!(
                "重建完成：扫描 {} 个文件，新增 {}，更新 {}，移除 {}，跳过 {} 个无法读取的文件（已保留）",
                scanned_count, inserted, updated, removed, skipped_count
            )
        } else {
            format!(
                "重建完成：扫描 {} 个文件，新增 {}，更新 {}，移除 {}",
                scanned_count, inserted, updated, removed
            )
        };
        Ok(RebuildAssetsResponse {
            success: true,
            message,
            scanned: scanned_count,
            inserted,
            updated,
            removed: removed as i64,
            ref_count,
            skipped: skipped_count,
        })
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}
