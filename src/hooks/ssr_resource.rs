//! SSR 数据资源。
//!
//! # 路由数据：[`route_data`][crate::hooks::ssr_resource::route_data]
//! 懒加载路由的 `LazyRoute::data()` 编译进主包（SSR 会多次执行 `view()`，资源只能在
//! `data()` 里建）。若在那里直接建 `Resource<Result<T, ApiError>>`，每个路由的数据
//! 类型、反序列化代码与 server fn 客户端都会进主包，所有页面首屏一起付费。
//!
//! [`route_data`][crate::hooks::ssr_resource::route_data] 把资源的值类型擦除成 JSON 文本（[`RawJson`][crate::hooks::ssr_resource::RawJson]，所有路由共用同一份
//! `Resource` 实例化），取数函数由调用方写成 `#[leptos::lazy]` 函数、返回 [`PageJson<T>`][crate::hooks::ssr_resource::PageJson]：
//! 服务端直接调用并编码，客户端首次取数时才加载它所在的分包。`view()` 里
//! `data.await` 得到 `Result<T, ApiError>`，解码代码随视图分包加载。
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
//! # 悬停预取：[`prefetch_route_data`][crate::hooks::ssr_resource::prefetch_route_data]
//! 悬停、聚焦、触屏按下站内链接时，`route_prefetch` 调用各路由的 `prefetch_data` 提前请求
//! 页面数据，结果存进浏览器端的小缓存；之后导航到该页，[`route_data`][crate::hooks::ssr_resource::route_data] 的取数闭包先查缓存，
//! 命中就直接复用进行中或已完成的请求。
//!
//! - **键** = 取数函数的类型（`TypeId::of::<F>()`）+ `source` 的值。具名函数项与 [`unit()`][crate::hooks::ssr_resource::unit]
//!   返回的闭包类型都是唯一的，`data()` 与预取两边传同一个函数就是同一个键；闭包字面量
//!   每处类型都不同，**永远不会命中**，只是不预取，不会取错数据。
//! - **一次性**：命中即取走。完成后保留 10 秒，过期丢弃；进行中的请求不按 TTL 过期。
//! - **出错不缓存**：错误结果只交给已经在等的消费者，之后的导航重新请求。
//! - 只用于不依赖会话的公开数据；服务端不保留任何跨请求状态（非 wasm 为空操作）。
//!
//! # 主包内的数据
//! 主包组件自己的数据（页脚）同样走 [`route_data`][crate::hooks::ssr_resource::route_data]。
//! [`JsonSliceCodec`][crate::hooks::ssr_resource::JsonSliceCodec] 给 `share_meta` 的
//! `SharedValue` 用：水合解码与 server fn 共用 `serde_json::from_slice` 的实例化（默认
//! `JsonSerdeCodec` 用 `from_str`，同一类型会多出一份 `StrRead` 反序列化代码）。

use std::future::{Future, IntoFuture};
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;

use std::any::TypeId;

use leptos::prelude::*;
use leptos::server::codee::{Decoder, Encoder};
use serde::{de::DeserializeOwned, Serialize};

use crate::api::error::ApiError;

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

/// `Result<T, ApiError>` 的 JSON 文本：路由数据资源的统一值类型。
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

/// 路由取数函数（`#[leptos::lazy]`）的返回值：已编码的 `Result<T, ApiError>`。
pub struct PageJson<T> {
    raw: RawJson,
    _type: PhantomData<fn() -> T>,
}

