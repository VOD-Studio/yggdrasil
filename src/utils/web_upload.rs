//! WASM 端 multipart 文件上传助手：FormData POST + `{success, error, ...}` JSON 契约解析。
//!
//! 图片上传（`bridges::tiptap::upload_image_file`）与备份导入共用同一份 fetch 样板。
//! 仅当 `success == true` 返回完整 JSON（调用方自取 `url`/`filename` 字段）；
//! 其余一律 `Err`：优先服务端中文 `error`，兜底状态码。
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;

/// 以 multipart 表单 POST 单个文件，解析服务端统一 JSON 契约。
pub async fn post_multipart_file(
    url: &str,
    field_name: &str,
    file: &web_sys::File,
) -> Result<serde_json::Value, String> {
    let form = web_sys::FormData::new().map_err(|_| "无法构造上传表单".to_string())?;
    form.append_with_blob(field_name, file)
        .map_err(|_| "无法附加文件".to_string())?;

    // credentials same-origin 携带 session cookie
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    // set_body 接收 &JsValue（非 Option）；FormData: AsRef<JsValue>。
    init.set_body(form.as_ref());
    init.set_credentials(web_sys::RequestCredentials::SameOrigin);

    let request = web_sys::Request::new_with_str_and_init(url, &init)
        .map_err(|_| "无法构造上传请求".to_string())?;

    // 该函数仅被 WASM 事件 handler 里的 spawn 调用，window 缺失意味着不在浏览器
    // 环境运行——属部署 bug 而非运行时输入。
    let window = web_sys::window().expect("multipart 上传仅在浏览器环境发生");
    let promise = window.fetch_with_request(&request);

    // fetch Promise → Future → 解析响应体
    let resp_val = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("上传请求失败: {e:?}"))?;
    let resp: web_sys::Response = resp_val
        .dyn_into()
        .map_err(|_| "上传响应类型异常".to_string())?;

    // 读响应体文本（无论 2xx 与否，服务端都返回 JSON）
    let text_promise = resp.text().map_err(|e| format!("读取响应失败: {e:?}"))?;
    let text_val = wasm_bindgen_futures::JsFuture::from(text_promise)
        .await
        .map_err(|e| format!("读取响应失败: {e:?}"))?;
    let text = text_val.as_string().unwrap_or_default();
    let data: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);

    if data["success"].as_bool() == Some(true) {
        Ok(data)
    } else {
        // 失败：优先用服务端中文 error，兜底用状态码
        Err(data["error"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("上传失败: {}", resp.status())))
    }
}
