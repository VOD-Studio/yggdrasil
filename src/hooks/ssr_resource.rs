//! SSR 数据资源。
//!
//! # 路由数据：[`route_data`]
//! 懒加载路由的 `LazyRoute::data()` 编译进主包（SSR 会多次执行 `view()`，资源只能在
//! `data()` 里建）。若在那里直接建 `Resource<Result<T, ServerFnError>>`，每个路由的数据
//! 类型、反序列化代码与 server fn 客户端都会进主包，所有页面首屏一起付费。
//!
//! [`route_data`] 把资源的值类型擦除成 JSON 文本（[`RawJson`]，所有路由共用同一份
//! `Resource` 实例化），取数函数由调用方写成 `#[leptos::lazy]` 函数、返回 [`PageJson<T>`]：
//! 服务端直接调用并编码，客户端首次取数时才加载它所在的分包。`view()` 里
//! `data.await` 得到 `Result<T, ServerFnError>`，解码代码随视图分包加载。
//!
//! ```ignore
//! #[leptos::lazy]
//! async fn fetch_post(slug: String) -> PageJson<PublicPostResponse> {
//!     PageJson::new(get_public_post_by_slug(slug).await)
//! }
//! // data():  post: route_data(move || slug.get(), fetch_post),
//! // view():  match post.await { Ok(..) => .., Err(..) => .. }
//! ```
//!
//! # 主包内的数据：[`ssr_resource`]
//! 页脚等主包组件自己的数据仍是普通资源；[`JsonSliceCodec`] 让水合解码与 server fn 共用
//! `serde_json::from_slice` 的实例化（默认 `JsonSerdeCodec` 用 `from_str`，同一类型会多出
//! 一份 `StrRead` 反序列化代码）。

use std::future::{Future, IntoFuture};
use std::marker::PhantomData;
use std::pin::Pin;

use leptos::prelude::*;
use leptos::server::codee::{Decoder, Encoder};
use serde::{de::DeserializeOwned, Serialize};

/// `Resource` 的 JSON 编解码器：编码 `serde_json::to_string`，解码 `serde_json::from_slice`。
pub struct JsonSliceCodec;

impl<T: Serialize> Encoder<T> for JsonSliceCodec {
    type Error = serde_json::Error;
    type Encoded = String;

    fn encode(val: &T) -> Result<Self::Encoded, Self::Error> {
        serde_json::to_string(val)
    }
}

impl<T: DeserializeOwned> Decoder<T> for JsonSliceCodec {
    type Error = serde_json::Error;
    type Encoded = str;

    fn decode(val: &Self::Encoded) -> Result<T, Self::Error> {
        serde_json::from_slice(val.as_bytes())
    }
}

/// 使用 [`JsonSliceCodec`] 的 SSR 资源。
pub type SsrResource<T> = Resource<T, JsonSliceCodec>;

/// 创建主包组件自己的 SSR 资源（语义同 `Resource::new`）。路由数据用 [`route_data`]。
#[track_caller]
pub fn ssr_resource<S, T, Fut>(
    source: impl Fn() -> S + Send + Sync + 'static,
    fetcher: impl Fn(S) -> Fut + Send + Sync + 'static,
) -> SsrResource<T>
where
    S: PartialEq + Clone + Send + Sync + 'static,
    T: Serialize + DeserializeOwned + Send + Sync + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    Resource::new_with_options(source, fetcher, false)
}

/// `Result<T, ServerFnError>` 的 JSON 文本：路由数据资源的统一值类型。
#[derive(Clone, Debug, PartialEq)]
pub struct RawJson(String);

/// [`RawJson`] 原样作为资源的序列化形式，不再二次编码。
pub struct RawJsonCodec;

impl Encoder<RawJson> for RawJsonCodec {
    type Error = std::convert::Infallible;
    type Encoded = String;

    fn encode(val: &RawJson) -> Result<Self::Encoded, Self::Error> {
        Ok(val.0.clone())
    }
}

impl Decoder<RawJson> for RawJsonCodec {
    type Error = std::convert::Infallible;
    type Encoded = str;

    fn decode(val: &Self::Encoded) -> Result<RawJson, Self::Error> {
        Ok(RawJson(val.to_owned()))
    }
}

/// 路由取数函数（`#[leptos::lazy]`）的返回值：已编码的 `Result<T, ServerFnError>`。
pub struct PageJson<T> {
    raw: RawJson,
    _type: PhantomData<fn() -> T>,
}

