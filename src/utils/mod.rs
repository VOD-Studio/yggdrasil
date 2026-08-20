//! 通用工具函数子模块。
//!
//! - `comment_storage`：评论草稿 localStorage 持久化（WASM 端）。
//! - `html`：HTML 转义（两端通用）。
//! - `js`：WASM 端调用 `window.__init*` 可选全局函数（仅 wasm32）。
//! - `text`：Markdown / 纯文本处理（仅 `server` feature）。
//! - `time`：跨平台时间/睡眠工具（WASM 与原生异步版本）。
//! - `web_upload`：multipart 文件上传 fetch 助手（仅 wasm32）。

/// 评论草稿 localStorage 持久化（仅在 WASM 端实际读写）。
pub mod comment_storage;
/// HTML 转义工具（前端后端通用）。
pub mod html;
/// WASM 端 JS 全局函数调用工具（仅 wasm32 编译）。
#[cfg(target_arch = "wasm32")]
pub mod js;
/// 服务端共享常量与工具（hash、正则、上限）。
#[cfg(feature = "server")]
pub mod server;
/// Markdown / 纯文本处理工具。
#[cfg(feature = "server")]
pub mod text;
/// 跨平台时间/睡眠工具。
pub mod time;
/// WASM 端 multipart 上传助手（仅 wasm32 编译）。
#[cfg(target_arch = "wasm32")]
pub mod web_upload;

/// 字节数 → 人类可读字符串（如 `1.2 MB`）。
///
/// 全项目唯一实现：素材管理页、上传列表与系统管理各 tab 共用。
/// 按 1024 进位取到 TB；不足 1 KB 时原样输出整数 B，其余保留一位小数。
pub fn format_bytes(bytes: i64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size.abs() >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::format_bytes;

    #[test]
    fn bytes_under_1k_render_as_integer() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn kilo_and_mega_render_with_one_decimal() {
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1_258_291), "1.2 MB");
    }

    #[test]
    fn giga_and_tera_render_with_one_decimal() {
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
        assert_eq!(format_bytes(3 * 1024_i64.pow(4)), "3.0 TB");
    }

    #[test]
    fn values_stop_at_tb() {
        // 超过 TB 后继续按 TB 表示，不进位到不存在的单位。
        assert_eq!(format_bytes(2048 * 1024_i64.pow(4)), "2048.0 TB");
    }
}
