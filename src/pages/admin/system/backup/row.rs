use crate::components::ui::{BTN_GHOST, BTN_TEXT_AMBER, BTN_TEXT_RED};
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
/// 删除/恢复不再用浏览器原生 confirm()，改用 [`Popover`](crate::components::ui::Popover) 确认框（`position:fixed`
/// 逃出表格 `overflow-hidden`）。点击按钮读 `MouseEvent::client_coordinates()` 作为
/// popover 锚点，`confirm` 按钮回调父组件的 `on_delete`/`on_restore`。
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
    use crate::components::ui::Popover;
    use crate::components::ui::BTN_DANGER_OUTLINE;

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
    let (origin_label, origin_class) = if props.info.origin == "auto" {
        ("自动", "text-paper-accent border-paper-accent/40")
    } else {
        ("手动", "text-paper-secondary border-paper-border")
    };
    let uploads_dl = props.info.uploads_filename.as_ref().map(|f| {
        (
            format_bytes(props.info.uploads_size_bytes.unwrap_or(0) as i64),
            format!("/api/database/backups/{}", urlencode_dl(f)),
        )
    });

    // Popover 状态：哪个动作的确认框打开 + 锚点坐标。None = 都关闭。
    // 用一个 String("delete"/"restore") 而非两个 bool，避免同时开两个 popover。
    let mut open_action = use_signal(|| Option::<String>::None);
    // 锚点坐标：按钮点击的视口坐标（client_coordinates）。
    let mut anchor_x = use_signal(|| 0i32);
    let mut anchor_y = use_signal(|| 0i32);

    rsx! {
        tr { class: "border-b border-paper-border last:border-0 hover:bg-paper-entry transition-colors",
            td { class: "px-4 py-2 font-mono text-xs text-paper-primary", "{props.info.filename}" }
            td { class: "px-4 py-2",
                span { class: "inline-block px-1.5 py-0.5 text-xs rounded border {origin_class}",
                    "{origin_label}"
                }
            }
            td { class: "px-4 py-2 text-paper-secondary", "{props.info.mode}" }
            td { class: "px-4 py-2 text-right text-paper-secondary",
                "{size_str}"
                if let Some((up_size, _)) = &uploads_dl {
                    div { class: "text-xs text-paper-tertiary", "素材 {up_size}" }
                }
            }
            td { class: "px-4 py-2 text-right whitespace-nowrap",
                a {
                    class: "text-xs text-paper-accent hover:underline mr-3",
                    href: "{dl_url}",
                    download: "",
                    "下载"
                }
                if let Some((_, up_url)) = &uploads_dl {
                    a {
                        class: "text-xs text-paper-accent hover:underline mr-3",
                        href: "{up_url}",
                        download: "",
                        "素材"
                    }
                }
                button {
                    class: "{BTN_TEXT_AMBER} mr-3 disabled:opacity-50",
                    disabled: props.busy,
                    // 点击记录坐标并打开恢复确认 popover。client_coordinates 两端编译。
                    onclick: move |e| {
                        let c = e.client_coordinates();
                        anchor_x.set(c.x as i32);
                        anchor_y.set(c.y as i32);
                        open_action.set(Some("restore".to_string()));
                    },
                    "恢复"
                }
                button {
                    class: "{BTN_TEXT_RED} disabled:opacity-50",
                    disabled: props.busy,
                    onclick: move |e| {
                        let c = e.client_coordinates();
                        anchor_x.set(c.x as i32);
                        anchor_y.set(c.y as i32);
                        open_action.set(Some("delete".to_string()));
                    },
                    "删除"
                }
            }

            // 恢复确认 popover。操作列贴视口右缘,居中面板会越界——
            // align="end" 让面板右缘贴点击点、向左延伸进视口。
            Popover {
                open: open_action().as_deref() == Some("restore"),
                anchor_x: anchor_x(),
                anchor_y: anchor_y(),
                placement: "bottom",
                align: "end",
                on_close: move |_| open_action.set(None),
                div { class: "w-64 space-y-3",
                    p { class: "text-sm text-paper-primary leading-relaxed",
                        "恢复将覆盖现有数据，确认恢复 "
                        span { class: "font-mono text-xs break-all", "{props.info.filename}" }
                        "？"
                    }
                    p { class: "text-xs text-paper-secondary",
                        "仅恢复数据库；uploads 素材不随恢复还原，需手动从配对压缩包恢复。"
                    }
                    div { class: "flex justify-end gap-2 pt-1",
                        button {
                            class: "{BTN_GHOST}",
                            onclick: move |_| open_action.set(None),
                            "取消"
                        }
                        button {
                            class: "{BTN_DANGER_OUTLINE}",
                            onclick: move |_| {
                                open_action.set(None);
                                on_restore.call(fname_for_restore.clone());
                            },
                            "确认恢复"
                        }
                    }
                }
            }

            // 删除确认 popover(同 align="end",防右缘越界)
            Popover {
                open: open_action().as_deref() == Some("delete"),
                anchor_x: anchor_x(),
                anchor_y: anchor_y(),
                placement: "bottom",
                align: "end",
                on_close: move |_| open_action.set(None),
                div { class: "w-64 space-y-3",
                    p { class: "text-sm text-paper-primary",
                        "确认删除 "
                        span { class: "font-mono text-xs break-all", "{props.info.filename}" }
                        "？"
                    }
                    div { class: "flex justify-end gap-2 pt-1",
                        button {
                            class: "{BTN_GHOST}",
                            onclick: move |_| open_action.set(None),
                            "取消"
                        }
                        button {
                            class: "{BTN_DANGER_OUTLINE}",
                            onclick: move |_| {
                                open_action.set(None);
                                on_delete.call(fname_for_delete.clone());
                            },
                            "确认删除"
                        }
                    }
                }
            }
        }
    }
}
