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

// ============================================================================
// 限流配置（重启生效）
// ============================================================================
//
// 原本经环境变量在首次请求时经 LazyLock 读取（src/api/rate_limit.rs 的六个
// IP 键控限流器与 GC 间隔）。迁移到 settings 表后语义为 Tier B：env 首启播种
// DB（ON CONFLICT DO NOTHING），之后面板写入的 DB 值优先。由于限流器是
// LazyLock 静态量（首次请求构造即固化），修改 DB 值需**重启进程**生效，
// 面板会标注「需重启生效」。启动时由 main.rs 将 DB 值加载进 config::RATE_LIMIT_CFG，
// rate_limit.rs 的 LazyLock 改为从 config::rate_limit() 读取。

/// 默认严格限流（注册/登录）：1 req/s，突发 5。
pub const DEFAULT_RATE_LIMIT_STRICT_PER_SEC: u32 = 1;
pub const DEFAULT_RATE_LIMIT_STRICT_BURST: u32 = 5;
/// 默认上传限流：2 req/s，突发 15。
pub const DEFAULT_RATE_LIMIT_UPLOAD_PER_SEC: u32 = 2;
pub const DEFAULT_RATE_LIMIT_UPLOAD_BURST: u32 = 15;
/// 默认图片访问限流：10 req/s，突发 50。
pub const DEFAULT_RATE_LIMIT_IMAGE_PER_SEC: u32 = 10;
pub const DEFAULT_RATE_LIMIT_IMAGE_BURST: u32 = 50;
/// 默认评论限流：1 req/s，突发 5。
pub const DEFAULT_RATE_LIMIT_COMMENT_PER_SEC: u32 = 1;
pub const DEFAULT_RATE_LIMIT_COMMENT_BURST: u32 = 5;
/// 默认代码执行限流：1 req/s，突发 3。
pub const DEFAULT_RATE_LIMIT_CODE_EXEC_PER_SEC: u32 = 1;
pub const DEFAULT_RATE_LIMIT_CODE_EXEC_BURST: u32 = 3;
/// 默认代码执行日限额：50 次/天。
pub const DEFAULT_RATE_LIMIT_CODE_EXEC_DAILY: u32 = 50;
/// 默认 unknown 桶限流：30 req/s，突发 100。
pub const DEFAULT_RATE_LIMIT_UNKNOWN_PER_SEC: u32 = 30;
pub const DEFAULT_RATE_LIMIT_UNKNOWN_BURST: u32 = 100;
/// 默认限流桶 GC 间隔：300 秒。
pub const DEFAULT_RATE_LIMIT_GC_INTERVAL_SECS: u32 = 300;

/// 所有 per_sec 字段下限：至少 1 req/s（NonZeroU32 不允许 0）。
#[cfg(feature = "server")]
pub const MIN_RATE_LIMIT_PER_SEC: u32 = 1;
/// 所有 burst 字段下限：至少允许 1 次突发。
#[cfg(feature = "server")]
pub const MIN_RATE_LIMIT_BURST: u32 = 1;
/// 日限额下限：至少允许 1 次/天。
#[cfg(feature = "server")]
pub const MIN_RATE_LIMIT_DAILY: u32 = 1;
/// GC 间隔下限（秒）：至少 1 秒。
#[cfg(feature = "server")]
pub const MIN_RATE_LIMIT_GC_INTERVAL_SECS: u32 = 1;

