//! 全站提示（Toast）：操作结果的短暂反馈，出现在屏幕底部居中；区块加载失败卡片。
use super::controls::{Button, ButtonSize, ButtonVariant, IconButton};
use super::icons::{Icon, IconName};
use leptos::prelude::*;
use std::time::Duration;

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
///
/// 两个信号都创建在应用根 owner 上；计时器用普通 `set_timeout` 直接操作它们，
/// 调用页卸载（例如提示后立即导航）也不会取消自动消失。
#[derive(Clone, Copy, PartialEq)]
pub struct Toaster {
    entries: RwSignal<Vec<ToastEntry>>,
    next_id: RwSignal<u64>,
}

impl Toaster {
    fn new() -> Self {
        Self {
            entries: RwSignal::new(Vec::new()),
            next_id: RwSignal::new(0),
        }
    }

    /// 显示一条提示，到时自动消失。
    pub fn show(self, tone: ToastTone, message: impl Into<String>) {
        let id = self.next_id.get_untracked();
        self.next_id.set(id + 1);
        let message = message.into();
        self.entries.update(|list| {
            list.push(ToastEntry {
                id,
                tone,
                message,
                leaving: false,
            });
            let overflow = list.len().saturating_sub(MAX_TOASTS);
            list.drain(..overflow);
        });
        set_timeout(
            move || self.dismiss(id),
            Duration::from_millis(u64::from(tone.duration_ms())),
        );
    }

    pub fn success(self, message: impl Into<String>) {
        self.show(ToastTone::Success, message);
    }

    pub fn error(self, message: impl Into<String>) {
        self.show(ToastTone::Danger, message);
    }

    /// 先播退出动画，再从列表移除。
    fn dismiss(self, id: u64) {
        let mut found = false;
        self.entries.update(|list| {
            if let Some(entry) = list.iter_mut().find(|e| e.id == id && !e.leaving) {
                entry.leaving = true;
                found = true;
            }
        });
        if found {
            let entries = self.entries;
            set_timeout(
                move || entries.update(|list| list.retain(|e| e.id != id)),
                Duration::from_millis(u64::from(LEAVE_MS)),
            );
        }
    }
}

/// 在应用根部提供提示上下文（`App` 调用一次）。
pub fn provide_toaster() -> Toaster {
    let toaster = Toaster::new();
    provide_context(toaster);
    toaster
}

/// 取得发提示的句柄。
pub fn use_toast() -> Toaster {
    expect_context::<Toaster>()
}

/// 提示显示区：固定在屏幕底部居中，`aria-live` 让读屏软件播报新提示。
#[component]
pub fn ToastRegion() -> impl IntoView {
    let toaster = use_toast();
    let entries = toaster.entries;
    view! {
        <div class="toast-region" role="region" aria-label="通知" aria-live="polite">
            <For
                each=move || entries.get()
                key=|entry| entry.id
                children=move |entry| {
                    let id = entry.id;
                    let leaving = move || {
                        entries.with(|list| list.iter().any(|e| e.id == id && e.leaving))
                    };
                    view! {
                        <div
                            class="toast"
                            role=if entry.tone == ToastTone::Danger { "alert" } else { "status" }
                            data-tone=entry.tone.attr()
                            data-leaving=move || leaving().then_some("true")
                        >
                            <span class="toast-icon">
                                <Icon name=entry.tone.icon() size=16/>
                            </span>
                            <span class="min-w-0 flex-1">{entry.message}</span>
                            <IconButton
                                icon=IconName::X
                                label="关闭提示"
                                size=ButtonSize::Xs
                                onclick=move |_| toaster.dismiss(id)
                            />
                        </div>
                    }
                }
            />
        </div>
    }
}

/// 区块加载失败卡片：页面其余部分照常显示，只有出错的区块换成它；重试在原位完成。
///
/// `retry_failed` 为再次失败后的文案；重试中按钮转圈并向读屏软件播报。
#[component]
pub fn LoadError(
    #[prop(into)] title: Signal<String>,
    #[prop(optional, into)] retrying: Signal<bool>,
    #[prop(optional, into)] retry_failed: Signal<bool>,
    #[prop(optional, into)] class: Signal<String>,
    #[prop(into)] on_retry: Callback<()>,
) -> impl IntoView {
    view! {
        <div
            class=move || format!("load-error {}", class.get())
            role="group"
            aria-label=move || title.get()
        >
            <div class="load-error-copy">
                <h3>{move || title.get()}</h3>
                <p role="status" aria-live="polite" aria-atomic="true">
                    {move || {
                        if retry_failed.get() {
                            "这次仍未加载成功，稍后再试一次吧。"
                        } else {
                            "网络或服务出了点问题，稍后再试一次。"
                        }
                    }}
                    <Show when=move || retrying.get()>
                        <span class="sr-only">"正在重新加载，请稍候。"</span>
                    </Show>
                </p>
            </div>
            <Button
                variant=ButtonVariant::Secondary
                icon=IconName::RotateCw
                loading=retrying
                onclick=move |_| {
                    if !retrying.get_untracked() {
                        on_retry.run(());
                    }
                }
            >
                "重新加载"
            </Button>
        </div>
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

    #[test]
    fn toaster_keeps_at_most_max_toasts() {
        Owner::new().with(|| {
            let toaster = Toaster::new();
            for n in 0..5 {
                toaster.entries.update(|list| {
                    list.push(ToastEntry {
                        id: n,
                        tone: ToastTone::Info,
                        message: n.to_string(),
                        leaving: false,
                    });
                    let overflow = list.len().saturating_sub(MAX_TOASTS);
                    list.drain(..overflow);
                });
            }
            assert_eq!(toaster.entries.get_untracked().len(), MAX_TOASTS);
        });
    }

    #[test]
    fn toast_region_renders_aria_container() {
        let html = Owner::new().with(|| {
            provide_toaster();
            view! { <ToastRegion/> }.to_html()
        });
        assert!(html.contains("class=\"toast-region\""), "{html}");
        assert!(html.contains("aria-live=\"polite\""), "{html}");
    }
}
