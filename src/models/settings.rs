//! 回收站与站点配置模型。

/// 默认保留天数（天）。
pub const DEFAULT_RETENTION_DAYS: i32 = 30;
/// 默认不启用自动清理。
pub const DEFAULT_AUTO_PURGE_ENABLED: bool = false;
/// 保留天数下限（天）。
#[cfg(feature = "server")]
pub const MIN_RETENTION_DAYS: i32 = 1;
/// 保留天数上限（天）。防止误填超大值导致永不清理。
#[cfg(feature = "server")]
pub const MAX_RETENTION_DAYS: i32 = 365;

/// 回收站配置。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TrashSettings {
    /// 是否启用自动定时清理。
    pub auto_purge_enabled: bool,
    /// 已删除文章保留天数，超过后被后台任务物理删除。
    pub retention_days: i32,
}

impl Default for TrashSettings {
    fn default() -> Self {
        Self {
            auto_purge_enabled: DEFAULT_AUTO_PURGE_ENABLED,
            retention_days: DEFAULT_RETENTION_DAYS,
        }
    }
}

impl TrashSettings {
    /// 将保留天数钳制到合法范围 [MIN, MAX]。
    #[cfg(feature = "server")]
    pub fn clamp_retention(days: i32) -> i32 {
        days.clamp(MIN_RETENTION_DAYS, MAX_RETENTION_DAYS)
    }
}

/// 默认 GitHub 链接：空字符串表示不展示页脚图标。
pub const DEFAULT_SITE_GITHUB_URL: &str = "";
/// GitHub 链接最大长度（字符），防止滥用。
pub const MAX_SITE_GITHUB_URL_LEN: usize = 500;

/// 站点公开配置（前台展示用，所有访客可见）。
///
/// 目前仅包含页脚 GitHub 链接；后续可在此结构上扩展更多公开站点配置。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SiteSettings {
    /// GitHub 仓库链接，空字符串表示不展示页脚图标。
    pub github_url: String,
}

impl Default for SiteSettings {
    fn default() -> Self {
        Self {
            github_url: DEFAULT_SITE_GITHUB_URL.to_string(),
        }
    }
}

impl SiteSettings {
    /// 规范化 GitHub 链接：trim、截断、补全 scheme。
    ///
    /// - 空串（trim 后）→ 返回空串，表示未配置。
    /// - 已带 `http://` / `https://` → 原样保留（截断到上限）。
    /// - 否则在前面补 `https://`；这同时把 `javascript:` / `data:` 等危险 scheme
    ///   变成无效 URL（`https://javascript:...`），杜绝通过页脚 `href` 注入脚本。
    pub fn normalize_github_url(url: &str) -> String {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        let limited: String = trimmed.chars().take(MAX_SITE_GITHUB_URL_LEN).collect();
        if limited.starts_with("http://") || limited.starts_with("https://") {
            limited
        } else {
            format!("https://{limited}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_disabled_30_days() {
        let s = TrashSettings::default();
        assert!(!s.auto_purge_enabled);
        assert_eq!(s.retention_days, 30);
    }

    #[test]
    #[cfg(feature = "server")]
    fn clamp_retention_keeps_valid() {
        assert_eq!(TrashSettings::clamp_retention(7), 7);
        assert_eq!(TrashSettings::clamp_retention(30), 30);
    }

    #[test]
    #[cfg(feature = "server")]
    fn clamp_retention_clamps_below_min() {
        assert_eq!(TrashSettings::clamp_retention(0), MIN_RETENTION_DAYS);
        assert_eq!(TrashSettings::clamp_retention(-5), MIN_RETENTION_DAYS);
    }

    #[test]
    #[cfg(feature = "server")]
    fn clamp_retention_clamps_above_max() {
        assert_eq!(TrashSettings::clamp_retention(366), MAX_RETENTION_DAYS);
        assert_eq!(TrashSettings::clamp_retention(i32::MAX), MAX_RETENTION_DAYS);
    }

    #[test]
    #[cfg(feature = "server")]
    fn clamp_retention_boundary() {
        assert_eq!(
            TrashSettings::clamp_retention(MIN_RETENTION_DAYS),
            MIN_RETENTION_DAYS
        );
        assert_eq!(
            TrashSettings::clamp_retention(MAX_RETENTION_DAYS),
            MAX_RETENTION_DAYS
        );
    }

    #[test]
    fn site_settings_default_empty() {
        let s = SiteSettings::default();
        assert_eq!(s.github_url, "");
    }

    #[test]
    fn normalize_github_url_empty() {
        assert_eq!(SiteSettings::normalize_github_url(""), "");
        assert_eq!(SiteSettings::normalize_github_url("   "), "");
    }

    #[test]
    fn normalize_github_url_prepends_https() {
        assert_eq!(
            SiteSettings::normalize_github_url("github.com/DefectingCat/yggdrasil"),
            "https://github.com/DefectingCat/yggdrasil"
        );
    }

    #[test]
    fn normalize_github_url_keeps_scheme() {
        assert_eq!(
            SiteSettings::normalize_github_url("https://github.com/foo"),
            "https://github.com/foo"
        );
        assert_eq!(
            SiteSettings::normalize_github_url("http://example.com"),
            "http://example.com"
        );
    }

    #[test]
    fn normalize_github_url_neutralizes_dangerous_scheme() {
        // javascript:/data: 不以 http(s) 开头 → 补 https:// 变成无效 URL，无法注入脚本。
        assert_eq!(
            SiteSettings::normalize_github_url("javascript:alert(1)"),
            "https://javascript:alert(1)"
        );
    }

    #[test]
    fn normalize_github_url_trims_and_truncates() {
        assert_eq!(
            SiteSettings::normalize_github_url("  github.com/x  "),
            "https://github.com/x"
        );
        let long = "a".repeat(MAX_SITE_GITHUB_URL_LEN + 50);
        let normalized = SiteSettings::normalize_github_url(&long);
        // 补的 https:// 前缀不计入截断后的主体长度。
        assert_eq!(normalized.len(), "https://".len() + MAX_SITE_GITHUB_URL_LEN);
    }
}
