//! 固定路径静态资源的内容版本号。
//!
//! 全局样式、启动脚本和按需加载的浏览器库放在 `public/` 下，文件名固定。页面引用它们时带
//! `?v=<内容摘要>`：内容一变地址就变。EdgeOne 的缓存键包含查询参数，所以浏览器和 CDN
//! 节点都能长期缓存，见 `middleware::cache_control_for_path`。只有版本号与当前文件一致时
//! 才返回 immutable；滚动发布期间旧页面带旧版本号来请求，拿到的新内容仍须每次校验，
//! 不会以旧地址被长期缓存。
//!
//! 服务端按文件内容计算摘要，按修改时间和长度缓存。`make dev-assets` 改了文件，下一次
//! 渲染就会换新版本号。版本表经 `shell` 里的 `<script type="application/json">`
//! （id 为 [`VERSIONS_ELEMENT_ID`]）下发给浏览器：Rust 端用 [`url`] 读取，libs 用
//! `@yggdrasil/shared` 的 `assetUrl` 读取。文件不存在时（未构建 libs 的测试或开发环境）
//! 返回不带版本号的地址，缓存策略也退回 no-cache。

/// 站点样式（Tailwind 产物）。
pub const STYLE_CSS: &str = "/style.css";
/// 代码高亮样式。
pub const HIGHLIGHT_CSS: &str = "/highlight.css";
/// KaTeX 样式。
pub const KATEX_CSS: &str = "/katex/katex.min.css";
/// yggdrasil-core 样式。
pub const CORE_CSS: &str = "/yggdrasil-core/yggdrasil-core.css";
/// yggdrasil-core 启动脚本（导航协调器、主题、浏览器库加载器）。
pub const CORE_JS: &str = "/yggdrasil-core/yggdrasil-core.js";
/// 正文增强脚本（mermaid、目录、changelog、瀑布流）。
pub const ENHANCE_JS: &str = "/yggdrasil-core/yggdrasil-enhance.js";
/// 灯箱脚本。
pub const LIGHTBOX_JS: &str = "/lightbox/lightbox.js";
/// 灯箱样式。
pub const LIGHTBOX_CSS: &str = "/lightbox/lightbox.css";

/// 下发版本表的 `<script type="application/json">` 元素 id，libs 里同名常量须保持一致。
pub const VERSIONS_ELEMENT_ID: &str = "ygg-asset-versions";

/// 参与版本化的全部资源。后六项由 libs 按需加载，地址写在
/// `libs/yggdrasil-core/src/browser-library.ts` 与 `libs/shared/src/index.ts`。
pub const ALL: &[&str] = &[
    STYLE_CSS,
    HIGHLIGHT_CSS,
    KATEX_CSS,
    CORE_CSS,
    CORE_JS,
    ENHANCE_JS,
    LIGHTBOX_JS,
    LIGHTBOX_CSS,
    "/tiptap/editor.js",
    "/tiptap/editor.css",
    "/codemirror/editor.js",
    "/xterm/terminal.js",
    "/xterm/terminal.css",
    "/mermaid/mermaid.js",
];

/// 带版本号的地址；没有版本号（文件缺失）时原样返回。
pub fn url(path: &'static str) -> String {
    match version(path) {
        Some(version) => format!("{path}?v={version}"),
        None => path.to_string(),
    }
}

#[cfg(feature = "server")]
pub use server::{is_current, versions_json};

#[cfg(feature = "server")]
use server::version;

