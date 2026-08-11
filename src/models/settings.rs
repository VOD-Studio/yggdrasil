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

// ============================================================================
// 自动备份配置
// ============================================================================

/// 默认不启用自动备份。
pub const DEFAULT_BACKUP_AUTO_ENABLED: bool = false;
/// 默认每天 04:00 (UTC) 执行（服务器低峰）。
pub const DEFAULT_BACKUP_TIME_UTC: &str = "04:00";
/// 默认保留最近 30 份自动备份。
pub const DEFAULT_BACKUP_RETENTION_COUNT: i32 = 30;
/// 默认备份产物包含 uploads 素材包。
pub const DEFAULT_BACKUP_INCLUDE_UPLOADS: bool = true;
/// 保留份数下限。
#[cfg(feature = "server")]
pub const MIN_BACKUP_RETENTION_COUNT: i32 = 1;
/// 保留份数上限。防止误填超大值导致磁盘无限增长。
#[cfg(feature = "server")]
pub const MAX_BACKUP_RETENTION_COUNT: i32 = 365;

/// 自动备份配置。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BackupSettings {
    /// 是否启用每天定时备份。
    pub auto_enabled: bool,
    /// 每天执行时间（UTC，"HH:MM"）。面板负责浏览器本地时区转换。
    pub time_utc: String,
    /// 自动备份保留份数，超出后最旧的连配对 uploads 包一起删除。
    pub retention_count: i32,
    /// 备份产物是否包含 uploads 打包（tar.gz，排除可重建的 .cache）。
    pub include_uploads: bool,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            auto_enabled: DEFAULT_BACKUP_AUTO_ENABLED,
            time_utc: DEFAULT_BACKUP_TIME_UTC.to_string(),
            retention_count: DEFAULT_BACKUP_RETENTION_COUNT,
            include_uploads: DEFAULT_BACKUP_INCLUDE_UPLOADS,
        }
    }
}

impl BackupSettings {
    /// 将保留份数钳制到合法范围 [MIN, MAX]。
    #[cfg(feature = "server")]
    pub fn clamp_retention(count: i32) -> i32 {
        count.clamp(MIN_BACKUP_RETENTION_COUNT, MAX_BACKUP_RETENTION_COUNT)
    }

    /// 校验 "HH:MM" 是否合法（env 播种等需要严格拒绝脏输入的场景用）。
    #[cfg(feature = "server")]
    pub fn is_valid_time_utc(s: &str) -> bool {
        parse_hhmm(s).is_some()
    }

    /// 规范化 "HH:MM"：容忍单数字小时/分钟与前后空白，输出零填充；
    /// 非法输入回退默认时间（面板/调度永不因脏数据停摆）。
    #[cfg(feature = "server")]
    pub fn normalize_time_utc(s: &str) -> String {
        match parse_hhmm(s) {
            Some((h, m)) => format!("{h:02}:{m:02}"),
            None => DEFAULT_BACKUP_TIME_UTC.to_string(),
        }
    }

    /// 计算启用时下一次执行时刻（UTC）。time_utc 非法时返回 None。
    #[cfg(feature = "server")]
    pub fn next_run_after(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        let (h, m) = parse_hhmm(&self.time_utc)?;
        let today = now.date_naive();
        let today_at = today.and_hms_opt(h as u32, m as u32, 0)?.and_utc();
        if today_at > now {
            Some(today_at)
        } else {
            Some(
                today
                    .succ_opt()?
                    .and_hms_opt(h as u32, m as u32, 0)?
                    .and_utc(),
            )
        }
    }
}

/// 解析 "HH:MM"（容忍单数字与空白）为 (hour, minute)；非法返回 None。
#[cfg(feature = "server")]
fn parse_hhmm(s: &str) -> Option<(u8, u8)> {
    let mut parts = s.trim().split(':');
    let h: u8 = parts.next()?.trim().parse().ok()?;
    let m: u8 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() || h > 23 || m > 59 {
        return None;
    }
    Some((h, m))
}

