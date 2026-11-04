//! HTTP 响应压缩：`COMPRESSION_ALGORITHMS` 配置、`Accept-Encoding` 协商与编码器。
//!
//! 协商规则与 tower-http 0.7 的 `CompressionLayer` 一致，SSR HTML 缓存命中时据此挑选
//! 预压缩变体（`ssr_cache`），动态响应仍由压缩层现压。

#![cfg(feature = "server")]

use std::io;
use std::sync::LazyLock;

use axum::http::{header, HeaderMap, HeaderValue};
use compression_codecs::core::util::{PartialBuffer, WriteBuffer};
use compression_codecs::core::Level;
use compression_codecs::{
    brotli::params::EncoderParams as BrotliParams, flate::params::FlateEncoderParams,
    zstd::params::CParameter, BrotliEncoder, EncodeV2, GzipEncoder, ZlibEncoder, ZstdEncoder,
};

/// 压缩算法配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompressionAlgorithms {
    pub(crate) gzip: bool,
    pub(crate) brotli: bool,
    pub(crate) deflate: bool,
    pub(crate) zstd: bool,
}

impl CompressionAlgorithms {
    pub(crate) fn all_enabled() -> Self {
        Self {
            gzip: true,
            brotli: true,
            deflate: true,
            zstd: true,
        }
    }

    fn is_empty(&self) -> bool {
        !self.gzip && !self.brotli && !self.deflate && !self.zstd
    }

    fn supports(&self, encoding: Encoding) -> bool {
        match encoding {
            Encoding::Identity => true,
            Encoding::Deflate => self.deflate,
            Encoding::Gzip => self.gzip,
            Encoding::Brotli => self.brotli,
            Encoding::Zstd => self.zstd,
        }
    }

    /// 已启用的压缩编码（不含 identity），按偏好从低到高。
    pub(crate) fn encodings(self) -> impl Iterator<Item = Encoding> {
        Encoding::COMPRESSED
            .into_iter()
            .filter(move |encoding| self.supports(*encoding))
    }
}

/// 解析 COMPRESSION_ALGORITHMS 环境变量值。
/// ""、"none"、"off" 返回 None；"all" 或未识别到任何算法时启用全部。
pub(crate) fn parse_compression_algorithms(env: &str) -> Option<CompressionAlgorithms> {
    let env = env.trim();
    if env.is_empty() || env.eq_ignore_ascii_case("none") || env.eq_ignore_ascii_case("off") {
        return None;
    }

    let mut all = false;
    let mut gzip = false;
    let mut brotli = false;
    let mut deflate = false;
    let mut zstd = false;

    for part in env.split(',') {
        match part.trim().to_lowercase().as_str() {
            "all" => all = true,
            "gzip" => gzip = true,
            "brotli" | "br" => brotli = true,
            "deflate" => deflate = true,
            "zstd" => zstd = true,
            other => tracing::warn!(
                "Unknown compression algorithm in COMPRESSION_ALGORITHMS: '{}'",
                other
            ),
        }
    }

    if all {
        return Some(CompressionAlgorithms::all_enabled());
    }

    let algorithms = CompressionAlgorithms {
        gzip,
        brotli,
        deflate,
        zstd,
    };
    if algorithms.is_empty() {
        return None;
    }

    Some(algorithms)
}

static ALGORITHMS: LazyLock<Option<CompressionAlgorithms>> = LazyLock::new(|| {
    let env = std::env::var("COMPRESSION_ALGORITHMS").unwrap_or_default();
    parse_compression_algorithms(&env)
});

/// 进程内生效的压缩配置（默认关闭）；进程启动后不变，改环境变量需重启。
pub(crate) fn algorithms() -> Option<CompressionAlgorithms> {
    *ALGORITHMS
}

/// 内容编码，变体按偏好从低到高排列：同 q 值时 zstd > br > gzip > deflate > identity。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Encoding {
    Identity,
    Deflate,
    Gzip,
    Brotli,
    Zstd,
}

impl Encoding {
    const COMPRESSED: [Encoding; 4] = [
        Encoding::Deflate,
        Encoding::Gzip,
        Encoding::Brotli,
        Encoding::Zstd,
    ];

    fn parse(token: &str) -> Option<Self> {
        [
            ("identity", Encoding::Identity),
            ("deflate", Encoding::Deflate),
            ("gzip", Encoding::Gzip),
            ("x-gzip", Encoding::Gzip),
            ("br", Encoding::Brotli),
            ("zstd", Encoding::Zstd),
        ]
        .into_iter()
        .find_map(|(name, encoding)| token.eq_ignore_ascii_case(name).then_some(encoding))
    }

