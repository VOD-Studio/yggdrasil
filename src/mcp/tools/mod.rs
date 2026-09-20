//! MCP 服务器工具集。
//!
//! 工具按作用域分组到子模块：
//! - `read`：查询/知识库（search_posts/get_post/list_tags）。
//! - `posts`/`comments`/`tags`：写操作（write 作用域）。
//! - `media`：上传（write）与素材管理（admin）。
//! - `settings`/`runner`：管理操作（admin 作用域）。
//!
//! 每个子模块用 `#[tool_router]` 实现一个工具组结构体；`server.rs` 在最终装配时
//! 把它们组合成单一 `ServerHandler`（Main 负责 wire-up）。

#[cfg(feature = "server")]
pub mod comments;
#[cfg(feature = "server")]
pub(super) mod common;
#[cfg(feature = "server")]
pub mod media;
pub mod notes;
#[cfg(feature = "server")]
pub mod posts;
#[cfg(feature = "server")]
pub mod read;
#[cfg(feature = "server")]
pub mod runner;
#[cfg(feature = "server")]
pub mod settings;
#[cfg(feature = "server")]
pub mod tags;
