//! 上传引擎内部状态机：与 UI 完全解耦的纯逻辑（无 `rsx!` / 无 Dioxus 组件），
//! 供 [`super::asset_upload::AssetUploadModal`] 组合使用。
//!
//! 三条入口（点击选择 / 拖拽 / 粘贴）收敛到同一个 [`enqueue_files`]：拿到的
//! `web_sys::File` 先过 [`validate_file`]（镜像服务端 5MiB / 四种 MIME 的硬限制，
//! 不合格立即记失败行、不发请求），合格项入共享队列后由 **worker 池**并发上传：
//! 在跑 worker 数上限 = 并发配置（「站点配置」面板 / `UPLOAD_CONCURRENCY` env 播种，
//! 挂载时经 `get_upload_settings` 拉取，失败回退默认 3）。每个 worker 张间停顿
//! `500ms × 当前并发数`——N 路并行时聚合速率恒 ≤ 2/s，与默认上传限流桶
//! （`RATE_LIMIT_UPLOAD_PER_SEC=2` / `BURST=15`）对齐：N=1 时与旧顺序版逐张 500ms
//! 完全一致，N 调大也不触发 429。上传管线与文章编辑器完全同源
//! （`upload_image_file` → `POST /api/upload`）。
//!
//! 注：worker 池共享状态用 `Rc<UploadPool>` 而非裸 Cell/RefCell——`use_hook` 每次渲染
//! 都会 clone 存储值（dioxus-core `use_hook_inner` 走 `.cloned()`），裸结构会被按值
//! 拷贝，各入口闭包拿到互不同步的副本导致 id 冲突与队列分裂；Rc 克隆共享同一实例。

#[cfg(target_arch = "wasm32")]
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::bridges::tiptap::upload_image_file;
#[cfg(target_arch = "wasm32")]
use crate::utils::format_bytes;

/// 单文件大小硬上限（5MiB），镜像服务端 `crate::utils::server::MAX_FILE_SIZE`。
#[cfg(any(test, target_arch = "wasm32"))]
const MAX_UPLOAD_BYTES: u64 = 5 * 1024 * 1024;
/// 允许的 MIME 白名单，镜像服务端 `api/upload.rs` 的 `ALLOWED_MIME_TYPES`。
#[cfg(any(test, target_arch = "wasm32"))]
const ALLOWED_MIME: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// 单条上传状态机：Queued → Uploading → Done / Failed(原因)。
// 变体仅在 WASM 端构造，server SSR 只匹配渲染，非 wasm 构建放行 dead_code。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone, PartialEq)]
pub(crate) enum UploadStatus {
    Queued,
    Uploading,
    Done,
    Failed(String),
}

/// 列表行数据（纯数据，两端都可编译；`web_sys::File` 句柄不进 signal，
/// 由 WASM 端的 files 表以 id 关联保存，供重试取回）。
// 同 UploadStatus：实例仅在 WASM 端构造。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone, PartialEq)]
pub(crate) struct UploadItem {
    pub(crate) id: u64,
    pub(crate) name: String,
    /// 入队时已用 `format_bytes` 格式化好的可读大小。
    pub(crate) size: String,
    pub(crate) status: UploadStatus,
    /// 用户点了 ×：先播退出动画（animate-row-leave），EXIT_ANIM_MS 后真正摘除。
    pub(crate) removing: bool,
}

/// 一批入队（一次拖拽/选择/粘贴）的完成追踪：remaining 归零且 ≥1 成功时回调一次。
#[cfg(target_arch = "wasm32")]
struct BatchCtx {
    remaining: std::cell::Cell<usize>,
    any_done: std::cell::Cell<bool>,
}

