//! The component scope owns requests and polling. Only the current task may update progress.
use crate::api::database::backup::{
    create_backup, delete_backup, list_backups, restore_backup, BackupInfo,
};
use crate::api::database::tasks::TaskProgress;
use crate::utils::error::failure_message;
use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
enum Operation {
    Idle,
    Starting,
    Running(String),
    Deleting,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct BackupState {
    pub backups: Signal<Vec<BackupInfo>>,
    pub loading: Signal<bool>,
    pub importing: Signal<bool>,
    pub error: Signal<Option<String>>,
    pub progress: Signal<Option<TaskProgress>>,
    operation: Signal<Operation>,
    list_generation: Signal<u64>,
}

impl BackupState {
    pub fn busy(self) -> bool {
        *self.operation.read() != Operation::Idle || (self.importing)()
    }

    pub fn refresh(mut self) {
        self.error.set(None);
        let next_generation = *self.list_generation.peek() + 1;
        self.list_generation.set(next_generation);
        let generation = *self.list_generation.peek();
        self.loading.set(true);
        spawn(async move {
            let result = list_backups().await;
            if *self.list_generation.peek() != generation {
                return;
            }
            self.loading.set(false);
            match result {
                Ok(list) => self.backups.set(list),
                Err(error) => self
                    .error
                    .set(Some(failure_message("读取备份列表失败", &error))),
            }
        });
    }

    pub fn start(mut self, restore: Option<String>) {
        if self.busy() {
            return;
        }
        self.operation.set(Operation::Starting);
        self.progress.set(None);
        self.error.set(None);
        spawn(async move {
            let restoring = restore.is_some();
            let result = match restore {
                Some(filename) => restore_backup(filename, true).await,
                None => create_backup().await,
            };
            match result {
                Ok(id) => self.operation.set(Operation::Running(id)),
                Err(error) => {
                    let action = if restoring {
                        "恢复失败"
                    } else {
                        "备份失败"
                    };
                    self.error.set(Some(failure_message(action, &error)));
                    self.operation.set(Operation::Idle);
                }
            }
        });
    }

    pub fn delete(mut self, filename: String) {
        if self.busy() {
            return;
        }
        self.operation.set(Operation::Deleting);
        self.error.set(None);
        spawn(async move {
            match delete_backup(filename).await {
                Ok(()) => self.refresh(),
                Err(error) => self.error.set(Some(failure_message("删除失败", &error))),
            }
            self.operation.set(Operation::Idle);
        });
    }
}

pub(super) fn use_backup_task() -> BackupState {
    let state = BackupState {
        backups: use_signal(Vec::new),
        loading: use_signal(|| false),
        importing: use_signal(|| false),
        error: use_signal(|| None),
        progress: use_signal(|| None),
        operation: use_signal(|| Operation::Idle),
        list_generation: use_signal(|| 0),
    };
    #[cfg(target_arch = "wasm32")]
    {
        use_effect(move || state.refresh());
        // Changing task id cancels the previous resource future; unmount cancels it too.
        use_resource(move || {
            let current = (state.operation)();
            async move {
                use crate::api::database::tasks::{get_task_progress, TaskStatus};
                let Operation::Running(id) = current else {
                    return;
                };
                let mut state = state;
                loop {
                    crate::utils::time::sleep_ms(1500).await;
                    let result = get_task_progress(id.clone()).await;
                    if *state.operation.peek() != Operation::Running(id.clone()) {
                        return;
                    }
                    match result {
                        Ok(progress) => {
                            let done = progress.status != TaskStatus::Running;
                            state.progress.set(Some(progress));
                            if done {
                                state.refresh();
                                state.operation.set(Operation::Idle);
                                return;
                            }
                        }
                        Err(error) => {
                            state
                                .error
                                .set(Some(failure_message("获取备份进度失败", &error)));
                            state.operation.set(Operation::Idle);
                            return;
                        }
                    }
                }
            }
        });
    }
    state
}
