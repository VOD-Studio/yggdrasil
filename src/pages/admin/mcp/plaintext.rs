//! One-time plaintext token display.
use super::configs::McpCopyButton;
use crate::components::ui::BTN_PRIMARY_SM;
use dioxus::prelude::*;

/// 明文展示弹窗。
#[cfg(target_arch = "wasm32")]
#[component]
pub(super) fn PlaintextModal(
    title: String,
    plaintext: String,
    on_use_config: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-4 animate-modal-overlay-enter",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] rounded-3xl p-7 max-w-xl w-full flex flex-col gap-5 border border-[var(--color-paper-border)] shadow-2xl animate-modal-panel-enter",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between border-b border-[var(--color-paper-border)]/60 pb-3",
                    h3 { class: "text-base sm:text-lg font-bold text-[var(--color-paper-primary)]",
                        "{title}"
                    }
                    button {
                        class: "text-[var(--color-paper-tertiary)] hover:text-[var(--color-paper-primary)] p-1 rounded-lg hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }
                pre { class: "bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]/60 rounded-2xl p-4 text-xs font-mono break-all select-all leading-relaxed",
                    code { "{plaintext}" }
                }
                div { class: "flex flex-wrap gap-2.5 justify-end pt-1",
                    McpCopyButton { text: plaintext.clone(), label: "复制令牌" }
                    button {
                        class: "{BTN_PRIMARY_SM}",
                        onclick: move |_| on_use_config.call(()),
                        "用于配置"
                    }
                    button {
                        class: "px-4 py-1.5 text-sm font-medium text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] hover:bg-[var(--color-paper-entry)] rounded-full transition-colors cursor-pointer",
                        onclick: move |_| on_close.call(()),
                        "关闭"
                    }
                }
            }
        }
    }
}
