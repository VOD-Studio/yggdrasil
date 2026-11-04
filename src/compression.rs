//! HTTP 响应压缩：`COMPRESSION_ALGORITHMS` 配置、`Accept-Encoding` 协商、编码器与动态压缩中间件。
//!
//! 协商与跳过规则沿用 tower-http 0.7 的 `CompressionLayer`。动态响应由 [`compress`] 现压；
//! SSR HTML 缓存命中时按同一协商挑选预压缩变体（`ssr_cache`）。

#![cfg(feature = "server")]

use std::io;
use std::sync::LazyLock;

use axum::body::{Body, BodyDataStream, HttpBody};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use bytes::Bytes;
use compression_codecs::core::util::{PartialBuffer, WriteBuffer};
use compression_codecs::core::Level;
use compression_codecs::{
    brotli::params::EncoderParams as BrotliParams, flate::params::FlateEncoderParams,
    zstd::params::CParameter, BrotliEncoder, EncodeV2, GzipEncoder, ZlibEncoder, ZstdEncoder,
};
use futures::StreamExt;

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
    /// 每个请求现压：统一质量 7。brotli 4 只比 gzip-6 小 2–4%；取 7 时 brotli 比 6 再小
    /// 1.9–2.9%，每次只多几毫秒；q9 起 feed 压缩耗时成倍增加。
    Dynamic,
    /// 一次压缩、多次复用（SSR HTML 缓存）：取各算法高档，brotli 11 比 7 再小约 8%。
    Cached,
}

impl Effort {
    fn level(self, encoding: Encoding) -> Level {
        Level::Precise(match (self, encoding) {
            (Effort::Dynamic, _) => 7,
            (Effort::Cached, Encoding::Brotli) => 11,
            (Effort::Cached, Encoding::Zstd) => 19,
            (Effort::Cached, _) => 9,
        })
    }
}

