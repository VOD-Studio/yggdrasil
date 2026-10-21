//! 前台导航项配置
//!
//! 根据当前路由生成前台 Header 所需的导航项列表。

use crate::components::header::NavItemConfig;
use crate::router::Route;

/// 生成前台导航项列表，当前访问的路由会被标记为激活。
///
/// 纯数据转换函数（非 Dioxus hook——不调用 use_signal/use_effect 等，不受 hook
/// 调用顺序约束，可在渲染体内任意位置调用），故不用易生歧义的 `use_` 前缀命名。
///
/// 参数：
/// - `route`：当前路由
///
/// 返回：包含首页、归档（含标签）、友链、关于的导航配置数组。
/// 搜索以图标形式置于 Header 右侧（主题切换左边），不在此文本导航中。
pub fn build_nav_items(route: Route) -> Vec<NavItemConfig> {
    vec![
        NavItemConfig {
            route: Route::Home {},
            label: "首页",
            is_active: matches!(route, Route::Home {} | Route::HomePage { .. }),
        },
        NavItemConfig {
            route: Route::Notes {},
            label: "笔记",
            is_active: matches!(
                route,
                Route::Notes {} | Route::NoteDetail { .. } | Route::NotebookDetail { .. }
            ),
        },
        NavItemConfig {
            route: Route::Archives {},
            label: "归档",
            is_active: matches!(route, Route::Archives {} | Route::TagDetail { .. }),
        },
        NavItemConfig {
            route: Route::Friends {},
            label: "友链",
            is_active: matches!(route, Route::Friends {}),
        },
        NavItemConfig {
            route: Route::About {},
            label: "关于",
            is_active: is_about_family(&route),
        },
    ]
}

/// 「关于」家族（关于 / 写作指南 / 组件图鉴）的高亮判定。
///
/// 图鉴变体仅存在于 full 客户端（plan §6.1）；reader 构建经
/// [`crate::router::route_is_showcase`] 恒为 false。
fn is_about_family(route: &Route) -> bool {
    matches!(route, Route::About {} | Route::WritingGuide {})
        || crate::router::route_is_showcase(route)
}
