//! MCP 媒体工具：write 上传图片，admin 管理素材。
//!
//! Option B 的第三通道：LLM 工具只收 `url: String`（JSON-RPC 纯文本），服务端
//! 按 SSRF 防护抓取二进制，再走 [`crate::api::upload::process_image_upload`] 共享
//! 入库流水线。**二进制从不进 JSON-RPC**——彻底绕开 rmcp 4MiB 请求体上限与
//! base64 的 33% 膨胀 + 上下文窗口烧灼。
//!
//! 另有第二通道 `POST /api/mcp/upload`（bearer multipart）供 host/shell 直接 POST
//! 二进制（如 Claude Code 的 Bash+curl）；两条通道共用同一入库流水线。
//!
//! SSRF 防护（多层纵深）见 [`crate::api::url_fetch`]：强制 https、解析即锁 IP
//! 杜绝 DNS rebinding、禁重定向、流式体积上限、超时。
//!
//! 本模块仅 `feature = "server"` 编译。

#![cfg(feature = "server")]

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{schemars, tool, tool_router, ErrorData as McpError};
use serde::Deserialize;

use super::common::{internal, ok_json, require_admin, require_scope};
use crate::models::asset::{AssetFilter, AssetSort};
use crate::models::mcp_token::TokenScope;

#[tool_router(router = media_router, vis = "pub")]
impl crate::mcp::server::YggMcpServer {
    /// 从一个图片 URL 抓取并入库（服务端转 WebP 若更小），返回可直接嵌入
    /// Markdown 正文的 `/uploads/...` URL。要求 write 作用域。
    ///
    /// 仅接受 `https://` URL；服务端做 SSRF 防护（私网/回环/保留段拒绝、
    /// DNS 锁定防 rebinding、禁重定向、体积上限）。二进制不经 JSON-RPC。
    #[tool(
        description = "从图片 URL 抓取并入库（服务端转 WebP 若更小），返回 asset_id、alt 和 /uploads/... URL（可直接用于 Markdown 正文 img），提供 alt 时保存到素材。仅接受 https:// URL，支持 JPEG/PNG/GIF/WebP。二进制不经 JSON-RPC。"
    )]
    async fn upload_media(
        &self,
        Parameters(p): Parameters<UploadMediaParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let _principal = require_scope(&parts, "upload_media", TokenScope::Write)?;

        // SSRF 防护抓取 + 共享入库流水线。
        let outcome = crate::api::url_fetch::fetch_and_ingest(&p.url, p.alt)
            .await
            .map_err(|e| match e {
                crate::api::url_fetch::FetchError::Invalid(msg)
                | crate::api::url_fetch::FetchError::BadStatus(msg) => {
                    McpError::invalid_request(msg, None)
                }
                crate::api::url_fetch::FetchError::TooLarge => McpError::invalid_request(
                    format!(
                        "文件超过大小限制（{} bytes）",
                        crate::utils::server::MAX_FILE_SIZE
                    ),
                    None,
                ),
                crate::api::url_fetch::FetchError::Fetch(ctx) => internal(ctx, "url fetch"),
            })?;

        tracing::info!(
            "MCP media uploaded via URL: {} ({}x{}, reused={})",
            outcome.url,
            outcome.width,
            outcome.height,
            outcome.reused
        );

        ok_json(UploadResult {
            success: true,
            url: outcome.url,
            asset_id: outcome.asset_id,
            alt: outcome.alt,
            reused: outcome.reused,
            width: outcome.width,
            height: outcome.height,
            mime: outcome.mime,
        })
    }
    #[tool(
        description = "分页查询素材，每页 60 张，返回元数据和文章/评论/头像引用明细。query 搜索文件名或 alt；filter 为 All/Used/Orphan，sort 为 CreatedDesc/SizeDesc，page 从 1 开始。图片 URL 为 /uploads/ 加 path。需要 admin 作用域。"
    )]
    async fn list_assets(
        &self,
        Parameters(p): Parameters<ListAssetsParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        require_admin(&parts, "list_assets")?;
        let result = crate::api::assets::list::list_assets_impl(p.filter, p.query, p.sort, p.page)
            .await
            .map_err(|e| internal(e, "list_assets"))?;
        ok_json(result)
    }

    #[tool(description = "修改素材 alt，空白清除；不回写已有文章 HTML。需要 admin 作用域。")]
    async fn update_asset_alt(
        &self,
        Parameters(p): Parameters<UpdateAssetAltParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        require_admin(&parts, "update_asset_alt")?;
        let result = crate::api::assets::delete::update_asset_alt_impl(p.id, p.alt)
            .await
            .map_err(|e| internal(e, "update_asset_alt"))?;
        ok_json(result)
    }

    #[tool(
        description = "永久删除一张无引用素材（文件、记录及缓存）。文章（含草稿与回收站）、存活评论或头像引用中的素材拒绝删除。需要 admin 作用域。"
    )]
    async fn delete_asset(
        &self,
        Parameters(p): Parameters<AssetIdParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        require_admin(&parts, "delete_asset")?;
        let result = crate::api::assets::delete::delete_asset_impl(p.id)
            .await
            .map_err(|e| internal(e, "delete_asset"))?;
        ok_json(result)
    }

    #[tool(
        description = "批量永久删除素材，跳过被引用项，返回删除、跳过、失败统计；每次 1..=100 个 id。需要 admin 作用域。"
    )]
    async fn batch_delete_assets(
        &self,
        Parameters(p): Parameters<AssetIdsParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        require_admin(&parts, "batch_delete_assets")?;
        if p.ids.is_empty() || p.ids.len() > 100 {
            return Err(McpError::invalid_params(
                "ids must contain 1..=100 entries",
                None,
            ));
        }

        let result = crate::api::assets::delete::batch_delete_assets_impl(p.ids)
            .await
            .map_err(|e| internal(e, "batch_delete_assets"))?;
        ok_json(result)
    }

    #[tool(
        description = "永久清理无引用且上传超过 7 天的素材。可先用 list_assets 查看 purgeable_count 和 purgeable_bytes。需要 admin 作用域。"
    )]
    async fn purge_orphan_assets(
        &self,
        Parameters(_p): Parameters<EmptyMediaParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        require_admin(&parts, "purge_orphan_assets")?;
        let result = crate::api::assets::delete::purge_orphan_assets_impl()
            .await
            .map_err(|e| internal(e, "purge_orphan_assets"))?;
        ok_json(result)
    }

    #[tool(
        description = "扫描 uploads 目录重建素材索引及文章引用，保留已有 alt 和文件名，移除文件已消失的记录。需要 admin 作用域。"
    )]
    async fn rebuild_assets_index(
        &self,
        Parameters(_p): Parameters<EmptyMediaParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        require_admin(&parts, "rebuild_assets_index")?;
        let result = crate::api::assets::rebuild::rebuild_assets_index_impl()
            .await
            .map_err(|e| internal(e, "rebuild_assets_index"))?;
        ok_json(result)
    }
}