impl<T: Serialize> PageJson<T> {
    /// 编码取数结果。
    pub fn new(result: Result<T, ApiError>) -> Self {
        let text = serde_json::to_string(&result).unwrap_or_else(|err| {
            let failed: Result<(), ApiError> = Err(ApiError::Framework(
                ServerFnError::Serialization(err.to_string()),
            ));
            serde_json::to_string(&failed).expect("ApiError 可序列化")
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
    type Output = Result<T, ApiError>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        let raw = self.raw;
        Box::pin(async move { decode_page_json(&raw.await) })
    }
}

fn decode_page_json<T: DeserializeOwned>(raw: &RawJson) -> Result<T, ApiError> {
    serde_json::from_slice::<Result<T, ApiError>>(raw.0.as_bytes()).unwrap_or_else(|err| {
        Err(ApiError::Framework(ServerFnError::Deserialization(
            err.to_string(),
        )))
    })
}

/// 把无参取数函数包成 `Fn(())`，与 `route_data(|| (), unit(f))` 搭配。
/// 返回的闭包类型只取决于 `f` 的函数项类型，`data()` 与悬停预取得到同一个预取键。
pub fn unit<F, Fut>(fetcher: F) -> impl Fn(()) -> Fut + Send + Sync + Copy + 'static
where
    F: Fn() -> Fut + Send + Sync + Copy + 'static,
{
    move |()| fetcher()
}

/// 创建路由数据（语义同 `Resource::new`：服务端执行 `fetcher` 并把结果写进 HTML，
/// 客户端水合直接复用；`source` 变化即重新请求）。`fetcher` 应是 `#[leptos::lazy]`
/// 函数，主包里只留下这一份与数据类型无关的资源代码。
///
/// 悬停预取按 `fetcher` 的类型加 `source` 的值查缓存：公开页面传具名函数项，
/// 无参函数用 [`unit()`] 包装；闭包字面量不会命中预取（见模块文档）。
///
/// SSR 时同一请求内 (fetcher 类型, source) 相同的调用复用同一个资源（见 `RouteDataRegistry`）。
#[track_caller]
pub fn route_data<S, T, F, Fut>(
    source: impl Fn() -> S + Send + Sync + 'static,
    fetcher: F,
) -> RouteData<T>
where
    S: PartialEq + Clone + Send + Sync + 'static,
    F: Fn(S) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = PageJson<T>> + Send + 'static,
{
    let source: ErasedSource = Arc::new(move || ErasedKey(Arc::new(source())));
    let fetch: ErasedFetch = Arc::new(move |key: ErasedKey| {
        let source: S = key.downcast::<S>();
        #[cfg(target_arch = "wasm32")]
        {
            let pending = match PREFETCH.take(TypeId::of::<F>(), &source, now_ms()) {
                Some(hit) => Ok(hit),
                None => Err(fetcher(source)),
            };
            Box::pin(async move {
                match pending {
                    Ok(hit) => hit.await,
                    Err(fut) => fut.await.raw,
                }
            }) as RawFuture
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let fut = fetcher(source);
            Box::pin(async move { fut.await.raw }) as RawFuture
        }
    });
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(registry) = use_context::<RouteDataRegistry>() {
        let key = untrack(|| source());
        let mut entries = registry.0.lock().unwrap_or_else(|e| e.into_inner());
        let slot = entries
            .iter()
            .position(|(tid, k, _)| *tid == TypeId::of::<F>() && *k == key);
        let raw = match slot {
            Some(i) if !entries[i].2.is_disposed() => entries[i].2,
            // 资源建在已丢弃的 owner 下：换一份新的。
            Some(i) => {
                let raw = erased_route_resource(source, fetch);
                entries[i].2 = raw;
                raw
            }
            None => {
                let raw = erased_route_resource(source, fetch);
                entries.push((TypeId::of::<F>(), key, raw));
                raw
            }
        };
        return RouteData {
            raw,
            _type: PhantomData,
        };
    }
    RouteData {
        raw: erased_route_resource(source, fetch),
        _type: PhantomData,
    }
}

/// SSR：本次请求已创建的路由数据资源，键为（取数函数类型, source 值）。
///
/// Suspense 在 SSR 时会遍历子树三次（两次 dry_resolve、一次 resolve），`<Outlet/>` 每次都会重新选择视图；
/// `Lazy::choose` 只能取走一次预载数据，之后每次都重新调用 `LazyRoute::data()`。
/// 不复用的话，同一份数据会被取三次、写进 HTML 三次，而客户端水合只会创建并读取第一份。
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Default)]
struct RouteDataRegistry(Arc<std::sync::Mutex<Vec<RegistryEntry>>>);

