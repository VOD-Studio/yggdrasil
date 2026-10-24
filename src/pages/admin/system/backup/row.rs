use crate::api::database::backup::BackupInfo;
use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, ConfirmDialog, IconName, StatusDot,
};
use crate::utils::format_bytes;
use leptos::prelude::*;

/// 备份列表单行（抽取为子组件：各自 scope 内的确认框状态互不干扰）。
///
/// 恢复与删除先弹危险确认对话框，确认后回调父组件的 `on_restore` / `on_delete`。
#[component]
pub(super) fn BackupRow(
    info: BackupInfo,
    #[prop(into)] busy: Signal<bool>,
    #[prop(into)] on_restore: Callback<String>,
    #[prop(into)] on_delete: Callback<String>,
) -> impl IntoView {
    let fname_for_restore = info.filename.clone();
    let fname_for_delete = info.filename.clone();

    // 展示用派生值（纯计算）。
    let dl_url = format!(
        "/api/database/backups/{}",
        super::super::urlencode(&info.filename)
    );
    let size_str = format_bytes(info.size_bytes as i64);
    let (origin_label, origin_tone) = if info.origin == "auto" {
        ("自动", "clay")
    } else {
        ("手动", "muted")
    };
    let uploads_dl = info.uploads_filename.as_ref().map(|f| {
        (
            format_bytes(info.uploads_size_bytes.unwrap_or(0) as i64),
            format!("/api/database/backups/{}", super::super::urlencode(f)),
        )
    });
    let up_size = uploads_dl.as_ref().map(|(size, _)| size.clone());
    let up_url = uploads_dl.map(|(_, url)| url);

    let confirm_restore = RwSignal::new(false);
    let confirm_delete = RwSignal::new(false);
    let restore_description = format!(
        "现有数据会被「{}」覆盖。只恢复数据库，素材不随之还原，需要从配对的压缩包手动恢复。",
        info.filename
    );
    let delete_description = format!("「{}」会从磁盘上删除，无法恢复。", info.filename);

    view! {
        <tr>
            <td class="sys-mono">{info.filename.clone()}</td>
            <td>
                <StatusDot label=origin_label tone=origin_tone/>
            </td>
            <td class="admin-table-muted">{info.mode.clone()}</td>
            <td class="admin-table-num">
                {size_str}
                {up_size.map(|up_size| view! { <div class="sys-sub">{format!("素材 {up_size}")}</div> })}
            </td>
            <td class="sys-row-actions">
                <a
                    class=button_class(ButtonVariant::Ghost, ButtonSize::Xs)
                    href=dl_url
                    rel="external"
                    download=""
                >
                    "下载"
                </a>
                {up_url
                    .map(|up_url| {
                        view! {
                            <a
                                class=button_class(ButtonVariant::Ghost, ButtonSize::Xs)
                                href=up_url
                                rel="external"
                                download=""
                            >
                                "素材"
                            </a>
                        }
                    })}
                <button
                    class=button_class(ButtonVariant::Ghost, ButtonSize::Xs)
                    type="button"
                    disabled=move || busy.get()
                    on:click=move |_| confirm_restore.set(true)
                >
                    "恢复"
                </button>
                <button
                    class=format!("{} sys-danger", button_class(ButtonVariant::Ghost, ButtonSize::Xs))
                    type="button"
                    disabled=move || busy.get()
                    on:click=move |_| confirm_delete.set(true)
                >
                    "删除"
                </button>
                <ConfirmDialog
                    visible=confirm_restore
                    title="用这份备份恢复数据库？"
                    description=restore_description
                    confirm_label="确认恢复"
                    icon=IconName::RotateCcw
                    on_confirm=move |_| on_restore.run(fname_for_restore.clone())
                />
                <ConfirmDialog
                    visible=confirm_delete
                    title="删除这份备份？"
                    description=delete_description
                    confirm_label="确认删除"
                    on_confirm=move |_| on_delete.run(fname_for_delete.clone())
                />
            </td>
        </tr>
    }
}
