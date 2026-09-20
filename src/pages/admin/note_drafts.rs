//! 同一标签页的恢复副本与离开保护；不替代服务端保存。
use dioxus::prelude::*;
use serde::{de::DeserializeOwned, Serialize};

pub(super) fn use_draft_protection<
    T: Clone + PartialEq + Serialize + DeserializeOwned + 'static,
>(
    slot: String,
    current: Signal<T>,
    dirty: impl Fn() -> bool + Copy + 'static,
) -> (Signal<Option<T>>, Signal<bool>) {
    #[cfg(target_arch = "wasm32")]
    {
        use std::rc::Rc;
        use wasm_bindgen::{closure::Closure, JsCast};
        let user: crate::context::UserContext = use_context();
        let key = use_hook(move || {
            format!(
                "ygg.note-draft.{}.{}",
                user.user.peek().as_ref().map(|u| u.id).unwrap_or_default(),
                slot
            )
        });
        let read_key = key.clone();
        let recovery = use_signal(move || {
            let storage = web_sys::window()?.session_storage().ok()??;
            let draft: T = serde_json::from_str(&storage.get_item(&read_key).ok()??).ok()?;
            (draft != current()).then_some(draft)
        });
        let failed = use_signal(|| false);
        let flush = use_hook(move || {
            Rc::new(move || {
                let mut failed = failed;
                let unsaved = dirty();
                let snapshot = current();
                if recovery().is_some() {
                    return;
                }
                let result = (|| {
                    let storage = web_sys::window()?.session_storage().ok()??;
                    if unsaved {
                        storage
                            .set_item(&key, &serde_json::to_string(&snapshot).ok()?)
                            .ok()?;
                    } else {
                        storage.remove_item(&key).ok()?;
                    }
                    Some(())
                })();
                let error = result.is_none();
                if *failed.peek() != error {
                    failed.set(error);
                }
            })
        });
        let cache = flush.clone();
        use_effect(move || cache());
        let listeners = use_hook(move || {
            let cache = flush.clone();
            let unload = Rc::new(Closure::<dyn FnMut(web_sys::Event)>::new(
                move |event: web_sys::Event| {
                    cache();
                    if dirty() {
                        event.prevent_default();
                        let _ =
                            js_sys::Reflect::set(event.as_ref(), &"returnValue".into(), &"".into());
                    }
                },
            ));
            let leave = Rc::new(Closure::<dyn FnMut() -> bool>::new(move || {
                // Read live signals and flush synchronously: a click can precede the cache effect.
                flush();
                !dirty()
                    || web_sys::window().is_some_and(|w| {
                        w.confirm_with_message(
                    "还有未保存内容，或保存 / 上传尚未完成。确定离开？本地恢复副本不等于保存成功。"
                ).unwrap_or(false)
                    })
            }));
            if let Some(window) = web_sys::window() {
                let _ = window.add_event_listener_with_callback(
                    "beforeunload",
                    unload.as_ref().as_ref().unchecked_ref(),
                );
            }
            let module = crate::bridges::navigation::wasm::get_module().map(Rc::new);
            let id = module.as_ref().map(|m| m.set_leave_guard(&leave));
            (unload, leave, module, id)
        });
        use_drop(move || {
            let (unload, _leave, module, id) = listeners;
            if let (Some(module), Some(id)) = (module, id) {
                module.clear_leave_guard(id);
            }
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback(
                    "beforeunload",
                    unload.as_ref().as_ref().unchecked_ref(),
                );
            }
        });
        (recovery, failed)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (slot, current, dirty);
        (use_signal(|| None), use_signal(|| false))
    }
}

/// A completed request must never clear text entered after its snapshot was submitted.
pub(super) fn clear_submitted(current: &mut String, submitted: &str) -> bool {
    if current != submitted {
        return false;
    }
    current.clear();
    true
}

#[cfg(test)]
mod tests {
    use super::clear_submitted;

    #[test]
    fn quick_save_clears_only_the_submitted_snapshot() {
        let mut current = "first".to_string();
        assert!(clear_submitted(&mut current, "first"));
        assert!(current.is_empty());
        current = "first plus a new thought".to_string();
        assert!(!clear_submitted(&mut current, "first"));
        assert_eq!(current, "first plus a new thought");
        current.clear();
        assert!(!clear_submitted(&mut current, "first"));
    }
}
