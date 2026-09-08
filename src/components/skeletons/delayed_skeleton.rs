//! 延迟骨架屏包装组件
//!
//! 200ms 内完全不渲染骨架屏，避免快速加载时骨架屏一闪而过；
//! 超过 200ms 后渲染骨架屏并带有 pulse 动画，提示正在加载。

use crate::utils::time::sleep_ms;
use dioxus::prelude::*;

/// 骨架屏显示延迟（毫秒）。
///
/// 加载时间低于此值时完全不显示骨架屏，避免闪烁。
const SKELETON_DELAY_MS: u32 = 200;

/// 延迟渲染的骨架屏包装组件。
///
/// 前 `SKELETON_DELAY_MS` 毫秒内不渲染任何内容；
/// 超过延迟后渲染子元素并启动 pulse 动画。
///
/// 快加载（< 200ms）：组件在渲染前就被卸载，用户完全看不到骨架屏。
/// 慢加载：骨架屏正常显示并 pulse，提示正在加载。
/// `pulse = false` 可保留延迟、关闭外层呼吸动画，供自带扫光的骨架使用。
#[component]
pub fn DelayedSkeleton(children: Element, #[props(default = true)] pulse: bool) -> Element {
    let mut visible = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            sleep_ms(SKELETON_DELAY_MS).await;
            visible.set(true);
        });
    });

    if visible() {
        rsx! {
            div { class: if pulse { "animate-pulse" } else { "" }, {children} }
        }
    } else {
        rsx! {}
    }
}
