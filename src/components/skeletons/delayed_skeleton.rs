//! 延迟骨架屏包装组件
//!
//! 200ms 内完全不渲染骨架屏，避免快速加载时骨架屏一闪而过；
//! 超过 200ms 后渲染骨架屏；占位块自带扫光（`.skeleton`），提示正在加载。
//! 导航挂起（`is-route-pending`）期间挂载时立即显示，接上路由骨架。

use leptos::prelude::*;
use std::time::Duration;

/// 骨架屏显示延迟（毫秒）。
///
/// 加载时间低于此值时完全不显示骨架屏，避免闪烁。
const SKELETON_DELAY_MS: u64 = 200;

/// 延迟渲染的骨架屏包装组件。
///
/// 前 `SKELETON_DELAY_MS` 毫秒内不渲染任何内容；
/// 超过延迟后渲染子元素。计时只在浏览器端（`Effect` 内）启动，
/// SSR 与 hydration 首帧都输出空内容，DOM 一致。
///
/// 快加载（< 200ms）：组件在渲染前就被卸载，用户完全看不到骨架屏。
/// 慢加载：骨架屏正常显示，占位块自带扫光。
/// 导航挂起期间挂载（分包迟到、用户已看过路由骨架）：立即显示，不再留 200ms 空白。
/// `pulse = true` 额外给整体加呼吸动画，仅用于没有使用 `SkeletonBox` 的旧骨架。
#[component]
pub fn DelayedSkeleton(children: ChildrenFn, #[prop(optional)] pulse: bool) -> impl IntoView {
    let visible = RwSignal::new(false);

    Effect::new(move |_| {
        // 分包迟到的导航刚提交：用户已等过 300ms 并看着路由骨架，直接显示，不再留 200ms 空白。
        if crate::bridges::navigation::route_pending() {
            visible.set(true);
            return;
        }
        set_timeout(
            move || {
                // 计时到期前组件可能已卸载（信号已释放），用 try_set 静默跳过。
                visible.try_set(true);
            },
            Duration::from_millis(SKELETON_DELAY_MS),
        );
    });

    view! {
        <Show when=move || visible.get()>
            <div class=if pulse { "animate-pulse" } else { "" }>{children()}</div>
        </Show>
    }
}