/// 最近一次自动备份结果（落库 settings 键，重启后可查；内存任务表 1 小时即清）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LastBackupRun {
    /// 执行时间（RFC3339）。
    pub at: String,
    /// 成败。
    pub ok: bool,
    /// 成功时的 SQL 文件名。
    pub file: Option<String>,
    /// 失败时的错误摘要。
    pub error: Option<String>,
}

/// `get_backup_settings` 的响应：设置 + 上次结果 + 下次执行时间。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BackupSettingsView {
    pub settings: BackupSettings,
    /// 尚无自动备份记录时为空。
    pub last_run: Option<LastBackupRun>,
    /// 下次计划执行（RFC3339；未启用或时间非法时为空）。
    pub next_run_at: Option<String>,
}

// ============================================================================
// 素材上传配置
// ============================================================================

/// 默认并发上传数：worker 池并发 3，张间停顿随之放大（见 asset_upload.rs），
/// 聚合速率与默认上传限流桶（2/s）对齐。
pub const DEFAULT_UPLOAD_CONCURRENCY: i32 = 3;
/// 并发数下限（1 = 退化为顺序上传）。
#[cfg(feature = "server")]
pub const MIN_UPLOAD_CONCURRENCY: i32 = 1;
/// 并发数上限。防止误填超大值瞬间打满浏览器连接与上传限流突发桶。
#[cfg(feature = "server")]
pub const MAX_UPLOAD_CONCURRENCY: i32 = 8;

/// 素材上传配置（/admin/assets 上传弹窗行为，仅 admin 可见/可改）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UploadSettings {
    /// 同时发起的上传任务数（worker 池大小）。
    pub concurrency: i32,
}

impl Default for UploadSettings {
    fn default() -> Self {
        Self {
            concurrency: DEFAULT_UPLOAD_CONCURRENCY,
        }
    }
}

impl UploadSettings {
    /// 将并发数钳制到合法范围 [MIN, MAX]。
    #[cfg(feature = "server")]
    pub fn clamp_concurrency(n: i32) -> i32 {
        n.clamp(MIN_UPLOAD_CONCURRENCY, MAX_UPLOAD_CONCURRENCY)
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

// ============================================================================
// 安全配置（即时生效）
// ============================================================================
//
// 这些设置原本经环境变量每次请求/每次登录读取（src/api/csrf.rs 的
// trusted_origin、src/auth/session.rs 的 cookie_secure、src/api/rate_limit.rs
// 的 trusted_proxy_count、src/api/auth.rs 的 max_sessions_per_user）。迁移到
// settings 表后语义不变：env 在首次部署播种 DB（ON CONFLICT DO NOTHING），
// 之后面板写入的 DB 值优先，重启不被 env 覆盖。读取走 moka 短 TTL 缓存，
// 面板保存时失效缓存——最长滞后数秒即全链路生效。

/// 默认 APP_BASE_URL：空串表示未配置（回退到 Host 头推导，仅本地安全）。
pub const DEFAULT_APP_BASE_URL: &str = "";
/// 默认不给会话 cookie 加 Secure 标志（本地 HTTP 开发需要）。
pub const DEFAULT_COOKIE_SECURE: bool = false;
/// 默认反向代理层数 0（直接对外服务）。
pub const DEFAULT_TRUSTED_PROXY_COUNT: u32 = 0;
/// 默认每用户最大并发会话数 5。
pub const DEFAULT_MAX_SESSIONS_PER_USER: u32 = 5;

/// `TRUSTED_PROXY_COUNT` 上限。超过真实代理层数会信任客户端伪造的 IP，
/// 导致限流被绕过；上限给一个宽松天花板防误填危险大值。
#[cfg(feature = "server")]
pub const MAX_TRUSTED_PROXY_COUNT: u32 = 10;
/// `MAX_SESSIONS_PER_USER` 下限（至少允许 1 个会话）。
#[cfg(feature = "server")]
pub const MIN_MAX_SESSIONS_PER_USER: u32 = 1;
/// `MAX_SESSIONS_PER_USER` 上限，防误填危险大值。
#[cfg(feature = "server")]
pub const MAX_MAX_SESSIONS_PER_USER: u32 = 100;

/// 安全配置（CSRF 可信源、cookie Secure 标志、真实 IP 提取、并发会话上限）。
///
/// 即时生效层：读取走 moka 缓存，面板保存后失效缓存，数秒内全链路生效。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SecuritySettings {
    /// 写请求 CSRF 校验的可信来源（如 `https://your-domain.example`）。
    /// 空串表示未配置，回退到 Host 头 + X-Forwarded-Proto（生产建议显式配置）。
    pub app_base_url: String,
    /// 是否给会话 cookie 加 Secure 标志（仅 HTTPS 下发送）。
    pub cookie_secure: bool,
    /// 应用前方的反向代理层数，用于从 X-Forwarded-For 提取真实客户端 IP。
    pub trusted_proxy_count: u32,
    /// 单用户最大并发会话数，超出按最旧优先淘汰。
    pub max_sessions_per_user: u32,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            app_base_url: DEFAULT_APP_BASE_URL.to_string(),
            cookie_secure: DEFAULT_COOKIE_SECURE,
            trusted_proxy_count: DEFAULT_TRUSTED_PROXY_COUNT,
            max_sessions_per_user: DEFAULT_MAX_SESSIONS_PER_USER,
        }
    }
}

