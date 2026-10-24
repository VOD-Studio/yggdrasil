//! 备份 tab：创建与导入备份、任务进度、备份列表（下载、恢复、删除）。
//! 定时备份的调度在「设置 › 自动备份」。
mod import;
mod row;
mod task;

use crate::api::database::backup::BackupInfo;
use crate::api::database::tasks::TaskProgress;
use crate::components::empty_state::EmptyState;
use crate::components::ui::{button_class, ButtonSize, ButtonVariant, Icon, IconName};
use crate::routes::Route;
use import::ImportBackup;
use leptos::prelude::*;
use row::BackupRow;

/// 读取当前进度的某个字段（无进度时取默认值）。
fn progress_field<T: Default>(
    progress: RwSignal<Option<TaskProgress>>,
    f: impl FnOnce(&TaskProgress) -> T,
) -> T {
    progress.with(|p| p.as_ref().map(f).unwrap_or_default())
}

/// 备份 tab。
#[component]
pub(super) fn BackupTab() -> impl IntoView {
    let state = task::use_backup_task();
    let loading = state.loading;
    let busy = Signal::derive(move || state.busy());
    let has_backups = Memo::new(move |_| state.backups.with(|list| !list.is_empty()));
    let show_empty = move || !has_backups.get() && !loading.get();
    let has_progress = Memo::new(move |_| state.progress.with(Option::is_some));
    let progress = state.progress;
    let percent = move || progress_field(progress, |p| p.percent);

    view! {
        <div class="sys-backup-head">
            <p>
                "优先用 pg_dump 生成含表结构的完整备份；包含素材时另打包配对的 tar.gz。定时备份在 "
                <a href=Route::SiteSettingsPage.to_string()>"设置 › 自动备份"</a>
                " 里调整。"
            </p>
            <div class="sys-backup-actions">
                <button
                    class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                    type="button"
                    disabled=move || loading.get() || busy.get()
                    on:click=move |_| state.refresh()
                >
                    <Icon name=IconName::RefreshCw size=14/>
                    "刷新列表"
                </button>
                <ImportBackup state/>
                <button
                    class=button_class(ButtonVariant::Primary, ButtonSize::Sm)
                    type="button"
                    disabled=move || busy.get()
                    on:click=move |_| state.start(None)
                >
                    {move || if busy.get() { "处理中…" } else { "创建备份" }}
                </button>
            </div>
        </div>

        <Show when=move || has_progress.get()>
            <div class="admin-panel sys-progress" role="status">
                <span class="sys-progress-stage">
                    {move || progress_field(progress, |p| p.stage.clone())}
                </span>
                <div class="sys-progress-bar" aria-hidden="true">
                    <div style=move || format!("width: {}%", percent())></div>
                </div>
                <span class="sys-progress-pct">{move || format!("{}%", percent())}</span>
                {move || {
                    progress_field(progress, |p| p.detail.clone())
                        .map(|detail| view! { <p class="sys-progress-detail">{detail}</p> })
                }}
                {move || {
                    progress_field(progress, |p| p.error.clone())
                        .map(|err| view! { <p class="sys-error">{format!("错误：{err}")}</p> })
                }}
            </div>
        </Show>

        {move || {
            state
                .error
                .get()
                .map(|err| view! { <p class="sys-error" role="alert">{err}</p> })
        }}

        {move || {
            if has_backups.get() {
                view! {
                    <section class="admin-panel sys-section">
                        <div class="sys-table-scroll">
                            <table class="admin-table">
                                <thead>
                                    <tr>
                                        <th>"文件"</th>
                                        <th>"来源"</th>
                                        <th>"模式"</th>
                                        <th class="admin-table-num">"大小"</th>
                                        <th class="admin-table-actions">
                                            <span class="sr-only">"操作"</span>
                                        </th>
                                    </tr>
                                </thead>
                                <tbody>
                                    <For
                                        each=move || state.backups.get()
                                        key=|b: &BackupInfo| {
                                            (
                                                b.filename.clone(),
                                                b.size_bytes,
                                                b.origin.clone(),
                                                b.mode.clone(),
                                                b.uploads_filename.clone(),
                                                b.uploads_size_bytes,
                                            )
                                        }
                                        children=move |b: BackupInfo| {
                                            view! {
                                                <BackupRow
                                                    info=b
                                                    busy=busy
                                                    on_restore=move |filename: String| state.start(Some(filename))
                                                    on_delete=move |filename: String| state.delete(filename)
                                                />
                                            }
                                        }
                                    />
                                </tbody>
                            </table>
                        </div>
                    </section>
                }
                    .into_any()
            } else if show_empty() {
                view! {
                    <div class="admin-panel">
                        <EmptyState
                            icon=IconName::Archive
                            title="还没有备份"
                            description="创建一份手动备份，或在设置里开启定时备份。"
                        />
                    </div>
                }
                    .into_any()
            } else {
                ().into_any()
            }
        }}
        <p class="sys-footnote">
            "恢复只接受本系统生成的备份，且只恢复数据库；素材需要从配对的 tar.gz 手动还原。导入只接受本系统生成的 .sql 备份（签名校验，单个文件默认不超过 512 MB），以 auto_ 开头的文件会参与自动轮转。"
        </p>
    }
}
