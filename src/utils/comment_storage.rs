//! 评论草稿在浏览器 localStorage 中的持久化支持。
//!
//! 注意：所有 localStorage 读写均通过 `#[cfg(target_arch = "wasm32")]` 限定，
//! 仅在 WASM 前端生效；服务端渲染（SSR）路径下这些函数会返回 None 或空操作。

use chrono::DateTime;
use serde::{Deserialize, Serialize};

/// localStorage 中用于存储评论作者信息的键名。
const AUTHOR_KEY: &str = "yggdrasil-comment-author";

/// localStorage 中用于存储待发布评论草稿的键名。
const PENDING_KEY: &str = "yggdrasil-pending-comments";

/// 待发布评论草稿的过期时间（天）。
const TTL_DAYS: i64 = 7;

/// 评论作者信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorInfo {
    /// 昵称。
    pub name: String,
    /// 邮箱地址。
    pub email: String,
    /// 个人主页 URL。
    #[serde(default)]
    pub url: String,
}

/// 待发布评论草稿。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingComment {
    /// 评论 ID。
    pub id: i64,
    /// 父评论 ID，顶级评论为 None。
    pub parent_id: Option<i64>,
    /// 评论层级深度。
    pub depth: i32,
    /// 作者昵称。
    pub author_name: String,
    /// 作者主页 URL。
    pub author_url: Option<String>,
    /// 头像 URL。
    pub avatar_url: String,
    /// Markdown 格式的评论内容。
    pub content_md: String,
    /// 评论创建时间（RFC3339 字符串）。
    pub created_at: String,
    /// 草稿存入 localStorage 的时间（RFC3339 字符串）。
    pub stored_at: String,
}

/// 按文章 ID 组织的待发布评论草稿映射。
type PendingMap = std::collections::HashMap<String, Vec<PendingComment>>;

/// 从 localStorage 读取指定键的值。
///
/// 仅在 `wasm32` 目标下执行实际读取；SSR 构建下直接返回 None。
#[allow(unused_variables)]
fn read_storage(key: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window()?;
        let storage = window.local_storage().ok()??;
        storage.get_item(key).ok()?
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// 将值写入 localStorage 指定键。
///
/// 仅在 `wasm32` 目标下执行实际写入；SSR 构建下为空操作。
#[allow(unused_variables)]
fn write_storage(key: &str, value: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item(key, value);
            }
        }
    }
}

/// 判断给定存储时间是否已经过期。
fn is_expired(stored_at: &str) -> bool {
    let Ok(dt) = DateTime::parse_from_rfc3339(stored_at) else {
        return true;
    };
    let now_ms = crate::utils::time::now_millis();
    let stored_ms = dt.timestamp_millis();
    (now_ms - stored_ms) > (TTL_DAYS * 24 * 60 * 60 * 1000)
}

/// 保存评论作者信息到 localStorage。
pub fn save_author(info: &AuthorInfo) {
    if let Ok(json) = serde_json::to_string(info) {
        write_storage(AUTHOR_KEY, &json);
    }
}

/// 从 localStorage 读取评论作者信息。
pub fn load_author() -> Option<AuthorInfo> {
    let json = read_storage(AUTHOR_KEY)?;
    serde_json::from_str(&json).ok()
}

/// 将一条待发布评论草稿保存到指定文章的草稿列表中。
///
/// 如果同一 ID 的草稿已存在则忽略，避免重复。
pub fn save_pending_comment(post_id: i32, comment: PendingComment) {
    let mut map: PendingMap = load_all_pending();
    let key = post_id.to_string();
    let list = map.entry(key).or_default();

    // 已存在相同 ID 时直接返回，避免重复保存。
    if list.iter().any(|c| c.id == comment.id) {
        return;
    }
    list.push(comment);

    if let Ok(json) = serde_json::to_string(&map) {
        write_storage(PENDING_KEY, &json);
    }
}

/// 加载指定文章下所有未过期的待发布评论草稿。
///
/// 读取时会自动清理过期草稿，并将结果写回 localStorage。
pub fn load_pending_comments(post_id: i32) -> Vec<PendingComment> {
    let mut map = load_all_pending();
    let key = post_id.to_string();

    let comments = map.remove(&key).unwrap_or_default();
    let original_len = comments.len();
    let non_expired: Vec<PendingComment> = comments
        .into_iter()
        .filter(|c| !is_expired(&c.stored_at))
        .collect();

    // 若有草稿被清理，或该文章已无草稿，都需要把更新后的映射写回 localStorage。
    let pruned = non_expired.len() != original_len;
    if !non_expired.is_empty() {
        map.insert(key, non_expired.clone());
    }
    if pruned || non_expired.is_empty() {
        if let Ok(json) = serde_json::to_string(&map) {
            write_storage(PENDING_KEY, &json);
        }
    }

    non_expired
}

/// 从指定文章的草稿列表中移除指定 ID 的评论。
///
/// 若移除后该文章无草稿，则删除该文章对应的键。
pub fn remove_pending_ids(post_id: i32, ids: &[i64]) {
    let mut map = load_all_pending();
    let key = post_id.to_string();

    let should_remove = if let Some(comments) = map.get_mut(&key) {
        comments.retain(|c| !ids.contains(&c.id));
        comments.is_empty()
    } else {
        false
    };
    if should_remove {
        map.remove(&key);
    }

    if let Ok(json) = serde_json::to_string(&map) {
        write_storage(PENDING_KEY, &json);
    }
}

/// 清理所有文章中已过期的待发布评论草稿。
pub fn prune_all_expired() {
    let mut map = load_all_pending();
    let mut changed = false;

    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        let should_remove = if let Some(comments) = map.get_mut(&key) {
            let before = comments.len();
            comments.retain(|c| !is_expired(&c.stored_at));
            if comments.len() != before {
                changed = true;
            }
            comments.is_empty()
        } else {
            false
        };
        if should_remove {
            map.remove(&key);
            changed = true;
        }
    }

    if changed {
        if let Ok(json) = serde_json::to_string(&map) {
            write_storage(PENDING_KEY, &json);
        }
    }
}

/// 加载全部待发布评论草稿映射。
fn load_all_pending() -> PendingMap {
    let json = match read_storage(PENDING_KEY) {
        Some(j) => j,
        None => return PendingMap::new(),
    };
    serde_json::from_str(&json).unwrap_or_default()
}

/// 将待发布评论的 Markdown 内容渲染为安全的 HTML（纯文本 + 换行转 `<br>`）。
pub fn render_pending_content(md: &str) -> String {
    let escaped = crate::utils::html::escape_html(md);
    escaped.replace('\n', "<br>")
}
