//! export behavior of the log page.

#[cfg(target_arch = "wasm32")]
pub(super) fn download_text_file(filename: &str, content: &str) {
    use wasm_bindgen::JsCast;

    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let parts = js_sys::Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(content));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("text/plain;charset=utf-8");
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    if let Ok(el) = document.create_element("a") {
        if let Ok(anchor) = el.dyn_into::<web_sys::HtmlAnchorElement>() {
            anchor.set_href(&url);
            anchor.set_download(filename);
            anchor.click();
        }
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

/// 导出文件名：`yggdrasil-logs-<YYYY-MM-DD>.log`（浏览器本地日期）。
#[cfg(target_arch = "wasm32")]
pub(super) fn log_export_filename() -> String {
    let iso: String = js_sys::Date::new_0().to_iso_string().into();
    let date = iso.get(..10).unwrap_or("unknown-date");
    format!("yggdrasil-logs-{date}.log")
}
