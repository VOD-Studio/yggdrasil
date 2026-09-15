//! MCP 客户端配置片段生成。
//!
//! 不同客户端的配置文件格式不同，这里生成多种可直接复制粘贴的片段，全部指向
//! 同一个 `/mcp` 端点、携带同一个 `Authorization: Bearer` 头。
//!
//! 形状来源：`docs/mcp-research.md` §"Client-config output format"，各客户端官方文档
//! （Claude Code / Cursor / Cline / Oh-My-Pi / OpenCode / Codex）核实。所有 JSON 都是 `serde_json`
//! 构造再 pretty-print，保证格式合法（不会手抖写错逗号/引号）。

use serde::Serialize;

/// 各客户端配置 + 一个 CLI 命令。
///
/// 所有字段是可直接复制粘贴的最终字符串（JSON 已 pretty-print，CLI 是单行 shell）。
/// `token` 形如 `ygg_...`，已嵌入各片段的 `Authorization` 头中。
#[derive(Debug, Clone, Serialize)]
pub struct ClientConfigs {
    /// Claude Code（`.mcp.json` / `~/.claude.json`）。注意 `type` 值是 `"http"`
    ///（不是 `"streamable-http"`——Claude Code 用 `"streamable-http"` 会静默失败/卡在
    /// "connecting"，2026 官方文档明确要求 `http`）。字段：`type`，`url`，`headers.Authorization`。
    pub claude_code_json: String,
    /// Cursor（`~/.cursor/mcp.json`）。与 Claude Code 的关键差异：**不带 `type` 字段**——
    /// Cursor 按远程 URL 自动识别 streamable-http；仅需 `url` + `headers.Authorization`。
    pub cursor_json: String,
    /// Cline（`cline_mcp_settings.json`）。`type: "streamableHttp"`（注意驼峰，非 `sse`），
    /// 额外带 `disabled` / `autoApprove` 字段。
    pub cline_json: String,
    /// Oh-My-Pi（项目根 `.mcp.json` / 全局 `~/.omp/agent/mcp.json` 或 `~/.mcp.json`）。
    /// omp 的协议字段是 `type: "http"`（与 Claude Code 同形），**不识别** `transport` /
    /// `streamable-http`——后者会让 omp 退化为 stdio 并因缺 `command` 字段报错丢弃。
    pub omp_json: String,
    /// OpenCode（`opencode.json` 全局 `~/.config/opencode/opencode.json` / 项目根）。
    /// 关键差异：schema 根键是 `mcp`（非 `mcpServers`），远程端点用 `type: "remote"`
    ///（非 `streamable-http`），并带 `$schema` 与 `enabled` 字段（2026 opencode.ai 官方文档）。
    pub opencode_json: String,
    /// Codex（`~/.codex/config.toml` / 项目根 `.codex/config.toml`）。
    /// Streamable HTTP 使用 `url` + `http_headers.Authorization`。
    pub codex_toml: String,
    /// 通用原始 JSON：一个 server entry 的纯净形式，供其它兼容客户端粘贴。
    pub generic_json: String,
    /// Claude Code CLI 一行命令：`claude mcp add --transport http <name> <url> --header ...`。
    pub claude_cli: String,
}

/// `mcpServers` 条目里的 server 名（客户端侧的标识，与令牌 name 无关）。
const SERVER_NAME: &str = "yggdrasil";

/// 构造 `/mcp` 端点 URL：`base_url`（无尾斜杠） + `/mcp`。
///
/// `base_url` 来自 `APP_BASE_URL` 环境变量（调用方传入），形如 `https://rua.plus`。
/// 这里只做最小拼接：去掉尾部斜杠再追加 `/mcp`，避免 `//mcp`。
fn join_mcp_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    format!("{trimmed}/mcp")
}

