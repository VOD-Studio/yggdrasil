//! 备份恢复 tab。

use dioxus::prelude::*;

use crate::components::ui::{LoadingButton, BTN_OUTLINE, BTN_TEXT_AMBER, BTN_TEXT_RED};

use super::format_bytes;

/// 备份恢复 tab：备份按钮 + 进度轮询 + 备份列表（下载/恢复/删除）。
#[allow(non_snake_case)]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn BackupTab() -> Element {
    use crate::api::database::backup::BackupInfo;
    #[cfg(target_arch = "wasm32")]
    use crate::api::database::backup::{
        create_backup, delete_backup, list_backups, restore_backup,
    };
    use crate::api::database::tasks::TaskProgress;
    #[cfg(target_arch = "wasm32")]
    use crate::api::database::tasks::{get_task_progress, TaskStatus};
    use crate::components::ui::{ADMIN_CARD_CLASS, ADMIN_TABLE_CLASS};

    // backups/active_task_id 仅在闭包内的重绑定副本上 .set()（如 backups_f），
    // 外层绑定本身不改值，故无需 mut。
    let backups = use_signal(Vec::<BackupInfo>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);
    // 当前进行中的任务（备份/恢复）id + 进度
    let active_task_id: Signal<Option<String>> = use_signal(|| None);
    let mut active_progress = use_signal(|| Option::<TaskProgress>::None);
    let mut busy = use_signal(|| false);

    // 刷新备份列表
    let mut refresh_list = move || {
        loading.set(true);
        #[cfg(target_arch = "wasm32")]
        {
            let mut backups = backups;
            let mut error = error;
            spawn(async move {
                match list_backups().await {
                    Ok(list) => {
                        backups.set(list);
                        error.set(None);
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
                loading.set(false);
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            loading.set(false);
        }
    };

    use_effect(move || {
        refresh_list();
    });

    // 任务进度轮询：active_task_id 存在时每 1.5s 拉取进度，Done/Failed 后停止 + 刷新列表。
    //
    // 同样用长生命周期 loop + 循环内读 active_task_id() 的模式。原先在挂载时把
    // active_task_id 快照进 _task_id_for_poll（彼时为 None），use_future 只跑一次
    // 即 return；用户点"创建备份"后 create_backup 返回 task id 并设置信号，但
    // future 已结束 → 轮询永不启动，busy 永远为 true（用户报告的 bug）。
    use_future(move || {
        let mut active_task_id = active_task_id;
        let mut active_progress = active_progress;
        let mut backups_f = backups;
        let mut busy_f = busy;
        async move {
            #[cfg(target_arch = "wasm32")]
            {
                loop {
                    let tid = match active_task_id() {
                        Some(t) => t,
                        None => {
                            // 空闲：短 yield，最多 200ms 后响应新任务。
                            crate::utils::time::sleep_ms(200).await;
                            continue;
                        }
                    };
                    // 有任务在途：进入 1.5s 轮询，直到 Done/Failed/出错。
                    loop {
                        crate::utils::time::sleep_ms(1500).await;
                        match get_task_progress(tid.clone()).await {
                            Ok(p) => {
                                let done =
                                    p.status == TaskStatus::Done || p.status == TaskStatus::Failed;
                                active_progress.set(Some(p));
                                if done {
                                    // 刷新列表（备份完成后新文件出现）并清理任务态
                                    if let Ok(list) = list_backups().await {
                                        backups_f.set(list);
                                    }
                                    active_task_id.set(None);
                                    busy_f.set(false);
                                    break;
                                }
                            }
                            Err(_) => {
                                active_task_id.set(None);
                                busy_f.set(false);
                                break;
                            }
                        }
                    }
                    // 内层 loop 退出后回到外层，继续等待下一个任务或空闲。
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let _ = (active_task_id, active_progress, backups_f, busy_f);
            }
        }
    });

    let current_backups = backups.read().clone();
    let current_error = error.read().clone();
    let current_progress = active_progress.read().clone();
    let is_busy = busy();

    rsx! {
        div { class: "space-y-4",
            // 自动备份设置卡片（顶部，与手动按钮/列表同屏）
            BackupSettingsCard {}

            // 操作栏
            div { class: "flex items-center gap-3",
                LoadingButton {
                    label: "创建备份".to_string(),
                    loading: is_busy,
                    variant: "sm",
                    onclick: move |_| {
                        #[cfg(target_arch = "wasm32")]
                        {
                            busy.set(true);
                            active_progress.set(None);
                            let mut active_task_id = active_task_id;
                            spawn(async move {
                                match create_backup().await {
                                    Ok(id) => active_task_id.set(Some(id)),
                                    Err(e) => {
                                        error.set(Some(e.to_string()));
                                        busy.set(false);
                                    }
                                }
                            });
                        }
                    },
                }
                button {
                    class: "{BTN_OUTLINE}",
                    disabled: loading() || is_busy,
                    onclick: move |_| refresh_list(),
                    "刷新列表"
                }
            }

            // 进度
            if let Some(p) = current_progress {
                div { class: "{ADMIN_CARD_CLASS} p-4",
                    div { class: "flex items-center justify-between mb-2",
                        span { class: "text-sm font-medium text-paper-primary", "{p.stage}" }
                        span { class: "text-sm text-paper-secondary", "{p.percent}%" }
                    }
                    div { class: "w-full bg-paper-entry rounded-full h-2 overflow-hidden",
                        div {
                            class: "bg-paper-accent h-full transition-all",
                            style: "width: {p.percent}%",
                        }
                    }
                    if let Some(detail) = p.detail {
                        p { class: "text-xs text-paper-secondary mt-2", "{detail}" }
                    }
                    if let Some(err) = p.error {
                        p { class: "text-xs text-red-600 dark:text-red-400 mt-2",
                            "错误：{err}"
                        }
                    }
                }
            }

            // 错误
            if let Some(err) = current_error {
                div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-3 text-sm text-red-700 dark:text-red-300",
                    "{err}"
                }
            }

            // 备份列表
            if !current_backups.is_empty() {
                div { class: "{ADMIN_TABLE_CLASS}",
                    div { class: "overflow-x-auto",
                        table { class: "w-full text-sm",
                            thead {
                                tr { class: "border-b border-paper-border text-left text-paper-secondary",
                                    th { class: "px-4 py-2 font-medium", "文件名" }
                                    th { class: "px-4 py-2 font-medium", "来源" }
                                    th { class: "px-4 py-2 font-medium", "模式" }
                                    th { class: "px-4 py-2 font-medium text-right",
                                        "大小"
                                    }
                                    th { class: "px-4 py-2 font-medium text-right",
                                        "操作"
                                    }
                                }
                            }
                            tbody {
                                for b in current_backups.iter() {
                                    BackupRow {
                                        key: "{b.filename}",
                                        info: b.clone(),
                                        busy: is_busy,
                                        // 恢复：确认已在 BackupRow 的 Popover 内完成,
                                        // 这里直接发起 restore_backup 并交由轮询 use_future 接管。
                                        // pending_restore signal + 确认 use_future 链路已移除
                                        //（原生 confirm 是阻塞式才需要那套间接机制）。
                                        on_restore: move |f: String| {
                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                let mut busy = busy;
                                                let mut active_progress = active_progress;
                                                let mut active_task_id = active_task_id;
                                                let mut error = error;
                                                spawn(async move {
                                                    busy.set(true);
                                                    active_progress.set(None);
                                                    match restore_backup(f, true).await {
                                                        Ok(id) => active_task_id.set(Some(id)),
                                                        Err(e) => {
                                                            error.set(Some(e.to_string()));
                                                            busy.set(false);
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        // 删除:确认已在 BackupRow 的 Popover 内完成,
                                        // 直接执行 delete_backup + 刷新列表。
                                        on_delete: move |fname_del: String| {
                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                let mut backups = backups;
                                                spawn(async move {
                                                    let _ = delete_backup(fname_del).await;
                                                    if let Ok(list) = list_backups().await {
                                                        backups.set(list);
                                                    }
                                                });
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            } else if !loading() {
                div { class: "text-paper-secondary text-sm py-4", "暂无备份文件" }
            }

            p { class: "text-xs text-paper-secondary",
                "备份优先用 pg_dump（含 schema），不可用时回退纯 SQL（仅数据，且不可经 psql 恢复）。"
                "恢复仅接受本系统生成的备份，且只恢复数据库；uploads 素材需从配对 tar.gz 手动还原。"
            }
        }
    }
}
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
struct BackupRowProps {
    info: crate::api::database::backup::BackupInfo,
    busy: bool,
    on_restore: Callback<String>,
    on_delete: Callback<String>,
}

#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn BackupRow(props: BackupRowProps) -> Element {
    use crate::components::ui::Popover;
    use crate::components::ui::BTN_DANGER_OUTLINE;

    // Callback 是 Copy，直接复用；filename 需 clone（确认框闭包各取一份）。
    let on_restore = props.on_restore;
    let on_delete = props.on_delete;
    let fname_for_restore = props.info.filename.clone();
    let fname_for_delete = props.info.filename.clone();

    // 展示用派生值（纯计算，render 内联合法）。
    let dl_url = format!("/api/database/backups/{}", urlencode_dl(&props.info.filename));
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
                            class: "px-3 py-1.5 text-xs text-paper-secondary hover:text-paper-primary transition-colors cursor-pointer",
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
                            class: "px-3 py-1.5 text-xs text-paper-secondary hover:text-paper-primary transition-colors cursor-pointer",
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

/// 自动备份设置卡片：启用开关 / 执行时间 / 保留份数 / 包含 uploads，
/// 以及上次自动备份结果与下次执行时间。
///
/// 时区模型：服务端只存 UTC "HH:MM"；卡片用 js_sys 在挂载回填与保存时
/// 做浏览器本地时区转换。SSR 与客户端首帧都是 view=None 的占位态，
/// 数据在 hydration 后落地，不存在两端渲染不一致。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn BackupSettingsCard() -> Element {
    #[cfg(target_arch = "wasm32")]
    use crate::api::settings::{get_backup_settings, update_backup_settings};
    use crate::components::ui::{ADMIN_CARD_CLASS, BTN_ICON};
    use crate::models::settings::BackupSettingsView;

    // 服务端视图（设置 + 上次结果 + 下次执行）。SSR/首帧为 None（占位态）。
    let view = use_signal(|| Option::<BackupSettingsView>::None);// 表单草稿：与已存值天然分叉，是独立状态（非镜像）。
    let mut draft_enabled = use_signal(|| false);
    let mut draft_time = use_signal(String::new); // 浏览器本地 "HH:MM"
    let mut draft_retention = use_signal(|| "30".to_string());
    let mut draft_include_uploads = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut just_saved = use_signal(|| false);
    let mut card_error = use_signal(|| Option::<String>::None);

    // 挂载拉取并回填草稿（一次性种子回填，合法 effect）。
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            let mut view = view;
            match get_backup_settings().await {
                Ok(v) => {
                    draft_enabled.set(v.settings.auto_enabled);
                    draft_time.set(utc_hhmm_to_local(&v.settings.time_utc));
                    draft_retention.set(v.settings.retention_count.to_string());
                    draft_include_uploads.set(v.settings.include_uploads);
                    view.set(Some(v));
                }
                Err(e) => card_error.set(Some(e.to_string())),
            }
        });
    });

    // 派生：草稿是否与已存设置分叉（memo 避免每次渲染重复 parse）。
    let dirty = use_memo(move || {
        let guard = view.read();
        let Some(v) = guard.as_ref() else {
            return false;
        };
        let retention_changed = draft_retention()
            .trim()
            .parse::<i32>()
            .map(|d| d != v.settings.retention_count)
            .unwrap_or(true);
        draft_enabled() != v.settings.auto_enabled
            || retention_changed
            || draft_include_uploads() != v.settings.include_uploads
            || draft_time() != utc_hhmm_to_local(&v.settings.time_utc)
    });

    // 预格式化状态行文本（render 内联算派生值，不存 signal）。
    let (last_run_label, next_run_label) = {
        let v = view.read();
        let last = v.as_ref().and_then(|v| v.last_run.as_ref()).map(|r| {
            let at = rfc3339_to_local_display(&r.at);
            if r.ok {
                match &r.file {
                    Some(f) => format!("{at} 成功（{f}）"),
                    None => format!("{at} 成功"),
                }
            } else {
                format!("{at} 失败：{}", r.error.clone().unwrap_or_default())
            }
        });
        let next = v
            .as_ref()
            .and_then(|v| v.next_run_at.as_ref())
            .map(|t| rfc3339_to_local_display(t));
        (last, next)
    };
    let loaded = view.read().is_some();

    rsx! {
        div { class: "{ADMIN_CARD_CLASS} p-5 space-y-5",
            // 标题行
            div {
                div { class: "text-sm font-medium text-paper-primary", "自动备份" }
                div { class: "text-xs text-paper-secondary mt-1",
                    "每天定点备份数据库与 uploads 素材，超出保留份数的最旧自动备份将被删除（手动备份永不自动删除）"
                }
            }

            if !loaded {
                // SSR/首帧占位：与客户端初始渲染一致，fetch 落地后替换。
                div { class: "text-xs text-paper-secondary py-2", "正在加载设置…" }
            } else {
                // 启用开关行（toggle 样式镜像回收站设置卡片）
                div { class: "flex items-center justify-between gap-4",
                    div { class: "min-w-0",
                        div { class: "text-sm font-medium text-paper-primary", "启用自动备份" }
                        div { class: "text-xs text-paper-secondary mt-1",
                            "到达设定时间后由后台任务执行"
                        }
                    }
                    button {
                        role: "switch",
                        aria_checked: "{draft_enabled()}",
                        class: if draft_enabled() { "relative w-11 h-6 flex-shrink-0 rounded-full bg-paper-accent cursor-pointer transition-colors duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-paper-accent/40" } else { "relative w-11 h-6 flex-shrink-0 rounded-full bg-paper-tertiary cursor-pointer transition-colors duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-paper-accent/40" },
                        onclick: move |_| {
                            draft_enabled.set(!draft_enabled());
                            just_saved.set(false);
                        },
                        span { class: if draft_enabled() { "absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm dark:shadow-black/30 transition-transform duration-200 translate-x-5" } else { "absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm dark:shadow-black/30 transition-transform duration-200" } }
                    }
                }

                // 执行时间（本地时间显示/编辑，存 UTC）
                div { class: "space-y-3",
                    div { class: "min-w-0",
                        div { class: "text-sm font-medium text-paper-primary", "执行时间" }
                        div { class: "text-xs text-paper-secondary mt-1",
                            "按浏览器本地时间显示与编辑，服务端以 UTC 存储"
                        }
                    }
                    div { class: "flex items-center gap-3",
                        div { class: "flex items-center rounded-lg border border-paper-border bg-paper-entry overflow-hidden",
                            input {
                                r#type: "time",
                                class: "h-9 px-3 text-sm tabular-nums text-paper-primary bg-transparent border-0 focus:outline-none",
                                value: "{draft_time()}",
                                oninput: move |e| {
                                    draft_time.set(e.value());
                                    just_saved.set(false);
                                },
                            }
                        }
                        span { class: "text-xs text-paper-secondary", "本地时间" }
                    }
                }

                // 保留份数（步进器镜像回收站保留天数）
                div { class: "space-y-3",
                    div { class: "min-w-0",
                        div { class: "text-sm font-medium text-paper-primary", "保留份数" }
                        div { class: "text-xs text-paper-secondary mt-1",
                            "自动备份保留的最近份数，超出后最旧的连配对素材包一起删除（1–365）"
                        }
                    }
                    div { class: "flex items-center gap-3",
                        div { class: "flex items-center rounded-lg border border-paper-border bg-paper-entry overflow-hidden",
                            button {
                                class: "{BTN_ICON}",
                                r#type: "button",
                                aria_label: "减少保留份数",
                                onclick: move |_| {
                                    let cur: i32 = draft_retention().trim().parse().unwrap_or(30);
                                    let next = cur.saturating_sub(1).max(1);
                                    draft_retention.set(next.to_string());
                                    just_saved.set(false);
                                },
                                "−"
                            }
                            input {
                                r#type: "number",
                                min: "1",
                                max: "365",
                                class: "w-14 h-9 px-1 text-center text-sm tabular-nums text-paper-primary bg-transparent border-0 focus:outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none",
                                value: "{draft_retention()}",
                                oninput: move |e| {
                                    draft_retention.set(e.value());
                                    just_saved.set(false);
                                },
                            }
                            button {
                                class: "{BTN_ICON}",
                                r#type: "button",
                                aria_label: "增加保留份数",
                                onclick: move |_| {
                                    let cur: i32 = draft_retention().trim().parse().unwrap_or(30);
                                    let next = cur.saturating_add(1).min(365);
                                    draft_retention.set(next.to_string());
                                    just_saved.set(false);
                                },
                                "+"
                            }
                        }
                        span { class: "text-xs text-paper-secondary", "份" }
                    }
                }

                // 包含 uploads 开关
                div { class: "flex items-center justify-between gap-4",
                    div { class: "min-w-0",
                        div { class: "text-sm font-medium text-paper-primary", "包含 uploads 素材" }
                        div { class: "text-xs text-paper-secondary mt-1",
                            "每次备份附带 uploads/ 打包（tar.gz，排除可重建的转码缓存）"
                        }
                    }
                    button {
                        role: "switch",
                        aria_checked: "{draft_include_uploads()}",
                        class: if draft_include_uploads() { "relative w-11 h-6 flex-shrink-0 rounded-full bg-paper-accent cursor-pointer transition-colors duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-paper-accent/40" } else { "relative w-11 h-6 flex-shrink-0 rounded-full bg-paper-tertiary cursor-pointer transition-colors duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-paper-accent/40" },
                        onclick: move |_| {
                            draft_include_uploads.set(!draft_include_uploads());
                            just_saved.set(false);
                        },
                        span { class: if draft_include_uploads() { "absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm dark:shadow-black/30 transition-transform duration-200 translate-x-5" } else { "absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm dark:shadow-black/30 transition-transform duration-200" } }
                    }
                }

                // 状态行：上次结果 / 下次执行
                if last_run_label.is_some() || next_run_label.is_some() {
                    div { class: "text-xs text-paper-secondary space-y-1 border-t border-paper-border pt-3",
                        if let Some(label) = last_run_label {
                            div { "上次自动备份：{label}" }
                        }
                        if let Some(label) = next_run_label {
                            div { "下次执行：{label}（本地时间）" }
                        }
                    }
                }

                if let Some(err) = card_error() {
                    div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-3 text-sm text-red-700 dark:text-red-300",
                        "{err}"
                    }
                }

                // 底部操作行
                div { class: "flex items-center justify-between gap-4 pt-1",
                    if just_saved() {
                        span { class: "inline-flex items-center gap-1.5 text-xs text-paper-accent",
                            svg {
                                class: "w-3.5 h-3.5",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2.5",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M5 13l4 4L19 7",
                                }
                            }
                            "已保存"
                        }
                    } else if dirty() {
                        span { class: "text-xs text-paper-secondary", "有未保存的更改" }
                    } else {
                        span { class: "text-xs text-transparent select-none", "·" }
                    }
                    LoadingButton {
                        label: "保存设置".to_string(),
                        loading: saving(),
                        disabled: just_saved() || !dirty(),
                        variant: "sm",
                        onclick: move |_| {
                            #[cfg(target_arch = "wasm32")]
                            {
                                let enabled = draft_enabled();
                                let time_utc = local_hhmm_to_utc(&draft_time());
                                let retention: i32 =
                                    draft_retention().trim().parse().unwrap_or(30);
                                let include = draft_include_uploads();
                                saving.set(true);
                                spawn(async move {
                                    let mut view = view;
                                    match update_backup_settings(
                                            enabled, time_utc, retention, include,
                                        )
                                        .await
                                    {
                                        Ok(v) => {
                                            // 以服务端正典值回填草稿（normalize/clamp 后的值）。
                                            draft_enabled.set(v.settings.auto_enabled);
                                            draft_time.set(utc_hhmm_to_local(&v.settings.time_utc));
                                            draft_retention
                                                .set(v.settings.retention_count.to_string());
                                            draft_include_uploads.set(v.settings.include_uploads);
                                            view.set(Some(v));
                                            just_saved.set(true);
                                            card_error.set(None);
                                        }
                                        Err(e) => card_error.set(Some(e.to_string())),
                                    }
                                    saving.set(false);
                                });
                            }
                        },
                    }
                }
            }
        }
    }
}

/// UTC "HH:MM" → 浏览器本地 "HH:MM"（按当天时区偏移换算）。
/// 非 wasm32 原样返回（SSR 不渲染设置值，此分支不会出现在用户可见路径）。
fn utc_hhmm_to_local(t: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let mut parts = t.split(':');
        let h = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
        let m = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
        let d = js_sys::Date::new_0();
        d.set_utc_hours(h);
        d.set_utc_minutes(m);
        d.set_utc_seconds(0);
        d.set_utc_milliseconds(0);
        format!("{:02}:{:02}", d.get_hours(), d.get_minutes())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        t.to_string()
    }
}

/// 浏览器本地 "HH:MM" → UTC "HH:MM"。空串/非法输入回退 "04:00"
/// （服务端 normalize 会再兜底一次）。仅 wasm 端保存按钮调用。
#[cfg(target_arch = "wasm32")]
fn local_hhmm_to_utc(t: &str) -> String {
    let mut parts = t.split(':');
    let Some(h) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
        return "04:00".to_string();
    };
    let Some(m) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
        return "04:00".to_string();
    };
    let d = js_sys::Date::new_0();
    d.set_hours(h);
    d.set_minutes(m);
    d.set_seconds(0);
    d.set_milliseconds(0);
    format!("{:02}:{:02}", d.get_utc_hours(), d.get_utc_minutes())
}

/// RFC3339 → 浏览器本地「YYYY-MM-DD HH:MM」。解析失败原样返回。
/// 仅在 view 已落地（hydration 后）的渲染路径出现，无 SSR 不一致问题。
fn rfc3339_to_local_display(iso: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let ms = js_sys::Date::parse(iso);
        if ms.is_nan() {
            return iso.to_string();
        }
        let d = js_sys::Date::new(&ms.into());
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            d.get_full_year(),
            d.get_month() + 1,
            d.get_date(),
            d.get_hours(),
            d.get_minutes()
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        iso.to_string()
    }
}
