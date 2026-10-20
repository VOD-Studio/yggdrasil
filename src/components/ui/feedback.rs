//! 全站提示（Toast）：操作结果的短暂反馈，出现在屏幕底部居中；区块加载失败卡片。
use super::controls::{Button, ButtonSize, ButtonVariant, IconButton};
use super::icons::{Icon, IconName};
use dioxus::prelude::*;

/// 提示的语气。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastTone {
    Info,
    Success,
    Danger,
}

impl ToastTone {
    fn icon(self) -> IconName {
        match self {
            ToastTone::Info => IconName::Info,
            ToastTone::Success => IconName::CircleCheck,
            ToastTone::Danger => IconName::CircleAlert,
        }
    }

    fn attr(self) -> &'static str {
        match self {
            ToastTone::Info => "info",
            ToastTone::Success => "success",
            ToastTone::Danger => "danger",
        }
    }

    /// 停留时长：错误多留一会儿，给人读完的时间。
    fn duration_ms(self) -> u32 {
        match self {
            ToastTone::Danger => 6000,
            _ => 3600,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ToastEntry {
    id: u64,
    tone: ToastTone,
    message: String,
    leaving: bool,
}

/// 同时最多显示的提示条数，更早的会被挤掉。
const MAX_TOASTS: usize = 3;
/// 退出动画时长，与 ui.css 的 `toast-leave` 一致。
const LEAVE_MS: u32 = 180;

/// 发提示的句柄，经 [`use_toast`] 取得。
#[derive(Clone, Copy, PartialEq)]
pub struct Toaster {
    entries: Signal<Vec<ToastEntry>>,
    next_id: Signal<u64>,
}

impl Toaster {
    /// 显示一条提示，到时自动消失。
    pub fn show(mut self, tone: ToastTone, message: impl Into<String>) {
        let id = *self.next_id.peek();
        self.next_id.set(id + 1);
        let mut entries = self.entries;
        entries.with_mut(|list| {
            list.push(ToastEntry {
                id,
                tone,
                message: message.into(),
                leaving: false,
            });
            let overflow = list.len().saturating_sub(MAX_TOASTS);
            list.drain(..overflow);
        });
        spawn(async move {
            crate::utils::time::sleep_ms(tone.duration_ms()).await;
            self.dismiss(id);
        });
    }

    #[allow(dead_code)] // 随页面重写逐步启用
    pub fn success(self, message: impl Into<String>) {
        self.show(ToastTone::Success, message);
    }

    #[allow(dead_code)] // 随页面重写逐步启用
    pub fn error(self, message: impl Into<String>) {
        self.show(ToastTone::Danger, message);
    }

    /// 先播退出动画，再从列表移除。
    fn dismiss(self, id: u64) {
        let mut entries = self.entries;
        let mut found = false;
        entries.with_mut(|list| {
            if let Some(entry) = list.iter_mut().find(|e| e.id == id && !e.leaving) {
                entry.leaving = true;
                found = true;
            }
        });
        if found {
            spawn(async move {
                crate::utils::time::sleep_ms(LEAVE_MS).await;
                entries.with_mut(|list| list.retain(|e| e.id != id));
            });
        }
    }
}

/// 在应用根部提供提示上下文（`AppRouter` 调用一次）。
pub fn use_toast_provider() -> Toaster {
    let entries = use_signal(Vec::new);
    let next_id = use_signal(|| 0_u64);
    use_context_provider(|| Toaster { entries, next_id })
}

/// 取得发提示的句柄。
#[allow(dead_code)] // 随页面重写逐步启用
pub fn use_toast() -> Toaster {
    use_context::<Toaster>()
}

/// 提示显示区：固定在屏幕底部居中，`aria-live` 让读屏软件播报新提示。
#[component]
pub fn ToastRegion() -> Element {
    let toaster = use_context::<Toaster>();
    let entries = toaster.entries;
    rsx! {
        div { class: "toast-region", role: "region", aria_label: "通知", aria_live: "polite",
            for entry in entries() {
                div {
                    key: "{entry.id}",
                    class: "toast",
                    role: if entry.tone == ToastTone::Danger { "alert" } else { "status" },
                    "data-tone": entry.tone.attr(),
                    "data-leaving": entry.leaving.then_some("true"),
                    span { class: "toast-icon", Icon { name: entry.tone.icon(), size: 16 } }
                    span { class: "min-w-0 flex-1", "{entry.message}" }
                    IconButton {
                        icon: IconName::X,
                        label: "关闭提示",
                        size: ButtonSize::Xs,
                        onclick: move |_| toaster.dismiss(entry.id),
                    }
                }
            }
        }
    }
}

/// 区块加载失败卡片：页面其余部分照常显示，只有出错的区块换成它；重试在原位完成。
///
/// `retry_failed` 为再次失败后的文案；重试中按钮转圈并向读屏软件播报。
#[component]
pub fn LoadError(
    #[props(into)] title: String,
    #[props(default)] retrying: bool,
    #[props(default)] retry_failed: bool,
    #[props(default)] class: String,
    on_retry: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "load-error {class}", role: "group", aria_label: "{title}",
            div { class: "load-error-copy",
                h3 { "{title}" }
                p { role: "status", aria_live: "polite", aria_atomic: "true",
                    if retry_failed {
                        "这次仍未加载成功，稍后再试一次吧。"
                    } else {
                        "网络或服务出了点问题，稍后再试一次。"
                    }
                    if retrying {
                        span { class: "sr-only", "正在重新加载，请稍候。" }
                    }
                }
            }
            Button {
                variant: ButtonVariant::Secondary,
                icon: IconName::RotateCw,
                loading: retrying,
                onclick: move |_| {
                    if !retrying {
                        on_retry.call(());
                    }
                },
                "重新加载"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn danger_toasts_stay_longer() {
        assert!(ToastTone::Danger.duration_ms() > ToastTone::Success.duration_ms());
        assert_eq!(ToastTone::Success.icon(), IconName::CircleCheck);
        assert_eq!(ToastTone::Danger.attr(), "danger");
    }
}
