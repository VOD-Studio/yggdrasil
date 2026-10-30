//! 浏览器端 `DateTime<Utc>` 反序列化：用 chrono 的严格 RFC 3339 解析（`parse_from_rfc3339`）。
//!
//! 默认 `Deserialize` 走 `FromStr` → `parse_rfc3339_relaxed` → 通用 `parse_internal`，
//! 会把 RFC 2822 等整套解析器带进 WASM。服务端写出的是 chrono `Serialize`（`write_rfc3339`，
//! AutoSi，`Z` 结尾），严格解析器全部接受。只经 `cfg_attr(target_arch = "wasm32", …)` 生效。

use chrono::{DateTime, Utc};
use serde::de::{Deserializer, Error, Visitor};
use std::fmt;

fn parse<E: Error>(s: &str) -> Result<DateTime<Utc>, E> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(E::custom)
}

struct Rfc3339;

impl<'de> Visitor<'de> for Rfc3339 {
    type Value = DateTime<Utc>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("RFC 3339 时间")
    }

    fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
        parse(v)
    }
}

/// `DateTime<Utc>` 字段：`serde(deserialize_with = "crate::models::rfc3339::deserialize")`。
pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<DateTime<Utc>, D::Error> {
    d.deserialize_str(Rfc3339)
}

struct OptionRfc3339;

impl<'de> Visitor<'de> for OptionRfc3339 {
    type Value = Option<DateTime<Utc>>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("RFC 3339 时间或 null")
    }

    fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        deserialize(d).map(Some)
    }
}

/// `Option<DateTime<Utc>>` 字段：配合 `serde(default)`，缺失字段仍得到 `None`。
pub fn deserialize_option<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<DateTime<Utc>>, D::Error> {
    d.deserialize_option(OptionRfc3339)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Row {
        #[serde(deserialize_with = "super::deserialize")]
        at: chrono::DateTime<Utc>,
        #[serde(default, deserialize_with = "super::deserialize_option")]
        maybe: Option<chrono::DateTime<Utc>>,
    }

    #[test]
    fn chrono_serialized_times_round_trip() {
        let whole = Utc.with_ymd_and_hms(2026, 10, 30, 8, 5, 9).unwrap();
        let fractional = whole + chrono::Duration::nanoseconds(123_456_789);
        for row in [
            Row {
                at: whole,
                maybe: Some(fractional),
            },
            Row {
                at: fractional,
                maybe: Some(whole),
            },
        ] {
            let json = serde_json::to_string(&row).unwrap();
            assert_eq!(serde_json::from_str::<Row>(&json).unwrap(), row);
        }
    }

    #[test]
    fn null_and_missing_option_are_none() {
        let at = Utc.with_ymd_and_hms(2026, 10, 30, 0, 0, 0).unwrap();
        for json in [
            r#"{"at":"2026-10-30T00:00:00Z","maybe":null}"#,
            r#"{"at":"2026-10-30T00:00:00Z"}"#,
        ] {
            assert_eq!(
                serde_json::from_str::<Row>(json).unwrap(),
                Row { at, maybe: None }
            );
        }
    }

    #[test]
    fn date_only_is_rejected() {
        assert!(serde_json::from_str::<Row>(r#"{"at":"2026-10-30"}"#).is_err());
        assert!(serde_json::from_str::<Row>(
            r#"{"at":"2026-10-30T00:00:00Z","maybe":"2026-10-30"}"#
        )
        .is_err());
    }
}