    pub(crate) fn header_value(self) -> HeaderValue {
        HeaderValue::from_static(match self {
            Encoding::Identity => "identity",
            Encoding::Deflate => "deflate",
            Encoding::Gzip => "gzip",
            Encoding::Brotli => "br",
            Encoding::Zstd => "zstd",
        })
    }
}

/// RFC 9110 §5.3.1 的 q 值，按千分比存为 0..=1000；格式不合法返回 None。
fn parse_qvalue(param: &str) -> Option<u16> {
    let value = param
        .strip_prefix("q=")
        .or_else(|| param.strip_prefix("Q="))?;
    let (int, frac) = value.split_once('.').unwrap_or((value, ""));
    let base = match int {
        "0" => 0,
        "1" => 1000,
        _ => return None,
    };
    if frac.len() > 3 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let frac = frac
        .bytes()
        .zip([100, 10, 1])
        .map(|(digit, scale)| u16::from(digit - b'0') * scale)
        .sum::<u16>();
    let q = base + frac;
    (q <= 1000).then_some(q)
}

/// 按 `Accept-Encoding` 选出响应编码；返回 None 表示客户端连 identity 都拒绝（应回 406）。
///
/// 与 tower-http 0.7 相同：未列出的编码忽略；没有 `*` 时只看显式列出的编码，全为 q=0
/// 则回退 identity（除非 `identity;q=0`）；有 `*` 时未列出的已启用编码取通配 q 值。
pub(crate) fn negotiate(
    headers: &HeaderMap,
    algorithms: CompressionAlgorithms,
) -> Option<Encoding> {
    let mut explicit: Vec<(Encoding, u16)> = Vec::new();
    let mut wildcard = None;
    let entries = headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','));
    for entry in entries {
        let (token, param) = match entry.split_once(';') {
            Some((token, param)) => (token.trim(), Some(param.trim())),
            None => (entry.trim(), None),
        };
        let Some(q) = param.map_or(Some(1000), parse_qvalue) else {
            continue;
        };
        if token == "*" {
            wildcard.get_or_insert(q);
        } else if let Some(encoding) = Encoding::parse(token) {
            if algorithms.supports(encoding) {
                explicit.push((encoding, q));
            }
        }
    }

    let best = |candidates: &mut dyn Iterator<Item = (Encoding, u16)>| {
        candidates
            .filter(|(_, q)| *q > 0)
            .max_by_key(|&(encoding, q)| (q, encoding))
            .map(|(encoding, _)| encoding)
    };
    match wildcard {
        None => best(&mut explicit.iter().copied()).or_else(|| {
            let identity_rejected = explicit
                .iter()
                .any(|&(encoding, q)| encoding == Encoding::Identity && q == 0);
            (!identity_rejected).then_some(Encoding::Identity)
        }),
        Some(wildcard) => best(
            &mut std::iter::once(Encoding::Identity)
                .chain(algorithms.encodings())
                .map(|encoding| {
                    let q = explicit
                        .iter()
                        .find(|(listed, _)| *listed == encoding)
                        .map_or(wildcard, |&(_, q)| q);
                    (encoding, q)
                }),
        ),
    }
}

/// 压缩力度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Effort {
    /// 一次压缩、多次复用（SSR HTML 缓存）：取各算法高档，brotli 11 比 7 再小约 8%。
    Cached,
}

/// 现压质量（tower-http 压缩层）。tower-http 默认 brotli 4，只比 gzip-6 小 2–4%；
/// 取 7 时 brotli 比 6 再小 1.9–2.9%，每次只多几毫秒；q9 起 feed 压缩耗时成倍增加。
pub(crate) const DYNAMIC_QUALITY: i32 = 7;

impl Effort {
    fn level(self, encoding: Encoding) -> Level {
        Level::Precise(match (self, encoding) {
            (Effort::Cached, Encoding::Brotli) => 11,
            (Effort::Cached, Encoding::Zstd) => 19,
            (Effort::Cached, _) => 9,
        })
    }
}