/// 限流配置（多级 IP 键控限流器的速率/突发/日限额与 GC 间隔）。
///
/// **重启生效层**：限流器是 LazyLock 静态量，首次请求时构造即固化；
/// 修改 DB 值需重启进程才能生效。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RateLimitSettings {
    /// 严格限流（注册/登录）每秒请求数。
    pub strict_per_sec: u32,
    /// 严格限流突发上限。
    pub strict_burst: u32,
    /// 上传限流每秒请求数。
    pub upload_per_sec: u32,
    /// 上传限流突发上限。
    pub upload_burst: u32,
    /// 图片访问限流每秒请求数。
    pub image_per_sec: u32,
    /// 图片访问限流突发上限。
    pub image_burst: u32,
    /// 评论限流每秒请求数。
    pub comment_per_sec: u32,
    /// 评论限流突发上限。
    pub comment_burst: u32,
    /// 代码执行限流每秒请求数。
    pub code_exec_per_sec: u32,
    /// 代码执行限流突发上限。
    pub code_exec_burst: u32,
    /// 代码执行日限额（次/天）。
    pub code_exec_daily: u32,
    /// unknown 桶（无法识别 IP）每秒请求数。
    pub unknown_per_sec: u32,
    /// unknown 桶突发上限。
    pub unknown_burst: u32,
    /// 限流桶 GC 间隔（秒）。
    pub gc_interval_secs: u32,
}

impl Default for RateLimitSettings {
    fn default() -> Self {
        Self {
            strict_per_sec: DEFAULT_RATE_LIMIT_STRICT_PER_SEC,
            strict_burst: DEFAULT_RATE_LIMIT_STRICT_BURST,
            upload_per_sec: DEFAULT_RATE_LIMIT_UPLOAD_PER_SEC,
            upload_burst: DEFAULT_RATE_LIMIT_UPLOAD_BURST,
            image_per_sec: DEFAULT_RATE_LIMIT_IMAGE_PER_SEC,
            image_burst: DEFAULT_RATE_LIMIT_IMAGE_BURST,
            comment_per_sec: DEFAULT_RATE_LIMIT_COMMENT_PER_SEC,
            comment_burst: DEFAULT_RATE_LIMIT_COMMENT_BURST,
            code_exec_per_sec: DEFAULT_RATE_LIMIT_CODE_EXEC_PER_SEC,
            code_exec_burst: DEFAULT_RATE_LIMIT_CODE_EXEC_BURST,
            code_exec_daily: DEFAULT_RATE_LIMIT_CODE_EXEC_DAILY,
            unknown_per_sec: DEFAULT_RATE_LIMIT_UNKNOWN_PER_SEC,
            unknown_burst: DEFAULT_RATE_LIMIT_UNKNOWN_BURST,
            gc_interval_secs: DEFAULT_RATE_LIMIT_GC_INTERVAL_SECS,
        }
    }
}

impl RateLimitSettings {
    /// 将 per_sec 字段钳制到 [MIN_RATE_LIMIT_PER_SEC, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_per_sec(n: u32) -> u32 {
        n.max(MIN_RATE_LIMIT_PER_SEC)
    }

    /// 将 burst 字段钳制到 [MIN_RATE_LIMIT_BURST, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_burst(n: u32) -> u32 {
        n.max(MIN_RATE_LIMIT_BURST)
    }

    /// 将日限额字段钳制到 [MIN_RATE_LIMIT_DAILY, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_daily(n: u32) -> u32 {
        n.max(MIN_RATE_LIMIT_DAILY)
    }

    /// 将 GC 间隔钳制到 [MIN_RATE_LIMIT_GC_INTERVAL_SECS, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_gc_interval(n: u32) -> u32 {
        n.max(MIN_RATE_LIMIT_GC_INTERVAL_SECS)
    }
}

// ============================================================================
// WebP 编码配置（需重启生效）
// ============================================================================
//
// 原本经环境变量在首启读取（src/webp.rs 的 WEBP_QUALITY / WEBP_METHOD）。
// 迁移到 settings 表后：env 首启播种 DB（ON CONFLICT DO NOTHING），之后面板值
// 优先。这些值在进程启动时烘焙进 LazyLock 静态量，改 DB 值后需重启才生效。

/// 默认 WebP 质量 85.0。
pub const DEFAULT_WEBP_QUALITY: f32 = 85.0;
/// 默认 WebP 编码方法 2。
pub const DEFAULT_WEBP_METHOD: u32 = 2;

