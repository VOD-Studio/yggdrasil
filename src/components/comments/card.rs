//! 评论卡片外壳组件
//!
//! `CommentItem`（已审核）与 `PendingCommentItem`（待审核）曾各自手抄几乎相同的
//! 卡片骨架：深度缩进（孤儿评论归零、`margin-left` 阶梯、子评论左边框）、
//! 头像 + 内容两栏布局、作者名/时间头部行。两处独立维护一度导致悄然漂移
//! （已审核评论的作者链接一度缺失文本子节点，待审核评论的对应实现却是对的）。
//! 本组件抽取该共享骨架，两个调用方只负责各自的差异部分（作者徽章、状态徽章、
//! 时间展示、正文 HTML、正文下方的附加区域）。

use dioxus::prelude::*;

use crate::components::ui::UserAvatar;

/// 评论卡片外壳组件。
///
/// Props：
/// - `depth`：归一化后的嵌套深度（孤儿评论已由调用方按 0 处理）
/// - `muted`：是否降低整体透明度展示（待审核评论用，缺省 `false`）
/// - `avatar_url` / `author_name`：头像 URL 与无障碍 alt 文本
/// - `author_element`：作者名展示节点（链接或纯文本，由调用方根据 `author_url` 构建）
/// - `author_badge`：作者名与分隔符之间的徽章（如已审核评论的"作者"标签）；
///   无需展示时传 `rsx! {}`
/// - `timestamp`：时间展示节点（由调用方构建，两种评论的时间语义不同：已审核评论
///   展示格式化后的绝对时间，待审核评论展示实时计算的相对时间）
/// - `status_badge`：时间之后的状态徽章（如待审核评论的"审核中"标签）；
///   无需展示时传 `rsx! {}`
/// - `content_html`：已渲染为安全 HTML 的评论正文
/// - `content_extra_class`：正文容器附加 CSS 类（已审核评论需额外挂 `md-content`
///   以命中全局 Markdown 排版/代码高亮 CSS 作用域，缺省空串）
///
/// 样式见 `styles/components/comments.css`：顶层评论之间以发丝线分隔，回复左侧一条
/// 细线并按 `--comment-indent` 缩进。
/// - `children`：正文下方的附加区域（如已审核评论的回复按钮与回复表单）；
///   无需展示时传 `rsx! {}`
#[component]
pub fn CommentCardShell(
    depth: i32,
    #[props(default)] muted: bool,
    avatar_url: String,
    author_name: String,
    author_element: Element,
    author_badge: Element,
    timestamp: Element,
    status_badge: Element,
    content_html: String,
    #[props(default)] content_extra_class: &'static str,
    children: Element,
) -> Element {
    let mut class = String::from("comment");
    if depth > 0 {
        class.push_str(" is-child");
    }
    if muted {
        class.push_str(" is-pending");
    }
    // 回复按层级缩进，五层后不再加深，深层回复不至于被挤到最右。
    let style = (depth > 1).then(|| format!("--comment-indent: {};", depth.min(5) - 1));

    rsx! {
        div { class, style,
            UserAvatar {
                name: author_name,
                avatar_url: Some(avatar_url),
                class: "comment-avatar",
            }
            div { class: "comment-main",
                div { class: "comment-head",
                    {author_element}
                    {author_badge}
                    {timestamp}
                    {status_badge}
                }
                div {
                    class: "comment-body {content_extra_class}",
                    dangerous_inner_html: "{content_html}",
                }
                {children}
            }
        }
    }
}
