//! 前台导航项配置
//!
//! 根据当前路径生成前台 Header 所需的导航项列表。

use crate::components::header::NavItemConfig;

/// 生成前台导航项列表，当前访问的路径会被标记为激活。
///
/// 纯数据转换函数，可在任意响应式闭包里调用。
///
/// 参数：
/// - `path`：当前路径（`use_location().pathname`）
///
/// 返回：包含首页、笔记、归档（含标签）、友链、关于的导航配置数组。
/// 搜索以图标形式置于 Header 右侧（主题切换左边），不在此文本导航中。
pub fn build_nav_items(path: &str) -> Vec<NavItemConfig> {
    // 去掉尾斜杠，根路径保持 "/"。
    let path = match path.trim_end_matches('/') {
        "" => "/",
        trimmed => trimmed,
    };
    vec![
        NavItemConfig {
            route: "/",
            label: "首页",
            is_active: path == "/" || path.starts_with("/page/"),
        },
        NavItemConfig {
            route: "/notes",
            label: "笔记",
            is_active: path == "/notes" || path.starts_with("/notes/"),
        },
        NavItemConfig {
            route: "/archives",
            label: "归档",
            is_active: path == "/archives" || path.starts_with("/tags/"),
        },
        NavItemConfig {
            route: "/friends",
            label: "友链",
            is_active: path == "/friends",
        },
        NavItemConfig {
            route: "/about",
            label: "关于",
            is_active: is_about_family(path),
        },
    ]
}

/// 「关于」家族（关于 / 写作指南）的高亮判定。原型不含组件图鉴路由。
fn is_about_family(path: &str) -> bool {
    matches!(path, "/about" | "/about/writing")
}

#[cfg(test)]
mod tests {
    use super::build_nav_items;

    fn active(path: &str) -> Vec<&'static str> {
        build_nav_items(path)
            .into_iter()
            .filter(|item| item.is_active)
            .map(|item| item.label)
            .collect()
    }

    #[test]
    fn highlights_by_path() {
        assert_eq!(active("/"), ["首页"]);
        assert_eq!(active("/page/2"), ["首页"]);
        assert_eq!(active("/tags/rust"), ["归档"]);
        assert_eq!(active("/about/writing/"), ["关于"]);
        assert!(active("/post/hello").is_empty());
    }
}
