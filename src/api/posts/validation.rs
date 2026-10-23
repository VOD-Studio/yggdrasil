//! 与认证方式无关的文章写入校验。

use super::service::WriteError;

/// 与 `migrations/002_posts.sql` 的列宽对齐；Postgres 按字符计长。
const MAX_TITLE_CHARS: usize = 200;
const MAX_SUMMARY_CHARS: usize = 500;
const MAX_COVER_CHARS: usize = 500;
/// 单篇文章标签数上限（与笔记一致）。
const MAX_TAGS: usize = 16;
const MAX_TAG_CHARS: usize = 50;

pub(super) fn title(value: &str) -> Result<(), WriteError> {
    if value.trim().is_empty() {
        return Err(WriteError::Invalid("标题不能为空".into()));
    }
    if value.trim().chars().count() > MAX_TITLE_CHARS {
        return Err(WriteError::Invalid("标题过长（上限 200 字符）".into()));
    }
    Ok(())
}

pub(super) fn content(value: &str) -> Result<(), WriteError> {
    if value.trim().is_empty() {
        return Err(WriteError::Invalid("内容不能为空".into()));
    }
    Ok(())
}

pub(super) fn summary(value: &str) -> Result<(), WriteError> {
    if value.trim().chars().count() > MAX_SUMMARY_CHARS {
        return Err(WriteError::Invalid("摘要过长（上限 500 字符）".into()));
    }
    Ok(())
}

pub(super) fn cover(value: &str) -> Result<(), WriteError> {
    if value.trim().chars().count() > MAX_COVER_CHARS {
        return Err(WriteError::Invalid("封面地址过长（上限 500 字符）".into()));
    }
    Ok(())
}

/// 标签数量与单标签长度；与笔记一样在清洗（去空白、去重）之后校验。
pub(super) fn tags(raw: &[String]) -> Result<(), WriteError> {
    let tags = super::helpers::clean_tags(raw);
    if tags.len() > MAX_TAGS {
        return Err(WriteError::Invalid("标签最多 16 个".into()));
    }
    if tags.iter().any(|t| t.chars().count() > MAX_TAG_CHARS) {
        return Err(WriteError::Invalid("单个标签过长（上限 50 字符）".into()));
    }
    Ok(())
}

pub(super) fn slug(value: &str) -> Result<(), WriteError> {
    if !crate::api::slug::is_valid_slug(value) {
        return Err(WriteError::Invalid(
            "slug 格式无效，只能包含字母、数字、连字符和下划线".into(),
        ));
    }
    Ok(())
}

pub(super) fn base_slug(title_value: &str, supplied: Option<&str>) -> Result<String, WriteError> {
    match supplied.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => {
            slug(s)?;
            Ok(s.into())
        }
        None => Ok(crate::api::slug::slugify(title_value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_title_and_content_before_database_access() {
        assert!(title(" \t\n").is_err());
        assert!(content(" \t\n").is_err());
        assert!(title("文章").is_ok());
        assert!(content("正文").is_ok());
    }

    #[test]
    fn slug_is_validated_or_generated_from_title() {
        assert_eq!(
            base_slug("标题", Some(" custom-slug ")).unwrap(),
            "custom-slug"
        );
        assert_eq!(base_slug("标题", None).unwrap(), "biao-ti");
        assert!(base_slug("标题", Some("bad/slug")).is_err());
    }

    #[test]
    fn rejects_overlong_fields_and_too_many_tags() {
        assert!(title(&"字".repeat(200)).is_ok());
        assert!(title(&"字".repeat(201)).is_err());
        assert!(summary(&"字".repeat(500)).is_ok());
        assert!(summary(&"字".repeat(501)).is_err());
        assert!(cover(&"a".repeat(500)).is_ok());
        assert!(cover(&"a".repeat(501)).is_err());

        let sixteen: Vec<String> = (0..16).map(|i| format!("t{i}")).collect();
        let seventeen: Vec<String> = (0..17).map(|i| format!("t{i}")).collect();
        assert!(tags(&sixteen).is_ok());
        assert!(tags(&seventeen).is_err());

        assert!(tags(&["字".repeat(50)]).is_ok());
        assert!(tags(&["字".repeat(51)]).is_err());

        // 清洗（去空白、大小写去重）后只剩 1 个，不应被数量上限误伤。
        let mut dup: Vec<String> = (0..15).map(|i| format!("t{i}")).collect();
        dup.extend([" a ".to_owned(), "A".to_owned()]);
        assert!(tags(&dup).is_ok());
    }
}