/// Worker 池共享状态：id 分配、重试句柄表、待传队列、在跑 worker 数。
/// `use_hook` 持 `Rc<UploadPool>`：渲染期 clone 的是 Rc（共享同一实例），各入口闭包
/// 与 worker 看到的永远是同一份队列/句柄表。字段私有——UI 层只经下方方法访问，
/// 不直接触碰 files/queue 等内部表示。
#[cfg(target_arch = "wasm32")]
pub(crate) struct UploadPool {
    next_id: std::cell::Cell<u64>,
    files: std::cell::RefCell<Vec<(u64, web_sys::File)>>,
    queue: std::cell::RefCell<std::collections::VecDeque<(u64, std::rc::Rc<BatchCtx>)>>,
    active_workers: std::cell::Cell<u32>,
}

#[cfg(target_arch = "wasm32")]
impl UploadPool {
    pub(crate) fn new() -> Self {
        Self {
            next_id: std::cell::Cell::new(0),
            files: std::cell::RefCell::new(Vec::new()),
            queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            active_workers: std::cell::Cell::new(0),
        }
    }

    /// 取回指定 id 的文件句柄克隆（供 UI 层「重试」重新读取原始文件；不移除句柄表条目）。
    pub(crate) fn find_file(&self, id: u64) -> Option<web_sys::File> {
        self.files
            .borrow()
            .iter()
            .find(|(fid, _)| *fid == id)
            .map(|(_, f)| f.clone())
    }

    /// 移除指定 id 的文件句柄（UI 层用户点「移除」摘除该条目时调用，释放持有）。
    pub(crate) fn remove_file(&self, id: u64) {
        self.files.borrow_mut().retain(|(fid, _)| *fid != id);
    }
}

/// 预校验：MIME 白名单 + 5MiB 上限。失败返回可读原因（直接展示在行内，不发请求）。
///
/// `pub(crate)`：`asset_picker.rs` 的内嵌上传入口复用同一份校验规则，与本模块
/// worker 池入队时执行的规则保持单一实现来源。
#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) fn validate_file(mime: &str, size: u64) -> Result<(), String> {
    if !ALLOWED_MIME.contains(&mime) {
        return Err("不支持的文件类型（仅 JPEG / PNG / GIF / WebP）".into());
    }
    if size > MAX_UPLOAD_BYTES {
        return Err("大小超过 5MB 限制".into());
    }
    Ok(())
}

/// 更新单条状态：write guard 在函数内立即释放，绝不跨 await 持有。
#[cfg(target_arch = "wasm32")]
pub(crate) fn set_status(items: &mut Signal<Vec<UploadItem>>, id: u64, status: UploadStatus) {
    let mut guard = items.write();
    if let Some(it) = guard.iter_mut().find(|it| it.id == id) {
        it.status = status;
    }
}

/// 单个 worker 的消费循环：队列空即退出并归还名额。
///
/// 张间停顿 `500ms × 当前并发数`：N 个 worker 并行时聚合速率恒 ≤ 2/s（停顿随 N
/// 线性放大），与默认上传限流桶（`RATE_LIMIT_UPLOAD_PER_SEC=2` / `BURST=15`）对齐——
/// N=1 时与旧顺序版逐张 500ms 完全一致，N 调大也不会触发 429。
#[cfg(target_arch = "wasm32")]
async fn worker_loop(
    mut items: Signal<Vec<UploadItem>>,
    pool: std::rc::Rc<UploadPool>,
    concurrency: Signal<i32>,
    on_uploaded: EventHandler<()>,
) {
    loop {
        // borrow 不出块，guard 不跨 await。
        let next = pool.queue.borrow_mut().pop_front();
        let Some((id, batch)) = next else { break };
        // 句柄可能已被「×」移除：跳过上传但仍计入批次完成度（否则该批永不合拢，
        // 其他成功项的 on_uploaded 无法触发）。
        let file = pool.find_file(id);
        if let Some(file) = file {
            set_status(&mut items, id, UploadStatus::Uploading);
            match upload_image_file(file).await {
                Ok(_) => {
                    set_status(&mut items, id, UploadStatus::Done);
                    batch.any_done.set(true);
                }
                Err(msg) => set_status(&mut items, id, UploadStatus::Failed(msg)),
            }
        }
        // 本批最后一条收尾且 ≥1 成功 → 回调一次（父组件刷新网格）。
        let remaining = batch.remaining.get() - 1;
        batch.remaining.set(remaining);
        if remaining == 0 && batch.any_done.get() {
            on_uploaded.call(());
        }
        // 队列未空则停顿压速率；停顿随并发数线性放大（见 fn doc），live 读取
        // 让面板改动即时生效。clamp(1, 32) 纯防御：服务端已钳到 1–8，
        // 此处只防异常值导致 500*n 溢出或路径级长停。
        if !pool.queue.borrow().is_empty() {
            let n = (*concurrency.peek()).clamp(1, 32) as u32;
            crate::utils::time::sleep_ms(500 * n).await;
        }
    }
    pool.active_workers.set(pool.active_workers.get() - 1);
}