impl SecuritySettings {
    /// 将代理层数钳制到合法范围 [0, MAX]。
    #[cfg(feature = "server")]
    pub fn clamp_trusted_proxy_count(n: u32) -> u32 {
        n.min(MAX_TRUSTED_PROXY_COUNT)
    }

    /// 将并发会话上限钳制到合法范围 [MIN, MAX]。
    #[cfg(feature = "server")]
    pub fn clamp_max_sessions(n: u32) -> u32 {
        n.clamp(MIN_MAX_SESSIONS_PER_USER, MAX_MAX_SESSIONS_PER_USER)
    }

    /// 规范化 APP_BASE_URL：trim；空串保留为空串（表示未配置）。
    /// 不在此补 scheme——CSRF 校验需要精确的 origin，补全反而可能失配。
    pub fn normalize_app_base_url(url: &str) -> String {
        url.trim().to_string()
    }
}

// ============================================================================
// 图片磁盘缓存配置（即时生效）
// ============================================================================
//
// 原本经环境变量在每次清理 tick 读取（src/tasks/image_cache_cleanup.rs）。
// 迁移到 settings 表后语义不变：env 首启播种，之后面板值优先。

/// 默认图片磁盘缓存上限 1024 MB。
pub const DEFAULT_IMAGE_DISK_CACHE_MAX_MB: u32 = 1024;
/// 默认图片磁盘缓存保留 168 小时（7 天）。
pub const DEFAULT_IMAGE_DISK_CACHE_MAX_AGE_HOURS: u32 = 168;
/// 磁盘缓存上限下限（MB），防误填危险小值。
#[cfg(feature = "server")]
pub const MIN_IMAGE_DISK_CACHE_MAX_MB: u32 = 1;
/// 缓存保留时长下限（小时）。
#[cfg(feature = "server")]
pub const MIN_IMAGE_DISK_CACHE_MAX_AGE_HOURS: u32 = 1;

/// 图片磁盘缓存配置（uploads/.cache/ 的容量与保留策略）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImageCacheSettings {
    /// 最大总容量（MB），超限按修改时间删最旧文件。
    pub disk_cache_max_mb: u32,
    /// 文件最大保留时长（小时），超期优先删除。
    pub disk_cache_max_age_hours: u32,
}

impl Default for ImageCacheSettings {
    fn default() -> Self {
        Self {
            disk_cache_max_mb: DEFAULT_IMAGE_DISK_CACHE_MAX_MB,
            disk_cache_max_age_hours: DEFAULT_IMAGE_DISK_CACHE_MAX_AGE_HOURS,
        }
    }
}

