//! 前台导航项配置
//!
//! 根据当前路径生成前台 Header 所需的导航项列表。

use crate::components::header::NavItemConfig;
use crate::routes::{is_showcase_path, Route};

/// 生成前台导航项列表，当前访问的路径会被标记为激活。
///
/// 纯数据转换函数，可在任意响应式闭包里调用。
///
/// 参数：
/// - `path`：当前路径（`use_location().pathname`，不含查询串；尾斜杠会被忽略）
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
            route: Route::Home,
            label: "首页",
            is_active: path == "/" || path.starts_with("/page/"),
        },
        NavItemConfig {
            route: Route::Notes,
            label: "笔记",
            is_active: path == "/notes" || path.starts_with("/notes/"),
        },
        NavItemConfig {
            route: Route::Archives,
            label: "归档",
            is_active: path == "/archives" || path.starts_with("/tags/"),
        },
        NavItemConfig {
            route: Route::Friends,
            label: "友链",
            is_active: path == "/friends",
        },
        NavItemConfig {
            route: Route::About,
            label: "关于",
            is_active: is_about_family(path),
        },
    ]
}

/// 「关于」家族（关于 / 写作指南 / 组件图鉴）的高亮判定。
///
/// 图鉴路径（`/about/components` 及其子路径）经 [`is_showcase_path`] 判定。
fn is_about_family(path: &str) -> bool {
    matches!(path, "/about" | "/about/writing") || is_showcase_path(path)
}

#[cfg(test)]
mod tests {
    use super::build_nav_items;
    use crate::routes::Route;

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
        assert_eq!(active("/notes"), ["笔记"]);
        assert_eq!(active("/notes/book/3"), ["笔记"]);
        assert_eq!(active("/notes/some-note"), ["笔记"]);
        assert_eq!(active("/archives"), ["归档"]);
        assert_eq!(active("/tags/rust"), ["归档"]);
        assert_eq!(active("/friends"), ["友链"]);
        assert_eq!(active("/about"), ["关于"]);
        assert_eq!(active("/about/writing/"), ["关于"]);
        assert!(active("/post/hello").is_empty());
        assert!(active("/search").is_empty());
        assert!(active("/changelog").is_empty());
    }

    #[test]
    fn showcase_belongs_to_about_family() {
        assert_eq!(active("/about/components"), ["关于"]);
        assert_eq!(active("/about/components/"), ["关于"]);
        assert_eq!(active("/about/components/button"), ["关于"]);
        // 前缀相同但不是图鉴路径。
        assert!(active("/about/componentsx").is_empty());
    }

    #[test]
    fn tags_index_without_slug_is_not_highlighted() {
        // `/tags` 由服务端永久重定向到 `/archives`，不属于任何栏目。
        assert!(active("/tags").is_empty());
    }

    #[test]
    fn items_keep_fixed_order_and_targets() {
        let items = build_nav_items("/");
        let seen: Vec<_> = items
            .iter()
            .map(|item| (item.label, item.route.to_string()))
            .collect();
        assert_eq!(
            seen,
            [
                ("首页", "/".to_string()),
                ("笔记", "/notes".to_string()),
                ("归档", "/archives".to_string()),
                ("友链", "/friends".to_string()),
                ("关于", "/about".to_string()),
            ]
        );
        assert_eq!(items[0].route, Route::Home);
    }
}