/// `WEBP_QUALITY` 下限。
#[cfg(feature = "server")]
pub const MIN_WEBP_QUALITY: f32 = 0.0;
/// `WEBP_QUALITY` 上限。
#[cfg(feature = "server")]
pub const MAX_WEBP_QUALITY: f32 = 100.0;
/// `WEBP_METHOD` 上限。
#[cfg(feature = "server")]
pub const MAX_WEBP_METHOD: u32 = 6;

/// WebP 有损编码配置（质量与方法）。
///
/// 需重启生效层：值在进程启动时烘焙进 LazyLock，改 DB 值后需重启才生效。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WebpSettings {
    /// 质量系数，范围 0.0–100.0。
    pub quality: f32,
    /// 编码方法，范围 0–6，数值越大压缩率越高但越慢。
    pub method: u32,
}

impl Default for WebpSettings {
    fn default() -> Self {
        Self {
            quality: DEFAULT_WEBP_QUALITY,
            method: DEFAULT_WEBP_METHOD,
        }
    }
}

impl WebpSettings {
    /// 将质量钳制到合法范围 [0.0, 100.0]。
    #[cfg(feature = "server")]
    pub fn clamp_quality(q: f32) -> f32 {
        q.clamp(MIN_WEBP_QUALITY, MAX_WEBP_QUALITY)
    }

    /// 将编码方法钳制到合法范围 [0, 6]。
    #[cfg(feature = "server")]
    pub fn clamp_method(m: u32) -> u32 {
        m.min(MAX_WEBP_METHOD)
    }
}

// ============================================================================
// 图片尺寸限制配置（需重启生效）
// ============================================================================
//
// 原本经环境变量在首启读取（src/api/image.rs 的 MAX_IMAGE_DIMENSION /
// MAX_IMAGE_PIXELS / IMAGE_DIMENSIONS_CACHE_TTL_SECS）。迁移到 settings 表后：
// env 首启播种 DB，之后面板值优先。值烘焙进 LazyLock，需重启生效。

/// 默认图片单边尺寸上限 8192 像素。
pub const DEFAULT_IMAGE_MAX_DIMENSION: u32 = 8192;
/// 默认图片像素总数上限 50_000_000（约 7k×7k）。
pub const DEFAULT_IMAGE_MAX_PIXELS: u64 = 50_000_000;
/// 默认图片尺寸缓存 TTL 86400 秒（24 小时）。
pub const DEFAULT_IMAGE_DIMENSIONS_CACHE_TTL_SECS: u64 = 86400;

/// 图片单边尺寸下限（防误调到危险小值导致正常图都传不上）。
#[cfg(feature = "server")]
pub const MIN_IMAGE_MAX_DIMENSION: u32 = 512;
/// 图片像素总数下限（防误调）。
#[cfg(feature = "server")]
pub const MIN_IMAGE_MAX_PIXELS: u64 = 1_000_000;
/// 尺寸缓存 TTL 下限（秒）。
#[cfg(feature = "server")]
pub const MIN_IMAGE_DIMENSIONS_CACHE_TTL_SECS: u64 = 1;

/// 图片处理尺寸限制与尺寸缓存配置。
///
/// 需重启生效层：值在进程启动时烘焙进 LazyLock，改 DB 值后需重启才生效。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImageLimitSettings {
    /// 图片单边（宽或高）尺寸上限（像素）。
    pub max_dimension: u32,
    /// 允许处理的最大图片像素数。⚠️ 此值同时决定单图解码内存缓冲
    /// （max_alloc = pixels × 4 + 1MB），默认 50M 像素对应约 200MB/图。
    pub max_pixels: u64,
    /// 图片尺寸缓存 TTL（秒）。
    pub dimensions_cache_ttl_secs: u64,
}

impl Default for ImageLimitSettings {
    fn default() -> Self {
        Self {
            max_dimension: DEFAULT_IMAGE_MAX_DIMENSION,
            max_pixels: DEFAULT_IMAGE_MAX_PIXELS,
            dimensions_cache_ttl_secs: DEFAULT_IMAGE_DIMENSIONS_CACHE_TTL_SECS,
        }
    }
}