#[cfg(feature = "server")]
mod server {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex, PoisonError};
    use std::time::SystemTime;

    use sha2::Digest;

    use super::ALL;

    struct Hashed {
        modified: SystemTime,
        len: u64,
        version: String,
    }

    static CACHE: LazyLock<Mutex<HashMap<&'static str, Hashed>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    /// 当前文件内容的版本号：SHA-256 前 8 字节的十六进制。不在 [`ALL`] 里或文件不存在时为 None。
    pub(super) fn version(path: &str) -> Option<String> {
        let path = *ALL.iter().find(|known| **known == path)?;
        let file = crate::startup::site_root().join(path.trim_start_matches('/'));
        let meta = std::fs::metadata(&file).ok()?;
        let (modified, len) = (meta.modified().ok()?, meta.len());
        {
            let cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(hashed) = cache.get(path) {
                if hashed.modified == modified && hashed.len == len {
                    return Some(hashed.version.clone());
                }
            }
        }
        let bytes = std::fs::read(&file).ok()?;
        let version = hex::encode(&sha2::Sha256::digest(&bytes)[..8]);
        CACHE.lock().unwrap_or_else(PoisonError::into_inner).insert(
            path,
            Hashed {
                modified,
                len,
                version: version.clone(),
            },
        );
        Some(version)
    }

    /// 请求的 `v` 参数是否等于该资源当前的版本号。
    pub fn is_current(path: &str, query: Option<&str>) -> bool {
        let Some(requested) = query
            .into_iter()
            .flat_map(|query| query.split('&'))
            .find_map(|pair| pair.strip_prefix("v="))
        else {
            return false;
        };
        version(path).is_some_and(|current| current == requested)
    }

    /// 版本表 JSON（`{"/style.css":"…",…}`），只含存在的文件。路径与摘要都不含需转义的字符。
    pub fn versions_json() -> String {
        let entries: Vec<String> = ALL
            .iter()
            .filter_map(|path| version(path).map(|version| format!("\"{path}\":\"{version}\"")))
            .collect();
        format!("{{{}}}", entries.join(","))
    }
}

/// 浏览器端从 SSR 下发的版本表里读取。
#[cfg(not(feature = "server"))]
fn version(path: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let text = web_sys::window()?
            .document()?
            .get_element_by_id(VERSIONS_ELEMENT_ID)?
            .text_content()?;
        let versions = js_sys::JSON::parse(&text).ok()?;
        js_sys::Reflect::get(&versions, &path.into())
            .ok()?
            .as_string()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = path;
        None
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    /// 在临时站点目录里写文件，验证版本号跟随内容变化、只认当前版本。
    #[test]
    #[serial_test::serial]
    fn version_follows_content_and_only_current_is_immutable() {
        let root = std::env::temp_dir().join(format!("ygg-static-assets-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let previous = std::env::var_os("LEPTOS_SITE_ROOT");
        std::env::set_var("LEPTOS_SITE_ROOT", &root);

        assert_eq!(url(HIGHLIGHT_CSS), HIGHLIGHT_CSS, "文件缺失时不带版本号");
        assert!(!is_current(HIGHLIGHT_CSS, Some("v=anything")));

        let file = root.join("highlight.css");
        std::fs::write(&file, "a{}").unwrap();
        let first = url(HIGHLIGHT_CSS);
        let v1 = first.strip_prefix("/highlight.css?v=").unwrap().to_string();
        assert_eq!(v1.len(), 16);
        assert!(is_current(HIGHLIGHT_CSS, Some(&format!("x=1&v={v1}"))));
        assert!(!is_current(HIGHLIGHT_CSS, None));
        assert!(!is_current(HIGHLIGHT_CSS, Some("v=0000000000000000")));
        assert_eq!(versions_json(), format!("{{\"/highlight.css\":\"{v1}\"}}"));

        // 改内容（长度也变）后版本号随之改变，旧版本号不再算当前。
        std::fs::write(&file, "a{color:red}").unwrap();
        let v2 = url(HIGHLIGHT_CSS)
            .strip_prefix("/highlight.css?v=")
            .unwrap()
            .to_string();
        assert_ne!(v1, v2);
        assert!(!is_current(HIGHLIGHT_CSS, Some(&format!("v={v1}"))));

        // 不在清单里的路径一律不版本化，防止借此读取任意文件。
        std::fs::write(root.join("other.css"), "b{}").unwrap();
        assert!(!is_current("/other.css", Some("v=x")));

        match previous {
            Some(value) => std::env::set_var("LEPTOS_SITE_ROOT", value),
            None => std::env::remove_var("LEPTOS_SITE_ROOT"),
        }
        std::fs::remove_dir_all(&root).unwrap();
    }
}