/// 三入口收敛点：校验入队 + 按需补足 worker。仅 WASM 端存在（`web_sys::File` /
/// `spawn` / `upload_image_file` 都是 WASM-only 符号）。
#[cfg(target_arch = "wasm32")]
pub(crate) fn enqueue_files(
    mut items: Signal<Vec<UploadItem>>,
    pool: std::rc::Rc<UploadPool>,
    concurrency: Signal<i32>,
    on_uploaded: EventHandler<()>,
    new_files: Vec<web_sys::File>,
) {
    // 1) 校验入队：不合格直接记 Failed（不发请求）；合格记 Queued 并留存句柄供重试。
    let mut valid_ids = Vec::new();
    for file in new_files {
        let id = pool.next_id.get() + 1;
        pool.next_id.set(id);
        let item = UploadItem {
            id,
            name: file.name(),
            size: format_bytes(file.size() as i64),
            removing: false,
            status: match validate_file(&file.type_(), file.size() as u64) {
                Ok(()) => {
                    pool.files.borrow_mut().push((id, file));
                    valid_ids.push(id);
                    UploadStatus::Queued
                }
                Err(msg) => UploadStatus::Failed(msg),
            },
        };
        items.write().push(item);
    }
    if valid_ids.is_empty() {
        return;
    }

    // 2) 一批一个 BatchCtx 追踪完成度；id 入共享队列后补足 worker：在跑数低于
    //    并发上限且队列非空时逐个 spawn（worker 队列空自退，不会超额驻留）。
    let batch = std::rc::Rc::new(BatchCtx {
        remaining: std::cell::Cell::new(valid_ids.len()),
        any_done: std::cell::Cell::new(false),
    });
    {
        let mut q = pool.queue.borrow_mut();
        for id in valid_ids {
            q.push_back((id, batch.clone()));
        }
    }
    // clamp 上界与 worker_loop 的停顿同款防御（正常值 1–8）。
    let target = (*concurrency.peek()).clamp(1, 32) as u32;
    while pool.active_workers.get() < target && !pool.queue.borrow().is_empty() {
        pool.active_workers.set(pool.active_workers.get() + 1);
        spawn(worker_loop(items, pool.clone(), concurrency, on_uploaded));
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_file, ALLOWED_MIME, MAX_UPLOAD_BYTES};

    /// 四种支持的类型在上限边缘全部接受。
    #[test]
    fn validate_file_accepts_supported_types_at_limit() {
        for mime in ALLOWED_MIME {
            assert!(
                validate_file(mime, MAX_UPLOAD_BYTES).is_ok(),
                "{mime} 应被接受"
            );
        }
    }

    /// svg 不在白名单（服务端同样拒绝）。
    #[test]
    fn validate_file_rejects_svg() {
        assert!(validate_file("image/svg+xml", 1024).is_err());
    }

    /// 空 MIME（浏览器给不出类型）也拒绝。
    #[test]
    fn validate_file_rejects_empty_mime() {
        assert!(validate_file("", 1024).is_err());
    }

    /// 上限 +1 字节拒绝。
    #[test]
    fn validate_file_rejects_oversize() {
        assert!(validate_file("image/png", MAX_UPLOAD_BYTES + 1).is_err());
    }
}
