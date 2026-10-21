use super::task::BackupState;
use crate::components::ui::{button_class, ButtonSize, ButtonVariant};
use dioxus::prelude::*;

#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn ImportBackup(state: BackupState) -> Element {
    #[cfg(target_arch = "wasm32")]
    use dioxus::web::{WebEventExt, WebFileExt};
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen::JsCast;
    let mut import_busy = state.importing;
    let mut error = state.error;
    let is_busy = state.busy();
    let on_import = move |evt: FormEvent| {
        #[cfg(target_arch = "wasm32")]
        {
            if state.busy() {
                return;
            }
            let Some(file) = evt.files().into_iter().next() else {
                return;
            };
            let Some(web_file) = file.get_web_file() else {
                return;
            };
            // 重置 input value，允许重选同一文件再次触发 change。
            if let Some(input) = evt
                .try_as_web_event()
                .and_then(|e| e.target())
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            {
                input.set_value("");
            }
            // 客户端预检（服务端仍兜底）：.sql 后缀 + 体积上限。
            if !web_file.name().ends_with(".sql") {
                error.set(Some("仅接受 .sql 备份文件".to_string()));
                return;
            }
            if web_file.size() as u64 > IMPORT_MAX_BYTES_CLIENT {
                error.set(Some(format!(
                    "文件超过导入上限（{}MB）",
                    IMPORT_MAX_BYTES_CLIENT / 1024 / 1024
                )));
                return;
            }
            import_busy.set(true);
            error.set(None);
            let state = state;
            spawn(async move {
                match crate::utils::web_upload::post_multipart_file(
                    "/api/database/backups/import",
                    "file",
                    &web_file,
                )
                .await
                {
                    Ok(_) => {
                        state.refresh();
                    }
                    Err(msg) => error.set(Some(msg)),
                }
                import_busy.set(false);
            });
        }
    };
    rsx! {
        label {
            class: "{button_class(ButtonVariant::Secondary, ButtonSize::Sm)} sys-file-button",
            "aria-disabled": (import_busy() || is_busy).then_some("true"),
            if import_busy() { "导入中…" } else { "导入备份" }
            input {
                r#type: "file",
                accept: ".sql",
                class: "sr-only",
                disabled: import_busy() || is_busy,
                onchange: on_import,
            }
        }
    }
}
#[cfg(target_arch = "wasm32")]
const IMPORT_MAX_BYTES_CLIENT: u64 = 512 * 1024 * 1024;
