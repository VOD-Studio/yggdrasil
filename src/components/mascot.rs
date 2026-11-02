//! 吉祥物像素贴图：橙发女孩的各种表情，只作装饰，对读屏隐藏。

use leptos::prelude::*;

/// 表情与 `public/images/mascot/*.webp` 一一对应（256×256，由原始 1024 图缩放）。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mascot {
    Wave,
    Panic,
    Cry,
    Hearts,
    Sit,
    Sleepy,
    Doze,
    Blissful,
    Peace,
    Curious,
    Pout,
}

impl Mascot {
    pub const fn src(self) -> &'static str {
        match self {
            Mascot::Wave => "/images/mascot/wave.webp",
            Mascot::Panic => "/images/mascot/panic.webp",
            Mascot::Cry => "/images/mascot/cry.webp",
            Mascot::Hearts => "/images/mascot/hearts.webp",
            Mascot::Sit => "/images/mascot/sit.webp",
            Mascot::Sleepy => "/images/mascot/sleepy.webp",
            Mascot::Doze => "/images/mascot/doze.webp",
            Mascot::Blissful => "/images/mascot/blissful.webp",
            Mascot::Peace => "/images/mascot/peace.webp",
            Mascot::Curious => "/images/mascot/curious.webp",
            Mascot::Pout => "/images/mascot/pout.webp",
        }
    }

    /// 装饰性 `<img>`。`class` 需写全，含基础类 `mascot`（例：`"mascot home-hero-mascot"`）。
    pub fn img(self, class: &'static str) -> impl IntoView {
        view! {
            <img class=class src=self.src() alt="" aria-hidden="true"
                width="256" height="256" decoding="async" loading="lazy"/>
        }
    }
}