impl ImageCacheSettings {
    /// 将容量上限钳制到合法范围 [MIN, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_max_mb(n: u32) -> u32 {
        n.max(MIN_IMAGE_DISK_CACHE_MAX_MB)
    }

    /// 将保留时长钳制到合法范围 [MIN, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_max_age_hours(n: u32) -> u32 {
        n.max(MIN_IMAGE_DISK_CACHE_MAX_AGE_HOURS)
    }
}

/// 系统启动配置的只读快照（面板展示用）。
///
/// 这些值在进程启动时读取，无法运行时修改。密钥类仅展示是否已设置，
/// 不暴露实际值。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SystemInfo {
    /// 数据库连接串（脱敏：仅显示 host/dbname，隐藏密码）。
    pub database_url_masked: String,
    /// tracing 日志过滤器。
    pub rust_log: String,
    /// 数据库连接池大小。
    pub db_pool_size: u32,
    /// 单条 SQL 查询超时秒数。
    pub statement_timeout_secs: u64,
    /// SSR 页面缓存时长（秒）。
    pub ssr_cache_secs: u64,
    /// 响应压缩算法。
    pub compression_algorithms: String,
    /// 是否附加版本响应头。
    pub expose_version_headers: bool,
    /// Docker socket 路径。
    pub docker_socket_path: String,
    /// MCP 令牌加密主密钥是否已设置（不暴露值）。
    pub mcp_token_enc_key_set: bool,
    /// 启动迁移重试窗口（秒）。
    pub migrate_startup_timeout_secs: u64,
    /// 系统信息采样间隔（秒）。
    pub sysinfo_sample_secs: f64,
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

    // ── BackupSettings ───────────────────────────────────────────

    #[test]
    fn backup_settings_default() {
        let s = BackupSettings::default();
        assert!(!s.auto_enabled);
        assert_eq!(s.time_utc, "04:00");
        assert_eq!(s.retention_count, 30);
        assert!(s.include_uploads);
    }

    #[test]
    fn normalize_time_pads_single_digits() {
        assert_eq!(BackupSettings::normalize_time_utc("4:5"), "04:05");
        assert_eq!(BackupSettings::normalize_time_utc(" 23:59 "), "23:59");
        assert_eq!(BackupSettings::normalize_time_utc("0:00"), "00:00");
    }

    #[test]
    fn normalize_time_rejects_garbage() {
        for bad in ["", "abc", "24:00", "12:60", "1:2:3", "-1:30", "12:", ":30"] {
            assert_eq!(
                BackupSettings::normalize_time_utc(bad),
                DEFAULT_BACKUP_TIME_UTC,
                "非法时间应回退默认: {bad:?}"
            );
        }
    }

    #[test]
    #[cfg(feature = "server")]
    fn backup_clamp_retention() {
        assert_eq!(
            BackupSettings::clamp_retention(0),
            MIN_BACKUP_RETENTION_COUNT
        );
        assert_eq!(
            BackupSettings::clamp_retention(366),
            MAX_BACKUP_RETENTION_COUNT
        );
        assert_eq!(BackupSettings::clamp_retention(30), 30);
    }

    #[test]
    #[cfg(feature = "server")]
    fn next_run_same_day_when_future() {
        use chrono::{TimeZone, Utc};
        let s = BackupSettings {
            time_utc: "04:00".into(),
            ..Default::default()
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 10, 1, 30, 0).unwrap();
        let next = s.next_run_after(now).expect("合法时间必有下次执行");
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 8, 10, 4, 0, 0).unwrap());
    }

    #[test]
    #[cfg(feature = "server")]
    fn next_run_rolls_to_tomorrow_when_past() {
        use chrono::{TimeZone, Utc};
        let s = BackupSettings {
            time_utc: "04:00".into(),
            ..Default::default()
        };
        // 恰等于触发时刻也算「已过」——避免刚错过即触发两次。
        for now in [
            Utc.with_ymd_and_hms(2026, 8, 10, 4, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 8, 10, 23, 59, 59).unwrap(),
        ] {
            let next = s.next_run_after(now).expect("合法时间必有下次执行");
            assert_eq!(next, Utc.with_ymd_and_hms(2026, 8, 11, 4, 0, 0).unwrap());
        }
    }

    #[test]
    #[cfg(feature = "server")]
    fn next_run_none_on_invalid_time() {
        use chrono::{TimeZone, Utc};
        let s = BackupSettings {
            time_utc: "garbage".into(),
            ..Default::default()
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 10, 0, 0, 0).unwrap();
        assert!(s.next_run_after(now).is_none());
    }

    // ── UploadSettings ───────────────────────────────────────────

    #[test]
    fn upload_settings_default() {
        assert_eq!(UploadSettings::default().concurrency, 3);
    }

    #[test]
    #[cfg(feature = "server")]
    fn upload_clamp_concurrency() {
        assert_eq!(UploadSettings::clamp_concurrency(0), MIN_UPLOAD_CONCURRENCY);
        assert_eq!(
            UploadSettings::clamp_concurrency(-1),
            MIN_UPLOAD_CONCURRENCY
        );
        assert_eq!(UploadSettings::clamp_concurrency(9), MAX_UPLOAD_CONCURRENCY);
        assert_eq!(
            UploadSettings::clamp_concurrency(i32::MAX),
            MAX_UPLOAD_CONCURRENCY
        );
        assert_eq!(UploadSettings::clamp_concurrency(5), 5);
    }

    // ── SecuritySettings ─────────────────────────────────────────

    #[test]
    fn security_settings_default() {
        let s = SecuritySettings::default();
        assert!(s.app_base_url.is_empty());
        assert!(!s.cookie_secure);
        assert_eq!(s.trusted_proxy_count, 0);
        assert_eq!(s.max_sessions_per_user, 5);
    }

    #[test]
    #[cfg(feature = "server")]
    fn security_clamp_trusted_proxy_count() {
        assert_eq!(SecuritySettings::clamp_trusted_proxy_count(0), 0);
        assert_eq!(SecuritySettings::clamp_trusted_proxy_count(3), 3);
        assert_eq!(
            SecuritySettings::clamp_trusted_proxy_count(99),
            MAX_TRUSTED_PROXY_COUNT
        );
    }

    #[test]
    #[cfg(feature = "server")]
    fn security_clamp_max_sessions() {
        assert_eq!(
            SecuritySettings::clamp_max_sessions(0),
            MIN_MAX_SESSIONS_PER_USER
        );
        assert_eq!(SecuritySettings::clamp_max_sessions(5), 5);
        assert_eq!(
            SecuritySettings::clamp_max_sessions(u32::MAX),
            MAX_MAX_SESSIONS_PER_USER
        );
    }

    #[test]
    fn security_normalize_app_base_url_trims() {
        assert_eq!(SecuritySettings::normalize_app_base_url("  https://x.com "), "https://x.com");
        assert_eq!(SecuritySettings::normalize_app_base_url(""), "");
        assert_eq!(SecuritySettings::normalize_app_base_url("   "), "");
    }

    // ── ImageCacheSettings ───────────────────────────────────────

    #[test]
    fn image_cache_settings_default() {
        let s = ImageCacheSettings::default();
        assert_eq!(s.disk_cache_max_mb, 1024);
        assert_eq!(s.disk_cache_max_age_hours, 168);
    }

    #[test]
    #[cfg(feature = "server")]
    fn image_cache_clamp_max_mb() {
        assert_eq!(
            ImageCacheSettings::clamp_max_mb(0),
            MIN_IMAGE_DISK_CACHE_MAX_MB
        );
        assert_eq!(ImageCacheSettings::clamp_max_mb(2048), 2048);
    }

    #[test]
    #[cfg(feature = "server")]
    fn image_cache_clamp_max_age_hours() {
        assert_eq!(
            ImageCacheSettings::clamp_max_age_hours(0),
            MIN_IMAGE_DISK_CACHE_MAX_AGE_HOURS
        );
        assert_eq!(ImageCacheSettings::clamp_max_age_hours(720), 720);
    }
}
