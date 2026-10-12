//! 与认证方式无关的文章写入校验。

use super::service::WriteError;

pub(super) fn title(value: &str) -> Result<(), WriteError> {
    if value.trim().is_empty() {
        return Err(WriteError::Invalid("标题不能为空".into()));
    }
    Ok(())
}

pub(super) fn content(value: &str) -> Result<(), WriteError> {
    if value.trim().is_empty() {
        return Err(WriteError::Invalid("内容不能为空".into()));
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
}