/// brotli 默认窗口 4 MiB（lgwin 22）。流式编码不知道总长时环形缓冲区按窗口整块分配并清零，
/// 超过约 64 KiB 的页面每次都要清 4 MiB；长度已知时取刚好容纳整段输入的窗口，压缩结果不变。
fn brotli_window(len: Option<usize>) -> i32 {
    const MIN: i32 = 16;
    const MAX: i32 = 22;
    // 窗口实际容量为 2^lgwin - 16 字节。
    len.map_or(MAX, |len| {
        let bits = (len + 16).next_power_of_two().trailing_zeros();
        (bits as i32).clamp(MIN, MAX)
    })
}

/// 同步编码器：增量写入，`finish` 收尾；输出追加到调用方的 Vec。
pub(crate) struct Encoder {
    codec: Box<dyn EncodeV2 + Send>,
    scratch: Box<[u8]>,
}

impl Encoder {
    /// `len` 为已知的输入总长，用于收窄 brotli 窗口；identity 返回 None。
    pub(crate) fn new(encoding: Encoding, effort: Effort, len: Option<usize>) -> Option<Self> {
        let level = effort.level(encoding);
        let codec: Box<dyn EncodeV2 + Send> = match encoding {
            Encoding::Identity => return None,
            Encoding::Deflate => Box::new(ZlibEncoder::new(FlateEncoderParams::from(level))),
            Encoding::Gzip => Box::new(GzipEncoder::new(FlateEncoderParams::from(level))),
            Encoding::Brotli => Box::new(BrotliEncoder::new(
                BrotliParams::default()
                    .quality(level)
                    .window_size(brotli_window(len)),
            )),
            Encoding::Zstd => Box::new(ZstdEncoder::new(CParameter::quality(level))),
        };
        Some(Self {
            codec,
            scratch: vec![0; 32 * 1024].into_boxed_slice(),
        })
    }

    pub(crate) fn write(&mut self, input: &[u8], out: &mut Vec<u8>) -> io::Result<()> {
        let mut input = PartialBuffer::new(input);
        while !input.unwritten().is_empty() {
            let mut output = WriteBuffer::new_initialized(&mut self.scratch);
            self.codec.encode(&mut input, &mut output)?;
            out.extend_from_slice(output.written());
        }
        Ok(())
    }

    pub(crate) fn finish(mut self, out: &mut Vec<u8>) -> io::Result<()> {
        loop {
            let mut output = WriteBuffer::new_initialized(&mut self.scratch);
            let done = self.codec.finish(&mut output)?;
            out.extend_from_slice(output.written());
            if done {
                return Ok(());
            }
        }
    }
}

/// 一次性压缩整段输入；identity 返回 None。
pub(crate) fn encode(
    encoding: Encoding,
    effort: Effort,
    data: &[u8],
) -> Option<io::Result<Vec<u8>>> {
    let mut encoder = Encoder::new(encoding, effort, Some(data.len()))?;
    let mut out = Vec::new();
    Some(
        encoder
            .write(data, &mut out)
            .and_then(|()| encoder.finish(&mut out))
            .map(|()| out),
    )
}

#[cfg(test)]
pub(crate) fn decode(encoding: Encoding, data: &[u8]) -> Vec<u8> {
    use std::io::Read;
    let mut out = Vec::new();
    match encoding {
        Encoding::Identity => out.extend_from_slice(data),
        Encoding::Deflate => {
            flate2::read::ZlibDecoder::new(data)
                .read_to_end(&mut out)
                .unwrap();
        }
        Encoding::Gzip => {
            flate2::read::GzDecoder::new(data)
                .read_to_end(&mut out)
                .unwrap();
        }
        Encoding::Brotli => {
            let mut decoder = compression_codecs::BrotliDecoder::new();
            decode_with(&mut decoder, data, &mut out);
        }
        Encoding::Zstd => {
            let mut decoder = compression_codecs::ZstdDecoder::new();
            decode_with(&mut decoder, data, &mut out);
        }
    }
    out
}