/// 登记表条目：（取数函数类型, source 值, 资源）。
#[cfg(not(target_arch = "wasm32"))]
type RegistryEntry = (TypeId, ErasedKey, Resource<RawJson, RawJsonCodec>);

/// 在应用根部调用：SSR 时为本次请求建立路由数据登记表（见 [`route_data`]）。浏览器端不做任何事。
pub fn provide_route_data_registry() {
    #[cfg(not(target_arch = "wasm32"))]
    provide_context(RouteDataRegistry::default());
}

/// 路由数据键：擦除 `source` 的具体类型，所有路由共用一份 `Resource` 实例化。
trait DynKey: Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
    fn eq_dyn(&self, other: &dyn DynKey) -> bool;
}

impl<S: PartialEq + Send + Sync + 'static> DynKey for S {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn eq_dyn(&self, other: &dyn DynKey) -> bool {
        other.as_any().downcast_ref::<S>() == Some(self)
    }
}

#[derive(Clone)]
struct ErasedKey(Arc<dyn DynKey>);

impl ErasedKey {
    /// 还原 `source` 的值：键只由同一次 [`route_data`] 的 `source` 产生、交给同一个取数闭包，
    /// 类型必然一致。
    fn downcast<S: Clone + 'static>(&self) -> S {
        self.0
            .as_any()
            .downcast_ref::<S>()
            .expect("路由数据键类型一致")
            .clone()
    }
}

impl PartialEq for ErasedKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_dyn(&*other.0)
    }
}

type RawFuture = Pin<Box<dyn Future<Output = RawJson> + Send>>;
type ErasedSource = Arc<dyn Fn() -> ErasedKey + Send + Sync>;
type ErasedFetch = Arc<dyn Fn(ErasedKey) -> RawFuture + Send + Sync>;

/// 唯一一处 `Resource<RawJson, _>` 实例化：`source`、`fetch` 都是 trait 对象。
#[track_caller]
#[inline(never)]
fn erased_route_resource(
    source: ErasedSource,
    fetch: ErasedFetch,
) -> Resource<RawJson, RawJsonCodec> {
    Resource::new_with_options(move || source(), move |key| fetch(key), false)
}

