//! 基于 moka 的内存缓存层。
//!
//! 仅在启用 `server` feature 时编译，为文章列表、标签、单篇文章、统计信息、
//! 评论、会话用户以及搜索结果提供按键缓存与失效能力。
//! 缓存使用 `std::sync::LazyLock` 全局实例，按不同业务数据设置独立的 TTL。

#[cfg(feature = "server")]
use moka::future::Cache;
#[cfg(feature = "server")]
use std::sync::LazyLock;
#[cfg(feature = "server")]
use std::time::Duration;

#[cfg(feature = "server")]
use crate::models::comment::PublicComment;
#[cfg(feature = "server")]
use crate::models::friend_link::FriendLink;
#[cfg(feature = "server")]
use crate::models::post::{FeedItem, Post, PostListItem, PostStats, Tag};
#[cfg(feature = "server")]
use crate::models::settings::{ImageCacheSettings, SecuritySettings, SiteSettings};
#[cfg(feature = "server")]
use crate::models::user::SessionUser;

// ============================================================================
// 缓存 TTL 配置
// ============================================================================

/// 文章列表缓存 TTL：60 秒。
#[cfg(feature = "server")]
const TTL_POST_LIST: Duration = Duration::from_secs(60);

/// 标签列表缓存 TTL：300 秒。
#[cfg(feature = "server")]
const TTL_TAG_LIST: Duration = Duration::from_secs(300);

/// 单篇文章缓存 TTL：600 秒。
#[cfg(feature = "server")]
const TTL_SINGLE_POST: Duration = Duration::from_secs(600);

/// 文章统计缓存 TTL：60 秒。
#[cfg(feature = "server")]
const TTL_POST_STATS: Duration = Duration::from_secs(60);

/// 标签下文章列表缓存 TTL：120 秒。
#[cfg(feature = "server")]
const TTL_TAG_POSTS: Duration = Duration::from_secs(120);

/// 评论列表缓存 TTL：60 秒。
#[cfg(feature = "server")]
const TTL_COMMENTS: Duration = Duration::from_secs(60);

/// 待审核评论数量缓存 TTL：10 秒，因管理后台需要较实时数据。
#[cfg(feature = "server")]
const TTL_PENDING_COUNT: Duration = Duration::from_secs(10);

/// 会话用户缓存 TTL：300 秒（5 分钟），短于 DB 会话过期时间。
#[cfg(feature = "server")]
const TTL_SESSION: Duration = Duration::from_secs(300);

/// 搜索结果缓存 TTL：10 秒。
#[cfg(feature = "server")]
const TTL_SEARCH: Duration = Duration::from_secs(10);

/// 站点配置缓存 TTL：600 秒。配置极少变更，但前台页脚每页读取，缓存兜底 DB。
#[cfg(feature = "server")]
const TTL_SITE_SETTINGS: Duration = Duration::from_secs(600);

/// 安全配置缓存 TTL：15 秒。安全相关（CSRF/cookie/代理/会话），面板修改后
/// 需尽快全链路生效，TTL 仅作 DB 兜底而非主失效手段（面板保存会主动失效）。
#[cfg(feature = "server")]
const TTL_SECURITY_SETTINGS: Duration = Duration::from_secs(15);

/// 图片磁盘缓存配置 TTL：60 秒。仅清理任务每小时读取一次，TTL 纯为 DB 兜底。
#[cfg(feature = "server")]
const TTL_IMAGE_CACHE_SETTINGS: Duration = Duration::from_secs(60);

// ============================================================================
// 缓存 Key 类型
// ============================================================================

/// 统一的缓存键枚举，每个变体对应一类可缓存数据。
#[cfg(feature = "server")]
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum CacheKey {
    /// 已发布文章分页列表。
    PublishedPosts { page: i32, per_page: i32 },
    /// 已发布文章总数。
    TotalPublishedPosts,
    /// 全部标签。
    AllTags,
    /// 按 slug 查询的单篇文章。
    PostBySlug(String),
    /// 按标签查询的文章列表（不分页，返回全部）。
    PostsByTag(String),
    /// 文章统计信息。
    PostStats,
    /// 某篇文章下的评论列表。
    CommentsByPost { post_id: i32 },
    /// 待审核评论总数。
    PendingCommentCount,
    /// Feed 条目列表（RSS / JSON Feed 共用单键）。
    Feed,
    /// 全部友链（前台可见集）。
    FriendLinks,
    /// 站点公开配置（页脚 GitHub 链接等，单键）。
    SiteSettings,
    /// 安全配置（即时生效层，单键）。
    SecuritySettings,
    /// 图片磁盘缓存配置（即时生效层，单键）。
    ImageCacheSettings,
}

// ============================================================================
// 缓存实例
// ============================================================================

/// 文章列表缓存类型，值为（文章列表，总数）。
#[cfg(feature = "server")]
pub type PostListCache = Cache<CacheKey, (Vec<PostListItem>, i64)>;

