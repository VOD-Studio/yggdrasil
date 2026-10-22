use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, ConfirmDialog, IconName, StatusDot,
};
use crate::utils::format_bytes;
use dioxus::prelude::*;

/// 下载链接用的 URL 编码（wasm32 才编码，server 端原样返回——rsx 构造 dl_url 时两端都调）。
/// 自包含实现，不跨文件依赖 export.rs 的 urlencode。
fn urlencode_dl(s: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(b as char);
                }
                b' ' => out.push('+'),
                _ => out.push_str(&format!("%{:02X}", b)),
            }
        }
        out
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        s.to_string()
    }
}
/// 备份列表单行（抽取为子组件：各自 scope 内 let/clone 不冲突）。
///
/// 恢复与删除先弹危险确认对话框，确认后回调父组件的 `on_restore` / `on_delete`。
#[derive(Props, Clone, PartialEq)]
pub(super) struct BackupRowProps {
    info: crate::api::database::backup::BackupInfo,
    busy: bool,
    on_restore: Callback<String>,
    on_delete: Callback<String>,
}

#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn BackupRow(props: BackupRowProps) -> Element {
    // Callback 是 Copy，直接复用；filename 需 clone（确认框闭包各取一份）。
    let on_restore = props.on_restore;
    let on_delete = props.on_delete;
    let fname_for_restore = props.info.filename.clone();
    let fname_for_delete = props.info.filename.clone();

    // 展示用派生值（纯计算，render 内联合法）。
    let dl_url = format!(
        "/api/database/backups/{}",
        urlencode_dl(&props.info.filename)
    );
    let size_str = format_bytes(props.info.size_bytes as i64);
    let (origin_label, origin_tone) = if props.info.origin == "auto" {
        ("自动", "clay")
    } else {
        ("手动", "muted")
    };
    let uploads_dl = props.info.uploads_filename.as_ref().map(|f| {
        (
            format_bytes(props.info.uploads_size_bytes.unwrap_or(0) as i64),
            format!("/api/database/backups/{}", urlencode_dl(f)),
        )
    });

    let mut confirm_restore = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);
    rsx! {
        tr {
            td { class: "sys-mono", "{props.info.filename}" }
            td {
                StatusDot { label: origin_label, tone: origin_tone }
            }
            td { class: "admin-table-muted", "{props.info.mode}" }
            td { class: "admin-table-num",
                "{size_str}"
                if let Some((up_size, _)) = &uploads_dl {
                    div { class: "sys-sub", "素材 {up_size}" }
                }
            }
            td { class: "sys-row-actions",
                a { class: button_class(ButtonVariant::Ghost, ButtonSize::Xs), href: "{dl_url}", download: "", "下载" }
                if let Some((_, up_url)) = &uploads_dl {
                    a { class: button_class(ButtonVariant::Ghost, ButtonSize::Xs), href: "{up_url}", download: "", "素材" }
                }
                button {
                    class: button_class(ButtonVariant::Ghost, ButtonSize::Xs),
                    r#type: "button",
                    disabled: props.busy,
                    onclick: move |_| confirm_restore.set(true),
                    "恢复"
                }
                button {
                    class: "{button_class(ButtonVariant::Ghost, ButtonSize::Xs)} sys-danger",
                    r#type: "button",
                    disabled: props.busy,
                    onclick: move |_| confirm_delete.set(true),
                    "删除"
                }
                ConfirmDialog {
                    visible: confirm_restore,
                    title: "用这份备份恢复数据库？",
                    description: format!(
                        "现有数据会被「{}」覆盖。只恢复数据库，素材不随之还原，需要从配对的压缩包手动恢复。",
                        props.info.filename
                    ),
                    confirm_label: "确认恢复",
                    icon: IconName::RotateCcw,
                    on_confirm: move |_| on_restore.call(fname_for_restore.clone()),
                }
                ConfirmDialog {
                    visible: confirm_delete,
                    title: "删除这份备份？",
                    description: format!("「{}」会从磁盘上删除，无法恢复。", props.info.filename),
                    confirm_label: "确认删除",
                    on_confirm: move |_| on_delete.call(fname_for_delete.clone()),
                }
            }
        }
    }
}