/// brotli 默认窗口 4 MiB（lgwin 22）。流式编码不知道总长时环形缓冲区按窗口整块分配并清零，
/// 超过约 64 KiB 的页面每次都要清 4 MiB；长度已知时取刚好容纳整段输入的窗口，压缩结果不变。
/// 下限 18：q7 在 lgwin 16 时换用另一种哈希，33 KB 的首页反而慢 4 倍（0.54 → 2.2 ms）。
fn brotli_window(len: Option<usize>) -> i32 {
    const MIN: i32 = 18;
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

    /// 把已写入的输入全部压出，输出可被客户端立即解码（流式响应的前段）。
    pub(crate) fn flush(&mut self, out: &mut Vec<u8>) -> io::Result<()> {
        loop {
            let mut output = WriteBuffer::new_initialized(&mut self.scratch);
            let done = self.codec.flush(&mut output)?;
            out.extend_from_slice(output.written());
            if done {
                return Ok(());
            }
        }
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

/// 动态压缩的缓冲上限：长度未知的正文在此之内结束时按总长收窄 brotli 窗口，超过后转流式。
const BUFFER_LIMIT: usize = 1024 * 1024;

/// 动态响应压缩中间件（`axum::middleware::from_fn`）。跳过规则同 tower-http 的 `DefaultPredicate`：
/// - 已带 `Content-Encoding`（public 下预压缩的 `.br`、SSR 缓存的预压缩变体）或 `Content-Range` 的原样透传；
/// - 跳过 `image/*`（`image/svg+xml` 除外）、gRPC、`text/event-stream` 与小于 32 字节的响应；
/// - 客户端连 identity 都拒绝时回 406。
///
/// 与 tower-http 不同的一步：长度未知的正文先缓冲，若上游一口气给完且不超过 [`BUFFER_LIMIT`]，
/// 就按总长选 brotli 窗口（[`brotli_window`]），省掉默认 4 MiB 环形缓冲区的分配与清零。
/// 上游暂时没有数据（流式 SSR 等后续分块）时与 tower-http 一样先 flush 已收到的部分，
/// 此后按 4 MiB 窗口流式压缩，前段照旧尽早到达浏览器。
pub(crate) async fn compress(
    algorithms: CompressionAlgorithms,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let encoding = negotiate(req.headers(), algorithms);
    let mut response = next.run(req).await;
    let Some(encoding) = encoding else {
        *response.status_mut() = StatusCode::NOT_ACCEPTABLE;
        add_vary(response.headers_mut());
        return response;
    };
    if !should_compress(&response) {
        return response;
    }
    add_vary(response.headers_mut());
    if encoding == Encoding::Identity {
        return response;
    }
    let known_len = response
        .body()
        .size_hint()
        .exact()
        .and_then(|len| usize::try_from(len).ok());
    let (mut parts, body) = response.into_parts();
    parts.headers.remove(header::ACCEPT_RANGES);
    parts.headers.remove(header::CONTENT_LENGTH);
    parts
        .headers
        .insert(header::CONTENT_ENCODING, encoding.header_value());
    let stream = CompressStream {
        input: body.into_data_stream(),
        encoding,
        stage: match known_len {
            Some(len) => Stage::Streaming {
                encoder: dynamic_encoder(encoding, Some(len)),
                pending: false,
            },
            None => Stage::Buffering(Vec::new()),
        },
    };
    let stream = futures::stream::unfold(stream, |mut stream| async move {
        stream.next_chunk().await.map(|chunk| (chunk, stream))
    })
    // 下游在流结束后可能再次 poll；unfold 对此会 panic。
    .fuse();
    Response::from_parts(parts, Body::from_stream(stream))
}

fn add_vary(headers: &mut HeaderMap) {
    let listed = headers
        .get_all(header::VARY)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|name| name.trim().eq_ignore_ascii_case("accept-encoding"));
    if !listed {
        headers.append(header::VARY, HeaderValue::from_static("accept-encoding"));
    }
}

fn should_compress(response: &Response) -> bool {
    let headers = response.headers();
    if headers.contains_key(header::CONTENT_ENCODING) || headers.contains_key(header::CONTENT_RANGE)
    {
        return false;
    }
    let size = response.body().size_hint().exact().or_else(|| {
        headers
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
    });
    if size.is_some_and(|size| size < 32) {
        return false;
    }
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let excluded = |prefix: &str, exception: Option<&str>| {
        content_type.starts_with(prefix)
            && !exception.is_some_and(|exception| content_type.starts_with(exception))
    };
    !(excluded("application/grpc", Some("application/grpc-web"))
        || excluded("image/", Some("image/svg+xml"))
        || excluded("text/event-stream", None))
}

fn dynamic_encoder(encoding: Encoding, len: Option<usize>) -> Encoder {
    Encoder::new(encoding, Effort::Dynamic, len).expect("identity 不进入压缩流")
}

enum Stage {
    /// 长度未知：攒到 [`BUFFER_LIMIT`]、上游暂停或正文结束。
    Buffering(Vec<u8>),
    /// `pending`：上次 flush 之后又写入过数据。
    Streaming {
        encoder: Encoder,
        pending: bool,
    },
    Done,
}

struct CompressStream {
    input: BodyDataStream,
    encoding: Encoding,
    stage: Stage,
}

impl CompressStream {
    /// 下一段非空压缩输出；None 表示结束。上游或编码出错后结束。
    async fn next_chunk(&mut self) -> Option<Result<Bytes, axum::Error>> {
        use futures::FutureExt;

        while !matches!(self.stage, Stage::Done) {
            let mut out = Vec::new();
            let next = match self.input.next().now_or_never() {
                Some(next) => next,
                None => {
                    // 上游暂停：先送出已收到的部分，再等后续分块。
                    if let Err(error) = self.flush(&mut out) {
                        self.stage = Stage::Done;
                        return Some(Err(axum::Error::new(error)));
                    }
                    if !out.is_empty() {
                        return Some(Ok(Bytes::from(out)));
                    }
                    self.input.next().await
                }
            };
            let result = match next {
                Some(Ok(chunk)) => self.push(&chunk, &mut out),
                Some(Err(error)) => {
                    self.stage = Stage::Done;
                    return Some(Err(error));
                }
                None => self.finish(&mut out),
            };
            if let Err(error) = result {
                self.stage = Stage::Done;
                return Some(Err(axum::Error::new(error)));
            }
            if !out.is_empty() {
                return Some(Ok(Bytes::from(out)));
            }
        }
        None
    }

    /// 转入流式压缩：总长未知，按默认窗口。
    fn start_streaming(&mut self, buffered: &[u8], out: &mut Vec<u8>) -> io::Result<()> {
        let mut encoder = dynamic_encoder(self.encoding, None);
        let result = encoder.write(buffered, out);
        self.stage = Stage::Streaming {
            encoder,
            pending: !buffered.is_empty(),
        };
        result
    }

    fn push(&mut self, chunk: &[u8], out: &mut Vec<u8>) -> io::Result<()> {
        match &mut self.stage {
            Stage::Buffering(buffered) => {
                buffered.extend_from_slice(chunk);
                if buffered.len() <= BUFFER_LIMIT {
                    return Ok(());
                }
                let buffered = std::mem::take(buffered);
                self.start_streaming(&buffered, out)
            }
            Stage::Streaming { encoder, pending } => {
                *pending |= !chunk.is_empty();
                encoder.write(chunk, out)
            }
            Stage::Done => Ok(()),
        }
    }

    fn flush(&mut self, out: &mut Vec<u8>) -> io::Result<()> {
        match &mut self.stage {
            Stage::Buffering(buffered) if !buffered.is_empty() => {
                let buffered = std::mem::take(buffered);
                self.start_streaming(&buffered, out)?;
                self.flush(out)
            }
            Stage::Streaming { encoder, pending } if *pending => {
                *pending = false;
                encoder.flush(out)
            }
            _ => Ok(()),
        }
    }

    fn finish(&mut self, out: &mut Vec<u8>) -> io::Result<()> {
        match std::mem::replace(&mut self.stage, Stage::Done) {
            Stage::Buffering(buffered) => {
                let mut encoder = dynamic_encoder(self.encoding, Some(buffered.len()));
                encoder.write(&buffered, out)?;
                encoder.finish(out)
            }
            Stage::Streaming { encoder, .. } => encoder.finish(out),
            Stage::Done => Ok(()),
        }
    }
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
        assert_eq!(brotli_window(Some(0)), 18);
        assert_eq!(brotli_window(Some((1 << 18) - 16)), 18);
        assert_eq!(brotli_window(Some((1 << 18) - 15)), 19);
        assert_eq!(brotli_window(Some((1 << 21) - 15)), 22);
        assert_eq!(brotli_window(Some(1 << 30)), 22);
    }

    /// 测试响应工厂：每次调用产出一份新响应（`Response` 不能克隆）。
    type Respond = fn() -> Response;

    /// 经 [`compress`] 请求一个由 `respond` 产出的响应。
    async fn through_middleware(config: &str, accept: &str, respond: Respond) -> Response {
        use tower::ServiceExt;
        let algorithms = parse_compression_algorithms(config).unwrap();
        axum::Router::new()
            .route("/", axum::routing::get(move || async move { respond() }))
            .layer(axum::middleware::from_fn(
                move |req: axum::extract::Request, next: axum::middleware::Next| {
                    compress(algorithms, req, next)
                },
            ))
            .oneshot(
                axum::http::Request::get("/")
                    .header(header::ACCEPT_ENCODING, accept)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    fn page(paragraphs: usize) -> String {
        (0..paragraphs)
            .map(|i| {
                format!(
                    "<p>第 {i} 段 dynamic compression regression {}</p>",
                    i * 7919
                )
            })
            .collect()
    }

    fn html(body: Body) -> Response {
        Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(body)
            .unwrap()
    }

    /// 小页面：长度已知 / 未知（分块）；大页面：超过缓冲上限，转流式。
    const BODIES: [(&str, Respond); 3] = [
        ("known", || html(Body::from(page(200)))),
        ("chunked", || {
            let chunks = page(200)
                .into_bytes()
                .chunks(700)
                .map(<[u8]>::to_vec)
                .collect::<Vec<_>>();
            html(Body::from_stream(futures::stream::iter(
                chunks.into_iter().map(Ok::<_, io::Error>),
            )))
        }),
        ("over-limit", || {
            let chunks = page(30_000)
                .into_bytes()
                .chunks(64 * 1024)
                .map(<[u8]>::to_vec)
                .collect::<Vec<_>>();
            html(Body::from_stream(futures::stream::iter(
                chunks.into_iter().map(Ok::<_, io::Error>),
            )))
        }),
    ];

    #[tokio::test]
    async fn dynamic_compression_round_trips_buffered_and_streamed_bodies() {
        assert!(page(30_000).len() > BUFFER_LIMIT);
        for (config, accept, expected) in [
            ("br, gzip", "gzip, deflate, br, zstd", Encoding::Brotli),
            ("br, gzip", "gzip", Encoding::Gzip),
            ("all", "gzip, deflate, br, zstd", Encoding::Zstd),
        ] {
            for (name, respond) in BODIES {
                let expected_body = axum::body::to_bytes(respond().into_body(), usize::MAX)
                    .await
                    .unwrap();
                let response = through_middleware(config, accept, respond).await;
                let headers = response.headers();
                assert_eq!(
                    headers[header::CONTENT_ENCODING],
                    expected.header_value(),
                    "{name} {accept}"
                );
                assert_eq!(headers[header::VARY], "accept-encoding", "{name}");
                assert!(!headers.contains_key(header::CONTENT_LENGTH), "{name}");
                let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                assert_eq!(decode(expected, &body), expected_body, "{name} {accept}");
            }
        }
    }

    #[tokio::test]
    async fn dynamic_compression_skips_ineligible_responses() {
        fn typed(content_type: &'static str) -> Response {
            Response::builder()
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(page(50)))
                .unwrap()
        }
        let cases: [(&str, Respond, bool); 6] = [
            ("png", || typed("image/png"), false),
            ("svg", || typed("image/svg+xml"), true),
            ("sse", || typed("text/event-stream"), false),
            ("tiny", || html(Body::from("<p>短</p>")), false),
            (
                "precompressed",
                || {
                    let mut response = typed("text/css");
                    response
                        .headers_mut()
                        .insert(header::CONTENT_ENCODING, HeaderValue::from_static("br"));
                    response
                },
                false,
            ),
            (
                "range",
                || {
                    let mut response = typed("text/plain");
                    response.headers_mut().insert(
                        header::CONTENT_RANGE,
                        HeaderValue::from_static("bytes 0-9/100"),
                    );
                    response
                },
                false,
            ),
        ];
        for (name, respond, compressed) in cases {
            let original = respond();
            let original_encoding = original.headers().get(header::CONTENT_ENCODING).cloned();
            let original_body = axum::body::to_bytes(original.into_body(), usize::MAX)
                .await
                .unwrap();
            let response = through_middleware("br, gzip", "br", respond).await;
            let encoding = response.headers().get(header::CONTENT_ENCODING).cloned();
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            if compressed {
                assert_eq!(encoding, Some(Encoding::Brotli.header_value()), "{name}");
                assert_eq!(decode(Encoding::Brotli, &body), original_body, "{name}");
            } else {
                assert_eq!(encoding, original_encoding, "{name}");
                assert_eq!(body, original_body, "{name}");
            }
        }
    }

    #[tokio::test]
    async fn dynamic_compression_answers_406_when_identity_is_refused() {
        let response = through_middleware("br, gzip", "zstd, identity;q=0", || {
            html(Body::from(page(50)))
        })
        .await;
        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
        assert_eq!(response.headers()[header::VARY], "accept-encoding");
        assert!(!response.headers().contains_key(header::CONTENT_ENCODING));
    }

    /// 流式响应（如流式 SSR）上游暂停时先送出已收到部分，不等正文结束。
    #[tokio::test]
    async fn dynamic_compression_flushes_when_upstream_pauses() {
        use std::sync::{Arc, Mutex};
        use tower::ServiceExt;

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, io::Error>>(4);
        let rx = Arc::new(Mutex::new(Some(rx)));
        let algorithms = parse_compression_algorithms("br, gzip").unwrap();
        let app = axum::Router::new()
            .route(
                "/",
                axum::routing::get(move || {
                    let rx = rx.lock().unwrap().take().unwrap();
                    async move {
                        html(Body::from_stream(
                            tokio_stream::wrappers::ReceiverStream::new(rx),
                        ))
                    }
                }),
            )
            .layer(axum::middleware::from_fn(
                move |req: axum::extract::Request, next: axum::middleware::Next| {
                    compress(algorithms, req, next)
                },
            ));
        let (head, tail) = (page(100), page(300));
        tx.send(Ok(Bytes::from(head.clone()))).await.unwrap();
        let response = app
            .oneshot(
                axum::http::Request::get("/")
                    .header(header::ACCEPT_ENCODING, "br")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let mut body = response.into_body().into_data_stream();
        let first = tokio::time::timeout(std::time::Duration::from_secs(5), body.next())
            .await
            .expect("上游暂停时应先送出前段")
            .unwrap()
            .unwrap();
        assert!(!first.is_empty());

        tx.send(Ok(Bytes::from(tail.clone()))).await.unwrap();
        drop(tx);
        let mut encoded = first.to_vec();
        while let Some(chunk) = body.next().await {
            encoded.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(
            decode(Encoding::Brotli, &encoded),
            format!("{head}{tail}").into_bytes()
        );
    }
}
