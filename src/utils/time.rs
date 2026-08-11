//! 跨平台时间/睡眠工具。
//!
//! 根据目标架构分别实现：
//! - `wasm32`：通过 `js_sys` 调用 JavaScript 的 `setTimeout` / `Date.now()`。
//! - 其他平台：使用 `tokio::time::sleep` / `chrono::Utc`。
//!
//! 相对时间分档（`relative_label_from_millis` / `format_relative_time_iso`）由
//! 前端待审核评论展示与服务端评论预渲染共享，保证两端口径一致。

use chrono::DateTime;

/// 异步睡眠指定毫秒数。
///
/// WASM 端用 `js_sys::Promise` + `web_sys::Window::set_timeout_*` 构造，
/// 避免 `js_sys::eval` 字符串求值。全项目统一的 sleep 入口。
///
/// `setTimeout` 的 delay 参数是 i32，超过 `i32::MAX` 会被浏览器立即触发；这里 clamp
/// 到安全上限，既避免 `u32 -> i32` 转换溢出 panic，也贴合 setTimeout 的合法范围。
#[cfg(target_arch = "wasm32")]
pub async fn sleep_ms(ms: u32) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        // ms 是 u32，setTimeout 接受 i32；clamp 到 i32::MAX（约 24.8 天）避免溢出。
        let delay = ms.min(i32::MAX as u32) as i32;
        let window = web_sys::window().expect("sleep_ms 必须在浏览器上下文中调用：无 window");
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve.unchecked_into(), delay)
            .expect("setTimeout with a number delay cannot fail per WebIDL");
    });
    let _ = JsFuture::from(promise).await;
}

/// 异步睡眠指定毫秒数（原生 tokio 版本）。
///
/// 仅在 `server` feature 启用且非 wasm32 目标下编译。`tokio` 是 server-only 的
/// optional 依赖（见 Cargo.toml），不可用 `#[cfg(not(target_arch = "wasm32"))]`——
/// 那样会在「非 wasm32 主机 + 仅 web feature」组合下误激活，此时 tokio 未引入，
/// 导致编译失败（此 bug 曾被 `[dev-dependencies] tokio` 掩盖，发布构建才暴露）。
#[cfg(all(feature = "server", not(target_arch = "wasm32")))]
pub async fn sleep_ms(ms: u32) {
    tokio::time::sleep(std::time::Duration::from_millis(ms as u64)).await;
}

/// `sleep_ms` 的占位 stub，仅用于「非 wasm32 且非 server」的无效构建组合。
///
/// 此组合（如非 wasm32 主机执行 `cargo build --features web`）不是有效部署目标——
/// web feature 的真实构建目标就是 wasm32，会走上面的 JS setTimeout 分支。
/// 此 stub 仅保证符号可编译，永远不会在有效运行时被调用；若被调用说明部署配置错误。
#[cfg(all(not(feature = "server"), not(target_arch = "wasm32")))]
pub async fn sleep_ms(_ms: u32) {
    panic!("sleep_ms 在非 wasm32 且非 server 的无效构建组合下被调用：请检查 feature 配置");
}

/// 获取当前时间戳（毫秒）。
///
/// WASM 端使用 `js_sys::Date::now()`，服务端回退到 `chrono::Utc`。
pub fn now_millis() -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as i64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        chrono::Utc::now().timestamp_millis()
    }
}

