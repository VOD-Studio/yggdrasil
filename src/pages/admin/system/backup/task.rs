//! 组件作用域持有请求与轮询：只有当前任务可以更新进度，组件卸载即停止轮询。
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::api::database::backup::{
    create_backup, delete_backup, list_backups, restore_backup, BackupInfo,
};
use crate::api::database::tasks::{get_task_progress, TaskProgress, TaskStatus};
use crate::utils::error::failure_message;
use leptos::prelude::*;

/// 备份任务的操作锁：同一时刻只允许一个创建 / 恢复 / 删除。
#[derive(Clone, PartialEq)]
enum Operation {
    Idle,
    Starting,
    Running(String),
    Deleting,
}

/// 备份 tab 的共享状态（全部是 `Copy` 的信号，可直接在闭包间传递）。
#[derive(Clone, Copy)]
pub(super) struct BackupState {
    pub backups: RwSignal<Vec<BackupInfo>>,
    pub loading: RwSignal<bool>,
    pub importing: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub progress: RwSignal<Option<TaskProgress>>,
    operation: RwSignal<Operation>,
    list_generation: StoredValue<u64>,
}

impl BackupState {
    /// 有操作进行中（响应式读取，供视图使用）。
    pub fn busy(self) -> bool {
        self.operation.with(|op| *op != Operation::Idle) || self.importing.get()
    }

    /// 同 [`busy`](Self::busy)，非响应式读取，供事件处理器使用。
    pub fn busy_untracked(self) -> bool {
        self.operation.with_untracked(|op| *op != Operation::Idle) || self.importing.get_untracked()
    }

    /// 重新读取备份列表。只接受最新一次请求的结果。
    pub fn refresh(self) {
        let _ = self.error.try_set(None);
        let Some(generation) = self.list_generation.try_update_value(|g| {
            *g += 1;
            *g
        }) else {
            return;
        };
        let _ = self.loading.try_set(true);
        leptos::task::spawn_local(async move {
            let result = list_backups().await;
            if self.list_generation.try_get_value() != Some(generation) {
                return;
            }
            let _ = self.loading.try_set(false);
            match result {
                Ok(list) => {
                    let _ = self.backups.try_set(list);
                }
                Err(error) => {
                    let _ = self
                        .error
                        .try_set(Some(failure_message("读取备份列表失败", &error)));
                }
            }
        });
    }

    /// 创建备份（`restore` 为 `None`）或从指定文件恢复。
    pub fn start(self, restore: Option<String>) {
        if self.busy_untracked() {
            return;
        }
        self.operation.set(Operation::Starting);
        self.progress.set(None);
        self.error.set(None);
        leptos::task::spawn_local(async move {
            let restoring = restore.is_some();
            let result = match restore {
                Some(filename) => restore_backup(filename, true).await,
                None => create_backup().await,
            };
            match result {
                Ok(id) => {
                    let _ = self.operation.try_set(Operation::Running(id));
                }
                Err(error) => {
                    let action = if restoring {
                        "恢复失败"
                    } else {
                        "备份失败"
                    };
                    let _ = self.error.try_set(Some(failure_message(action, &error)));
                    let _ = self.operation.try_set(Operation::Idle);
                }
            }
        });
    }

    /// 删除一份备份，成功后刷新列表。
    pub fn delete(self, filename: String) {
        if self.busy_untracked() {
            return;
        }
        self.operation.set(Operation::Deleting);
        self.error.set(None);
        leptos::task::spawn_local(async move {
            match delete_backup(filename).await {
                Ok(()) => self.refresh(),
                Err(error) => {
                    let _ = self
                        .error
                        .try_set(Some(failure_message("删除失败", &error)));
                }
            }
            let _ = self.operation.try_set(Operation::Idle);
        });
    }
}

/// 创建 tab 状态，挂载时读取列表，并在有运行中的任务时轮询进度。
pub(super) fn use_backup_task() -> BackupState {
    let state = BackupState {
        backups: RwSignal::new(Vec::new()),
        loading: RwSignal::new(false),
        importing: RwSignal::new(false),
        error: RwSignal::new(None),
        progress: RwSignal::new(None),
        operation: RwSignal::new(Operation::Idle),
        list_generation: StoredValue::new(0),
    };
    // Effect 只在浏览器执行：挂载后读一次列表。
    Effect::new(move |_| state.refresh());
    // 任务 id 变化（或卸载）时 effect 重跑 / 销毁，清理回调让上一轮轮询停止。
    Effect::new(move |_| {
        let Operation::Running(id) = state.operation.get() else {
            return;
        };
        let alive = Arc::new(AtomicBool::new(true));
        let stop = alive.clone();
        on_cleanup(move || stop.store(false, Ordering::Relaxed));
        leptos::task::spawn_local(async move {
            loop {
                crate::utils::time::sleep_ms(1500).await;
                if !alive.load(Ordering::Relaxed) {
                    return;
                }
                let result = get_task_progress(id.clone()).await;
                if !alive.load(Ordering::Relaxed) {
                    return;
                }
                match result {
                    Ok(progress) => {
                        let done = progress.status != TaskStatus::Running;
                        let _ = state.progress.try_set(Some(progress));
                        if done {
                            state.refresh();
                            let _ = state.operation.try_set(Operation::Idle);
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = state
                            .error
                            .try_set(Some(failure_message("获取备份进度失败", &error)));
                        let _ = state.operation.try_set(Operation::Idle);
                        return;
                    }
                }
            }
        });
    });
    state
}
