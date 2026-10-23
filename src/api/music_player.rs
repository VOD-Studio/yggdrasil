//! 前台播放器的只读启动配置。空配置不加载播放器，也不访问音乐服务。
#![allow(clippy::unused_unit, deprecated)]

use crate::models::music_player::MusicPlayerConfig;
use dioxus::prelude::*;

/// 只向访客公开 API 地址与歌单，不接受或传递管理 token、Cookie。
#[server(GetMusicPlayerConfig, "/api")]
pub async fn get_music_player_config() -> Result<Option<MusicPlayerConfig>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        static CONFIG: std::sync::LazyLock<Option<MusicPlayerConfig>> =
            std::sync::LazyLock::new(|| {
                let api = std::env::var("KITE_PLAYER_API_URL").unwrap_or_default();
                let playlists = std::env::var("KITE_PLAYER_PLAYLISTS").unwrap_or_default();
                match parse_config(&api, &playlists) {
                    Ok(config) => config,
                    Err(reason) => {
                        tracing::warn!("前台音乐播放器未启用：{reason}");
                        None
                    }
                }
            });
        Ok(CONFIG.clone())
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(None)
    }
}

#[cfg(feature = "server")]
fn parse_config(api: &str, playlists: &str) -> Result<Option<MusicPlayerConfig>, &'static str> {
    let api = api.trim();
    let playlists = playlists.trim();
    if api.is_empty() || playlists.is_empty() {
        return Ok(None);
    }
    if api.len() > 2048 {
        return Err("KITE_PLAYER_API_URL 过长");
    }
    let url = reqwest::Url::parse(api).map_err(|_| "KITE_PLAYER_API_URL 不是有效 URL")?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
        return Err("KITE_PLAYER_API_URL 必须是 HTTP(S) 地址");
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("KITE_PLAYER_API_URL 不得包含凭据、查询参数或片段");
    }
    let mut ids = Vec::new();
    for id in playlists
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if id.len() > 32 || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err("KITE_PLAYER_PLAYLISTS 必须是逗号分隔的数字歌单 ID");
        }
        if !ids.iter().any(|existing| existing == id) {
            ids.push(id.to_string());
        }
        if ids.len() > 20 {
            return Err("KITE_PLAYER_PLAYLISTS 最多配置 20 个歌单");
        }
    }
    if ids.is_empty() {
        return Ok(None);
    }
    Ok(Some(MusicPlayerConfig {
        api_url: url.as_str().trim_end_matches('/').to_string(),
        playlists: ids,
    }))
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn missing_config_disables_player() {
        for (api, playlists) in [("", "123"), ("https://music.example.com", ""), ("", "")] {
            assert_eq!(parse_config(api, playlists), Ok(None));
        }
        assert_eq!(parse_config("https://music.example.com", " , , "), Ok(None));
    }

    #[test]
    fn normalizes_url_and_deduplicates_playlists_in_order() {
        let config = parse_config(" https://music.example.com/music/ ", " 123,456,123,, ")
            .unwrap()
            .unwrap();
        assert_eq!(config.api_url, "https://music.example.com/music");
        assert_eq!(config.playlists, ["123", "456"]);
        assert_eq!(
            serde_json::to_value(config).unwrap()["apiUrl"],
            "https://music.example.com/music"
        );
    }

    #[test]
    fn rejects_credentials_and_non_public_url_parameters() {
        for api in [
            "javascript:alert(1)",
            "file:///tmp/music",
            "https://user:password@music.example.com",
            "https://music.example.com?token=secret",
            "https://music.example.com#fragment",
            "not a url",
        ] {
            assert!(parse_config(api, "123").is_err(), "{api}");
        }
    }

    #[test]
    fn accepts_local_development_http() {
        assert!(parse_config("http://localhost:3723", "123")
            .unwrap()
            .is_some());
    }

    #[test]
    fn rejects_invalid_or_excessive_playlist_ids() {
        for playlists in ["123,abc", "123/456", "<script>", "1 2"] {
            assert!(parse_config("https://music.example.com", playlists).is_err());
        }
        let many = (1..=21)
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        assert!(parse_config("https://music.example.com", &many).is_err());
    }
}
