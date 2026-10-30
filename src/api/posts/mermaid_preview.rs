//! 后台编辑器的 Mermaid 预览：与文章保存时同一套服务端渲染。

use crate::api::error::ApiError;
use leptos::prelude::*;

#[cfg(feature = "server")]
use super::helpers::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::error::AppError;

/// 预览源码长度上限（字节）。
#[cfg(feature = "server")]
const MAX_SOURCE_LEN: usize = 20_000;

/// 渲染一张图，返回浅色、深色两份 SVG 的 HTML（`mermaid-light` / `mermaid-dark` 两个 div）。
#[server(RenderMermaidPreview, endpoint = "render_mermaid_preview", input = leptos::server_fn::codec::Json)]
pub async fn render_mermaid_preview(source: String) -> Result<String, ApiError> {
    let _user = get_current_admin_user().await?;
    #[cfg(feature = "server")]
    {
        if source.len() > MAX_SOURCE_LEN {
            return Err(AppError::BadRequest("图表源码过长".into()).into());
        }
        let rendered = tokio::task::spawn_blocking(move || {
            crate::api::markdown::mermaid::render_diagram(&source, 0)
        })
        .await
        .map_err(|_| AppError::Internal("图表渲染任务失败"))?;
        rendered.map_err(|e| AppError::BadRequest(e).into())
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(String::new())
    }
}
