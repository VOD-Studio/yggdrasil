//! 更新日志接口。
//!
//! `CHANGELOG.md` 在编译期经 `include_str!` 内嵌进二进制（scratch 运行时镜像
//! 只有 binary + public/ + uploads/，磁盘上没有该文件；与 migrations、
//! highlight.css 的内嵌原则一致）。
//!
//! 与旧实现（整篇 Markdown → HTML blob → `dangerous_inner_html`）不同，本模块
//! 将 Keep a Changelog 格式的 Markdown **解析为结构化数据**（版本 → 分类 → 条目），
//! 供前端按版本卡片 + 分类色标 badge 渲染时间线视图。
//!
//! 解析流程（纯函数，进程生命周期内只执行一次，`LazyLock` 缓存）：
//! 1. `changelog_body` 剥去 preamble（`# Changelog` 标题 + Keep a Changelog 说明）
//! 2. `parse_changelog` 逐行扫描，按 `## `（版本）和 `### `（分类）切分
//! 3. 每个分类组的条目经 `render_markdown_enhanced` 渲染为 HTML 片段
//!    （保留 `**bold**` / `` `code` `` / `[link]` 等内联格式 + sanitizer 清理）
//!
//! 分类标签默认中性灰，只有新增（clay）与安全（danger）着色，见 `changelog.css`。

// 与 settings 等模块一致：Leptos `#[server]` 宏触发 deprecated/unit 提示，按项目惯例放行。
#![allow(clippy::unused_unit, deprecated)]

use crate::api::error::ApiError;
use leptos::prelude::*;

// ===========================================================================
// 数据结构（双 target 共享：server 序列化 → WASM 反序列化渲染）
// ===========================================================================

/// 变更分类。对应 Keep a Changelog 的标准分类 + 本项目扩展的 Internal。
///
/// 序列化为小写字符串（`"added"` / `"fixed"` …），前端据此选择 badge CSS 类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeCategory {
    Added,
    Changed,
    Fixed,
    Deprecated,
    Removed,
    Security,
    Internal,
}

impl ChangeCategory {
    /// 中文标签（badge 显示文本）。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Added => "新增",
            Self::Changed => "改进",
            Self::Fixed => "修复",
            Self::Deprecated => "弃用",
            Self::Removed => "移除",
            Self::Security => "安全",
            Self::Internal => "内部",
        }
    }

    /// 分类标签的 `data-category` 取值。
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Changed => "changed",
            Self::Fixed => "fixed",
            Self::Deprecated => "deprecated",
            Self::Removed => "removed",
            Self::Security => "security",
            Self::Internal => "internal",
        }
    }

    /// 从 Markdown `### Header` 文本解析分类。未知分类回退为 Internal。
    #[cfg(feature = "server")]
    fn from_name(name: &str) -> Self {
        match name.trim() {
            "Added" => Self::Added,
            "Changed" => Self::Changed,
            "Fixed" => Self::Fixed,
            "Deprecated" => Self::Deprecated,
            "Removed" => Self::Removed,
            "Security" => Self::Security,
            _ => Self::Internal,
        }
    }
}

/// 单个分类组（如 "Added" 下所有条目的渲染 HTML）。
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeGroup {
    /// 分类。
    pub category: ChangeCategory,
    /// 该分类下所有条目经 Markdown 渲染后的 HTML 片段（`<ul><li>…</li></ul>`）。
    pub items_html: String,
}

/// 单个版本条目。
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VersionEntry {
    /// 版本号（如 `"0.8.3"`）或 `"Unreleased"`。
    pub version: String,
    /// 发布日期（ISO 格式 `"2026-08-03"`），Unreleased 版本为 None。
    pub date: Option<String>,
    /// 是否为最新正式版（第一个非 Unreleased 的版本）。
    pub is_latest: bool,
    /// 版本下不属于任何 `###` 分类的正文 HTML（如 Unreleased 的占位文字）。
    pub intro_html: String,
    /// 按分类分组的变更条目。
    pub groups: Vec<ChangeGroup>,
}

