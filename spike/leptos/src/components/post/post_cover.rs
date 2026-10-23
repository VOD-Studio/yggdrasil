//! 文章封面组件
//!
//! 在文章详情页渲染封面大图，使用 blur-up 双层结构与 lightbox.js 灯箱。
//! 封面为单张（data-single），不参与正文图集切换。

use leptos::prelude::*;

/// 文章封面组件。
///
/// Props：
/// - `src`：封面原图 URL
/// - `post_id`：参与文章共享过渡的文章 ID（可选）
///
/// 渲染 `.blur-img` 结构，与正文图片一致；`lightbox-single` 标记为单张，
/// 由 `lightbox.js` 接管点击放大（原地缩放飞出 + 原图展示）。
/// 服务端读取真实尺寸写入 `--ar`，确保占位期间维持正确宽高比。
#[component]
pub fn PostCover(
    #[prop(into)] src: String,
    #[prop(optional)] post_id: Option<i32>,
) -> impl IntoView {
    // SSR 时读真实尺寸算 --ar；WASM 端不读（HTML 已在 SSR 写入，水合不改属性）。
    let ar_style = {
        #[cfg_attr(not(feature = "ssr"), allow(unused_mut))]
        let mut s = String::new();
        #[cfg(feature = "ssr")]
        {
            if let Some(rel) = src
                .strip_prefix("/uploads/")
                .map(|p| p.split('?').next().unwrap_or(p))
            {
                if let Some((w, h)) = image_dims::get_image_dimensions(rel) {
                    // CSS aspect-ratio 用斜杠分隔（width / height）
                    s = format!("--ar:{} / {};", w, h);
                }
            }
        }
        s
    };

    // 占位图 ?w=20，展示图 ?w=1200；灯箱原图由 lightbox.js 去 query 得到。
    let base = src.split('?').next().unwrap_or(&src);
    let (placeholder_src, full_src) = if src.contains('?') {
        (format!("{base}&w=20"), format!("{base}&w=1200"))
    } else {
        (format!("{src}?w=20"), format!("{src}?w=1200"))
    };
    let vt_post_id = post_id.map(|id| id.to_string());
    let vt_role = post_id.map(|_| "cover");

    view! {
        <figure class="entry-cover" data-vt-post-id=vt_post_id data-vt-role=vt_role>
            <span class="blur-img entry-cover-blur lightbox-single" style=ar_style>
                <img class="blur-img-placeholder" src=placeholder_src alt="封面图片"/>
                <img class="blur-img-full" data-src=full_src alt="封面图片"/>
            </span>
        </figure>
    }
}

/// 封面尺寸读取（对应主 crate `api::image::get_image_dimensions`）。
///
/// 只读文件头部 64 KiB：尺寸信息位于各格式 header（PNG IHDR / GIF / WebP RIFF /
/// JPEG SOF）。主 crate 用 `image` crate 解析 jpeg/png/gif；原型不引入该依赖，
/// 按格式规范手工解析 header，结果一致。
#[cfg(feature = "ssr")]
mod image_dims {
    use std::io::Read;
    use std::path::PathBuf;
    use std::sync::LazyLock;

    /// 尺寸缓存：key 为相对路径。
    static IMAGE_DIMENSIONS_CACHE: LazyLock<moka::sync::Cache<String, (u32, u32)>> =
        LazyLock::new(|| moka::sync::Cache::new(4096));