/// UTC "HH:MM" → 浏览器本地 "HH:MM"（按当天时区偏移换算）。
///
/// 非 wasm32 原样返回（SSR 不渲染设置值，此分支不会出现在用户可见路径）。
/// 供备份设置卡片在挂载回填时使用——服务端只存 UTC，面板按本地时区显示。
pub fn utc_hhmm_to_local(t: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let mut parts = t.split(':');
        let h = parts
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        let m = parts
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
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
pub fn local_hhmm_to_utc(t: &str) -> String {
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

/// 相对时间分档：根据"距现在的毫秒数"返回 (相对文本, 绝对日期 YYYY-MM-DD)。
///
/// 分档规则与服务端 `format_relative_time` 完全一致，前端在展示待审核评论时复用，
/// 保证两类评论的时间展示口径统一。返回绝对日期用于 `title` 悬浮提示。
///
/// - `delta_millis`：目标时间与"现在"的差值（毫秒）。正值表示过去，负值表示未来（兜底按刚刚处理）。
/// - `created_iso`：评论的 RFC3339 创建时间，用于兜底生成绝对日期。
pub fn relative_label_from_millis(delta_millis: i64, created_iso: &str) -> (String, String) {
    let dt = DateTime::parse_from_rfc3339(created_iso).ok();
    relative_label_inner(delta_millis, dt.as_ref())
}

/// 桶化相对时间标签 + 绝对日期，复用已解析的 DateTime 避免二次 ISO 解析。
fn relative_label_inner(
    delta_millis: i64,
    dt: Option<&chrono::DateTime<chrono::FixedOffset>>,
) -> (String, String) {
    let seconds = delta_millis / 1000;
    let label = if seconds < 60 {
        "刚刚".to_string()
    } else {
        let minutes = seconds / 60;
        if minutes < 60 {
            format!("{minutes} 分钟前")
        } else {
            let hours = minutes / 60;
            if hours < 24 {
                format!("{hours} 小时前")
            } else {
                let days = hours / 24;
                if days < 30 {
                    format!("{days} 天前")
                } else {
                    // 超过 30 天直接显示日期，下方 absolute 复用
                    String::new()
                }
            }
        }
    };

    // 绝对日期：优先解析 ISO；解析失败时退化为空串，避免组件报错。
    let absolute = dt
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();

    let label = if label.is_empty() {
        absolute.clone()
    } else {
        label
    };
    (label, absolute)
}

/// 前端友好的相对时间格式化：返回相对文本，用于展示待审核评论的创建时间。
///
/// 这是 `relative_label_from_millis` 的薄封装，仅返回相对文本。
pub fn format_relative_time_iso(created_iso: &str) -> String {
    let dt = DateTime::parse_from_rfc3339(created_iso).ok();
    let delta_millis = match &dt {
        Some(d) => now_millis() - d.timestamp_millis(),
        None => return "刚刚".to_string(),
    };
    relative_label_inner(delta_millis, dt.as_ref()).0
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISO: &str = "2026-06-22T05:43:57.565+00:00";

    #[test]
    fn relative_label_just_now_under_60s() {
        let (label, _) = relative_label_from_millis(0, ISO);
        assert_eq!(label, "刚刚");
        let (label, _) = relative_label_from_millis(59_999, ISO);
        assert_eq!(label, "刚刚");
    }

    #[test]
    fn relative_label_minutes() {
        let (label, _) = relative_label_from_millis(60_000, ISO);
        assert_eq!(label, "1 分钟前");
        let (label, _) = relative_label_from_millis(5 * 60_000, ISO);
        assert_eq!(label, "5 分钟前");
        let (label, _) = relative_label_from_millis(59 * 60_000, ISO);
        assert_eq!(label, "59 分钟前");
    }

    #[test]
    fn relative_label_hours() {
        let (label, _) = relative_label_from_millis(60 * 60_000, ISO);
        assert_eq!(label, "1 小时前");
        let (label, _) = relative_label_from_millis(3 * 3_600_000, ISO);
        assert_eq!(label, "3 小时前");
        let (label, _) = relative_label_from_millis(23 * 3_600_000, ISO);
        assert_eq!(label, "23 小时前");
    }

    #[test]
    fn relative_label_days() {
        let (label, _) = relative_label_from_millis(24 * 3_600_000, ISO);
        assert_eq!(label, "1 天前");
        let (label, _) = relative_label_from_millis(7 * 24 * 3_600_000, ISO);
        assert_eq!(label, "7 天前");
        let (label, _) = relative_label_from_millis(29 * 24 * 3_600_000, ISO);
        assert_eq!(label, "29 天前");
    }

    #[test]
    fn relative_label_falls_back_to_date_over_30_days() {
        let (label, absolute) = relative_label_from_millis(60 * 24 * 3_600_000, ISO);
        assert_eq!(label, "2026-06-22");
        assert_eq!(absolute, "2026-06-22");
    }

    #[test]
    fn relative_label_future_falls_back_to_just_now() {
        // 未来时间差为负，秒数 < 60，归为"刚刚"。
        let (label, _) = relative_label_from_millis(-5_000, ISO);
        assert_eq!(label, "刚刚");
    }

    #[test]
    fn relative_label_invalid_iso_still_returns_absolute_empty() {
        // 无法解析时 absolute 为空，但分档逻辑仍按 delta 决定。
        let (label, absolute) = relative_label_from_millis(0, "not-a-date");
        assert_eq!(label, "刚刚");
        assert_eq!(absolute, "");
    }

    #[test]
    fn format_relative_time_iso_invalid_iso_falls_back() {
        // 解析失败退化为"刚刚"，不 panic。
        assert_eq!(format_relative_time_iso("not-a-date"), "刚刚");
    }
}