/// 悬停预取一份路由数据：键与 [`route_data`] 相同（`fetcher` 的类型 + `source`）。
/// 之后导航到该页时 `route_data` 直接复用结果。只用于不依赖会话的公开数据。
pub fn prefetch_route_data<S, T, F, Fut>(source: S, fetcher: F)
where
    S: PartialEq + Clone + Send + 'static,
    F: Fn(S) -> Fut + 'static,
    Fut: Future<Output = PageJson<T>> + Send + 'static,
{
    #[cfg(target_arch = "wasm32")]
    {
        let Some(id) = PREFETCH.begin(TypeId::of::<F>(), source.clone(), now_ms()) else {
            return;
        };
        let fut = fetcher(source);
        leptos::task::spawn_local(async move {
            let raw = fut.await.raw;
            PREFETCH.complete(id, raw, now_ms());
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (source, fetcher);
}

#[cfg(target_arch = "wasm32")]
static PREFETCH: prefetch::Store = prefetch::Store::new();

#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    js_sys::Date::now()
}

/// 预取缓存的存储结构；只用 std，时间由调用方传入，便于单元测试。
#[cfg(any(target_arch = "wasm32", test))]
mod prefetch {
    use super::RawJson;
    use std::any::{Any, TypeId};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll, Waker};

    /// 完成后保留多久（毫秒）。
    pub(super) const TTL_MS: f64 = 10_000.0;
    pub(super) const CAPACITY: usize = 24;

    enum Slot {
        Pending(Vec<Waker>),
        Ready(RawJson),
    }

    struct Entry {
        id: u64,
        fetcher: TypeId,
        source: Box<dyn Any + Send>,
        /// `None` = 进行中。
        done_at: Option<f64>,
        /// 已被消费者取走：不再参与去重，完成后从列表删除。
        taken: bool,
        slot: Arc<Mutex<Slot>>,
    }

    impl Entry {
        fn matches<S: PartialEq + 'static>(&self, fetcher: TypeId, source: &S) -> bool {
            !self.taken
                && self.fetcher == fetcher
                && self.source.downcast_ref::<S>().is_some_and(|s| s == source)
        }

        fn expired(&self, now: f64) -> bool {
            self.done_at.is_some_and(|t| now - t > TTL_MS)
        }
    }

    struct Inner {
        next_id: u64,
        entries: Vec<Entry>,
    }

    pub(super) struct Store {
        inner: Mutex<Inner>,
    }

    impl Store {
        pub(super) const fn new() -> Self {
            Self {
                inner: Mutex::new(Inner {
                    next_id: 0,
                    entries: Vec::new(),
                }),
            }
        }

        fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
            self.inner.lock().unwrap_or_else(|e| e.into_inner())
        }

        /// 已有同键的进行中条目或未过期的已完成条目时返回 `None`（不重复请求）；
        /// 否则清理过期条目、超出容量时淘汰最早的已完成条目，登记进行中条目并返回 id。
        /// 容量全被进行中的请求占满时同样返回 `None`：淘汰进行中条目会让已在等待的
        /// 消费者永远等不到结果。
        pub(super) fn begin<S: PartialEq + Send + 'static>(
            &self,
            fetcher: TypeId,
            source: S,
            now: f64,
        ) -> Option<u64> {
            let mut inner = self.lock();
            inner.entries.retain(|e| !e.expired(now));
            if inner.entries.iter().any(|e| e.matches(fetcher, &source)) {
                return None;
            }
            if inner.entries.len() >= CAPACITY {
                let oldest_done = inner.entries.iter().position(|e| e.done_at.is_some())?;
                inner.entries.remove(oldest_done);
            }
            let id = inner.next_id;
            inner.next_id += 1;
            inner.entries.push(Entry {
                id,
                fetcher,
                source: Box::new(source),
                done_at: None,
                taken: false,
                slot: Arc::new(Mutex::new(Slot::Pending(Vec::new()))),
            });
            Some(id)
        }

        /// 写入结果并唤醒全部等待者。结果是错误（[`is_err_json`]）或条目已被取走时从列表
        /// 删掉条目：已经在等的消费者照样拿到这次结果，之后的导航重新请求。
        pub(super) fn complete(&self, id: u64, raw: RawJson, now: f64) {
            let slot = {
                let mut inner = self.lock();
                let Some(index) = inner.entries.iter().position(|e| e.id == id) else {
                    return;
                };
                let entry = &mut inner.entries[index];
                if entry.taken || is_err_json(&raw) {
                    inner.entries.remove(index).slot
                } else {
                    entry.done_at = Some(now);
                    entry.slot.clone()
                }
            };
            let wakers = {
                let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
                match std::mem::replace(&mut *slot, Slot::Ready(raw)) {
                    Slot::Pending(wakers) => wakers,
                    Slot::Ready(_) => Vec::new(),
                }
            };
            for waker in wakers {
                waker.wake();
            }
        }

        /// 取走同键条目（一次性）：进行中的直接加入等待；已完成且超过 TTL 的删掉并返回 `None`。
        pub(super) fn take<S: PartialEq + 'static>(
            &self,
            fetcher: TypeId,
            source: &S,
            now: f64,
        ) -> Option<Prefetched> {
            let mut inner = self.lock();
            let index = inner
                .entries
                .iter()
                .position(|e| e.matches(fetcher, source))?;
            if inner.entries[index].expired(now) {
                inner.entries.remove(index);
                return None;
            }
            if inner.entries[index].done_at.is_some() {
                return Some(Prefetched(inner.entries.remove(index).slot));
            }
            let entry = &mut inner.entries[index];
            entry.taken = true;
            Some(Prefetched(entry.slot.clone()))
        }
    }

    /// 预取结果的等待句柄。`Arc<Mutex<_>>` 内容都是 `Send`，满足 `Resource` 对 future 的 `Send` 约束。
    pub(super) struct Prefetched(Arc<Mutex<Slot>>);

    impl Future for Prefetched {
        type Output = RawJson;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<RawJson> {
            let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
            match &mut *slot {
                Slot::Ready(raw) => Poll::Ready(raw.clone()),
                Slot::Pending(wakers) => {
                    if !wakers.iter().any(|w| w.will_wake(cx.waker())) {
                        wakers.push(cx.waker().clone());
                    }
                    Poll::Pending
                }
            }
        }
    }

    /// 错误结果的判定不反序列化具体类型，用编码格式：`PageJson::new` 编码的是
    /// `Result<T, ApiError>`，外部标签格式，错误一定以 `{"Err"` 开头。
    pub(super) fn is_err_json(raw: &RawJson) -> bool {
        raw.0.starts_with("{\"Err\"")
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

        let missing = through_resource(PageJson::new(Err(ApiError::NotFound("文章不存在".into()))));
        let error = decode_page_json::<Sample>(&missing).unwrap_err();
        assert!(matches!(&error, ApiError::NotFound(message) if message == "文章不存在"));
        assert!(is_not_found(&error));
    }

    /// 编码的类型与解码期望不符时给出反序列化错误，而不是 panic。
    #[test]
    fn mismatched_page_json_is_a_deserialization_error() {
        let page = PageJson::new(Ok("not a sample".to_string()));
        assert!(matches!(
            decode_page_json::<Sample>(&page.raw),
            Err(ApiError::Framework(ServerFnError::Deserialization(_)))
        ));
    }

    async fn fetch_a() -> PageJson<u8> {
        PageJson::new(Ok(1))
    }

    async fn fetch_b() -> PageJson<u8> {
        PageJson::new(Ok(2))
    }

    fn key_of<F: 'static>(_: &F) -> std::any::TypeId {
        std::any::TypeId::of::<F>()
    }

    /// 预取键由取数函数的类型决定：同一个函数经 `unit` 得到同一个键，不同函数不同。
    #[test]
    fn unit_keys_follow_the_function() {
        assert_eq!(key_of(&unit(fetch_a)), key_of(&unit(fetch_a)));
        assert_ne!(key_of(&unit(fetch_a)), key_of(&unit(fetch_b)));
    }

    mod store {
        use super::*;
        use crate::hooks::ssr_resource::prefetch::{
            is_err_json, Prefetched, Store, CAPACITY, TTL_MS,
        };
        use std::any::TypeId;
        use std::task::{Context, Poll, Waker};

        const KEY: fn() -> TypeId = TypeId::of::<u8>;
        const OTHER_KEY: fn() -> TypeId = TypeId::of::<u16>;

        fn ok(n: u8) -> RawJson {
            PageJson::new(Ok(n)).raw
        }

        fn poll(hit: &mut Prefetched) -> Poll<RawJson> {
            Pin::new(hit).poll(&mut Context::from_waker(Waker::noop()))
        }

        #[test]
        fn begin_deduplicates_by_key_and_source() {
            let store = Store::new();
            assert!(store.begin(KEY(), 1, 0.0).is_some());
            assert!(store.begin(KEY(), 1, 0.0).is_none());
            assert!(store.begin(KEY(), 2, 0.0).is_some());
            assert!(store.begin(OTHER_KEY(), 1, 0.0).is_some());
        }

        #[test]
        fn pending_entry_resolves_for_a_waiting_consumer() {
            let store = Store::new();
            let id = store.begin(KEY(), 1, 0.0).unwrap();
            let mut hit = store.take(KEY(), &1, 5.0).unwrap();
            assert!(poll(&mut hit).is_pending());
            store.complete(id, ok(7), 10.0);
            assert_eq!(poll(&mut hit), Poll::Ready(ok(7)));
            // 取走的条目完成后不再留在列表里：同键可以重新请求。
            assert!(store.take(KEY(), &1, 11.0).is_none());
            assert!(store.begin(KEY(), 1, 11.0).is_some());
        }

        #[test]
        fn taken_pending_entry_no_longer_deduplicates() {
            let store = Store::new();
            store.begin(KEY(), 1, 0.0).unwrap();
            store.take(KEY(), &1, 0.0).unwrap();
            assert!(store.begin(KEY(), 1, 0.0).is_some());
        }

        #[test]
        fn completed_entry_is_consumed_once() {
            let store = Store::new();
            let id = store.begin(KEY(), 1, 0.0).unwrap();
            store.complete(id, ok(3), 1.0);
            let mut hit = store.take(KEY(), &1, 2.0).unwrap();
            assert_eq!(poll(&mut hit), Poll::Ready(ok(3)));
            assert!(store.take(KEY(), &1, 2.0).is_none());
        }

        #[test]
        fn completed_entry_expires_after_ttl_but_pending_does_not() {
            let store = Store::new();
            let fresh = store.begin(KEY(), 1, 0.0).unwrap();
            store.complete(fresh, ok(1), 100.0);
            assert!(store.take(KEY(), &1, 100.0 + TTL_MS - 1.0).is_some());

            let stale = store.begin(KEY(), 2, 0.0).unwrap();
            store.complete(stale, ok(2), 100.0);
            assert!(store.take(KEY(), &2, 100.0 + TTL_MS + 1.0).is_none());
            assert!(store.begin(KEY(), 2, 100.0 + TTL_MS + 1.0).is_some());

            store.begin(KEY(), 3, 0.0).unwrap();
            assert!(store.take(KEY(), &3, TTL_MS * 10.0).is_some());
        }

        #[test]
        fn error_results_are_not_cached_but_reach_waiters() {
            let store = Store::new();
            let id = store.begin(KEY(), 1, 0.0).unwrap();
            let mut waiter = store.take(KEY(), &1, 0.0).unwrap();
            let id2 = store.begin(KEY(), 2, 0.0).unwrap();
            let mut early = store.take(KEY(), &2, 0.0).unwrap();
            let failure = PageJson::<u8>::new(Err(ApiError::Internal("失败".into()))).raw;
            store.complete(id, failure.clone(), 1.0);
            assert_eq!(poll(&mut waiter), Poll::Ready(failure.clone()));
            store.complete(id2, failure.clone(), 1.0);
            assert_eq!(poll(&mut early), Poll::Ready(failure));
            assert!(store.take(KEY(), &1, 2.0).is_none());
            assert!(store.begin(KEY(), 1, 2.0).is_some());

            // 未被取走的条目收到错误后也直接删除。
            let id3 = store.begin(KEY(), 3, 0.0).unwrap();
            store.complete(
                id3,
                PageJson::<u8>::new(Err(ApiError::Internal("失败".into()))).raw,
                1.0,
            );
            assert!(store.take(KEY(), &3, 2.0).is_none());
        }

        #[test]
        fn capacity_evicts_the_oldest_completed_entry() {
            let store = Store::new();
            for source in 0..=CAPACITY as i32 {
                let id = store.begin(KEY(), source, 0.0).unwrap();
                store.complete(id, ok(1), 0.0);
            }
            assert!(store.take(KEY(), &0, 1.0).is_none());
            assert!(store.take(KEY(), &1, 1.0).is_some());
            assert!(store.take(KEY(), &(CAPACITY as i32), 1.0).is_some());
        }

        #[test]
        fn capacity_full_of_pending_requests_refuses_new_ones() {
            let store = Store::new();
            for source in 0..CAPACITY as i32 {
                assert!(store.begin(KEY(), source, 0.0).is_some());
            }
            assert!(store.begin(KEY(), CAPACITY as i32, 0.0).is_none());
        }

        /// `is_err_json` 依赖 `PageJson` 的编码格式：错误必以 `{"Err"` 开头。
        #[test]
        fn error_encoding_is_recognised() {
            assert!(is_err_json(
                &PageJson::<u8>::new(Err(ApiError::Internal("失败".into()))).raw
            ));
            assert!(!is_err_json(&PageJson::new(Ok(1u8)).raw));
        }
    }
}