/// 生成各客户端配置 + CLI 命令。
///
/// - `base_url`：站点根 URL（形如 `https://rua.plus`），不带 `/mcp` 后缀。
/// - `token`：明文 bearer 令牌（形如 `ygg_...`），会被嵌入 `Authorization` 头。
pub fn generate_client_configs(base_url: &str, token: &str) -> ClientConfigs {
    let mcp_url = join_mcp_url(base_url);
    let auth_header = format!("Bearer {token}");

    // --- Claude Code：type = "http"（非 "streamable-http"，否则静默连接失败） ---
    let claude_code_json = serde_json::json!({
        "mcpServers": {
            SERVER_NAME: {
                "type": "http",
                "url": mcp_url,
                "headers": { "Authorization": auth_header }
            }
        }
    });

    // --- Cursor：不带 type 字段，按 URL 自动识别 streamable-http ---
    let cursor_json = serde_json::json!({
        "mcpServers": {
            SERVER_NAME: {
                "url": mcp_url,
                "headers": { "Authorization": auth_header }
            }
        }
    });

    // --- Cline：type = "streamableHttp"（驼峰），带 disabled / autoApprove ---
    let cline_json = serde_json::json!({
        "mcpServers": {
            SERVER_NAME: {
                "type": "streamableHttp",
                "url": mcp_url,
                "headers": { "Authorization": auth_header },
                "disabled": false,
                "autoApprove": []
            }
        }
    });
    // --- Oh-My-Pi：type = "http"（与 Claude Code 同形）。omp 不识别 transport/streamable-http，
    //     遇未知字段会退化为 stdio 并因缺 command 报错丢弃。与 Claude Code 的 JSON 体相同，
    //     差异仅在配置文件路径（见上方字段文档）。
    let omp_json = serde_json::json!({
        "mcpServers": {
            SERVER_NAME: {
                "type": "http",
                "url": mcp_url,
                "headers": { "Authorization": auth_header }
            }
        }
    });

    // --- OpenCode：根键 mcp（非 mcpServers），remote 端点用 type: "remote"（非 streamable-http） ---
    // 带 $schema 与 enabled 字段（opencode.ai 官方文档要求）。
    let opencode_json = serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "mcp": {
            SERVER_NAME: {
                "type": "remote",
                "url": mcp_url,
                "enabled": true,
                "headers": { "Authorization": auth_header }
            }
        }
    });

    // Codex 官方文档：https://developers.openai.com/codex/mcp
    // JSON 字符串的引号与转义也适用于 TOML 基本字符串，复用 serde_json 避免直接插值。
    let codex_toml = format!(
        "[mcp_servers.{SERVER_NAME}]\nurl = {}\nhttp_headers = {{ Authorization = {} }}",
        serde_json::json!(mcp_url),
        serde_json::json!(auth_header),
    );

    // --- 通用：单个 server entry 的纯净形式 ---
    let generic_json = serde_json::json!({
        "type": "streamable-http",
        "url": mcp_url,
        "headers": { "Authorization": auth_header }
    });

    // --- Claude Code CLI 一行命令 ---
    // 注意 header 值用双引号包裹（含空格）；shell 安全起见整个 header 用双引号。
    let claude_cli = format!(
        "claude mcp add --transport http {SERVER_NAME} {mcp_url} \\\n  --header \"Authorization: Bearer {token}\""
    );

    ClientConfigs {
        claude_code_json: pretty_json(&claude_code_json),
        cursor_json: pretty_json(&cursor_json),
        cline_json: pretty_json(&cline_json),
        omp_json: pretty_json(&omp_json),
        opencode_json: pretty_json(&opencode_json),
        codex_toml,
        generic_json: pretty_json(&generic_json),
        claude_cli,
    }
}

/// `serde_json::Value` → 缩进 2 空格的 pretty JSON 字符串。
fn pretty_json(v: &serde_json::Value) -> String {
    // 缩进 2 空格与各客户端文档示例一致；序列化不会失败（值来自 json! 宏）。
    serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".to_string())
}