/// 完整的 changelog 结构化数据。
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangelogData {
    /// 版本列表（按 CHANGELOG.md 出现顺序，通常为最新在前）。
    pub versions: Vec<VersionEntry>,
}

// ===========================================================================
// 服务端：解析与渲染
// ===========================================================================

/// CHANGELOG.md 原文，编译期内嵌。
#[cfg(feature = "server")]
const CHANGELOG_MD: &str = include_str!("../../CHANGELOG.md");

/// 解析结果，进程生命周期内只计算一次。
#[cfg(feature = "server")]
static CHANGELOG_PARSED: std::sync::LazyLock<ChangelogData> =
    std::sync::LazyLock::new(|| parse_changelog(CHANGELOG_MD));

/// 剥去 CHANGELOG.md 开头的 `# Changelog` 标题与 Keep a Changelog 说明段，
/// 从第一个 `## ` 版本段起返回；不符合预期格式时回退全文。
#[cfg(feature = "server")]
fn changelog_body(full: &str) -> &str {
    match full.find("\n## ") {
        Some(i) => &full[i + 1..],
        None => full,
    }
}

/// 解析过程中的临时版本结构（收集原始 Markdown 文本，稍后统一渲染）。
#[cfg(feature = "server")]
struct RawVersion {
    version: String,
    date: Option<String>,
    intro_md: String,
    categories: Vec<(ChangeCategory, String)>,
}

/// 将 CHANGELOG.md 全文解析为结构化 `ChangelogData`。
///
/// 逐行扫描：`## ` 开头 → 版本边界，`### ` 开头 → 分类边界，
/// 其余行归入当前分类的原始 Markdown（若无分类则归入版本 intro）。
/// 解析完成后，每个分类的原始 Markdown 经 `render_markdown_enhanced` 渲染为 HTML。
#[cfg(feature = "server")]
fn parse_changelog(full_md: &str) -> ChangelogData {
    let body = changelog_body(full_md);

    let mut raw_versions: Vec<RawVersion> = Vec::new();
    let mut current: Option<RawVersion> = None;
    let mut current_cat: Option<(ChangeCategory, String)> = None;

    for line in body.lines() {
        if line.starts_with("## ") {
            flush_category(&mut current, &mut current_cat);
            flush_version(&mut current, &mut raw_versions);
            let (version, date) = parse_version_header(line);
            current = Some(RawVersion {
                version,
                date,
                intro_md: String::new(),
                categories: Vec::new(),
            });
        } else if line.starts_with("### ") {
            flush_category(&mut current, &mut current_cat);
            let cat_name = line.trim_start_matches("### ").trim();
            current_cat = Some((ChangeCategory::from_name(cat_name), String::new()));
        } else if let Some((_, md)) = current_cat.as_mut() {
            md.push_str(line);
            md.push('\n');
        } else if let Some(v) = current.as_mut() {
            v.intro_md.push_str(line);
            v.intro_md.push('\n');
        }
    }
    flush_category(&mut current, &mut current_cat);
    flush_version(&mut current, &mut raw_versions);

    // 将原始 Markdown 渲染为 HTML，转换为最终 VersionEntry。
    let mut versions: Vec<VersionEntry> = raw_versions
        .into_iter()
        .map(|rv| VersionEntry {
            version: rv.version,
            date: rv.date,
            is_latest: false,
            intro_html: render_section(&rv.intro_md),
            groups: rv
                .categories
                .into_iter()
                .map(|(cat, md)| ChangeGroup {
                    category: cat,
                    items_html: render_section(&md),
                })
                .collect(),
        })
        .collect();

    // 第一个非 Unreleased 版本标记为最新。
    for v in versions.iter_mut() {
        if v.version != "Unreleased" {
            v.is_latest = true;
            break;
        }
    }

    ChangelogData { versions }
}

/// 将 `current_cat` 中的累积内容存入当前版本的 categories 列表。
#[cfg(feature = "server")]
fn flush_category(
    current: &mut Option<RawVersion>,
    current_cat: &mut Option<(ChangeCategory, String)>,
) {
    if let Some((cat, md)) = current_cat.take() {
        if let Some(v) = current.as_mut() {
            v.categories.push((cat, md));
        }
    }
}

