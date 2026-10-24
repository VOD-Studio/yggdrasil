//! 同一标签页的恢复副本与离开保护；不替代服务端保存。
use leptos::prelude::*;
use serde::{de::DeserializeOwned, Serialize};

/// 返回（待恢复的本地副本，副本写入失败标记）。
///
/// 恢复副本在水合之后的首个 Effect 里读取：服务端与水合首帧一律没有副本，
/// 两端 DOM 一致；`beforeunload` 与导航协调器的离开保护只在浏览器注册。
pub(super) fn use_draft_protection<
    T: Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static,
>(
    slot: String,
    current: RwSignal<T>,
    dirty: impl Fn() -> bool + Copy + Send + Sync + 'static,
) -> (RwSignal<Option<T>>, RwSignal<bool>) {
    #[cfg(target_arch = "wasm32")]
    {
        use crate::context::UserContext;
        use std::rc::Rc;
        use wasm_bindgen::closure::Closure;

        fn session_storage() -> Option<web_sys::Storage> {
            web_sys::window()?.session_storage().ok()?
        }

        let user = expect_context::<UserContext>();
        let key = StoredValue::new(format!(
            "ygg.note-draft.{}.{}",
            user.user
                .with_untracked(|u| u.as_ref().map(|u| u.id))
                .unwrap_or_default(),
            slot
        ));
        let recovery = RwSignal::new(None::<T>);
        let failed = RwSignal::new(false);

        // 调用方必须保证在 Effect 内（订阅 current / dirty / recovery）或 `untrack` 内调用。
        let flush = move || {
            let unsaved = dirty();
            if recovery.with_untracked(Option::is_some) {
                return;
            }
            let result = (|| {
                let storage = session_storage()?;
                let key = key.try_get_value()?;
                if unsaved {
                    let json = current
                        .try_with_untracked(|c| serde_json::to_string(c))?
                        .ok()?;
                    storage.set_item(&key, &json).ok()?;
                } else {
                    storage.remove_item(&key).ok()?;
                }
                Some(())
            })();
            let error = result.is_none();
            if failed.try_get_untracked().is_some_and(|f| f != error) {
                let _ = failed.try_set(error);
            }
        };

        let loaded = StoredValue::new(false);
        Effect::new(move |_| {
            if !loaded.get_value() {
                loaded.set_value(true);
                let draft = (|| {
                    let storage = session_storage()?;
                    let raw = storage.get_item(&key.get_value()).ok()??;
                    let draft: T = serde_json::from_str(&raw).ok()?;
                    current.with_untracked(|c| draft != *c).then_some(draft)
                })();
                if draft.is_some() {
                    recovery.set(draft);
                }
            }
            current.track();
            recovery.track();
            flush();
        });

        let unload = window_event_listener_untyped("beforeunload", move |event| {
            untrack(flush);
            if untrack(dirty) {
                event.prevent_default();
                let _ = js_sys::Reflect::set(event.as_ref(), &"returnValue".into(), &"".into());
            }
        });
        // JS 持有闭包引用：句柄放进本地存储，卸载时先撤销守卫再丢弃。
        let guard = StoredValue::new_local(None::<(Rc<Closure<dyn FnMut() -> bool>>, u32)>);
        if let Some(module) = crate::bridges::navigation::wasm::get_module() {
            let leave = Rc::new(Closure::<dyn FnMut() -> bool>::new(move || {
                // 直接读取实时信号并同步写入：点击可能先于缓存 Effect 触发。
                untrack(flush);
                !untrack(dirty)
                    || web_sys::window().is_some_and(|w| {
                        w.confirm_with_message(
                            "还有未保存内容，或保存 / 上传尚未完成。确定离开？本地恢复副本不等于保存成功。",
                        )
                        .unwrap_or(false)
                    })
            }));
            let id = module.set_leave_guard(&leave);
            guard.set_value(Some((leave, id)));
        }
        on_cleanup(move || {
            unload.remove();
            if let Some(Some((_leave, id))) = guard.try_update_value(Option::take) {
                if let Some(module) = crate::bridges::navigation::wasm::get_module() {
                    module.clear_leave_guard(id);
                }
            }
        });
        (recovery, failed)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (slot, current, dirty);
        (RwSignal::new(None), RwSignal::new(false))
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
