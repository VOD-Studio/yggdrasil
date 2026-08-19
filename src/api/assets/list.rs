//! 素材分页列表接口。
//!
//! 按引用状态（全部/引用中/孤儿）筛选、按文件名/alt 搜索、按时间或大小排序。
//! 同时返回各筛选维度计数与可清理孤儿统计，供 tabs 与「清理孤儿」按钮展示。
//! Dioxus server function，注册在 `/api` 路径下，仅 admin 可用。

use dioxus::prelude::*;

use super::types::AssetListResponse;
use crate::models::asset::{AssetFilter, AssetSort};

/// 每页素材数（网格 6 列 x 10 行）。
#[cfg(feature = "server")]
const PER_PAGE: i64 = 60;

/// 孤儿清理保护窗：上传不满 7 天的无引用素材不可一键清理
/// （保护尚未首次保存的草稿引用）。
#[cfg(feature = "server")]
pub(crate) const PURGE_GRACE_DAYS: i32 = 7;

/// 获取素材分页列表。
#[server(ListAssets, "/api")]
pub async fn list_assets(
    filter: AssetFilter,
    query: String,
    sort: AssetSort,
    page: i32,
) -> Result<AssetListResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;
        use crate::api::error::AppError;
        use crate::db::pool::get_conn;
        use crate::models::asset::{Asset, AssetDto, AssetRef};

        let _admin = get_current_admin_user().await?;

        let client = get_conn().await.map_err(AppError::db_conn)?;

        let page = page.max(1);
        let offset: i64 = (page as i64 - 1) * PER_PAGE;
        let query = query.trim().to_string();

        // 筛选/搜索条件统一拼进 WHERE；参数按出现顺序编号。
        // 引用状态用 EXISTS 子查询，搜索用 ILIKE 转义通配符。
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        match filter {
            AssetFilter::Used => {
                conditions.push(format!(
                    "(EXISTS (SELECT 1 FROM asset_refs r WHERE r.asset_id = a.id) OR {})",
                    super::COMMENT_REF_CLAUSE
                ));
            }
            AssetFilter::Orphan => {
                conditions.push(format!(
                    "NOT EXISTS (SELECT 1 FROM asset_refs r WHERE r.asset_id = a.id) \
                     AND NOT {}",
                    super::COMMENT_REF_CLAUSE
                ));
            }
            AssetFilter::All => {}
        }
        if !query.is_empty() {
            params.push(&query);
            conditions.push(format!(
                "(a.filename ILIKE '%' || ${} || '%' OR a.alt ILIKE '%' || ${} || '%')",
                params.len(),
                params.len()
            ));
        }
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let order_clause = match sort {
            AssetSort::CreatedDesc => "a.created_at DESC, a.id",
            AssetSort::SizeDesc => "a.size_bytes DESC, a.id",
        };

        // 列表查询：ref_count = 文章引用数 + 存活评论引用数（评论引用判定见
        // super::COMMENT_REF_CLAUSE），与 Used/Orphan 筛选语义保持一致。
        // const 不能取引用（内联后借临时值会垂悬），绑定到局部变量再进参数列表。
        let per_page = PER_PAGE;
        params.push(&per_page);
        let limit_idx = params.len();
        params.push(&offset);
        let offset_idx = params.len();
        let rows = client
            .query(
                &format!(
                    "SELECT a.id AS id, a.path, a.filename, a.mime, a.size_bytes, \
                            a.width, a.height, a.alt, a.created_at, \
                            ((SELECT COUNT(*) FROM asset_refs r WHERE r.asset_id = a.id) + \
                             (SELECT COUNT(*) FROM comments c WHERE c.deleted_at IS NULL \
                              AND c.content_html LIKE '%' || a.path || '%')) AS ref_count \
                     FROM assets a {where_clause} \
                     ORDER BY {order_clause} LIMIT ${limit_idx} OFFSET ${offset_idx}"
                ),
                &params,
            )
            .await
            .map_err(AppError::query)?;

        let total: i64 = client
            .query_one(
                &format!("SELECT COUNT(*) FROM assets a {where_clause}"),
                &params[..params.len() - 2],
            )
            .await
            .map_err(AppError::query)?
            .get(0);

        // 汇总计数：tabs 与「清理孤儿」按钮徽标。不受筛选/搜索影响，始终全局。
        // 孤儿 = 无文章引用且无存活评论引用（与 AssetFilter::Orphan 筛选同语义）。
        let summary = client
            .query_one(
                &format!(
                    "SELECT \
                        COUNT(*) FILTER (WHERE EXISTS (SELECT 1 FROM asset_refs r WHERE r.asset_id = a.id) \
                            OR {comment_ref}), \
                        COUNT(*) FILTER (WHERE NOT EXISTS (SELECT 1 FROM asset_refs r WHERE r.asset_id = a.id) \
                            AND NOT {comment_ref}), \
                        COUNT(*) FILTER (WHERE NOT EXISTS (SELECT 1 FROM asset_refs r WHERE r.asset_id = a.id) \
                            AND NOT {comment_ref} \
                            AND a.created_at < NOW() - make_interval(days => $1)), \
                        COALESCE(SUM(a.size_bytes) FILTER (WHERE NOT EXISTS (SELECT 1 FROM asset_refs r WHERE r.asset_id = a.id) \
                            AND NOT {comment_ref} \
                            AND a.created_at < NOW() - make_interval(days => $1)), 0)::bigint \
                     FROM assets a",
                    comment_ref = super::COMMENT_REF_CLAUSE
                ),
                &[&PURGE_GRACE_DAYS],
            )
            .await
            .map_err(AppError::query)?;

        // 本页素材的引用文章（第二查询，避免 JOIN  fan-out 与分页错位）。
        let ids: Vec<uuid::Uuid> = rows.iter().map(|r| r.get::<_, uuid::Uuid>("id")).collect();
        let mut refs_map: std::collections::HashMap<String, Vec<AssetRef>> =
            std::collections::HashMap::new();
        if !ids.is_empty() {
            let ref_rows = client
                .query(
                    "SELECT r.asset_id AS asset_id, p.id AS post_id, p.title \
                     FROM asset_refs r JOIN posts p ON p.id = r.post_id \
                     WHERE r.asset_id = ANY($1) \
                     ORDER BY p.id",
                    &[&ids],
                )
                .await
                .map_err(AppError::query)?;
            for rr in ref_rows {
                let asset_key: String = rr.get::<_, uuid::Uuid>("asset_id").to_string();
                refs_map.entry(asset_key).or_default().push(AssetRef {
                    post_id: rr.get("post_id"),
                    title: rr.get("title"),
                });
            }
        }

        let assets = rows
            .into_iter()
            .map(|row| {
                let id: String = row.get::<_, uuid::Uuid>("id").to_string();
                let refs = refs_map.remove(&id).unwrap_or_default();
                AssetDto {
                    asset: Asset {
                        id,
                        path: row.get("path"),
                        filename: row.get("filename"),
                        mime: row.get("mime"),
                        size_bytes: row.get("size_bytes"),
                        width: row.get("width"),
                        height: row.get("height"),
                        alt: row.get("alt"),
                        created_at: row.get("created_at"),
                    },
                    ref_count: row.get("ref_count"),
                    refs,
                }
            })
            .collect();

        Ok(AssetListResponse {
            assets,
            total,
            used_count: summary.get(0),
            orphan_count: summary.get(1),
            purgeable_count: summary.get(2),
            purgeable_bytes: summary.get(3),
        })
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}
