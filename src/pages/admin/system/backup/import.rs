use super::task::BackupState;
use crate::components::ui::{button_class, ButtonSize, ButtonVariant};
use leptos::prelude::*;

/// 客户端预检的导入体积上限（服务端仍兜底）。
#[cfg(target_arch = "wasm32")]
const IMPORT_MAX_BYTES_CLIENT: u64 = 512 * 1024 * 1024;

#[component]
pub(super) fn ImportBackup(state: BackupState) -> impl IntoView {
    let import_busy = state.importing;
    let is_busy = move || state.busy();
    let on_import = move |evt| {
        #[cfg(target_arch = "wasm32")]
        {
            if state.busy_untracked() {
                return;
            }
            let input = event_target::<web_sys::HtmlInputElement>(&evt);
            let Some(web_file) = input.files().and_then(|files| files.get(0)) else {
                return;
            };
            // 重置 input value，允许重选同一文件再次触发 change。
            input.set_value("");
            // 客户端预检（服务端仍兜底）：.sql 后缀 + 体积上限。
            if !web_file.name().ends_with(".sql") {
                state.error.set(Some("仅接受 .sql 备份文件".to_string()));
                return;
            }
            if web_file.size() as u64 > IMPORT_MAX_BYTES_CLIENT {
                state.error.set(Some(format!(
                    "文件超过导入上限（{}MB）",
                    IMPORT_MAX_BYTES_CLIENT / 1024 / 1024
                )));
                return;
            }
            import_busy.set(true);
            state.error.set(None);
            leptos::task::spawn_local(async move {
                match crate::utils::web_upload::post_multipart_file(
                    "/api/database/backups/import",
                    "file",
                    &web_file,
                )
                .await
                {
                    Ok(_) => state.refresh(),
                    Err(msg) => {
                        let _ = state.error.try_set(Some(msg));
                    }
                }
                let _ = import_busy.try_set(false);
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = evt;
    };

    view! {
        <label
            class=format!("{} sys-file-button", button_class(ButtonVariant::Secondary, ButtonSize::Sm))
            aria-disabled=move || (import_busy.get() || is_busy()).then_some("true")
        >
            {move || if import_busy.get() { "导入中…" } else { "导入备份" }}
            <input
                type="file"
                accept=".sql"
                class="sr-only"
                disabled=move || import_busy.get() || is_busy()
                on:change=on_import
            />
        </label>
    }
}