impl<T: Serialize> PageJson<T> {
    /// 编码取数结果。
    pub fn new(result: Result<T, ServerFnError>) -> Self {
        let text = serde_json::to_string(&result).unwrap_or_else(|err| {
            let failed: Result<(), ServerFnError> =
                Err(ServerFnError::Serialization(err.to_string()));
            serde_json::to_string(&failed).expect("ServerFnError 可序列化")
        });
        Self {
            raw: RawJson(text),
            _type: PhantomData,
        }
    }
}

/// 路由数据句柄：在 `data()` 里由 [`route_data`] 创建，`view()` 里 `.await` 得到结果。
pub struct RouteData<T> {
    raw: Resource<RawJson, RawJsonCodec>,
    _type: PhantomData<fn() -> T>,
}

impl<T> Clone for RouteData<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for RouteData<T> {}

impl<T: DeserializeOwned + Send + 'static> IntoFuture for RouteData<T> {
    type Output = Result<T, ServerFnError>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        let raw = self.raw;
        Box::pin(async move { decode_page_json(&raw.await) })
    }
}

fn decode_page_json<T: DeserializeOwned>(raw: &RawJson) -> Result<T, ServerFnError> {
    serde_json::from_slice::<Result<T, ServerFnError>>(raw.0.as_bytes())
        .unwrap_or_else(|err| Err(ServerFnError::Deserialization(err.to_string())))
}

/// 创建路由数据（语义同 `Resource::new`：服务端执行 `fetcher` 并把结果写进 HTML，
/// 客户端水合直接复用；`source` 变化即重新请求）。`fetcher` 应是 `#[leptos::lazy]`
/// 函数，主包里只留下这一份与数据类型无关的资源代码。
#[track_caller]
pub fn route_data<S, T, Fut>(
    source: impl Fn() -> S + Send + Sync + 'static,
    fetcher: impl Fn(S) -> Fut + Send + Sync + 'static,
) -> RouteData<T>
where
    S: PartialEq + Clone + Send + Sync + 'static,
    Fut: Future<Output = PageJson<T>> + Send + 'static,
{
    let raw = Resource::new_with_options(
        source,
        move |source| {
            let fut = fetcher(source);
            async move { fut.await.raw }
        },
        false,
    );
    RouteData {
        raw,
        _type: PhantomData,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::error::is_not_found;

    #[derive(Debug, PartialEq, Serialize, serde::Deserialize)]
    struct Sample {
        id: i32,
        title: String,
    }

    #[test]
    fn json_slice_codec_round_trips() {
        let value = Sample {
            id: 7,
            title: "中文标题 \"quoted\"".into(),
        };
        let encoded = <JsonSliceCodec as Encoder<Sample>>::encode(&value).unwrap();
        let decoded: Sample = <JsonSliceCodec as Decoder<Sample>>::decode(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    /// 路由数据经 SSR 序列化（RawJsonCodec）往返后解码出原始结果，错误保留 404 语义。
    #[test]
    fn page_json_survives_resource_round_trip() {
        let through_resource = |page: PageJson<Sample>| {
            let wire = <RawJsonCodec as Encoder<RawJson>>::encode(&page.raw).unwrap();
            <RawJsonCodec as Decoder<RawJson>>::decode(&wire).unwrap()
        };

        let ok = through_resource(PageJson::new(Ok(Sample {
            id: 1,
            title: "标题".into(),
        })));
        assert_eq!(
            decode_page_json::<Sample>(&ok).unwrap(),
            Sample {
                id: 1,
                title: "标题".into()
            }
        );

        let missing = through_resource(PageJson::new(Err(ServerFnError::ServerError(format!(
            "{}文章不存在",
            crate::api::error::NOT_FOUND_PREFIX
        )))));
        assert!(is_not_found(
            &decode_page_json::<Sample>(&missing).unwrap_err()
        ));
    }

    /// 编码的类型与解码期望不符时给出反序列化错误，而不是 panic。
    #[test]
    fn mismatched_page_json_is_a_deserialization_error() {
        let page = PageJson::new(Ok("not a sample".to_string()));
        assert!(matches!(
            decode_page_json::<Sample>(&page.raw),
            Err(ServerFnError::Deserialization(_))
        ));
    }
}