#[cfg(test)]
fn decode_with(decoder: &mut dyn compression_codecs::DecodeV2, data: &[u8], out: &mut Vec<u8>) {
    let mut input = PartialBuffer::new(data);
    let mut scratch = vec![0; 32 * 1024];
    loop {
        let mut output = WriteBuffer::new_initialized(&mut scratch);
        let done = decoder.decode(&mut input, &mut output).unwrap();
        out.extend_from_slice(output.written());
        if done {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_all_enables_everything() {
        assert_eq!(
            parse_compression_algorithms("all"),
            Some(CompressionAlgorithms::all_enabled())
        );
    }

    #[test]
    fn compression_empty_none_off_disable() {
        for value in ["", "none", "NONE", "off", "OFF"] {
            assert_eq!(parse_compression_algorithms(value), None, "{value}");
        }
    }

    #[test]
    fn compression_single_and_multiple_algorithms() {
        assert_eq!(
            parse_compression_algorithms("br"),
            Some(CompressionAlgorithms {
                gzip: false,
                brotli: true,
                deflate: false,
                zstd: false,
            })
        );
        assert_eq!(
            parse_compression_algorithms("gzip, zstd"),
            Some(CompressionAlgorithms {
                gzip: true,
                brotli: false,
                deflate: false,
                zstd: true,
            })
        );
    }

    #[test]
    fn compression_case_insensitive_and_whitespace_tolerant() {
        assert_eq!(
            parse_compression_algorithms(" GZIP , Brotli , deflate , Zstd "),
            Some(CompressionAlgorithms::all_enabled())
        );
    }

    #[test]
    fn compression_unknown_algorithms_are_ignored() {
        assert_eq!(
            parse_compression_algorithms("gzip, unknown, lz4"),
            Some(CompressionAlgorithms {
                gzip: true,
                brotli: false,
                deflate: false,
                zstd: false,
            })
        );
    }

    fn pick(config: &str, accept: &str) -> Option<Encoding> {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_str(accept).unwrap(),
        );
        negotiate(&headers, parse_compression_algorithms(config).unwrap())
    }

    #[test]
    fn negotiation_prefers_recommended_brotli_and_falls_back() {
        let chrome = "gzip, deflate, br, zstd";
        assert_eq!(pick("br, gzip", chrome), Some(Encoding::Brotli));
        assert_eq!(pick("all", chrome), Some(Encoding::Zstd));
        assert_eq!(pick("br, gzip", "gzip"), Some(Encoding::Gzip));
        assert_eq!(pick("br, gzip", "zstd"), Some(Encoding::Identity));
        assert_eq!(pick("br, gzip", ""), Some(Encoding::Identity));
    }

    #[test]
    fn negotiation_honours_qvalues() {
        assert_eq!(
            pick("br, gzip", "br;q=0.5, gzip;q=0.8"),
            Some(Encoding::Gzip)
        );
        assert_eq!(pick("br, gzip", "br;q=0, gzip"), Some(Encoding::Gzip));
        assert_eq!(
            pick("br, gzip", "br;q=1.0, gzip;q=1"),
            Some(Encoding::Brotli)
        );
        // 不合法的 q 值整项忽略。
        assert_eq!(
            pick("br, gzip", "br;q=2, gzip;q=0.1234"),
            Some(Encoding::Identity)
        );
        // 只拒绝压缩时仍可回 identity；连 identity 都拒绝则 406。
        assert_eq!(pick("br, gzip", "br;q=0"), Some(Encoding::Identity));
        assert_eq!(pick("br, gzip", "identity;q=0"), None);
    }

    #[test]
    fn negotiation_applies_wildcard_to_unlisted_encodings() {
        assert_eq!(pick("br, gzip", "*"), Some(Encoding::Brotli));
        assert_eq!(pick("br, gzip", "gzip, *;q=0"), Some(Encoding::Gzip));
        assert_eq!(pick("br, gzip", "*;q=0"), None);
        assert_eq!(
            pick("br, gzip", "identity, *;q=0"),
            Some(Encoding::Identity)
        );
        assert_eq!(pick("br, gzip", "br;q=0.1, *;q=0.5"), Some(Encoding::Gzip));
    }

    #[test]
    fn encoders_round_trip_for_every_encoding() {
        let page =
            "<article><p>压缩回归测试段落 compression regression.</p></article>".repeat(3000);
        for encoding in Encoding::COMPRESSED {
            let encoded = encode(encoding, Effort::Cached, page.as_bytes())
                .unwrap()
                .unwrap();
            assert!(encoded.len() < page.len() / 10, "{encoding:?}");
            assert_eq!(decode(encoding, &encoded), page.as_bytes(), "{encoding:?}");
        }
        assert!(encode(Encoding::Identity, Effort::Cached, b"x").is_none());
    }

    #[test]
    fn brotli_window_covers_whole_known_input() {
        assert_eq!(brotli_window(None), 22);
        assert_eq!(brotli_window(Some(0)), 16);
        assert_eq!(brotli_window(Some((1 << 16) - 16)), 16);
        assert_eq!(brotli_window(Some((1 << 16) - 15)), 17);
        assert_eq!(brotli_window(Some(110_593)), 17);
        assert_eq!(brotli_window(Some(1 << 30)), 22);
    }
}