/// 读取 `APP_BASE_URL` 环境变量作为站点根 URL；缺失时回退到本地开发地址。
///
/// 由 UI 调用方使用，保证「未设置环境变量」时仍能展示一个可用（本地）配置。
pub fn base_url_from_env() -> String {
    std::env::var("APP_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "ygg_abcdef0123456789";
    const BASE: &str = "https://rua.plus";

    #[test]
    fn join_url_handles_trailing_slash() {
        assert_eq!(join_mcp_url("https://rua.plus/"), "https://rua.plus/mcp");
        assert_eq!(join_mcp_url("https://rua.plus"), "https://rua.plus/mcp");
        assert_eq!(join_mcp_url("https://rua.plus///"), "https://rua.plus/mcp");
    }

    #[test]
    fn claude_code_json_is_valid_and_carries_bearer() {
        let cfg = generate_client_configs(BASE, TOKEN);
        let v: serde_json::Value = serde_json::from_str(&cfg.claude_code_json).unwrap();
        assert_eq!(
            v["mcpServers"]["yggdrasil"]["headers"]["Authorization"],
            format!("Bearer {TOKEN}")
        );
        assert_eq!(v["mcpServers"]["yggdrasil"]["type"], "http"); // 非 "streamable-http"（会静默失败）
        assert_eq!(v["mcpServers"]["yggdrasil"]["url"], "https://rua.plus/mcp");
    }

    #[test]
    fn cursor_json_has_no_type_field() {
        let cfg = generate_client_configs(BASE, TOKEN);
        let v: serde_json::Value = serde_json::from_str(&cfg.cursor_json).unwrap();
        let entry = &v["mcpServers"]["yggdrasil"];
        // Cursor 按 URL 自动识别远程端点，不带 type 字段。
        assert!(entry.get("type").is_none(), "cursor 配置不应含 type 字段");
        assert_eq!(entry["url"], "https://rua.plus/mcp");
        assert_eq!(entry["headers"]["Authorization"], format!("Bearer {TOKEN}"));
    }

    #[test]
    fn cline_json_uses_streamable_http_camelcase_and_extra_fields() {
        let cfg = generate_client_configs(BASE, TOKEN);
        let v: serde_json::Value = serde_json::from_str(&cfg.cline_json).unwrap();
        let entry = &v["mcpServers"]["yggdrasil"];
        assert_eq!(entry["type"], "streamableHttp"); // 驼峰，非 streamable-http
        assert_eq!(entry["disabled"], false);
        assert_eq!(entry["autoApprove"], serde_json::json!([]));
        assert_eq!(entry["headers"]["Authorization"], format!("Bearer {TOKEN}"));
    }

    #[test]
    fn generic_json_is_bare_entry() {
        let cfg = generate_client_configs(BASE, TOKEN);
        let v: serde_json::Value = serde_json::from_str(&cfg.generic_json).unwrap();
        assert!(
            v.get("mcpServers").is_none(),
            "generic 应是单个 entry，无 mcpServers 外层"
        );
        assert_eq!(v["type"], "streamable-http");
        assert_eq!(v["url"], "https://rua.plus/mcp");
    }

    #[test]
    fn omp_json_uses_type_http_not_transport() {
        let cfg = generate_client_configs(BASE, TOKEN);
        let v: serde_json::Value = serde_json::from_str(&cfg.omp_json).unwrap();
        let entry = &v["mcpServers"]["yggdrasil"];
        // omp 协议字段是 type: "http"（与 Claude Code 同形）。
        assert_eq!(entry["type"], "http");
        // 不应含 transport 字段——会让 omp 退化为 stdio 报错。
        assert!(
            entry.get("transport").is_none(),
            "omp 配置不应含 transport 字段"
        );
        assert_eq!(entry["url"], "https://rua.plus/mcp");
        assert_eq!(entry["headers"]["Authorization"], format!("Bearer {TOKEN}"));
    }

    #[test]
    fn opencode_json_uses_mcp_root_key_and_remote_type() {
        let cfg = generate_client_configs(BASE, TOKEN);
        let v: serde_json::Value = serde_json::from_str(&cfg.opencode_json).unwrap();
        // 关键差异：根键是 mcp（非 mcpServers）。
        assert!(
            v.get("mcpServers").is_none(),
            "opencode 配置不应含 mcpServers 键"
        );
        let entry = &v["mcp"]["yggdrasil"];
        // 远程端点用 type: "remote"（非 streamable-http）。
        assert_eq!(entry["type"], "remote");
        assert_eq!(entry["enabled"], true);
        assert_eq!(v["$schema"], "https://opencode.ai/config.json");
        assert_eq!(entry["url"], "https://rua.plus/mcp");
        assert_eq!(entry["headers"]["Authorization"], format!("Bearer {TOKEN}"));
    }

    #[test]
    fn claude_cli_one_liner_contains_url_and_header() {
        let cfg = generate_client_configs(BASE, TOKEN);
        assert!(cfg.claude_cli.contains("claude mcp add --transport http"));
        assert!(cfg.claude_cli.contains("https://rua.plus/mcp"));
        assert!(cfg.claude_cli.contains(&format!("Bearer {TOKEN}")));
    }

    #[test]
    fn codex_toml_carries_endpoint_and_escaped_bearer() {
        let cfg = generate_client_configs("https://rua.plus/", "ygg_\"test\\token\n");
        assert_eq!(
            cfg.codex_toml,
            "[mcp_servers.yggdrasil]\nurl = \"https://rua.plus/mcp\"\nhttp_headers = { Authorization = \"Bearer ygg_\\\"test\\\\token\\n\" }"
        );
    }

    #[test]
    fn all_json_is_pretty_indented() {
        let cfg = generate_client_configs(BASE, TOKEN);
        for s in [
            &cfg.claude_code_json,
            &cfg.cursor_json,
            &cfg.cline_json,
            &cfg.omp_json,
            &cfg.opencode_json,
            &cfg.generic_json,
        ] {
            assert!(s.contains('\n'), "JSON 应是 pretty-printed: {s}");
            assert!(s.contains("  "), "JSON 应含 2 空格缩进: {s}");
        }
    }
}