// ---------------------------------------------------------------------------
// 参数与输出结构
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UploadMediaParams {
    /// 图片的 https URL（服务端抓取，二进制不经 JSON-RPC）。
    pub url: String,
    /// 保存素材 alt；重复上传时省略保留原值，空白清除，不回写已有文章。
    #[serde(default)]
    pub alt: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct UploadResult {
    success: bool,
    asset_id: String,
    alt: Option<String>,
    url: String,
    reused: bool,
    width: u32,
    height: u32,
    mime: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListAssetsParams {
    #[serde(default)]
    pub filter: AssetFilter,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub sort: AssetSort,
    #[serde(default = "first_page")]
    pub page: i32,
}

fn first_page() -> i32 {
    1
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateAssetAltParams {
    pub id: String,
    pub alt: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AssetIdParams {
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AssetIdsParams {
    pub ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EmptyMediaParams {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{auth::McpPrincipal, server::YggMcpServer};

    fn parts(scope: Option<TokenScope>) -> http::request::Parts {
        let (mut parts, _) = http::Request::new(()).into_parts();
        if let Some(scope) = scope {
            parts.extensions.insert(McpPrincipal {
                user_id: 1,
                scope,
                token_id: "test".into(),
            });
        }
        parts
    }

    #[tokio::test]
    async fn management_rejects_missing_read_and_write_principals_before_io() {
        let server = YggMcpServer;
        for scope in [None, Some(TokenScope::Read), Some(TokenScope::Write)] {
            let list = serde_json::from_str::<ListAssetsParams>("{}").unwrap();
            assert!(server
                .list_assets(Parameters(list), Extension(parts(scope)))
                .await
                .is_err());
            assert!(server
                .update_asset_alt(
                    Parameters(UpdateAssetAltParams {
                        id: "invalid".into(),
                        alt: "alt".into()
                    }),
                    Extension(parts(scope))
                )
                .await
                .is_err());
            assert!(server
                .delete_asset(
                    Parameters(AssetIdParams {
                        id: "invalid".into()
                    }),
                    Extension(parts(scope))
                )
                .await
                .is_err());
            assert!(server
                .batch_delete_assets(
                    Parameters(AssetIdsParams { ids: vec![] }),
                    Extension(parts(scope))
                )
                .await
                .is_err());
            assert!(server
                .purge_orphan_assets(Parameters(EmptyMediaParams {}), Extension(parts(scope)))
                .await
                .is_err());
            assert!(server
                .rebuild_assets_index(Parameters(EmptyMediaParams {}), Extension(parts(scope)))
                .await
                .is_err());
        }
    }

    #[tokio::test]
    async fn batch_delete_rejects_empty_and_oversized_batches_before_io() {
        for ids in [vec![], vec!["id".into(); 101]] {
            let err = YggMcpServer
                .batch_delete_assets(
                    Parameters(AssetIdsParams { ids }),
                    Extension(parts(Some(TokenScope::Admin))),
                )
                .await
                .unwrap_err();
            assert!(err.message.contains("1..=100"));
        }
    }

    #[test]
    fn media_router_exposes_upload_and_management_tools() {
        let tools = YggMcpServer::media_router().list_all();
        for name in [
            "upload_media",
            "list_assets",
            "update_asset_alt",
            "delete_asset",
            "batch_delete_assets",
            "purge_orphan_assets",
            "rebuild_assets_index",
        ] {
            assert!(tools.iter().any(|tool| tool.name == name), "missing {name}");
        }
        let list = tools
            .iter()
            .find(|tool| tool.name == "list_assets")
            .unwrap();
        let schema = serde_json::to_string(&list.input_schema).unwrap();
        assert!(schema.contains("Orphan"));
        assert!(schema.contains("SizeDesc"));
    }

    #[test]
    fn list_defaults_and_filters_match_the_schema() {
        let params: ListAssetsParams = serde_json::from_str("{}").unwrap();
        assert_eq!(params.page, 1);
        assert_eq!(params.filter, AssetFilter::All);
        assert_eq!(params.sort, AssetSort::CreatedDesc);
        assert!(params.query.is_empty());
        assert!(serde_json::from_str::<ListAssetsParams>(r#"{"filter":"invalid"}"#).is_err());
    }
}
