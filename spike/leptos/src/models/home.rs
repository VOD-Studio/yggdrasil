//! 首页用到的跨端 DTO。
//!
//! 移植自主 crate `src/api/posts/types.rs`（文章列表、标签列表、最新一篇目录预览）
//! 与 `src/models/settings.rs`（站点公开配置与首页标语），字段与序列化格式一致。
use serde::{Deserialize, Serialize};

use crate::models::post::{PostListItem, Tag};

/// 文章分页列表响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostListResponse {
    /// 文章列表（轻量 DTO，不含正文）。
    pub posts: Vec<PostListItem>,
    /// 符合查询条件的总数。
    pub total: i64,
}

/// 标签列表响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagListResponse {
    /// 标签列表。
    pub tags: Vec<Tag>,
}

/// 首页「最新一篇」的目录预览。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostOutline {
    /// 对应文章的 slug；前台据此确认与列表首篇一致。
    pub slug: String,
    /// 一级小节总数。
    pub total: u32,
    /// 前 `OUTLINE_PREVIEW_LIMIT` 个小节标题（纯文本）。
    pub headings: Vec<String>,
}

/// 目录预览保留的小节标题数。
pub const OUTLINE_PREVIEW_LIMIT: usize = 6;

/// 页脚 GitHub 链接的默认值：空串表示不展示图标。
pub const DEFAULT_SITE_GITHUB_URL: &str = "";
/// 首页标题第一行的默认文案。
pub const DEFAULT_HOME_TITLE: &str = "慢慢读源码，";
/// 首页标题第二行（次要色）的默认文案。
pub const DEFAULT_HOME_SUBTITLE: &str = "认真写长文。";
/// 首页简介的默认文案。
pub const DEFAULT_HOME_INTRO: &str =
    "Rust、Go 与前端工程的源码剖析和长文笔记。每一篇，都试着把一个实现讲清楚。";
/// 站点简介的默认文案（友链页「交换友链」信息）。
pub const DEFAULT_SITE_DESCRIPTION: &str = "Rust、Go 与前端工程的源码剖析和长文笔记。";

/// 站点公开配置。
///
/// 页脚 GitHub 链接、首页标语与站点简介。文案字段存原值，空字符串表示使用默认文案，
/// 前台通过 [`SiteSettings::home_tagline`] 与 [`SiteSettings::description`] 取展示值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteSettings {
    /// GitHub 仓库链接，空字符串表示不展示页脚图标。
    pub github_url: String,
    /// 首页标题第一行。
    #[serde(default)]
    pub home_title: String,
    /// 首页标题第二行（次要色）。
    #[serde(default)]
    pub home_subtitle: String,
    /// 首页简介。
    #[serde(default)]
    pub home_intro: String,
    /// 站点简介：一句话介绍，用于友链交换信息。
    #[serde(default)]
    pub site_description: String,
}

impl Default for SiteSettings {
    fn default() -> Self {
        Self {
            github_url: DEFAULT_SITE_GITHUB_URL.to_string(),
            home_title: String::new(),
            home_subtitle: String::new(),
            home_intro: String::new(),
            site_description: String::new(),
        }
    }
}

/// 首页开场的展示文案。
#[derive(Debug, Clone, PartialEq)]
pub struct HomeTagline {
    pub title: String,
    /// 为空时只显示一行标题。
    pub subtitle: String,
    pub intro: String,
}

impl SiteSettings {
    /// 首页标语的展示值。未配置标题时两行都用默认文案；配置了标题时第二行按原值，
    /// 可以留空只显示一行。简介为空时用默认文案。
    pub fn home_tagline(&self) -> HomeTagline {
        let (title, subtitle) = if self.home_title.is_empty() {
            (DEFAULT_HOME_TITLE, DEFAULT_HOME_SUBTITLE)
        } else {
            (self.home_title.as_str(), self.home_subtitle.as_str())
        };
        let intro = if self.home_intro.is_empty() {
            DEFAULT_HOME_INTRO
        } else {
            &self.home_intro
        };
        HomeTagline {
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            intro: intro.to_string(),
        }
    }

    /// 站点简介的展示值，未配置时用默认文案。
    pub fn description(&self) -> &str {
        if self.site_description.is_empty() {
            DEFAULT_SITE_DESCRIPTION
        } else {
            &self.site_description
        }
    }
}
