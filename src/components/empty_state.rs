//! 空列表、无搜索结果等状态共用的插画与行动入口。

use dioxus::prelude::*;

use crate::components::ui::BTN_PRIMARY;

const EMPTY_STATE_IMAGES: &[&str] = &[
    "/images/empty-state/dog-camera.webp",
    "/images/empty-state/dog-wind-chime.webp",
    "/images/empty-state/dog-delivery.webp",
    "/images/empty-state/dog-pajamas.webp",
    "/images/empty-state/dog-watermelon.webp",
    "/images/empty-state/dog-heart.webp",
    "/images/empty-state/dog-head-pat.webp",
    "/images/empty-state/dog-mirror.webp",
    "/images/empty-state/dog-chopsticks.webp",
    "/images/empty-state/dog-grass.webp",
    "/images/empty-state/dog-polaroid.webp",
    "/images/empty-state/dog-autumn-leaf.webp",
    "/images/empty-state/dog-laptop.webp",
    "/images/empty-state/dog-heartbeat.webp",
    "/images/empty-state/dog-travel.webp",
    "/images/empty-state/dog-ring.webp",
    "/images/empty-state/dog-hug.webp",
    "/images/empty-state/dog-sparklers.webp",
];

/// 空状态行动按钮。跳转或重试由调用方负责。
#[derive(Props, Clone, PartialEq)]
pub struct EmptyStateAction {
    #[props(into)]
    pub label: String,
    pub onclick: EventHandler<()>,
}

/// 展示标题、说明与随机小狗插画，可选行动按钮或指定配图。
#[component]
pub fn EmptyState(
    #[props(into, default = "还没有文章".to_string())] title: String,
    #[props(into, default = String::new())] description: String,
    /// 指定配图时覆盖随机图库；空字符串使用随机图库。
    #[props(into, default = String::new())]
    image: String,
    #[props(default)] action: Option<EmptyStateAction>,
) -> Element {
    // 首次 SSR 与 hydration 使用同一张图；挂载后随机一次，后续重绘保持不变。
    let selected = use_signal(|| 0_usize);
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        {
            let mut selected = selected;
            selected.set((js_sys::Math::random() * EMPTY_STATE_IMAGES.len() as f64) as usize);
        }
    });
    let src = if image.is_empty() {
        EMPTY_STATE_IMAGES[selected()]
    } else {
        &image
    };

    rsx! {
        section { class: "empty-state",
            div { class: "empty-state-art",
                img { src: "{src}", alt: "", width: "192", height: "192", draggable: "false" }
            }
            span { class: "empty-state-kicker", "YGGDRASIL / 一处留白" }
            h2 { "{title}" }
            if !description.is_empty() {
                p { class: "empty-state-description", "{description}" }
            }
            if let Some(act) = action {
                button {
                    class: "{BTN_PRIMARY} empty-state-action",
                    r#type: "button",
                    onclick: move |_| act.onclick.call(()),
                    "{act.label}"
                }
            }
        }
    }
}
