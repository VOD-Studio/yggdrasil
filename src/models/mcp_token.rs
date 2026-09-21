//! MCP 服务器访问令牌模型。
//!
//! `mcp_tokens` 表承载管理员为 AI 客户端签发的 bearer 令牌：绑定用户、作用域、
//! 可选过期时间。明文 token 经 AES-GCM 静态加密存储（`token_enc`），可由管理员
//! 解密重查；同时存 SHA-256 哈希（`token_hash`）做每请求 O(1) 常量查找。
//!
//! 与 assets 一致：id 以 String 承载（SQL 侧 `id::text` 读出、`$1::uuid` 写入），
//! 避免把 server-only 的 uuid crate 引入 WASM 前端构建。chrono 用于两端共享。

use serde::{Deserialize, Serialize};

/// 前后端共用的令牌名称校验，按去除首尾空白后的字符数计算。
pub fn validate_token_name(name: &str) -> Result<(), &'static str> {
    let name = name.trim();
    if name.is_empty() {
        Err("请输入令牌名称，不能只包含空白字符")
    } else if name.chars().count() > 64 {
        Err("令牌名称不能超过 64 个字符")
    } else {
        Ok(())
    }
}

/// 令牌作用域：read < write < admin，支持偏序比较用于工具调度鉴权。
///
/// - `read`：仅查询已发布文章（知识库）。
/// - `write`：`read` + 文章 CRUD（含草稿）、评论、标签、媒体上传。
/// - `admin`：`write` + 站点设置、代码运行器。
///
/// 比较语义：`scope >= required` 表示该令牌有权调用要求 `required` 作用域的工具。
/// 例如 `admin` 令牌可调用 `read`/`write`/`admin` 任一工具。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum TokenScope {
    Read,
    Write,
    Admin,
}

impl TokenScope {
    /// 数据库存储的字符串形式。
    pub fn as_str(self) -> &'static str {
        match self {
            TokenScope::Read => "read",
            TokenScope::Write => "write",
            TokenScope::Admin => "admin",
        }
    }

    /// 从数据库字符串解析；非法值返回 None（调用方按业务错误处理，不走 panic）。
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "read" => Some(TokenScope::Read),
            "write" => Some(TokenScope::Write),
            "admin" => Some(TokenScope::Admin),
            _ => None,
        }
    }

    /// 数值用于偏序比较：read=1 < write=2 < admin=3。
    fn rank(self) -> u8 {
        match self {
            TokenScope::Read => 1,
            TokenScope::Write => 2,
            TokenScope::Admin => 3,
        }
    }

    /// 该令牌是否满足某工具要求的作用域（`self.rank() >= required.rank()`）。
    pub fn grants(self, required: TokenScope) -> bool {
        self.rank() >= required.rank()
    }
}

impl PartialOrd for TokenScope {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TokenScope {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank().cmp(&other.rank())
    }
}

/// mcp_tokens 表一行（不含明文；密文在 `token_enc`）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpToken {
    pub id: String,
    pub user_id: i32,
    pub name: String,
    pub scope: TokenScope,
    pub notes: NoteGrant,
    /// AES-GCM 密文 hex（nonce ‖ ct ‖ tag）。仅服务端解密使用，不向前端暴露。
    #[serde(skip)]
    pub token_enc: String,
    /// 明文 SHA-256 hex。仅服务端查找用，不向前端暴露。
    #[serde(skip)]
    pub token_hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 列表响应 DTO：不含任何密钥材料，仅展示用元数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpTokenSummary {
    pub id: String,
    pub name: String,
    pub scope: TokenScope,
    #[serde(default)]
    pub notes: NoteGrant,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<McpToken> for McpTokenSummary {
    fn from(t: McpToken) -> Self {
        Self {
            id: t.id,
            name: t.name,
            scope: t.scope,
            notes: t.notes,
            created_at: t.created_at,
            expires_at: t.expires_at,
            last_used_at: t.last_used_at,
            revoked_at: t.revoked_at,
        }
    }
}

/// 独立于文章权限的笔记授权。None 表示全部笔记本，空列表不授权任何笔记本。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteGrant {
    pub read: bool,
    pub write: bool,
    pub notebook_ids: Option<Vec<i32>>,
}

/// 签发令牌的响应：摘要 + 一次性明文（明文仅在签发/重查时返回，不持久明文）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateTokenResponse {
    #[serde(flatten)]
    pub summary: McpTokenSummary,
    /// 完整 bearer 明文，形如 `ygg_<opaque>`；客户端写入 Authorization 头。
    pub plaintext: String,
}

#[cfg(test)]
mod tests {
    #[test]
    fn token_name_requires_non_whitespace_and_at_most_64_characters() {
        for name in ["", " \t\n", "\u{3000}"] {
            assert!(super::validate_token_name(name).is_err());
        }
        for name in [" claude-code-macbook ".to_string(), "名".repeat(64)] {
            assert!(super::validate_token_name(&name).is_ok());
        }
        assert!(super::validate_token_name(&"名".repeat(65)).is_err());
        assert!(super::validate_token_name(&"a".repeat(65)).is_err());
    }

    use super::*;

    #[test]
    fn scope_ranking_grants_chain() {
        // read 仅满足 read
        assert!(TokenScope::Read.grants(TokenScope::Read));
        assert!(!TokenScope::Read.grants(TokenScope::Write));
        assert!(!TokenScope::Read.grants(TokenScope::Admin));
        // write 满足 read+write，不满足 admin
        assert!(TokenScope::Write.grants(TokenScope::Read));
        assert!(TokenScope::Write.grants(TokenScope::Write));
        assert!(!TokenScope::Write.grants(TokenScope::Admin));
        // admin 满足全部
        assert!(TokenScope::Admin.grants(TokenScope::Read));
        assert!(TokenScope::Admin.grants(TokenScope::Write));
        assert!(TokenScope::Admin.grants(TokenScope::Admin));
    }

    #[test]
    fn scope_ord_total_order() {
        assert!(TokenScope::Read < TokenScope::Write);
        assert!(TokenScope::Write < TokenScope::Admin);
        assert!(TokenScope::Admin >= TokenScope::Read);
        // 偏序完备（Ord 实现，无 PartialOrd 退化分支）
        let mut v = [TokenScope::Admin, TokenScope::Read, TokenScope::Write];
        v.sort();
        assert_eq!(v, [TokenScope::Read, TokenScope::Write, TokenScope::Admin]);
    }

    #[test]
    fn scope_db_roundtrip() {
        for s in [TokenScope::Read, TokenScope::Write, TokenScope::Admin] {
            assert_eq!(TokenScope::from_db(s.as_str()), Some(s));
        }
        assert_eq!(TokenScope::from_db("root"), None);
        assert_eq!(TokenScope::from_db(""), None);
    }

    #[test]
    fn scope_serde_lowercase() {
        let s = serde_json::to_string(&TokenScope::Admin).unwrap();
        assert_eq!(s, "\"admin\"");
        let v: TokenScope = serde_json::from_str("\"read\"").unwrap();
        assert_eq!(v, TokenScope::Read);
    }
}