    /// 上传目录：`SPIKE_UPLOADS_DIR`，缺省为主仓库的 `uploads/`（与 `SPIKE_PUBLIC_DIR`
    /// 缺省 `../../public` 同一约定）。
    fn uploads_dir() -> PathBuf {
        std::env::var_os("SPIKE_UPLOADS_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("../../uploads"))
    }

    /// - `rel_path`：相对路径如 "2026/06/22/x.webp"（不含 /uploads/ 前缀和 query）
    /// - 优先查缓存；miss 时读文件、解析 header、写入缓存
    /// - 失败返回 None（调用方回退到不设 aspect-ratio）
    pub fn get_image_dimensions(rel_path: &str) -> Option<(u32, u32)> {
        if let Some(dims) = IMAGE_DIMENSIONS_CACHE.get(rel_path) {
            return Some(dims);
        }
        // 拒绝跳出上传目录的路径。
        if rel_path.split('/').any(|seg| seg == "..") {
            return None;
        }
        let file = std::fs::File::open(uploads_dir().join(rel_path)).ok()?;
        let mut header = Vec::new();
        file.take(65_536).read_to_end(&mut header).ok()?;
        let dims = read_dimensions_from_bytes(&header, rel_path)?;
        IMAGE_DIMENSIONS_CACHE.insert(rel_path.to_string(), dims);
        Some(dims)
    }

    /// 按扩展名分发。
    fn read_dimensions_from_bytes(data: &[u8], path: &str) -> Option<(u32, u32)> {
        let ext = std::path::Path::new(path)
            .extension()?
            .to_str()?
            .to_lowercase();
        let dims = match ext.as_str() {
            "webp" => read_webp_dimensions(data),
            "jpg" | "jpeg" => read_jpeg_dimensions(data),
            "png" => read_png_dimensions(data),
            "gif" => read_gif_dimensions(data),
            _ => None,
        }?;
        (dims.0 > 0 && dims.1 > 0).then_some(dims)
    }

    fn read_webp_dimensions(data: &[u8]) -> Option<(u32, u32)> {
        // RIFF(4) + size(4) + WEBP(4) + chunk(4) = 16 字节签名；VP8X 尺寸到 byte 29。
        if data.len() < 30 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
            return None;
        }
        match &data[12..16] {
            b"VP8 " => {
                // lossy：byte 26-27 / 28-29 为 u16_le，低 14 位是尺寸（高 2 位是缩放系数）。
                let w = u16::from_le_bytes([data[26], data[27]]) & 0x3FFF;
                let h = u16::from_le_bytes([data[28], data[29]]) & 0x3FFF;
                Some((w as u32, h as u32))
            }
            b"VP8L" => {
                // lossless：byte 21-24 为 u32_le bitstream header。
                let h = u32::from_le_bytes([data[21], data[22], data[23], data[24]]);
                Some(((1 + h) & 0x3FFF, (1 + (h >> 14)) & 0x3FFF))
            }
            b"VP8X" => {
                // extended：byte 24-26 / 27-29 各为 24-bit LE，+1 得画布尺寸。
                let w = u32::from_le_bytes([data[24], data[25], data[26], 0]) + 1;
                let h = u32::from_le_bytes([data[27], data[28], data[29], 0]) + 1;
                Some((w, h))
            }
            _ => None,
        }
    }

    fn read_png_dimensions(data: &[u8]) -> Option<(u32, u32)> {
        // 8 字节签名 + IHDR（长度 4 + 类型 4），宽高为 byte 16-23 的 u32_be。
        if data.len() < 24 || &data[0..8] != b"\x89PNG\r\n\x1a\n" || &data[12..16] != b"IHDR" {
            return None;
        }
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        Some((w, h))
    }

    fn read_gif_dimensions(data: &[u8]) -> Option<(u32, u32)> {
        // GIF87a/GIF89a + 逻辑屏幕宽高（u16_le）。
        if data.len() < 10 || !(data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
            return None;
        }
        let w = u16::from_le_bytes([data[6], data[7]]);
        let h = u16::from_le_bytes([data[8], data[9]]);
        Some((w as u32, h as u32))
    }

    fn read_jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
        // 逐段扫描标记，SOF0..SOF15（除 DHT C4、JPG C8、DAC CC）携带高宽（u16_be）。
        if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
            return None;
        }
        let mut i = 2;
        while i + 3 < data.len() {
            if data[i] != 0xFF {
                return None;
            }
            let marker = data[i + 1];
            // 填充字节。
            if marker == 0xFF {
                i += 1;
                continue;
            }
            // 无长度的独立标记。
            if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
                i += 2;
                continue;
            }
            let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            let is_sof = (0xC0..=0xCF).contains(&marker)
                && marker != 0xC4
                && marker != 0xC8
                && marker != 0xCC;
            if is_sof {
                if i + 8 >= data.len() {
                    return None;
                }
                let h = u16::from_be_bytes([data[i + 5], data[i + 6]]);
                let w = u16::from_be_bytes([data[i + 7], data[i + 8]]);
                return Some((w as u32, h as u32));
            }
            if marker == 0xDA || len < 2 {
                return None;
            }
            i += 2 + len;
        }
        None
    }
}