/// 将 `current` 版本存入 `raw_versions` 列表。
#[cfg(feature = "server")]
fn flush_version(current: &mut Option<RawVersion>, raw_versions: &mut Vec<RawVersion>) {
    if let Some(v) = current.take() {
        raw_versions.push(v);
    }
}

/// 解析版本头行。
///
/// `"## [0.8.3] - 2026-08-03"` → `("0.8.3", Some("2026-08-03"))`
/// `"## [Unreleased]"`         → `("Unreleased", None)`
#[cfg(feature = "server")]
fn parse_version_header(line: &str) -> (String, Option<String>) {
    let header = line.trim_start_matches("## ").trim();
    if header.starts_with('[') {
        if let Some(end) = header.find(']') {
            let version = header[1..end].to_string();
            let rest = header[end + 1..].trim();
            let date = rest
                .strip_prefix('-')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            return (version, date);
        }
    }
    (header.to_string(), None)
}

/// 将一段原始 Markdown 渲染为 sanitizer 清理后的 HTML 片段。
///
/// 复用全站统一的 `render_markdown_enhanced` 管线（内联格式 + 代码高亮 + sanitizer）。
/// 空内容返回空字符串。
#[cfg(feature = "server")]
fn render_section(md: &str) -> String {
    let trimmed = md.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    crate::api::markdown::render_markdown_enhanced(trimmed).html
}

// ===========================================================================
// Server function
// ===========================================================================

/// 获取结构化的更新日志数据。
///
/// 公开接口；首次调用解析 + 渲染后永久缓存，之后均为一次 `LazyLock` 读取 + clone。
#[server(GetChangelog, endpoint = "get_changelog", input = leptos::server_fn::codec::Json)]
pub async fn get_changelog() -> Result<ChangelogData, ApiError> {
    #[cfg(feature = "server")]
    {
        let data = tokio::task::spawn_blocking(|| CHANGELOG_PARSED.clone())
            .await
            .map_err(|_| crate::api::error::AppError::Internal("更新日志解析任务失败"))?;
        Ok(data)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(ChangelogData {
            versions: Vec::new(),
        })
    }
}

/// 最新发布的版本号与日期（不含 Unreleased），供关于页显示。
#[server(LatestRelease, endpoint = "latest_release", input = leptos::server_fn::codec::Json)]
pub async fn latest_release() -> Result<Option<(String, Option<String>)>, ApiError> {
    let data = get_changelog().await?;
    Ok(data
        .versions
        .into_iter()
        .find(|v| v.version != "Unreleased")
        .map(|v| (v.version, v.date)))
}