impl ImageLimitSettings {
    /// 将单边尺寸钳制到合法范围 [MIN, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_max_dimension(n: u32) -> u32 {
        n.max(MIN_IMAGE_MAX_DIMENSION)
    }

    /// 将像素总数钳制到合法范围 [MIN, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_max_pixels(n: u64) -> u64 {
        n.max(MIN_IMAGE_MAX_PIXELS)
    }

    /// 将尺寸缓存 TTL 钳制到合法范围 [MIN, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_dimensions_cache_ttl_secs(n: u64) -> u64 {
        n.max(MIN_IMAGE_DIMENSIONS_CACHE_TTL_SECS)
    }
}

// ============================================================================
// 代码运行器配置（需重启生效）
// ============================================================================
//
// 原本经环境变量在首启读取（src/infra/runner_config.rs 的 CODE_RUNNER_*）。
// 迁移到 settings 表后：env 首启播种 DB，之后面板值优先。值烘焙进 LazyLock，
// 需重启生效。注意 `docker_socket_path` 仍是 env-only（DOCKER_SOCKET_PATH），
// 不在此结构。

/// 默认不允许容器联网。
pub const DEFAULT_RUNNER_ALLOW_NETWORK: bool = false;
/// 默认最大并发 4。
pub const DEFAULT_RUNNER_MAX_CONCURRENT: u32 = 4;
/// 默认每任务最大 CPU 核数 2.0。
pub const DEFAULT_RUNNER_MAX_CPU_CORES: f64 = 2.0;
/// 默认每任务最大内存 1024 MB。
pub const DEFAULT_RUNNER_MAX_MEMORY_MB: u32 = 1024;
/// 默认每任务最大执行超时 30 秒。
pub const DEFAULT_RUNNER_MAX_TIMEOUT_SECS: u32 = 30;
/// 默认每任务最大输出 1048576 字节（1 MB）。
pub const DEFAULT_RUNNER_MAX_OUTPUT_BYTES: u64 = 1_048_576;
/// 默认每任务最大源码 65536 字节（64 KB）。
pub const DEFAULT_RUNNER_MAX_SOURCE_BYTES: u64 = 65_536;
/// 默认排队等待超时 30 秒。
pub const DEFAULT_RUNNER_QUEUE_TIMEOUT_SECS: u32 = 30;
/// 默认历史 task 保留 300 秒。
pub const DEFAULT_RUNNER_TASK_TTL_SECS: u32 = 300;

/// 最大并发下限（至少允许 1 个）。
#[cfg(feature = "server")]
pub const MIN_RUNNER_MAX_CONCURRENT: u32 = 1;
/// CPU 核数下限。
#[cfg(feature = "server")]
pub const MIN_RUNNER_MAX_CPU_CORES: f64 = 0.1;
/// 内存下限（MB）。
#[cfg(feature = "server")]
pub const MIN_RUNNER_MAX_MEMORY_MB: u32 = 16;
/// 超时下限（秒）。
#[cfg(feature = "server")]
pub const MIN_RUNNER_MAX_TIMEOUT_SECS: u32 = 1;
/// 最大并发上限，防误填危险大值。
#[cfg(feature = "server")]
pub const MAX_RUNNER_MAX_CONCURRENT: u32 = 64;
/// CPU 核数上限。
#[cfg(feature = "server")]
pub const MAX_RUNNER_MAX_CPU_CORES: f64 = 64.0;
/// 内存上限（MB）。
#[cfg(feature = "server")]
pub const MAX_RUNNER_MAX_MEMORY_MB: u32 = 65_536;

/// 代码运行器配置（资源限制与并发）。
///
/// 需重启生效层：值在进程启动时烘焙进 LazyLock，改 DB 值后需重启才生效。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunnerSettings {
    /// 是否允许容器联网。
    pub allow_network: bool,
    /// 最大并发任务数。
    pub max_concurrent: u32,
    /// 每任务最大 CPU 核数。
    pub max_cpu_cores: f64,
    /// 每任务最大内存（MB）。
    pub max_memory_mb: u32,
    /// 每任务最大执行超时（秒）。
    pub max_timeout_secs: u32,
    /// 每任务最大输出字节数。
    pub max_output_bytes: u64,
    /// 每任务最大源码字节数。
    pub max_source_bytes: u64,
    /// 排队等待超时（秒）。
    pub queue_timeout_secs: u32,
    /// 历史 task 保留时长（秒）。
    pub task_ttl_secs: u32,
    /// 语言白名单（逗号分隔，如 "python,node,rust"）；`None` 表示不限制（全开）。
    pub languages: Option<String>,
}