/// 标签列表缓存类型。
#[cfg(feature = "server")]
pub type TagListCache = Cache<CacheKey, Vec<Tag>>;

/// 单篇文章缓存类型。
#[cfg(feature = "server")]
pub type SinglePostCache = Cache<CacheKey, Option<Post>>;

/// 文章统计缓存类型。
#[cfg(feature = "server")]
pub type PostStatsCache = Cache<CacheKey, PostStats>;

/// Feed 条目列表缓存类型（RSS / JSON Feed 共用）。
#[cfg(feature = "server")]
pub type FeedCache = Cache<CacheKey, Vec<FeedItem>>;

/// 全局文章列表缓存实例，最大容量 100。
#[cfg(feature = "server")]
static POST_LIST_CACHE: LazyLock<PostListCache> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(100)
        .time_to_live(TTL_POST_LIST)
        .build()
});

/// 全局标签列表缓存实例，最大容量 50。
#[cfg(feature = "server")]
static TAG_LIST_CACHE: LazyLock<TagListCache> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(50)
        .time_to_live(TTL_TAG_LIST)
        .build()
});

/// 友链列表缓存类型。
#[cfg(feature = "server")]
pub type FriendLinksCache = Cache<CacheKey, Vec<FriendLink>>;

/// 全局友链列表缓存实例，最大容量 50，TTL 与标签列表一致。
#[cfg(feature = "server")]
static FRIEND_LINKS_CACHE: LazyLock<FriendLinksCache> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(50)
        .time_to_live(TTL_TAG_LIST)
        .build()
});

/// 全局单篇文章缓存实例，最大容量 200。
#[cfg(feature = "server")]
static SINGLE_POST_CACHE: LazyLock<SinglePostCache> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(200)
        .time_to_live(TTL_SINGLE_POST)
        .build()
});

/// 全局文章统计缓存实例，最大容量 10。
#[cfg(feature = "server")]
static POST_STATS_CACHE: LazyLock<PostStatsCache> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(10)
        .time_to_live(TTL_POST_STATS)
        .build()
});

/// 全局 Feed 条目缓存实例，最大容量 10（单键，20 篇全文条目）。
#[cfg(feature = "server")]
static FEED_CACHE: LazyLock<FeedCache> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(10)
        .time_to_live(TTL_SINGLE_POST)
        .build()
});

/// 全局标签文章列表缓存实例，最大容量 100。
#[cfg(feature = "server")]
static TAG_POSTS_CACHE: LazyLock<PostListCache> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(100)
        .time_to_live(TTL_TAG_POSTS)
        .build()
});

/// 评论列表缓存类型。
#[cfg(feature = "server")]
pub type CommentListCache = Cache<CacheKey, Vec<PublicComment>>;

/// 全局评论列表缓存实例，最大容量 200。
#[cfg(feature = "server")]
static COMMENT_CACHE: LazyLock<CommentListCache> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(200)
        .time_to_live(TTL_COMMENTS)
        .build()
});

/// 全局待审核评论数量缓存实例，最大容量 10。
#[cfg(feature = "server")]
static PENDING_COUNT_CACHE: LazyLock<Cache<CacheKey, i64>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(10)
        .time_to_live(TTL_PENDING_COUNT)
        .build()
});

/// 全局站点配置缓存实例（单键 `SiteSettings`），最大容量 2。
#[cfg(feature = "server")]
static SITE_SETTINGS_CACHE: LazyLock<Cache<CacheKey, SiteSettings>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(2)
        .time_to_live(TTL_SITE_SETTINGS)
        .build()
});

/// 全局安全配置缓存实例（单键 `SecuritySettings`），最大容量 2。
#[cfg(feature = "server")]
static SECURITY_SETTINGS_CACHE: LazyLock<Cache<CacheKey, SecuritySettings>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(2)
        .time_to_live(TTL_SECURITY_SETTINGS)
        .build()
});

/// 全局图片磁盘缓存配置实例（单键 `ImageCacheSettings`），最大容量 2。
#[cfg(feature = "server")]
static IMAGE_CACHE_SETTINGS_CACHE: LazyLock<Cache<CacheKey, ImageCacheSettings>> =
    LazyLock::new(|| {
        Cache::builder()
            .max_capacity(2)
            .time_to_live(TTL_IMAGE_CACHE_SETTINGS)
            .build()
    });

/// 会话用户缓存类型。
#[cfg(feature = "server")]
pub type SessionCache = Cache<String, SessionUser>;

/// 搜索结果缓存类型。
#[cfg(feature = "server")]
pub type SearchCache = Cache<String, (Vec<PostListItem>, i64)>;

/// 全局会话用户缓存实例，最大容量 1000，TTL 5 分钟。
#[cfg(feature = "server")]
pub static SESSION_CACHE: LazyLock<SessionCache> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(1000)
        .time_to_live(TTL_SESSION)
        .build()
});