// ===========================================================================
// 测试
// ===========================================================================

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn changelog_body_strips_preamble() {
        let md = "# Changelog\n\n基于 Keep a Changelog。\n\n## [Unreleased]\n\n### Added\n- x\n\n## [0.1.0] - 2026-01-01\n";
        let body = changelog_body(md);
        assert!(body.starts_with("## [Unreleased]"));
        assert!(!body.contains("# Changelog"));
        assert!(!body.contains("Keep a Changelog"));
    }

    #[test]
    fn changelog_body_falls_back_to_full_text_without_version_section() {
        let md = "# Changelog\n\n这里还没有任何版本段。\n";
        assert_eq!(changelog_body(md), md);
    }

    #[test]
    fn parse_version_header_with_date() {
        let (ver, date) = parse_version_header("## [0.8.3] - 2026-08-03");
        assert_eq!(ver, "0.8.3");
        assert_eq!(date.as_deref(), Some("2026-08-03"));
    }

    #[test]
    fn parse_version_header_unreleased() {
        let (ver, date) = parse_version_header("## [Unreleased]");
        assert_eq!(ver, "Unreleased");
        assert!(date.is_none());
    }

    #[test]
    fn parse_version_header_no_brackets() {
        let (ver, date) = parse_version_header("## Some Title");
        assert_eq!(ver, "Some Title");
        assert!(date.is_none());
    }

    #[test]
    fn parse_changelog_basic_structure() {
        let md = "\
# Changelog

基于 Keep a Changelog。

## [Unreleased]

_暂无未发布改动。_

## [0.1.0] - 2026-01-01

### Added

- **功能 A**：描述。
- 功能 B。

### Fixed

- 修复 X。
";
        let data = parse_changelog(md);
        assert_eq!(data.versions.len(), 2, "应解析出 2 个版本");

        // Unreleased 版本
        let unreleased = &data.versions[0];
        assert_eq!(unreleased.version, "Unreleased");
        assert!(unreleased.date.is_none());
        assert!(!unreleased.is_latest, "Unreleased 不是最新版");
        assert!(
            unreleased.intro_html.contains("暂无"),
            "intro 应包含占位文字"
        );
        assert!(unreleased.groups.is_empty(), "Unreleased 无分类组");

        // 0.1.0 版本
        let v010 = &data.versions[1];
        assert_eq!(v010.version, "0.1.0");
        assert_eq!(v010.date.as_deref(), Some("2026-01-01"));
        assert!(v010.is_latest, "第一个正式版应标记为最新");
        assert_eq!(v010.groups.len(), 2, "应有 Added + Fixed 两个组");

        // Added 组
        let added = &v010.groups[0];
        assert_eq!(added.category, ChangeCategory::Added);
        assert!(added.items_html.contains("<strong>功能 A</strong>"));
        assert!(added.items_html.contains("功能 B"));

        // Fixed 组
        let fixed = &v010.groups[1];
        assert_eq!(fixed.category, ChangeCategory::Fixed);
        assert!(fixed.items_html.contains("修复 X"));
    }

    #[test]
    fn parse_changelog_nested_items() {
        let md = "\
## [0.1.0] - 2026-01-01

### Added

- **父条目**：描述。
  - **子条目 A**：细节。
  - **子条目 B**：细节。
";
        let data = parse_changelog(md);
        let added = &data.versions[0].groups[0];
        assert!(added.items_html.contains("父条目"), "应包含父条目");
        assert!(added.items_html.contains("子条目 A"), "应包含嵌套子条目");
    }

    #[test]
    fn parse_changelog_unknown_category_maps_to_internal() {
        let md = "\
## [0.1.0] - 2026-01-01

### SomeNewCategory

- 测试条目。
";
        let data = parse_changelog(md);
        assert_eq!(
            data.versions[0].groups[0].category,
            ChangeCategory::Internal,
            "未知分类应回退为 Internal"
        );
    }

    #[test]
    fn parse_changelog_empty_input() {
        let data = parse_changelog("# Changelog\n\n没有版本段。\n");
        assert!(data.versions.is_empty(), "无版本段时应返回空版本列表");
    }

    /// 端到端守护：include_str! 路径有效 + 真实 CHANGELOG.md 解析成功。
    #[test]
    fn changelog_parses_real_file() {
        let data = &*CHANGELOG_PARSED;
        assert!(
            !data.versions.is_empty(),
            "真实 CHANGELOG 应解析出至少一个版本"
        );
        assert!(
            data.versions.iter().any(|v| v.version == "0.8.3"),
            "正文应包含 0.8.3 版本"
        );
        // 最新版标记
        let latest_count = data.versions.iter().filter(|v| v.is_latest).count();
        assert_eq!(latest_count, 1, "应恰好有一个版本标记为最新");
        // 每个正式版至少有一个组或有 intro
        for v in &data.versions {
            if v.version != "Unreleased" {
                assert!(
                    !v.groups.is_empty() || !v.intro_html.is_empty(),
                    "版本 {} 应有内容",
                    v.version
                );
            }
        }
        // preamble 应被剥离
        for v in &data.versions {
            assert!(
                !v.intro_html.contains("keepachangelog.com"),
                "preamble 不应出现在任何版本中"
            );
        }
    }

    #[test]
    fn category_label_and_css() {
        assert_eq!(ChangeCategory::Added.label(), "新增");
        assert_eq!(ChangeCategory::Fixed.label(), "修复");
        assert_eq!(ChangeCategory::Security.label(), "安全");
        assert_eq!(ChangeCategory::Added.css_class(), "added");
        assert_eq!(ChangeCategory::Security.css_class(), "security");
    }
}