impl Default for RunnerSettings {
    fn default() -> Self {
        Self {
            allow_network: DEFAULT_RUNNER_ALLOW_NETWORK,
            max_concurrent: DEFAULT_RUNNER_MAX_CONCURRENT,
            max_cpu_cores: DEFAULT_RUNNER_MAX_CPU_CORES,
            max_memory_mb: DEFAULT_RUNNER_MAX_MEMORY_MB,
            max_timeout_secs: DEFAULT_RUNNER_MAX_TIMEOUT_SECS,
            max_output_bytes: DEFAULT_RUNNER_MAX_OUTPUT_BYTES,
            max_source_bytes: DEFAULT_RUNNER_MAX_SOURCE_BYTES,
            queue_timeout_secs: DEFAULT_RUNNER_QUEUE_TIMEOUT_SECS,
            task_ttl_secs: DEFAULT_RUNNER_TASK_TTL_SECS,
            languages: None,
        }
    }
}

impl RunnerSettings {
    /// 将最大并发钳制到合法范围 [1, 64]。
    #[cfg(feature = "server")]
    pub fn clamp_max_concurrent(n: u32) -> u32 {
        n.clamp(MIN_RUNNER_MAX_CONCURRENT, MAX_RUNNER_MAX_CONCURRENT)
    }

    /// 将 CPU 核数钳制到合法范围 [0.1, 64.0]；NaN 回退默认值。
    #[cfg(feature = "server")]
    pub fn clamp_max_cpu_cores(n: f64) -> f64 {
        if n.is_nan() {
            return DEFAULT_RUNNER_MAX_CPU_CORES;
        }
        n.clamp(MIN_RUNNER_MAX_CPU_CORES, MAX_RUNNER_MAX_CPU_CORES)
    }

    /// 将内存钳制到合法范围 [16, 65536]。
    #[cfg(feature = "server")]
    pub fn clamp_max_memory_mb(n: u32) -> u32 {
        n.clamp(MIN_RUNNER_MAX_MEMORY_MB, MAX_RUNNER_MAX_MEMORY_MB)
    }