/// 全局搜索结果缓存实例，最大容量 200，TTL 10 秒。
#[cfg(feature = "server")]
static SEARCH_CACHE: LazyLock<SearchCache> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(200)
        .time_to_live(TTL_SEARCH)
        .build()
});

// ============================================================================
// 命中率统计（供系统状态面板展示）
// ============================================================================

/// 单个缓存的命中/未命中计数。用 AtomicU64 在 get_* 路径上记录。
#[cfg(feature = "server")]
pub struct CacheStats {
    pub name: &'static str,
    hits: std::sync::atomic::AtomicU64,
    misses: std::sync::atomic::AtomicU64,
}

#[cfg(feature = "server")]
impl CacheStats {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            hits: std::sync::atomic::AtomicU64::new(0),
            misses: std::sync::atomic::AtomicU64::new(0),
        }
    }
    fn record_hit(&self) {
        self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    fn record_miss(&self) {
        self.misses
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

// 每个缓存一份统计实例（const，启动时零开销初始化）。
#[cfg(feature = "server")]
static POST_LIST_STATS: CacheStats = CacheStats::new("文章列表");
#[cfg(feature = "server")]
static TAG_STATS: CacheStats = CacheStats::new("标签");
#[cfg(feature = "server")]
static SINGLE_POST_STATS: CacheStats = CacheStats::new("单篇文章");
#[cfg(feature = "server")]
static POST_STATS_STATS: CacheStats = CacheStats::new("文章统计");
#[cfg(feature = "server")]
static TAG_POSTS_STATS: CacheStats = CacheStats::new("标签文章");
#[cfg(feature = "server")]
static COMMENT_STATS: CacheStats = CacheStats::new("评论");
#[cfg(feature = "server")]
static PENDING_COUNT_STATS: CacheStats = CacheStats::new("待审评论数");
#[cfg(feature = "server")]
static SESSION_STATS: CacheStats = CacheStats::new("会话用户");
#[cfg(feature = "server")]
static SEARCH_STATS: CacheStats = CacheStats::new("搜索");
#[cfg(feature = "server")]
static FEED_STATS: CacheStats = CacheStats::new("Feed");
#[cfg(feature = "server")]
static FRIEND_STATS: CacheStats = CacheStats::new("友链");
#[cfg(feature = "server")]
static SITE_SETTINGS_STATS: CacheStats = CacheStats::new("站点配置");

/// 缓存统计快照项（序列化给前端展示）。
#[cfg(feature = "server")]
#[derive(Debug)]
pub struct CacheStatSnapshot {
    pub name: &'static str,
    pub entry_count: u64,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

/// 聚合所有缓存的统计快照（供 get_server_status 调用）。
#[cfg(feature = "server")]
pub fn cache_stats() -> Vec<CacheStatSnapshot> {
    fn snap(stats: &CacheStats, entry_count: u64) -> CacheStatSnapshot {
        let hits = stats.hits.load(std::sync::atomic::Ordering::Relaxed);
        let misses = stats.misses.load(std::sync::atomic::Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        };
        CacheStatSnapshot {
            name: stats.name,
            entry_count,
            hits,
            misses,
            hit_rate,
        }
    }
    vec![
        snap(&POST_LIST_STATS, POST_LIST_CACHE.entry_count()),
        snap(&TAG_STATS, TAG_LIST_CACHE.entry_count()),
        snap(&SINGLE_POST_STATS, SINGLE_POST_CACHE.entry_count()),
        snap(&POST_STATS_STATS, POST_STATS_CACHE.entry_count()),
        snap(&TAG_POSTS_STATS, TAG_POSTS_CACHE.entry_count()),
        snap(&COMMENT_STATS, COMMENT_CACHE.entry_count()),
        snap(&PENDING_COUNT_STATS, PENDING_COUNT_CACHE.entry_count()),
        snap(&SESSION_STATS, SESSION_CACHE.entry_count()),
        snap(&SEARCH_STATS, SEARCH_CACHE.entry_count()),
        snap(&FEED_STATS, FEED_CACHE.entry_count()),
        snap(&FRIEND_STATS, FRIEND_LINKS_CACHE.entry_count()),
        snap(&SITE_SETTINGS_STATS, SITE_SETTINGS_CACHE.entry_count()),
    ]
}

// ============================================================================
// 公共缓存 API
// ============================================================================

/// 记录缓存命中/未命中统计，返回原值。
#[cfg(any(feature = "server", test))]
macro_rules! record_hit_miss {
    ($v:expr, $stats:expr) => {{
        let v = $v;
        if v.is_some() {
            $stats.record_hit();
        } else {
            $stats.record_miss();
        }
        v
    }};
}

/// 读取文章分页列表缓存。
#[cfg(feature = "server")]
pub async fn get_post_list(key: &CacheKey) -> Option<(Vec<PostListItem>, i64)> {
    record_hit_miss!(POST_LIST_CACHE.get(key).await, POST_LIST_STATS)
}

/// 写入文章分页列表缓存。
#[cfg(feature = "server")]
pub async fn set_post_list(key: &CacheKey, posts: Vec<PostListItem>, total: i64) {
    let _ = POST_LIST_CACHE.insert(key.clone(), (posts, total)).await;
}

/// 读取已发布文章总数缓存。
#[cfg(feature = "server")]
pub async fn get_total_published_posts() -> Option<i64> {
    record_hit_miss!(
        POST_LIST_CACHE
            .get(&CacheKey::TotalPublishedPosts)
            .await
            .map(|(_, total)| total),
        POST_LIST_STATS
    )
}

/// 写入已发布文章总数缓存，文章列表部分置空以节省内存。
#[cfg(feature = "server")]
pub async fn set_total_published_posts(total: i64) {
    let _ = POST_LIST_CACHE
        .insert(CacheKey::TotalPublishedPosts, (vec![], total))
        .await;
}

/// 读取全部标签缓存。
#[cfg(feature = "server")]
pub async fn get_tag_list() -> Option<Vec<Tag>> {
    record_hit_miss!(TAG_LIST_CACHE.get(&CacheKey::AllTags).await, TAG_STATS)
}

/// 写入全部标签缓存。
#[cfg(feature = "server")]
pub async fn set_tag_list(tags: Vec<Tag>) {
    let _ = TAG_LIST_CACHE.insert(CacheKey::AllTags, tags).await;
}

/// 读取全部友链缓存（前台可见集）。
#[cfg(feature = "server")]
pub async fn get_friend_links() -> Option<Vec<FriendLink>> {
    record_hit_miss!(
        FRIEND_LINKS_CACHE.get(&CacheKey::FriendLinks).await,
        FRIEND_STATS
    )
}

/// 写入全部友链缓存。
#[cfg(feature = "server")]
pub async fn set_friend_links(links: Vec<FriendLink>) {
    let _ = FRIEND_LINKS_CACHE
        .insert(CacheKey::FriendLinks, links)
        .await;
}

/// 读取站点配置缓存。
#[cfg(feature = "server")]
pub async fn get_site_settings() -> Option<SiteSettings> {
    record_hit_miss!(
        SITE_SETTINGS_CACHE.get(&CacheKey::SiteSettings).await,
        SITE_SETTINGS_STATS
    )
}

/// 写入站点配置缓存。
#[cfg(feature = "server")]
pub async fn set_site_settings(settings: SiteSettings) {
    let _ = SITE_SETTINGS_CACHE
        .insert(CacheKey::SiteSettings, settings)
        .await;
}

/// 失效站点配置缓存。
///
/// 使用同步签名是因为 `moka::Cache::invalidate_all` 为同步操作。
#[cfg(feature = "server")]
pub fn invalidate_site_settings() {
    SITE_SETTINGS_CACHE.invalidate_all();
}

/// 读取安全配置缓存。
#[cfg(feature = "server")]
pub async fn get_security_settings() -> Option<SecuritySettings> {
    SECURITY_SETTINGS_CACHE
        .get(&CacheKey::SecuritySettings)
        .await
}

/// 写入安全配置缓存。
#[cfg(feature = "server")]
pub async fn set_security_settings(settings: SecuritySettings) {
    let _ = SECURITY_SETTINGS_CACHE
        .insert(CacheKey::SecuritySettings, settings)
        .await;
}

/// 失效安全配置缓存（面板保存后调用）。
#[cfg(feature = "server")]
pub fn invalidate_security_settings() {
    SECURITY_SETTINGS_CACHE.invalidate_all();
}

/// 读取图片磁盘缓存配置缓存。
#[cfg(feature = "server")]
pub async fn get_image_cache_settings() -> Option<ImageCacheSettings> {
    IMAGE_CACHE_SETTINGS_CACHE
        .get(&CacheKey::ImageCacheSettings)
        .await
}

/// 写入图片磁盘缓存配置缓存。
#[cfg(feature = "server")]
pub async fn set_image_cache_settings(settings: ImageCacheSettings) {
    let _ = IMAGE_CACHE_SETTINGS_CACHE
        .insert(CacheKey::ImageCacheSettings, settings)
        .await;
}

/// 失效图片磁盘缓存配置缓存（面板保存后调用）。
#[cfg(feature = "server")]
pub fn invalidate_image_cache_settings() {
    IMAGE_CACHE_SETTINGS_CACHE.invalidate_all();
}

/// 按 slug 读取单篇文章缓存。
#[cfg(feature = "server")]
pub async fn get_post_by_slug(slug: &str) -> Option<Option<Post>> {
    record_hit_miss!(
        SINGLE_POST_CACHE
            .get(&CacheKey::PostBySlug(slug.to_string()))
            .await,
        SINGLE_POST_STATS
    )
}

/// 按 slug 写入单篇文章缓存，None 表示文章不存在。
#[cfg(feature = "server")]
pub async fn set_post_by_slug(slug: &str, post: Option<Post>) {
    let _ = SINGLE_POST_CACHE
        .insert(CacheKey::PostBySlug(slug.to_string()), post)
        .await;
}

/// 按标签读取文章列表缓存。
#[cfg(feature = "server")]
pub async fn get_posts_by_tag(tag: &str) -> Option<(Vec<PostListItem>, i64)> {
    record_hit_miss!(
        TAG_POSTS_CACHE
            .get(&CacheKey::PostsByTag(tag.to_string()))
            .await,
        TAG_POSTS_STATS
    )
}

/// 按标签写入文章列表缓存。
#[cfg(feature = "server")]
pub async fn set_posts_by_tag(tag: &str, posts: Vec<PostListItem>, total: i64) {
    let _ = TAG_POSTS_CACHE
        .insert(CacheKey::PostsByTag(tag.to_string()), (posts, total))
        .await;
}

/// 读取文章统计缓存。
#[cfg(feature = "server")]
pub async fn get_post_stats() -> Option<PostStats> {
    record_hit_miss!(
        POST_STATS_CACHE.get(&CacheKey::PostStats).await,
        POST_STATS_STATS
    )
}

/// 写入文章统计缓存。
#[cfg(feature = "server")]
pub async fn set_post_stats(stats: PostStats) {
    let _ = POST_STATS_CACHE.insert(CacheKey::PostStats, stats).await;
}

/// 读取 Feed 条目列表缓存。
#[cfg(feature = "server")]
pub async fn get_feed() -> Option<Vec<FeedItem>> {
    record_hit_miss!(FEED_CACHE.get(&CacheKey::Feed).await, FEED_STATS)
}

/// 写入 Feed 条目列表缓存。
#[cfg(feature = "server")]
pub async fn set_feed(items: Vec<FeedItem>) {
    let _ = FEED_CACHE.insert(CacheKey::Feed, items).await;
}

// ============================================================================
// 缓存失效
// ============================================================================

/// 清空所有文章分页列表缓存。
#[cfg(feature = "server")]
pub fn invalidate_post_lists() {
    POST_LIST_CACHE.invalidate_all();
}

/// 清空所有标签缓存。
#[cfg(feature = "server")]
pub fn invalidate_all_tags() {
    TAG_LIST_CACHE.invalidate_all();
}

/// 清空友链缓存。
#[cfg(feature = "server")]
pub fn invalidate_friend_links() {
    FRIEND_LINKS_CACHE.invalidate_all();
}

/// 按 slug 失效单篇文章缓存。
#[cfg(feature = "server")]
pub async fn invalidate_post_by_slug(slug: &str) {
    SINGLE_POST_CACHE
        .invalidate(&CacheKey::PostBySlug(slug.to_string()))
        .await;
}

/// 按标签失效文章列表缓存。
#[cfg(feature = "server")]
pub async fn invalidate_posts_by_tag(tag: &str) {
    TAG_POSTS_CACHE
        .invalidate(&CacheKey::PostsByTag(tag.to_string()))
        .await;
}

/// 清空文章统计缓存。
#[cfg(feature = "server")]
pub fn invalidate_post_stats() {
    POST_STATS_CACHE.invalidate_all();
}

/// 按标签批量失效文章列表缓存。
#[cfg(feature = "server")]
pub async fn invalidate_tag_posts_for(tags: &[String]) {
    let futures: Vec<_> = tags
        .iter()
        .map(|tag| invalidate_posts_by_tag(tag))
        .collect();
    let _ = futures::future::join_all(futures).await;
}

/// 清空所有文章相关缓存（列表、标签、单篇、统计、标签文章）。
///
/// 这是一个“紧急”使用的全量失效开关，会一次性冲刷所有文章缓存；
/// 正常写路径应当使用更细粒度的 `invalidate_post_lists` / `invalidate_all_tags` /
/// `invalidate_post_by_slug` / `invalidate_posts_by_tag` / `invalidate_post_stats` /
/// `invalidate_tag_posts_for` 等函数，避免不必要的缓存击穿。
#[cfg(feature = "server")]
pub fn invalidate_all_post_caches() {
    POST_LIST_CACHE.invalidate_all();
    TAG_LIST_CACHE.invalidate_all();
    SINGLE_POST_CACHE.invalidate_all();
    POST_STATS_CACHE.invalidate_all();
    TAG_POSTS_CACHE.invalidate_all();
    invalidate_feed();
}

/// 清空 Feed 条目缓存。
///
/// 使用同步签名是因为 `moka::Cache::invalidate_all` 为同步操作，
/// 与 `invalidate_post_stats` 等元数据失效保持一致。
#[cfg(feature = "server")]
pub fn invalidate_feed() {
    FEED_CACHE.invalidate_all();
}

/// 失效文章「元数据」类缓存：列表、标签、统计、搜索结果、Feed。
///
/// 这些在每次文章写操作（创建/更新/删除/恢复/清空回收站）后都需要一起失效。
/// 单篇正文与标签下文章列表是定向失效（按 slug / tag），不在此处处理，由调用方
/// 根据实际涉及的 slug/tags 额外调用 `invalidate_post_by_slug` / `invalidate_tag_posts_for`。
#[cfg(feature = "server")]
pub fn invalidate_post_metadata() {
    invalidate_post_lists();
    invalidate_all_tags();
    invalidate_post_stats();
    invalidate_search_results();
    invalidate_feed();
}

/// 文章写操作（创建/更新/删除/恢复/清空回收站）后的统一缓存失效序列。
///
/// 按影响范围精准失效：先失效文章「元数据」类缓存（列表/标签云/统计/搜索），
/// 再按 slug 定向失效单篇正文与 SSR 详情页，按标签失效标签下文章列表，
/// 最后全量刷新公开页 SSR 缓存并递增全局世代号。
///
/// - `slugs`：受影响的文章 slug（旧/新均可，用于单篇缓存与 SSR 详情页定向失效）。
/// - `tags`：受影响的标签名（用于标签下文章列表失效）。
///
/// 对于 slug 变更等需要分别处理旧/新 slug 的复杂场景，调用方将涉及的 slug 一并传入即可——
/// 本函数对 `slugs` 中的每个元素一视同仁。注意：`invalidate_ssr_all_public` 会删除全部
/// 公开页 SSR 缓存，因此批量场景下逐 slug 的 `invalidate_ssr_route` 仅是先行的定向清理，
/// 不会改变最终失效结果。
#[cfg(feature = "server")]
pub async fn invalidate_for_post_write(slugs: &[String], tags: &[String]) {
    invalidate_post_metadata();
    for slug in slugs {
        invalidate_post_by_slug(slug).await;
    }
    invalidate_tag_posts_for(tags).await;
    for slug in slugs {
        crate::ssr_cache::invalidate_ssr_route(&format!("/post/{slug}"));
        crate::ssr_cache::invalidate_post_preview(slug);
    }
    crate::ssr_cache::invalidate_ssr_all_public();
    crate::ssr_cache::bump_global_generation();
}

/// 按文章主键读取评论列表缓存。
#[cfg(feature = "server")]
pub async fn get_comments_by_post(post_id: i32) -> Option<Vec<PublicComment>> {
    record_hit_miss!(
        COMMENT_CACHE
            .get(&CacheKey::CommentsByPost { post_id })
            .await,
        COMMENT_STATS
    )
}

/// 按文章主键写入评论列表缓存。
#[cfg(feature = "server")]
pub async fn set_comments_by_post(post_id: i32, comments: Vec<PublicComment>) {
    let _ = COMMENT_CACHE
        .insert(CacheKey::CommentsByPost { post_id }, comments)
        .await;
}

/// 读取待审核评论总数缓存。
#[cfg(feature = "server")]
pub async fn get_pending_count() -> Option<i64> {
    record_hit_miss!(
        PENDING_COUNT_CACHE
            .get(&CacheKey::PendingCommentCount)
            .await,
        PENDING_COUNT_STATS
    )
}

/// 写入待审核评论总数缓存。
#[cfg(feature = "server")]
pub async fn set_pending_count(count: i64) {
    let _ = PENDING_COUNT_CACHE
        .insert(CacheKey::PendingCommentCount, count)
        .await;
}

/// 规范化搜索查询键：trim、转小写、截断至 200 字符。
#[cfg(feature = "server")]
pub fn normalize_search_key(query: &str) -> String {
    query.trim().to_lowercase().chars().take(200).collect()
}

/// 读取会话用户缓存。
#[cfg(feature = "server")]
pub async fn get_session_user(token_hash: &str) -> Option<SessionUser> {
    record_hit_miss!(SESSION_CACHE.get(token_hash).await, SESSION_STATS)
}

/// 写入会话用户缓存。
#[cfg(feature = "server")]
pub async fn set_session_user(token_hash: &str, user: SessionUser) {
    let _ = SESSION_CACHE.insert(token_hash.to_string(), user).await;
}

/// 失效指定会话用户缓存。
#[cfg(feature = "server")]
pub async fn invalidate_session_user(token_hash: &str) {
    SESSION_CACHE.invalidate(token_hash).await;
}

/// 读取搜索结果缓存。
#[cfg(feature = "server")]
pub async fn get_search_results(query: &str) -> Option<(Vec<PostListItem>, i64)> {
    record_hit_miss!(
        SEARCH_CACHE.get(&normalize_search_key(query)).await,
        SEARCH_STATS
    )
}

/// 写入搜索结果缓存。
#[cfg(feature = "server")]
pub async fn set_search_results(query: &str, posts: Vec<PostListItem>, total: i64) {
    let _ = SEARCH_CACHE
        .insert(normalize_search_key(query), (posts, total))
        .await;
}

/// 清空所有搜索结果缓存。
///
/// 使用同步签名是因为 `moka::Cache::invalidate_all` 为同步操作；
/// 该函数通常由写路径直接调用，无需额外等待。
#[cfg(feature = "server")]
pub fn invalidate_search_results() {
    SEARCH_CACHE.invalidate_all();
}

/// 按文章主键失效评论列表缓存。
#[cfg(feature = "server")]
pub async fn invalidate_comments_by_post(post_id: i32) {
    COMMENT_CACHE
        .invalidate(&CacheKey::CommentsByPost { post_id })
        .await;
}

/// 失效待审核评论总数缓存。
#[cfg(feature = "server")]
pub async fn invalidate_pending_count() {
    PENDING_COUNT_CACHE
        .invalidate(&CacheKey::PendingCommentCount)
        .await;
}

/// 全量失效评论缓存（SQL 控制台兜底用：管理员直接改 comments 表时无法定向）。
///
/// 正常评论写路径用 [`invalidate_comments_by_post`]（按文章定向）+ [`invalidate_pending_count`]；
/// 这里全量清空是 SQL 控制台直改 DB 的兜底——无法从任意 SQL 精确解析受影响的 post_id。
#[cfg(feature = "server")]
pub fn invalidate_all_comments() {
    COMMENT_CACHE.invalidate_all();
    PENDING_COUNT_CACHE.invalidate_all();
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::models::comment::PublicComment;
    use crate::models::post::PostStatus;
    use crate::models::user::{SessionUser, UserRole};
    use serial_test::serial;

    #[test]
    #[serial]
    fn cache_key_equality() {
        let k1 = CacheKey::PublishedPosts {
            page: 1,
            per_page: 10,
        };
        let k2 = CacheKey::PublishedPosts {
            page: 1,
            per_page: 10,
        };
        let k3 = CacheKey::PublishedPosts {
            page: 2,
            per_page: 10,
        };
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    #[tokio::test]
    #[serial]
    async fn post_list_cache_roundtrip() {
        let key = CacheKey::PublishedPosts {
            page: 999,
            per_page: 99,
        };
        let posts = vec![PostListItem {
            id: 1,
            author_id: 1,
            title: "List Item".to_string(),
            slug: "list-item".to_string(),
            summary: None,
            status: PostStatus::Published,
            published_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            tags: vec!["rust".to_string()],
            cover_image: None,
            reading_time: 1,
            word_count: 10,
        }];

        set_post_list(&key, posts.clone(), 1).await;
        let cached = get_post_list(&key).await;

        assert!(cached.is_some());
        let (cached_posts, cached_total) = cached.unwrap();
        assert_eq!(cached_posts.len(), 1);
        assert_eq!(cached_posts[0].title, "List Item");
        assert_eq!(cached_total, 1);
    }

    #[tokio::test]
    #[serial]
    async fn tag_list_cache_roundtrip() {
        let tags = vec![Tag {
            id: 1,
            name: "rust".to_string(),
            post_count: 5,
        }];

        set_tag_list(tags.clone()).await;
        let cached = get_tag_list().await;

        assert!(cached.is_some());
        assert_eq!(cached.unwrap()[0].name, "rust");
    }

    #[tokio::test]
    #[serial]
    async fn single_post_cache_roundtrip() {
        let post = Some(Post {
            id: 1,
            author_id: 1,
            title: "Test".to_string(),
            slug: "test".to_string(),
            summary: None,
            content_md: "content".to_string(),
            content_html: None,
            status: PostStatus::Published,
            published_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            tags: vec![],
            cover_image: None,
            reading_time: 1,
            word_count: 10,
            toc_html: None,
            prev_post: None,
            next_post: None,
        });

        set_post_by_slug("test", post.clone()).await;
        let cached = get_post_by_slug("test").await;

        assert!(cached.is_some());
        assert_eq!(cached.unwrap().unwrap().title, "Test");
    }

    #[tokio::test]
    #[serial]
    async fn post_stats_cache_roundtrip() {
        let stats = PostStats {
            total: 10,
            drafts: 3,
            published: 7,
            trash: 2,
            recent_30d: 4,
            activity_30d: vec![0, 1, 3],
        };

        set_post_stats(stats.clone()).await;
        let cached = get_post_stats().await;

        assert!(cached.is_some());
        assert_eq!(cached.unwrap().total, 10);
    }

    #[tokio::test]
    #[serial]
    async fn cache_invalidation_works() {
        let post = Some(Post {
            id: 42,
            author_id: 1,
            title: "Invalidation Test".to_string(),
            slug: "invalidation-test".to_string(),
            summary: None,
            content_md: "test".to_string(),
            content_html: None,
            status: PostStatus::Published,
            published_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            tags: vec![],
            cover_image: None,
            reading_time: 1,
            word_count: 4,
            toc_html: None,
            prev_post: None,
            next_post: None,
        });

        set_post_by_slug("invalidation-test", post.clone()).await;
        let cached_before = get_post_by_slug("invalidation-test").await;
        assert!(cached_before.is_some());

        invalidate_post_by_slug("invalidation-test").await;

        let cached_after = get_post_by_slug("invalidation-test").await;
        assert!(cached_after.is_none());
    }

    #[tokio::test]
    #[serial]
    async fn comment_cache_roundtrip() {
        let comments = vec![PublicComment {
            id: 1,
            parent_id: None,
            depth: 0,
            author_name: "Alice".to_string(),
            author_url: None,
            avatar_url: "https://example.com/avatar".to_string(),
            content_html: Some("<p>Hello</p>".to_string()),
            created_at: "刚刚".to_string(),
            created_at_iso: "2026-01-01T00:00:00Z".to_string(),
        }];

        set_comments_by_post(42, comments.clone()).await;
        let cached = get_comments_by_post(42).await;

        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1);
    }

    #[tokio::test]
    #[serial]
    async fn pending_count_cache_roundtrip() {
        set_pending_count(7).await;
        let cached = get_pending_count().await;

        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), 7);
    }

    #[tokio::test]
    #[serial]
    async fn comment_cache_invalidation() {
        set_comments_by_post(99, vec![]).await;
        assert!(get_comments_by_post(99).await.is_some());

        invalidate_comments_by_post(99).await;
        assert!(get_comments_by_post(99).await.is_none());
    }

    #[tokio::test]
    #[serial]
    async fn pending_count_invalidation() {
        set_pending_count(3).await;
        assert!(get_pending_count().await.is_some());

        invalidate_pending_count().await;
        assert!(get_pending_count().await.is_none());
    }

    #[tokio::test]
    #[serial]
    async fn session_cache_roundtrip() {
        let user = SessionUser {
            id: 42,
            username: "cached_user".to_string(),
            email: "cached@example.com".to_string(),
            display_name: None,
            avatar_url: None,
            role: UserRole::Admin,
            created_at: chrono::Utc::now(),
            session_generation: 0,
        };
        let token_hash = "sha256_token_hash";

        set_session_user(token_hash, user.clone()).await;
        let cached = get_session_user(token_hash).await;

        assert!(cached.is_some());
        let cached_user = cached.unwrap();
        assert_eq!(cached_user.id, user.id);
        assert_eq!(cached_user.username, user.username);
        assert_eq!(cached_user.email, user.email);
        assert_eq!(cached_user.role, user.role);

        invalidate_session_user(token_hash).await;
        assert!(get_session_user(token_hash).await.is_none());
    }

    #[test]
    fn search_key_normalization() {
        assert_eq!(normalize_search_key("  Rust "), "rust");
        assert_eq!(normalize_search_key("Rust"), "rust");
        assert_eq!(normalize_search_key("  rust "), "rust");
        assert_eq!(normalize_search_key(""), "");

        let long = "a".repeat(250);
        let normalized = normalize_search_key(&long);
        assert_eq!(normalized.len(), 200);
        assert!(normalized.chars().all(|c| c == 'a'));

        // 大小写与空格差异应映射到同一键。
        assert_eq!(
            normalize_search_key("  Dioxus Fullstack "),
            normalize_search_key("dioxus fullstack")
        );
    }

    #[tokio::test]
    #[serial]
    async fn search_cache_roundtrip() {
        let query = "Rust";
        let posts = vec![PostListItem {
            id: 1,
            author_id: 1,
            title: "Search Result".to_string(),
            slug: "search-result".to_string(),
            summary: None,
            status: PostStatus::Published,
            published_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            tags: vec!["rust".to_string()],
            cover_image: None,
            reading_time: 1,
            word_count: 10,
        }];

        set_search_results(query, posts.clone(), 1).await;

        // 大小写与空格差异应命中同一缓存条目。
        let cached = get_search_results(" rust ").await;
        assert!(cached.is_some());
        let (cached_posts, cached_total) = cached.unwrap();
        assert_eq!(cached_posts.len(), 1);
        assert_eq!(cached_posts[0].title, "Search Result");
        assert_eq!(cached_total, 1);

        invalidate_search_results();
        assert!(get_search_results(query).await.is_none());
    }

    #[tokio::test]
    #[serial]
    async fn search_cache_invalidation() {
        set_search_results("tokio", vec![], 0).await;
        assert!(get_search_results("tokio").await.is_some());

        invalidate_search_results();
        assert!(get_search_results("tokio").await.is_none());
    }
}
