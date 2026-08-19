//! 评论列表组件
//!
//! 将已审核评论与待审核评论合并成一棵树并按时间排序渲染。

use dioxus::prelude::*;

use crate::components::comments::item::CommentItem;
use crate::components::comments::pending_item::PendingCommentItem;
use crate::models::comment::PublicComment;
use crate::utils::comment_storage::PendingComment;

/// 合并后的评论节点，可能是已审核或待审核评论。
#[derive(Clone)]
enum MergedComment {
    Approved(PublicComment),
    Pending(PendingComment),
}

/// 合并两类评论并构建成树形结构。
///
/// 处理逻辑：
/// - 将已审核与待审核评论统一为 `MergedComment`
/// - 若某条评论的 parent_id 不存在于当前集合中，则视为顶层评论
/// - 同一父节点下的子评论按时间排序
/// - 使用 DFS 前序遍历生成最终展示顺序
fn merge_and_treeify(
    approved: Vec<PublicComment>,
    pending: Vec<PendingComment>,
) -> Vec<MergedComment> {
    use std::collections::{HashMap, HashSet};

    let all: Vec<MergedComment> = approved
        .into_iter()
        .map(MergedComment::Approved)
        .chain(pending.into_iter().map(MergedComment::Pending))
        .collect();

    let all_ids: HashSet<i64> = all
        .iter()
        .map(|c| match c {
            MergedComment::Approved(c) => c.id,
            MergedComment::Pending(c) => c.id,
        })
        .collect();

    // 按 parent_id 分组，处理指向不存在父节点的 parent_id
    let mut children_map: HashMap<Option<i64>, Vec<MergedComment>> = HashMap::new();
    for comment in all {
        let parent_id = match &comment {
            MergedComment::Approved(c) => c.parent_id,
            MergedComment::Pending(c) => c.parent_id,
        };
        let effective_parent = match parent_id {
            Some(pid) if !all_ids.contains(&pid) => None,
            _ => parent_id,
        };
        children_map
            .entry(effective_parent)
            .or_default()
            .push(comment);
    }

    // 每个父节点下的子评论按创建时间排序
    for children in children_map.values_mut() {
        children.sort_by(|a, b| {
            let time_a = match a {
                MergedComment::Approved(c) => c.created_at_iso.as_str(),
                MergedComment::Pending(c) => c.created_at.as_str(),
            };
            let time_b = match b {
                MergedComment::Approved(c) => c.created_at_iso.as_str(),
                MergedComment::Pending(c) => c.created_at.as_str(),
            };
            time_a.cmp(time_b)
        });
    }

    // 深度优先遍历生成树形顺序
    fn dfs(
        parent_id: Option<i64>,
        children_map: &HashMap<Option<i64>, Vec<MergedComment>>,
        result: &mut Vec<MergedComment>,
    ) {
        if let Some(children) = children_map.get(&parent_id) {
            for child in children {
                result.push(child.clone());
                let child_id = match child {
                    MergedComment::Approved(c) => Some(c.id),
                    MergedComment::Pending(c) => Some(c.id),
                };
                dfs(child_id, children_map, result);
            }
        }
    }

    let mut result = Vec::new();
    dfs(None, &children_map, &mut result);
    result
}

/// 评论列表组件。
///
/// Props：
/// - `comments`：已审核评论列表
/// - `pending`：待审核评论列表
/// - `post_id`：所属文章 ID
///
/// 根据两类评论构建合并树，依次渲染为 `CommentItem` 或 `PendingCommentItem`。
#[component]
pub fn CommentList(
    comments: Vec<PublicComment>,
    pending: Vec<PendingComment>,
    post_id: i32,
) -> Element {
    let merged = merge_and_treeify(comments, pending);

    rsx! {
        div { class: "space-y-0 divide-y divide-[var(--color-paper-border)]/40",
            for item in merged {
                match item {
                    MergedComment::Approved(comment) => rsx! {
                        CommentItem { key: "{comment.id}", comment, post_id }
                    },
                    MergedComment::Pending(comment) => rsx! {
                        PendingCommentItem { key: "{comment.id}", comment, post_id }
                    },
                }
            }
        }
    }
}
