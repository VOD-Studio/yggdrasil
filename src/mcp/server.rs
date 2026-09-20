//! MCP 服务器：rmcp `ServerHandler` 实现，组合所有工具组。
//!
//! 装配模式（rmcp `tool_router` 宏支持的「命名路由 + 组合」）：
//! - 每个工具组在各自文件（`tools/<x>.rs`）里用
//!   `#[tool_router(router = <x>_router, vis = "pub")] impl YggMcpServer { ... }`
//!   生成一个返回 `ToolRouter<YggMcpServer>` 的公开函数。
//! - 本文件定义 `YggMcpServer` 结构体，并在 `#[tool_handler(router = ...)]`
//!   里把所有路由用 `+` 合并成单一 `ServerHandler`。
//!
//! 所有工具都在 `YggMcpServer` 上 `impl`，故 `ToolRouter` 的类型参数一致，
//! 可用 `+`（`Add` for `ToolRouter<S>`）合并。鉴权与作用域校验在各工具内部
//! 经 `Extension<http::request::Parts>` 读取 `McpPrincipal` 完成。

use rmcp::{tool_handler, ServerHandler};

/// MCP 服务器实例（无状态：每个请求由 service_factory 新建一份）。
///
/// 共享状态（DB 连接等）通过 `get_conn()` 全局池获取，无需在实例里持有。
/// 工具方法分散在各 `tools/*.rs` 的 `impl YggMcpServer` 块里。
#[derive(Clone, Default)]
pub struct YggMcpServer;

impl YggMcpServer {
    /// 组合所有工具组的路由表。新增工具组时在此追加一行。
    fn combined_router() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        YggMcpServer::read_router()
            + YggMcpServer::posts_router()
            + YggMcpServer::notes_router()
            + YggMcpServer::comments_router()
            + YggMcpServer::tags_router()
            + YggMcpServer::media_router()
            + YggMcpServer::settings_router()
            + YggMcpServer::runner_router()
    }
}

/// 单一 `ServerHandler`：工具调度委托给合并后的路由表。
/// `router = Self::combined_router()` 让 `tool_handler` 宏生成的
/// `call_tool`/`list_tools`/`get_tool` 全部走组合路由。
#[tool_handler(router = Self::combined_router())]
impl ServerHandler for YggMcpServer {
    // get_info 由宏自动生成（name/version 来自宏的默认或属性）。
    // 这里不手写 get_info，让宏用默认 ServerInfo。
}
