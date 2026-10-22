//! 前台音乐播放器的公开配置，不包含服务账号或管理凭据。

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPlayerConfig {
    /// kite-server 公开播放器服务的浏览器可访问地址。
    pub api_url: String,
    /// 允许在播放器中展示的歌单；实际访问权限由 kite-server 白名单控制。
    pub playlists: Vec<String>,
}
