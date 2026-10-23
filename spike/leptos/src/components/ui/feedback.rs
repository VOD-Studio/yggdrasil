//! 区块加载失败卡片。（全站 Toast 不在本原型范围内，错误改为就地展示。）
use super::controls::{Button, ButtonVariant};
use super::icons::IconName;
use leptos::prelude::*;

/// 区块加载失败卡片：页面其余部分照常显示，只有出错的区块换成它；重试在原位完成。
///
/// `retry_failed` 为再次失败后的文案；重试中按钮转圈并向读屏软件播报。
#[component]
pub fn LoadError(
    #[prop(into)] title: String,
    #[prop(optional, into)] retrying: Signal<bool>,
    #[prop(optional, into)] retry_failed: Signal<bool>,
    #[prop(optional, into)] class: String,
    #[prop(into)] on_retry: Callback<()>,
) -> impl IntoView {
    let aria_label = title.clone();
    view! {
        <div class=format!("load-error {class}") role="group" aria-label=aria_label>
            <div class="load-error-copy">
                <h3>{title}</h3>
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
                onclick=Callback::new(move |_| {
                    if !retrying.get_untracked() {
                        on_retry.run(());
                    }
                })
            >
                "重新加载"
            </Button>
        </div>
    }
}