    /// 将超时钳制到合法范围 [1, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_max_timeout_secs(n: u32) -> u32 {
        n.max(MIN_RUNNER_MAX_TIMEOUT_SECS)
    }

    /// 将输出上限钳制到合法范围 [1, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_max_output_bytes(n: u64) -> u64 {
        n.max(1)
    }

    /// 将源码上限钳制到合法范围 [1, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_max_source_bytes(n: u64) -> u64 {
        n.max(1)
    }

    /// 将排队超时钳制到合法范围 [1, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_queue_timeout_secs(n: u32) -> u32 {
        n.max(MIN_RUNNER_MAX_TIMEOUT_SECS)
    }

    /// 将 task 保留时长钳制到合法范围 [1, ∞)。
    #[cfg(feature = "server")]
    pub fn clamp_task_ttl_secs(n: u32) -> u32 {
        n.max(1)
    }

    /// 规范化语言白名单字符串：拆分、trim、转小写、去空；结果为空则返回 None。
    #[cfg(feature = "server")]
    pub fn normalize_languages(s: &str) -> Option<String> {
        let parts: Vec<String> = s
            .split(',')
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(","))
        }
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
        assert_eq!(
            SecuritySettings::normalize_app_base_url("  https://x.com "),
            "https://x.com"
        );
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

    #[test]
    fn rate_limit_default_values() {
        let s = RateLimitSettings::default();
        assert_eq!(s.strict_per_sec, 1);
        assert_eq!(s.strict_burst, 5);
        assert_eq!(s.upload_per_sec, 2);
        assert_eq!(s.upload_burst, 15);
        assert_eq!(s.image_per_sec, 10);
        assert_eq!(s.image_burst, 50);
        assert_eq!(s.comment_per_sec, 1);
        assert_eq!(s.comment_burst, 5);
        assert_eq!(s.code_exec_per_sec, 1);
        assert_eq!(s.code_exec_burst, 3);
        assert_eq!(s.code_exec_daily, 50);
        assert_eq!(s.unknown_per_sec, 30);
        assert_eq!(s.unknown_burst, 100);
        assert_eq!(s.gc_interval_secs, 300);
    }

    #[test]
    #[cfg(feature = "server")]
    fn rate_limit_clamp_per_sec() {
        assert_eq!(RateLimitSettings::clamp_per_sec(0), MIN_RATE_LIMIT_PER_SEC);
        assert_eq!(RateLimitSettings::clamp_per_sec(5), 5);
    }

    #[test]
    #[cfg(feature = "server")]
    fn rate_limit_clamp_burst() {
        assert_eq!(RateLimitSettings::clamp_burst(0), MIN_RATE_LIMIT_BURST);
        assert_eq!(RateLimitSettings::clamp_burst(15), 15);
    }

    #[test]
    #[cfg(feature = "server")]
    fn rate_limit_clamp_daily() {
        assert_eq!(RateLimitSettings::clamp_daily(0), MIN_RATE_LIMIT_DAILY);
        assert_eq!(RateLimitSettings::clamp_daily(50), 50);
    }

    #[test]
    #[cfg(feature = "server")]
    fn rate_limit_clamp_gc_interval() {
        assert_eq!(
            RateLimitSettings::clamp_gc_interval(0),
            MIN_RATE_LIMIT_GC_INTERVAL_SECS
        );
        assert_eq!(RateLimitSettings::clamp_gc_interval(300), 300);
    }
    // ── WebpSettings ─────────────────────────────────────────────

    #[test]
    fn webp_settings_default() {
        let s = WebpSettings::default();
        assert_eq!(s.quality, 85.0);
        assert_eq!(s.method, 2);
    }

    #[test]
    #[cfg(feature = "server")]
    fn webp_clamp_quality() {
        assert_eq!(WebpSettings::clamp_quality(-1.0), 0.0);
        assert_eq!(WebpSettings::clamp_quality(50.0), 50.0);
        assert_eq!(WebpSettings::clamp_quality(150.0), 100.0);
    }

    #[test]
    #[cfg(feature = "server")]
    fn webp_clamp_method() {
        assert_eq!(WebpSettings::clamp_method(0), 0);
        assert_eq!(WebpSettings::clamp_method(3), 3);
        assert_eq!(WebpSettings::clamp_method(9), MAX_WEBP_METHOD);
    }

    // ── ImageLimitSettings ───────────────────────────────────────

    #[test]
    fn image_limit_settings_default() {
        let s = ImageLimitSettings::default();
        assert_eq!(s.max_dimension, 8192);
        assert_eq!(s.max_pixels, 50_000_000);
        assert_eq!(s.dimensions_cache_ttl_secs, 86400);
    }

    #[test]
    #[cfg(feature = "server")]
    fn image_limit_clamp_max_dimension() {
        assert_eq!(
            ImageLimitSettings::clamp_max_dimension(0),
            MIN_IMAGE_MAX_DIMENSION
        );
        assert_eq!(ImageLimitSettings::clamp_max_dimension(2048), 2048);
    }

    #[test]
    #[cfg(feature = "server")]
    fn image_limit_clamp_max_pixels() {
        assert_eq!(
            ImageLimitSettings::clamp_max_pixels(0),
            MIN_IMAGE_MAX_PIXELS
        );
        assert_eq!(ImageLimitSettings::clamp_max_pixels(99_999_999), 99_999_999);
    }

    #[test]
    #[cfg(feature = "server")]
    fn image_limit_clamp_ttl() {
        assert_eq!(
            ImageLimitSettings::clamp_dimensions_cache_ttl_secs(0),
            MIN_IMAGE_DIMENSIONS_CACHE_TTL_SECS
        );
        assert_eq!(
            ImageLimitSettings::clamp_dimensions_cache_ttl_secs(7200),
            7200
        );
    }

    // ── RunnerSettings ───────────────────────────────────────────

    #[test]
    fn runner_settings_default() {
        let s = RunnerSettings::default();
        assert!(!s.allow_network);
        assert_eq!(s.max_concurrent, 4);
        assert_eq!(s.max_cpu_cores, 2.0);
        assert_eq!(s.max_memory_mb, 1024);
        assert_eq!(s.max_timeout_secs, 30);
        assert_eq!(s.max_output_bytes, 1_048_576);
        assert_eq!(s.max_source_bytes, 65_536);
        assert_eq!(s.queue_timeout_secs, 30);
        assert_eq!(s.task_ttl_secs, 300);
        assert!(s.languages.is_none());
    }

    #[test]
    #[cfg(feature = "server")]
    fn runner_clamp_max_concurrent() {
        assert_eq!(
            RunnerSettings::clamp_max_concurrent(0),
            MIN_RUNNER_MAX_CONCURRENT
        );
        assert_eq!(RunnerSettings::clamp_max_concurrent(8), 8);
        assert_eq!(
            RunnerSettings::clamp_max_concurrent(999),
            MAX_RUNNER_MAX_CONCURRENT
        );
    }

    #[test]
    #[cfg(feature = "server")]
    fn runner_clamp_max_cpu_cores() {
        assert_eq!(
            RunnerSettings::clamp_max_cpu_cores(0.0),
            MIN_RUNNER_MAX_CPU_CORES
        );
        assert_eq!(RunnerSettings::clamp_max_cpu_cores(4.0), 4.0);
        assert_eq!(
            RunnerSettings::clamp_max_cpu_cores(999.0),
            MAX_RUNNER_MAX_CPU_CORES
        );
        // NaN 回退默认值 2.0
        assert_eq!(RunnerSettings::clamp_max_cpu_cores(f64::NAN), 2.0);
    }

    #[test]
    #[cfg(feature = "server")]
    fn runner_clamp_max_memory_mb() {
        assert_eq!(
            RunnerSettings::clamp_max_memory_mb(0),
            MIN_RUNNER_MAX_MEMORY_MB
        );
        assert_eq!(RunnerSettings::clamp_max_memory_mb(2048), 2048);
        assert_eq!(
            RunnerSettings::clamp_max_memory_mb(u32::MAX),
            MAX_RUNNER_MAX_MEMORY_MB
        );
    }

    #[test]
    #[cfg(feature = "server")]
    fn runner_clamp_max_timeout_secs() {
        assert_eq!(
            RunnerSettings::clamp_max_timeout_secs(0),
            MIN_RUNNER_MAX_TIMEOUT_SECS
        );
        assert_eq!(RunnerSettings::clamp_max_timeout_secs(120), 120);
    }

    #[test]
    #[cfg(feature = "server")]
    fn runner_clamp_byte_limits() {
        assert_eq!(RunnerSettings::clamp_max_output_bytes(0), 1);
        assert_eq!(RunnerSettings::clamp_max_source_bytes(0), 1);
        assert_eq!(
            RunnerSettings::clamp_queue_timeout_secs(0),
            MIN_RUNNER_MAX_TIMEOUT_SECS
        );
        assert_eq!(RunnerSettings::clamp_task_ttl_secs(0), 1);
    }

    #[test]
    #[cfg(feature = "server")]
    fn runner_normalize_languages() {
        assert!(RunnerSettings::normalize_languages("").is_none());
        assert!(RunnerSettings::normalize_languages("  ,,  ").is_none());
        assert_eq!(
            RunnerSettings::normalize_languages("Python, Node, RUST").as_deref(),
            Some("python,node,rust")
        );
        assert_eq!(
            RunnerSettings::normalize_languages("  go  ").as_deref(),
            Some("go")
        );
    }
}
