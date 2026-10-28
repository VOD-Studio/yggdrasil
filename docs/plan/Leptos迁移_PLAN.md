# Yggdrasil：Dioxus 0.7 → Leptos 0.8.22 全量迁移执行方案

## Context

把主 crate 的前端、SSR、server fn 宏、构建/发布/CI 从 Dioxus 0.7.10 整体换成 Leptos 0.8.22 + cargo-leptos 0.3.11（`--split` 路由级 WASM 分割）。原型 `spike/leptos/`（提交 `ba1b918e`，独立 workspace）已验证：lazy route + SSR 水合、SSR HTML 缓存捕获流式响应、上一发布分包保留。终态：仓库里没有任何 `dioxus` 依赖、`dx`、`Dioxus.toml`、reader/full 双 flavor 机制和 `spike/` 目录；现有全部路由、server fn、浏览器回归套件可用；每条公开路由首屏 brotli wasm ≤ 594,603 B（现 Dioxus reader 产物大小）。

用户已确认的决策：
- 目标 **Leptos 0.8.22**（不是 0.9-rc）。
- **删除** reader/full flavor 机制，改为 `Lazy::<T>` 路由分割（后台、登录、图鉴全部懒加载）。
- 在新分支上**原地一次性替换**；中间提交允许前端不完整，但每个阶段结束时必须满足该阶段的验收。
- 体积门槛：**不劣于现状**；任一公开路由超出 594,603 B 时停下报告，不自行做额外体积优化（本方案里已列出的懒加载点除外）。

## 执行总则

### 分支与参照源
- 起点：当前 `spike/leptos-0.8.22` 分支 HEAD `ba1b918e`（= master `b672cb4a` + 原型提交）。执行：`git switch -c migrate/leptos ba1b918e`。下文 **BASE = `ba1b918e`**。
- 第 0 步：把本方案原样保存为 `docs/plan/Leptos迁移_PLAN.md` 并提交（`docs(plan): Leptos 迁移执行方案`），所有子代理从这个路径读取方案。
- 迁移中被覆盖的 Dioxus 原文件一律用 `git show BASE:<path>` 读取，不要在工作树里另存副本。
- 原型代码是**起点而不是成品**：`spike/leptos/src/<path>` 与主 crate 同名文件是部分移植（例如 spike `components/forms.rs` 134 行 vs 原版 999 行，`ui/feedback.rs` 缺 Toast，`ui/` 缺 `menu.rs`/`surfaces.rs`）。规则：存在 spike 对应文件时先复制它，再逐项对照 `git show BASE:<path>` 补齐全部功能、删除原型残留（清单见“已知坑”最后一条）。spike 的 `api/*`、`server/db.rs`、`error.rs`、`models/*` **不得**搬进主 crate——主 crate 保留自己的 service 层、`DB_POOL`、moka 缓存与 `api::error::AppError`。

### 并行方式（Phase 2B、3、5）
- 每个并行切片在独立 git worktree 里工作：`git worktree add ../ygg-mig-<切片ID> -b migrate/leptos-<切片ID> migrate/leptos`。
- 所有切片共用一个 target 目录：`export CARGO_TARGET_DIR=<主仓库绝对路径>/target/mig-slices`（cargo 自带构建锁，并发时排队即可）。
- 切片只能修改自己“文件归属”列出的文件。需要新增 `web-sys` feature 或改 `Cargo.toml` 时，不要改，写进完成报告由编排者统一加。
- 切片自检只做编译与单测：`cargo clippy --locked --all-targets -- -D warnings`、`cargo clippy --locked --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings`、`cargo test --locked <切片相关模块路径>`；切片内不跑服务器和浏览器（worktree 里没有 `libs/node_modules` 与生成的 `public/` 资源）。完成后在切片分支提交。
- 编排者按切片 ID 顺序 `git merge --no-ff migrate/leptos-<ID>` 合入 `migrate/leptos`，然后在主工作树跑该阶段完整验收，修复后提交，最后 `git worktree remove`。

### 阶段顺序
Phase 0 → Phase 1 → Phase 2A → 并行 {Phase 2B 三个切片, Phase 5 `release` 切片} → Phase 2B 合并验收 → Phase 3 六个并行切片 → Phase 3 合并验收 → Phase 4 → 合并 `release` 切片 → Phase 6。`release` 切片若先完成，等待到 Phase 4 结束后再合并（避免中途的 `make build` 依赖未完成页面）。

### 提交与停止条件
- 每个阶段验收通过后提交一次，格式 `type(scope): 中文摘要`；不推送。
- 以下情况**停下并向用户报告**，不要自行改方案：(1) Phase 2A 的视图过渡握手无法让 `bun scripts/test-view-transitions.cjs` 通过；(2) Phase 6 体积门槛任一路由超标；(3) 本方案“假设与预案”里写明“停止”的情形。

## 迁移约定（所有执行者共享的契约）

### Crate 结构与 feature
- 主 crate 改为 lib + bin：`src/lib.rs` 承载全部模块（`pub mod`），`src/main.rs` 只保留全局分配器与 `yggdrasil::startup::run()`；`[lib] crate-type = ["cdylib", "rlib"]`。`src/lib.rs` 与 `src/main.rs` 第一行都写 `#![recursion_limit = "256"]`（release 不擦除组件类型，默认 128 不够，原型实测）。
- feature：`default = ["server"]`；`server` = `leptos/ssr`、`leptos_meta/ssr`、`leptos_router/ssr`、`dep:leptos_axum` + 现有全部 `dep:*`；`hydrate = ["leptos/hydrate"]`。删除 `web`、`client-reader`、`dev-logger`。**禁止同时开启 `server` 与 `hydrate`**：仓库中所有 `--all-features` 一律删掉（`Makefile`、`.github/workflows/ci.yml`、`mk/docker.mk`、`docker-compose.tools.yml` 注释、`scripts/test-databases.sh`），改为默认 feature（即 server）。
- 门控：服务端代码 `#[cfg(feature = "server")]`（沿用现有写法，不改名）；浏览器 API `#[cfg(target_arch = "wasm32")]`；`ygg_client_reader` 全部删除，原来被它门控的代码无条件编译。
- 不启用 leptos 的 `delegation` feature（事件监听直接挂在元素上，导航拦截依赖这一点）。

### Dioxus → Leptos 写法对照
| Dioxus 0.7 | Leptos 0.8 |
|---|---|
| `#[component] fn X(p: T) -> Element` + `rsx!{}` | `#[component] pub fn X(p: T) -> impl IntoView` + `view!{}` |
| `#[props(default)]` / `#[props(into)]` / `#[props(default = v)]` | `#[prop(optional)]` / `#[prop(into)]` / `#[prop(default = v)]` |
| `children: Element` | `children: Children`（多次渲染用 `ChildrenFn`）；可选 `Option<Children>` |
| `Element` 类型的 prop | `ViewFn` 或 `AnyView`（需要多次渲染用 `ViewFn`） |
| `EventHandler<T>` / `Option<EventHandler<T>>` | `Callback<T>` / `#[prop(optional)] Option<Callback<T>>`，调用 `.run(v)`；闭包无法 `Send + Sync` 时用 `UnsyncCallback` |
| `Callback<A, R>` | `Callback<A, R>` |
| `use_signal(|| v)` | `RwSignal::new(v)`；值不是 `Send + Sync`（JS 句柄、`Closure`、`Rc`）时 `RwSignal::new_local` |
| `Signal<T>` prop（读写） / 只读 | `RwSignal<T>` / `Signal<T>`（`#[prop(into)]`） |
| `use_memo` | `Memo::new` |
| `use_effect(move || ..)` / `use_effect(use_reactive(..))` | `Effect::new(move |_| ..)`，在闭包里 `.get()` 需要追踪的信号 |
| `use_hook(|| v)` | 组件函数体直接 `let v = ..;`（Leptos 组件只运行一次）；需要可变存储用 `StoredValue::new(_local)` |
| `use_drop(f)` | `on_cleanup(f)` |
| `spawn(async ..)` | `leptos::task::spawn_local(async ..)` |
| `use_future` | `Effect::new` + `spawn_local` 循环，`on_cleanup` 里置停止标志 |
| `use_server_future(f)?` | 见下文“数据加载规则” |
| `use_resource` | 见下文“数据加载规则” |
| `use_context_provider(|| v)` / `use_context::<T>()` / `try_use_context` | `provide_context(v)` / `expect_context::<T>()` / `use_context::<T>()` |
| `SuspenseBoundary { fallback, .. }` | `<Suspense fallback=..>` |
| `onmounted` | `NodeRef::<html::Div>::new()` + `Effect::new` 读 `node_ref.get()` |
| `onclick: move |e| ..` | `on:click=move |e| ..`（`e` 已是 `web_sys::MouseEvent`；`as_web_event()` 删除） |
| `e.value()`（input） | `event_target_value(&e)` |
| `dangerous_inner_html: x` | `inner_html=x` |
| `class: "{a} {b}"` | `class=format!(..)` 或 `class=move || ..`；布尔切换用 `class:foo=move || ..` |
| `if c { rsx!{..} }` | `<Show when=move || c>` 或 `move || c.then(|| view!{..})` |
| `for x in xs { div { key: "{x.id}", .. } }` | `<For each=move || xs.get() key=|x| x.id let:x>..</For>` |
| 用 `key` 强制按参数重建子树 | `move || { let k = param.get(); view! { <Inner k/> } }` |
| `document::Title/Meta/Link/Script/Stylesheet` | `leptos_meta::{Title, Meta, Link, Script, Stylesheet}`（`Script` 不按 src 去重，脚本必须是幂等 IIFE） |
| `Link { to: Route::X {..}, .. }` | `<a href=Route::X { .. }.to_string()>`（见“导航规则”） |
| `navigator().push(r)` / `use_navigator()` | `crate::navigation::push(&r.to_string())`；**禁止** `leptos_router::hooks::use_navigate` 与 `<A>` |
| `router().current::<Route>()` / `use_route` | `use_location()`（`pathname`/`search` 是 `Memo<String>`）、`use_params_map()`、`use_query_map()` |
| 定时器 | `leptos::prelude::{set_timeout, set_interval}` |

### 数据加载规则
1. **SSR 数据**（原 `use_server_future`，26 处）：在路由的 `LazyRoute::data()`（主包内同步执行）里创建 `Resource::new(source, fetcher)`，source 由 `use_params_map()` / `use_query_map()` 派生的 `Memo` 构成（参数变化即重取）；`view()` 里用 `<Suspense fallback=..>{move || Suspend::new(async move { .. res.await .. })}</Suspense>` 消费。非路由组件（例如 `Footer` 的 `get_site_settings`）在组件内创建 `Resource` 并只在 `<Suspense>` 内读取。
2. **客户端数据**（后台页 `use_effect + spawn`、`hooks::query::use_paginated`）：`Effect::new` + `spawn_local` 写 `RwSignal`，带世代号防竞态（模式见 `spike/leptos/src/components/comments/section.rs:229-256`）；SSR 时不请求。
3. **会刷新的数据放在 `<Suspense>` 里时禁止用 `Resource`/`LocalResource`**：Resource 被外层 Suspense 计入挂起，刷新会让整段 DOM 退回 fallback 再插回（原型里评论编辑器因此失焦）。一律改用规则 2 的 Effect + 镜像信号。`use_resource`（16 处）按此规则二选一：只在挂载时取一次且不在 Suspense 内 → `LocalResource`；会重取或位于 Suspense 内 → 规则 2。
4. **水合一致**：服务端与客户端首次渲染必须产出相同 DOM 结构。`#[cfg(target_arch = "wasm32")]` 只能出现在 `Effect`、事件处理与桥接函数内，不能用来选择不同的视图分支；需要“仅客户端显示”的内容，用一个在 Effect 里置 `true` 的 `RwSignal<bool>` 控制。

### 错误规则
- server fn 签名保持 `Result<T, ServerFnError>`（`leptos::prelude::ServerFnError`），不改成自定义错误类型。`AppError` 不得实现 `std::error::Error`（否则与 `impl<E: Error> From<E> for ServerFnError` 冲突）。
- `src/api/error.rs`：新增 `pub const NOT_FOUND_PREFIX: &str = "not_found:";`；`From<AppError> for ServerFnError` 改为：`NotFound(m)` → `ServerFnError::ServerError(format!("{NOT_FOUND_PREFIX}{m}"))`；`Unauthorized/Forbidden/Internal(m)` → `ServerError(m.to_string())`；`BadRequest(m)` → `ServerError(m)`；`DbConn(_)` → `ServerError("服务暂时不可用")`；`Query(_)|Transaction(_)` → `ServerError("操作失败")`。现有防泄漏测试改断言新变体形状后保留。
- `src/utils/error.rs` 改写为：
  - `pub fn error_reason(err: &ServerFnError) -> Option<String>`：`ServerError(m)` → 去掉 `NOT_FOUND_PREFIX` 前缀后 trim，空串或 `"操作失败"` 返回 `None`，否则 `Some`；`Request(_)` → `"网络连接中断，请检查网络后重试"`；`Deserialization(m) | Response(m)`（代理返回 HTML 时 server_fn 解码失败，消息里带原始响应体）→ 依次查找 `"413"`、`"429"`、`"502"`、`"503"`、`"504"`，首个命中交给原 `status_reason(code)`，都不命中返回 `"服务暂时不可用，请稍后再试"`；其余变体 `None`。
  - `failure_message(action, err)` 语义不变。
  - 新增 `pub fn is_not_found(err: &ServerFnError) -> bool`（`ServerError(m)` 且 `m.starts_with(NOT_FOUND_PREFIX) || m == "not_found"`）与 `pub fn is_not_found_text(text: &str) -> bool`（`contains(NOT_FOUND_PREFIX) || contains("not_found")`）。
  - 现有测试改写到新变体后保留，并补一条：`Deserialization("Invalid format: missing delimiter in \"<html><title>413 Request Entity Too Large</title>\"")` → `"内容超过服务器允许的大小"`。
- UI 不得直接展示 `err.to_string()`，一律经 `failure_message`/`error_reason`。
- 页面错误：`Suspend` 内对 `Err(e)` 分支：`is_not_found(&e)` → `<NotFound/>`；否则 `<PageLoadFailed/>`（新组件，放 `src/pages/not_found.rs`，文案与 BASE `src/router.rs:127-144` 的“页面加载失败”EmptyState 相同，按钮调用 `crate::navigation::push("/")`，渲染时 `crate::request_ctx::set_status(500)`）。`FrontendLayout` 在 `<Outlet/>` 外包一层 `<ErrorBoundary>` 兜底：任何错误文本 `is_not_found_text` → `<NotFound/>`，否则 `<PageLoadFailed/>`。

### 导航规则（替代 Dioxus 自定义 History）
- JS 协调器 `window.__routeTransitions`（`libs/yggdrasil-core/src/route-transitions.ts`）继续拥有 history 与视图过渡；Leptos 的 `BrowserUrl` 只负责“读 URL → 渲染”。
- 所有站内链接是普通 `<a href>`；指向非 SPA 路由的链接（`/feed.xml`、`/feed.json`、`/doc/...`、`/uploads/...`、`/og/...`、`/api/...`、`/note-media/...`、下载链接、外站）必须带 `rel="external"`（或 `download`/`target`）。
- 程序化跳转只用 `crate::navigation::{push, replace, back, forward, external}`。
- `src/routes.rs` 的 `Route` 枚举只负责生成 href（见 Phase 2A），不参与路由匹配。

### 路由表（`src/app.rs`，Phase 2A 一次写全）
公开路由挂在 `<ParentRoute path=path!("") view=|| view! { <FrontendLayout/> }>` 下：

| 路径 | 视图（`Lazy::<T>`） | 文件 | SsrMode |
|---|---|---|---|
| `""` 与 `page/:page` | `HomeRoute` | `pages/home.rs` | `InOrder` |
| `notes` | `NotesRoute` | `pages/notes.rs` | `InOrder` |
| `notes/book/:id` | `NotebookRoute` | `pages/notes.rs` | `Async` |
| `notes/:slug`（query `book`） | `NoteRoute` | `pages/notes.rs` | `Async` |
| `archives` | `ArchivesRoute` | `pages/archives.rs` | `InOrder` |
| `tags/:tag` | `TagRoute` | `pages/tags.rs` | `Async` |
| `post/:slug` | `PostRoute` | `pages/post_detail.rs` | `Async` |
| `search`（query `q`） | `SearchRoute` | `pages/search.rs` | 默认 |
| `about` | `AboutRoute` | `pages/about.rs` | `InOrder` |
| `about/writing` | `WritingGuideRoute` | `pages/writing_guide.rs` | `InOrder` |
| `about/components` | `ShowcaseRoute` | `pages/components_showcase.rs` | `InOrder` |
| `about/components/:component` | `ShowcaseDetailRoute` | `pages/components_showcase.rs` | `Async` |
| `friends` | `FriendsRoute` | `pages/friends.rs` | `InOrder` |
| `changelog` | `ChangelogRoute` | `pages/changelog.rs` | `InOrder` |

后台挂在 `<ParentRoute path=path!("admin") view={Lazy::<AdminShellRoute>::new()}>`（`pages/admin/mod.rs`，视图渲染 `AdminLayout` 包住 `<Outlet/>`），子路由全部默认 SsrMode：

| 路径 | 类型 | 文件 |
|---|---|---|
| `""` | `DashboardRoute` | `pages/admin/dashboard.rs` |
| `write` / `write/:id` | `WriteRoute` / `WriteEditRoute` | `pages/admin/write.rs` |
| `preview/:slug` | `PreviewRoute` | `pages/admin/preview.rs` |
| `posts` | `PostsRoute` | `pages/admin/posts.rs` |
| `posts/trash` | `PostsTrashRoute` | `pages/admin/posts_trash.rs` |
| `notes` | `AdminNotesRoute` | `pages/admin/notes.rs` |
| `notes/new` / `notes/edit/:id` | `NewNoteRoute` / `EditNoteRoute` | `pages/admin/note_editor.rs` |
| `notebooks` | `NotebooksRoute` | `pages/admin/notes/notebooks.rs` |
| `assets` | `AssetsRoute` | `pages/admin/assets.rs` |
| `friends` | `FriendsAdminRoute` | `pages/admin/friends.rs` |
| `comments` 与 `comments/:page` | `AdminCommentsRoute`（无 page 参数视为 1） | `pages/admin/comments.rs` |
| `system` | `SystemRoute` | `pages/admin/system/mod.rs` |
| `runner` | `RunnerRoute` | `pages/admin/runner.rs` |
| `mcp` | `McpRoute` | `pages/admin/mcp.rs` |
| `logs` | `LogsRoute` | `pages/admin/logs.rs` |
| `settings` | `SettingsRoute` | `pages/admin/settings/mod.rs` |
| `profile` | `ProfileRoute` | `pages/admin/profile.rs` |

顶层（无布局）：`login` → `LoginRoute`（`pages/login.rs`），`register` → `RegisterRoute`（`pages/register.rs`）。
- `/tags` 不注册 Leptos 路由（axum 的永久重定向 `/tags` → `/archives` 保留；两处注册同一路径会让 axum `merge` panic）。站内若有指向精确 `/tags` 的链接，改成 `/archives`。
- 未匹配路由：`<Routes fallback=|| view! { <FrontendLayout><NotFound/></FrontendLayout> }>`。不要注册 `*any` 通配路由（会吞掉静态文件请求）。
- `SsrMode` 取值依据：页面 `<Title>`/`<Meta>` 或 404 状态码依赖服务端数据的用 `Async`；其余有 SSR 数据的用 `InOrder`；纯客户端数据的用默认。

`LazyRoute` 写法（照抄原型 `spike/leptos/src/pages/post_detail.rs:45-92` 的结构）：
```rust
pub struct PostRoute { post: Resource<Result<Option<Post>, ServerFnError>> }

#[lazy_route]
impl LazyRoute for PostRoute {
    fn data() -> Self {
        let params = use_params_map();
        let slug = Memo::new(move |_| params.read().get("slug").unwrap_or_default());
        Self { post: Resource::new(move || slug.get(), get_post_by_slug) }
    }
    fn view(this: Self) -> AnyView {
        let post = this.post;
        view! {
            <Suspense fallback=|| view! { <DelayedSkeleton><PostDetailSkeleton/></DelayedSkeleton> }>
                {move || Suspend::new(async move {
                    match post.await {
                        Ok(Some(p)) => view! { <PostDetail post=p/> }.into_any(),
                        Ok(None) => view! { <NotFound/> }.into_any(),
                        Err(e) if is_not_found(&e) => view! { <NotFound/> }.into_any(),
                        Err(_) => view! { <PageLoadFailed/> }.into_any(),
                    }
                })}
            </Suspense>
        }.into_any()
    }
}
```
（类型名与 server fn 名以主 crate 实际定义为准；导入 `use leptos_router::{lazy_route, LazyRoute, Lazy, SsrMode};`）

### 已知坑（原型实测）
- 需要 `#![recursion_limit = "256"]`（lib 与 bin 都要）。
- `SsrMode::Async` 路由的 HTML 不预加载分包（多一次往返），接受。
- 上一发布的 `pkg/` 必须可回退，否则旧标签页懒加载 404 会让 wasm panic（`SplitLoaderError`）。
- `leptos_meta::Script` 不去重。
- Leptos 0.8 信号值要求 `Send + Sync`；JS 句柄用 `_local` 变体或 `StoredValue::new_local`。
- 删除原型残留：`BUILD_ID`、`data-spike-build`、`<meta name="spike-build">`、`SPIKE_*` 环境变量、`../../public`/`../../uploads` 相对路径、硬编码 `http://127.0.0.1:8080/login`、简化 Markdown（`render_legacy_markdown`）、手写图片头解析（`post_cover.rs` 的 `mod image_dims`）、`x-spike-asset-source` 响应头。

## Phase 0：准备（编排者，顺序执行）

1. `git switch -c migrate/leptos ba1b918e`；确认 `git rev-parse HEAD` = `ba1b918e…`。
2. 保存并提交方案到 `docs/plan/Leptos迁移_PLAN.md`。
3. 工具确认：`cargo leptos --version` 输出 `cargo-leptos 0.3.11`（缺失则 `cargo install cargo-leptos --version 0.3.11 --locked`）；`make wasm-toolchain`（nightly-2026-10-19 + rust-src + wasm32）；`bun --version`、`tailwindcss --help` 可用；`docker`、`psql`、`pg_isready` 可用。
4. 冒烟数据库：使用已存在的一次性库 `ygg_leptos_spike`（开发库副本，与 `.env` 的 `DATABASE_URL` 同一 PostgreSQL 实例）。不存在时用 `createdb -T <.env 中的库名> ygg_leptos_spike` 创建（只读 `.env`，不要打印其内容）。**不得**连接 `.env` 原库做任何写操作。下文 `SMOKE_DB_URL` 指向它。

## Phase 1：crate 骨架与服务端层（单代理，顺序执行）

目标：去掉 dioxus 依赖，服务端（api/db/mcp/tasks/startup/middleware）在 Leptos 下编译、启动、可调用 server fn；UI 模块暂不声明（文件留在磁盘上不编译）。

1. **Cargo.toml**
   - 删除 `dioxus` 依赖及其注释块；新增：
     ```toml
     leptos = { version = "=0.8.22" }
     leptos_router = { version = "=0.8.17" }
     leptos_meta = { version = "=0.8.7" }
     leptos_axum = { version = "=0.8.10", optional = true }
     percent-encoding = "2"
     ```
   - `tokio` features 追加 `"signal"`（优雅退出）。
   - 新增 `[lib] crate-type = ["cdylib", "rlib"]` 与 `doctest = false`（原先是纯 bin crate，从未跑过 doctest；大量中文文档注释里的代码块不是可编译示例）；保留 `[[bin]] generate_highlight_css`。
   - `[features]` 改为：`default = ["server"]`；`hydrate = ["leptos/hydrate"]`；`server = ["leptos/ssr", "leptos_meta/ssr", "leptos_router/ssr", "dep:leptos_axum", <原有全部 dep: 条目>]`。删除 `web`、`client-reader`、`dev-logger` 及其注释。
   - wasm 目标 `web-sys` features 取现有列表与 `spike/leptos/Cargo.toml` 列表的并集（补 `HtmlTextAreaElement`、`ScrollToOptions`、`ScrollBehavior`、`Location`、`History`、`console`、`RequestCredentials`）。
   - `[profile.wasm-release]` 保留（cargo-leptos 的 `lib-profile-release` 使用它），注释改为说明 cargo-leptos 用途与 `strip=false` 原因（剥离会删 wasm-bindgen 元数据）。
   - 新增：
     ```toml
     [package.metadata.leptos]
     output-name = "yggdrasil"
     site-root = "target/site"
     site-pkg-dir = "pkg"
     assets-dir = "public"
     site-addr = "127.0.0.1:8080"
     reload-port = 3001
     hash-files = true
     env = "DEV"
     bin-target = "yggdrasil"
     bin-features = ["server"]
     bin-default-features = false
     lib-features = ["hydrate"]
     lib-default-features = false
     lib-profile-release = "wasm-release"
     ```
2. **build.rs**：删除 `emit_reader_cfg` 函数及其调用（含 `rustc-check-cfg=cfg(ygg_client_reader)`），保留 git/构建信息注入。
3. **.cargo/config.toml**：删除第 1-17 行（`SERVER_FN_OVERRIDE_KEY` 说明与 `[env]` 段）；第 18-20 行注释改写为“wasm32 目标的 getrandom 后端”；保留 `[target.wasm32-unknown-unknown] rustflags` 与 musl linker。
4. **删除 `Dioxus.toml`**。
5. **`src/lib.rs`（新建）**：第一行 `#![recursion_limit = "256"]`，第二行 `#![doc(html_favicon_url = "/favicon.svg")]`；把 `src/main.rs` 现有的模块声明搬来并全部改为 `pub mod`（保持原有 `#[cfg(feature = "server")]` 门控，删掉 `#![cfg_attr(ygg_client_reader, ..)]`）。本阶段**只声明**：`api, auth, build_info, cache, config, db, infra, highlight(server), middleware(server), models, og(server), mcp(server), ssr_cache(server), startup(server), tasks, utils, request_ctx, app`。末尾：
   ```rust
   #[cfg(feature = "hydrate")]
   #[wasm_bindgen::prelude::wasm_bindgen]
   pub fn hydrate() {
       leptos::mount::hydrate_lazy(crate::app::App);
   }
   ```
6. **`src/main.rs`** 改为：`#![recursion_limit = "256"]`；保留 mimalloc 全局分配器块；`#[cfg(feature = "server")] fn main() { yggdrasil::startup::run() }`；`#[cfg(not(feature = "server"))] fn main() {}`。
7. **`ShareContext` 解耦**：把 `ShareContext` 结构体从 `src/components/share_meta.rs` 移到 `src/models/share.rs`（纯数据，两端编译）；`src/api/og.rs:21` 与 `src/middleware.rs` 的引用改到 `crate::models::share::ShareContext`。
8. **`src/request_ctx.rs`（新建，两端编译）**，替代 `FullstackContext`：
   ```rust
   #[cfg(feature = "server")] pub fn request_parts() -> Option<http::request::Parts>          // use_context::<http::request::Parts>()
   #[cfg(feature = "server")] pub fn request_headers() -> Option<http::HeaderMap>            // request_parts().map(|p| p.headers)
   #[cfg(feature = "server")] pub fn request_extension<T: Clone + Send + Sync + 'static>() -> Option<T>
   #[cfg(feature = "server")] pub fn append_response_header(name: http::HeaderName, value: http::HeaderValue) // use_context::<leptos_axum::ResponseOptions>()?.append_header
   pub fn set_status(code: u16)   // 服务端：ResponseOptions::set_status；客户端：空操作
   #[cfg(all(feature = "server", test))]
   pub async fn scope_with_request<F: std::future::Future>(parts: http::request::Parts, fut: F) -> F::Output
   // 实现：let owner = Owner::new(); owner.with(|| { provide_context(parts); provide_context(leptos_axum::ResponseOptions::default());
   //       leptos::reactive::computed::ScopedFuture::new(fut) }).await
   ```
   替换全部 `FullstackContext` 调用点（`grep -rn "FullstackContext" src` 必须归零）：
   - 限流读请求头（`FullstackContext::current()` → `ctx.parts_mut().headers`）：`src/api/auth.rs:89,187`、`src/api/profile.rs:210`、`src/api/comments/check.rs:41`、`src/api/comments/create.rs:41,194-205`、`src/api/posts/search.rs:33`、`src/api/posts/suggest.rs:80`、`src/api/code_runner/execute.rs:56-59` → `crate::request_ctx::request_headers()`。
   - Set-Cookie：`src/api/auth.rs:322-325`（login）、`:353-356`（logout）→ `append_response_header(SET_COOKIE, value)`。
   - `src/auth/session.rs:61-67` `get_session_from_ctx()` → 读 `request_headers()`。
   - 测试辅助 `web_context()`：`src/api/posts/batch.rs:61-67` 与 `src/mcp/tools/posts/tests.rs:4-10` 改为返回 `http::request::Parts`，调用点 `web_context().scope(fut)` 全部改为 `crate::request_ctx::scope_with_request(web_context(), fut)`（14 处）。
   - `src/router.rs`、`src/pages/not_found.rs`、`src/pages/components_showcase.rs`、`src/components/share_meta.rs` 中的调用点随各自文件在 Phase 2A/3/4 移植时改用 `set_status` / `request_extension`。
9. **server fn 宏批量改写**（用结构化 codemod，例如 omp `xd://ast_edit`；不要手改 122 处）：
   - `#[server(Name, "/api")]`（120 处）与裸 `#[server]`（`src/api/writing_guide.rs:33`；`src/pages/components_showcase.rs:104` 留到 Phase 4）→ `#[server(endpoint = "<紧随其后的 fn 名>", input = leptos::server_fn::codec::Json)]`。最终 URL = `/api/<fn 名>`（如 `/api/login`、`/api/get_current_user`），不再有哈希后缀。
   - 改写前用 `grep -rn -A3 '^#\[server' src --include=*.rs` 核对每个宏下方的 fn 名唯一（当前 121 个唯一名），且没有名为 `upload` 的 server fn（会与 axum 路由 `/api/upload` 冲突）。
   - `src/api/**` 中 51 个文件的 `use dioxus::prelude::*;` → `use leptos::prelude::*;`；若某文件因 prelude 名称冲突编译失败，该文件改为 `use leptos::prelude::{server, ServerFnError};`。
   - `dioxus::prelude::ServerFnError` 全限定引用（`src/api/error.rs:6`、`src/api/posts/types.rs:189`、`src/mcp/tools/notes.rs:14`）→ `leptos::prelude::ServerFnError`。
   - `src/api/settings/mod.rs` 的 20 行 `#[cfg(not(ygg_client_reader))]`（第 24-44、199-222 行，全是这一种）直接删除，被门控的 `mod`/`pub use` 无条件编译；第 21-23 行注释删除。
   - `src/api/settings/{webp,security,runner,rate_limit,image_limit,image_cache}.rs` 里的 `.map_err(ServerFnError::from)` 依赖 `From<AppError> for ServerFnError`，保持不变。Dioxus 结构体变体 `ServerFnError::ServerError { message, code, details }` 只出现在 `src/utils/error.rs`（随第 10 步重写）、`src/router.rs`（删除）与 `src/pages/post_detail.rs:54-57`（Phase 3 按 LazyRoute 范式在 `Ok(None)` 分支直接渲染 `<NotFound/>`）。
   - 只是注释提到 dioxus 的地方（`src/api/upload.rs:59`、`src/db/migrate.rs:3`、`src/mcp/tools/read.rs:11`、`src/mcp/router.rs:56`、`src/utils/js.rs:14`）改写为不指名 Dioxus 的说明。
10. **错误映射**：按“错误规则”改 `src/api/error.rs` 与 `src/utils/error.rs`（含测试）。
11. **中间件**（`src/middleware.rs`）：
    - `cache_control_for_path`：删除 `/_dioxus/`、`/wasm/` 分支；在 `.wasm/.js/.css` 分支之前新增：路径以 `/pkg/` 开头且文件名形如 `<name>.<22 位 [A-Za-z0-9_-]>.<ext>` → `public, max-age=31536000, immutable`；其余 `/pkg/` 路径 → `public, no-cache`。测试 `unversioned_assets_require_revalidation`（`:511-540`）改用 `/pkg/yggdrasil.wasm`、`/pkg/yggdrasil.js` 断言 `no-cache`，并新增一条：`/pkg/yggdrasil.AbCdEfGhIjKlMnOpQrStUv.wasm` → immutable。
    - `previous_asset_file`（`:340-358`）：顶层段只放行 `pkg`（删除 `assets`/`wasm`）；更新函数与 `previous_release_assets` 的文档注释；相关测试改为 `/pkg/...` 路径。
12. **`src/ssr_cache.rs`**：逻辑不变；只改注释里的 Dioxus 字样。
13. **`src/startup.rs` 重写路由装配**（删除 `dioxus::server` 导入、`READER_TEMPLATE_DIR`、`build_flavor_states`、`FlavorStates`、`dioxus_public_root`、`load_reader_state`、`validate_public_templates`、`validate_template_assets`、`notes_routes`）：
    - `fn site_root() -> PathBuf`：环境变量 `LEPTOS_SITE_ROOT` 非空则用它（cargo-leptos 在 watch/serve 时自动设置；Docker 镜像显式设置）；否则 `<可执行文件目录>/public`。
    - `fn server_addr() -> SocketAddr`：`IP`（默认 `127.0.0.1`）+ `PORT`（默认 `8080`），与 dx 的 `fullstack_address_or_localhost` 默认一致；解析失败打印中文错误并 `exit(1)`。
    - `fn leptos_options() -> LeptosOptions`：`LeptosOptions::builder().output_name("yggdrasil").site_root(<site_root 字符串>).site_pkg_dir("pkg").site_addr(server_addr()).reload_port(3001).hash_files(true).env(if cfg!(debug_assertions) { Env::DEV } else { Env::PROD }).build()`。
    - `fn validate_site_assets(options)`：读 `<可执行文件目录>/hash.txt`，缺失则 panic：`"hash.txt 缺失（{path}）：前端产物与服务端二进制不配套，拒绝启动"`；按行解析 `js:`/`wasm:`/`manifest:`，检查 `<site_root>/pkg/yggdrasil.<js>.js`、`<site_root>/pkg/yggdrasil.<wasm>.wasm`，有 `manifest:` 行时检查 `<site_root>/pkg/__wasm_split_manifest.<hash>.json`；缺失则 panic 并列出缺失路径。debug 与 release 都执行。
    - `init_cache_namespace("leptos")`：读 `<site_root>/release-manifest.json` 的 `release_id`，缺失回退 `BUILD_INFO.git_describe`（逻辑同原函数）。
    - `serve_application`：debug/release 合并为同一路径——多线程 runtime、绑定 `server_addr()`、`axum::serve(listener, router.into_make_service()).with_graceful_shutdown(<ctrl_c 或 SIGTERM>)`；保留 `fatal_bind_error`。
    - `build_router`：
      ```rust
      let options = leptos_options();
      validate_site_assets(&options);
      let routes = leptos_axum::generate_route_list(crate::app::App);
      let app = axum::Router::new()
          .leptos_routes(&options, routes, { let o = options.clone(); move || crate::app::shell(o.clone()) })
          .fallback(site_fallback)
          .with_state(options.clone());
      ```
      其后依次：可选 `previous_release_assets` 层（原 453-466 行逻辑，日志改为“/pkg 缺失时回退”）→ 原 560-582 行的整套 layer（share_context → cache_public_html → ssr_generation → add_cache_control → csrf → 可选压缩 → 30 s 超时 → admin_guard）→ 原 upload/mcp_upload/backup_import/export/sse/logs_sse/static/feed/mcp 路由与 merge 顺序、`version_headers_middleware` 全部保留。
    - `async fn site_fallback(State(options): State<LeptosOptions>, req: Request) -> Response`：GET/HEAD 先用 `tower_http::services::ServeDir::new(site_root).precompressed_br()` `oneshot`，非 404 直接返回；路径以 `/pkg/` 开头 → 空 404；其余 → `leptos_axum::render_app_async(move || crate::app::shell(options.clone()))(req).await`（未匹配路由由 `NotFound` 设 404）。非 GET/HEAD → 404。
    - 第 43-45 行启动期清缓存保留。
14. **`src/app.rs`（新建）**：本阶段写入最终版 `shell()`（Phase 2A 只追加 `<ThemePreload/>`）与临时 `App`：
    ```rust
    pub fn shell(options: LeptosOptions) -> impl IntoView {
        view! {
            <!DOCTYPE html>
            <html lang="zh-CN">
                <head>
                    <meta charset="utf-8"/>
                    <meta name="viewport" content="width=device-width, initial-scale=1"/>
                    <meta name="theme-color" content="#f5f5f3" media="(prefers-color-scheme: light)"/>
                    <meta name="theme-color" content="#0f0f0e" media="(prefers-color-scheme: dark)"/>
                    <link rel="icon" href="/favicon.svg" type="image/svg+xml"/>
                    <link rel="icon" href="/favicon.ico" sizes="16x16 32x32 48x48"/>
                    <link rel="apple-touch-icon" href="/apple-touch-icon.png"/>
                    <link rel="manifest" href="/manifest.webmanifest"/>
                    <link rel="stylesheet" href="/style.css?v=5"/>
                    <link rel="stylesheet" href="/highlight.css?v=2"/>
                    <link rel="stylesheet" href="/katex/katex.min.css?v=3"/>
                    <link rel="stylesheet" href="/yggdrasil-core/yggdrasil-core.css?v=3"/>
                    <script src="/yggdrasil-core/yggdrasil-core.js?v=4"></script>
                    <AutoReload options=options.clone()/>
                    <HydrationScripts options/>
                    <MetaTags/>
                </head>
                <body><App/></body>
            </html>
        }
    }
    ```
    临时 `App`：`provide_meta_context()` + `<Title text="Yggdrasil Blog"/>` + `<Router>` 内一个 `<Routes fallback=|| "迁移中">`，只含一条 `<Route path=path!("__migration_stub") view=|| ()/>`；文件内加 `// MIGRATION_STUB` 注释。原 `Dioxus.toml` 的“`?v=N` 资源版本”规则从此改为维护 `shell()` 里的查询串。
15. **开发与回归入口**（本阶段就要可用）：
    - `Makefile`：删除 `DX`、`DX_NIGHTLY` 变量；`check-dev-tools` 把 dx 检查换成 `cargo leptos --version` 必须为 `0.3.11`（提示 `cargo install cargo-leptos --version 0.3.11 --locked`）；`dev` 目标改为 `@SSR_CACHE_SECS=0 IP=0.0.0.0 cargo leptos watch`（依赖 `dev-assets`，去掉 `esbuild-cache wasm-bindgen-cache`）；`regression-build` 改为依赖 `dev-assets` 后执行 `cargo leptos build`（debug、不加 `--split`），删掉 `restore-webp` 调用；`lint` 改为 `cargo clippy --all-targets -- -D warnings`、`cargo clippy --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings`、`cargo fmt -- --check`，删除 `dx fmt` 说明注释块（277-283 行）。
    - `scripts/regression.cjs`：`build` 改为服务端 `path.join(ROOT, 'target/debug/yggdrasil')`、站点根 `path.join(ROOT, 'target/site')`；缺失时报错 `Run make regression-build first`；删除 runtime 下 `public` 软链与 `DIOXUS_ASSET_DIR`，env 增加 `LEPTOS_SITE_ROOT: <site 绝对路径>`；server 仍以 runtime 目录为 cwd（`hash.txt` 从可执行文件目录 `target/debug/` 读取）。
    - `scripts/test-databases.sh`：四条 `cargo test` 去掉 `--all-features`。
16. **验收**
    - `cargo clippy --locked --all-targets -- -D warnings` 与 `cargo clippy --locked --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings` 通过；`cargo test --locked` 通过（此时不含 UI 模块测试）。
    - `make regression-build` 成功，`target/site/pkg/` 下有 `yggdrasil.<hash>.wasm`，`target/debug/hash.txt` 存在。
    - 冒烟：`DATABASE_URL=$SMOKE_DB_URL APP_BASE_URL=http://127.0.0.1:18080 PORT=18080 LEPTOS_SITE_ROOT=$PWD/target/site target/debug/yggdrasil`，然后：`curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:18080/readyz` → `200`；`curl -s -X POST -H 'content-type: application/json' -H 'Origin: http://127.0.0.1:18080' -d '{}' http://127.0.0.1:18080/api/get_current_user` → `200` 且 body 为 `{"user":null}`；`curl -sI http://127.0.0.1:18080/style.css` → `200`；`curl -s http://127.0.0.1:18080/` 的 HTML 含 `/pkg/yggdrasil.` 与 `rel="modulepreload"`。
    - 提交：`refactor(server): 服务端层迁移到 Leptos server fn 与 leptos_axum`。

## Phase 2A：前端基础契约（单代理，顺序执行）

目标：路由表、导航/视图过渡握手、主题、全局上下文、UI 原子、表单、骨架屏、前台布局、浏览器库桥接全部就位；其余页面以桩占位。完成后前台外壳可直接打开、主题切换可用、站内导航走视图过渡。

### 文件归属
`src/lib.rs`（追加模块声明）、`src/app.rs`、`src/routes.rs`（新）、`src/navigation.rs`、`src/theme.rs`、`src/context.rs`、`src/hooks/*`、`src/bridges/*`（全部 6 个）、`src/utils/web_upload.rs`、`src/components/mod.rs`、`src/components/ui.rs`、`src/components/ui/*`（12 个）、`src/components/{forms,empty_state,enhance_assets,share_meta,nav,header,footer,frontend_layout,palette,search_palette}.rs`、`src/components/skeletons/*`（含新增 `notes_route_skeleton.rs`）、`src/pages/mod.rs`、`src/pages/not_found.rs`、全部页面入口桩（见第 9 步）、`src/pages/admin/mod.rs`（桩）、`libs/yggdrasil-core/src/{route-transitions,anchor-click,index}.ts` 及其测试；删除 `src/router.rs` 与 `src/router/`。

### 步骤
1. **`src/lib.rs`** 追加 `pub mod bridges; pub mod components; pub mod context; pub mod hooks; pub mod navigation; pub mod pages; pub mod routes; pub mod theme;`。删除 `src/router.rs`、`src/router/routes.rs`（其 `flavor_tests` 随之删除）。
2. **`src/routes.rs`**：只做 href 生成的 `pub enum Route`，变体与字段取自 `git show BASE:src/router/routes.rs`，去掉 `NotFound`、去掉 Dioxus 派生与 `#[route]`/`#[layout]`/`#[nest]`/`#[redirect]` 属性，派生 `Clone, Debug, PartialEq`。变体：`Home, HomePage{page:i32}, Notes, NotebookDetail{id:i32}, NoteDetail{slug:String, book:String}, Archives, TagDetail{tag:String}, PostDetail{slug:String}, Search{q:String}, About, WritingGuide, ComponentShowcase, ComponentDetail{component:String}, Friends, Changelog, Admin, Write, WriteEdit{id:i32}, PostPreview{slug:String}, Posts, AdminNotes, NewNote, EditNote{id:i32}, AdminNotebooks, PostsTrash, Assets, FriendsAdmin, AdminComments, AdminCommentsPage{page:i32}, System, Runner, Mcp, Logs, SiteSettingsPage, Profile, Login, Register`。
   - `impl Display`：路径与原 `#[route]` 字符串一致；动态路径段用 `percent_encoding::utf8_percent_encode(seg, PATH_ASCII_SET)`，查询值用 `QUERY_ASCII_SET`。两个编码集照抄 dioxus-router 0.7.10：`QUERY_ASCII_SET = CONTROLS.add(b' ').add(b'"').add(b'#').add(b'<').add(b'>')`，`PATH_ASCII_SET = QUERY_ASCII_SET.add(b'?').add(b'^').add(b'`').add(b'{').add(b'}')`。查询参数值为空时整个 `?key=` 省略（`Search{q:""}` → `/search`，`NoteDetail{book:""}` → `/notes/<slug>`）。
   - 辅助函数：`pub fn is_post_path(path: &str) -> bool`（`/post/` 前缀）、`pub fn is_showcase_path(path: &str) -> bool`（`/about/components` 或其子路径），替代原 `route_is_showcase`。
   - 单元测试（`#[cfg(test)]`）：`TagDetail{tag:"Rust 编程"}` → `/tags/Rust%20%E7%BC%96%E7%A8%8B`；`PostDetail{slug:"a?b"}` → `/post/a%3Fb`；`Search{q:"a b"}` → `/search?q=a%20b`；`Search{q:""}` → `/search`；`AdminCommentsPage{page:2}` → `/admin/comments/2`；`NoteDetail{slug:"x", book:"3"}` → `/notes/x?book=3`。
   - 全仓库 `Link { to: Route::.. }`（104 处）在各自文件移植时改为 `<a href=Route::...to_string()>`。
3. **导航桥接**
   - `src/bridges/navigation.rs`：保留现有 extern 声明与 `read_state/write_state/entry_id/clear_admin_state/navigation_id/rendered` 封装；新增封装 `push(route)`、`replace(route)`、`back()`、`forward()`、`external(url) -> bool`、`connect(notify: &Closure<dyn FnMut()>)`（`prefix` 传 `None`）。
   - `src/navigation.rs`（整体重写，删除 `TransitionHistory`、`transition_history` 与原测试）：
     - `pub fn push(route: &str)` / `replace(route: &str)`：wasm 下若 `CONNECTED`（模块内 `static CONNECTED: AtomicBool`）为真则调用 JS `push/replace`，否则 `window.location.assign(route)`（replace 用 `location.replace`）；服务端空操作。`back/forward/external` 同理直通 JS，模块缺失时用 `history.back()/forward()`、`location.assign`。
     - `#[component] pub fn NavigationBridge() -> impl IntoView`：放在 `<Router>` 内、`<Routes>` 前，不渲染 DOM。`#[cfg(feature = "hydrate")]` 下 `Effect::new` 只执行一次：取 `use_context::<leptos_router::location::BrowserUrl>()`，构造 `Closure<dyn FnMut()>`，闭包体为 `if let Ok(url) = BrowserUrl::current() { browser_url.as_url().set(url) }`（需 `use leptos_router::location::LocationProvider`），调用 `connect(&closure)` 成功后置 `CONNECTED = true`；closure 存进 `StoredValue::new_local`，`on_cleanup` 里调用 JS `disconnect()` 并置 `CONNECTED = false`。
     - `#[component] pub fn RouteMarker(is_routing: ReadSignal<bool>, children: Children) -> impl IntoView`：渲染 `<div style="display: contents" data-vt-route=<首帧路由> data-vt-navigation="0" node_ref=marker>{children()}</div>`；首帧路由 = `use_location()` 的 `pathname` + （`search` 非空时 `"?" + search`），用 `get_untracked` 计算一次，服务端与客户端一致。`#[cfg(feature = "hydrate")]` 下 `Effect::new(move |_| { let route = <同上，追踪 pathname 与 search>; if is_routing.get() { return; } request_animation_frame(move || { if is_routing.get_untracked() { return; } let id = bridges::navigation::navigation_id(); <通过 marker 直接 set_attribute("data-vt-route", &route) 与 ("data-vt-navigation", &id.to_string())>; bridges::navigation::rendered(id, &route); }) })`。属性用 `set_attribute` 直接写，不经响应式绑定，保证 `rendered` 调用时 DOM 已带正确标记；rAF 内二次检查 `is_routing` 是为了避开“URL 已变、路由器还没来得及把 is_routing 置真”的微任务窗口——放弃的这一次会在 `is_routing` 回落时由 Effect 重新触发。
   - **JS 改动（`libs/yggdrasil-core/src/route-transitions.ts`）**：
     - `connect()`：`window.addEventListener('click', this.onClick, true)` 改为 `document.addEventListener('click', this.onClick)`（冒泡阶段，晚于元素自身的 Leptos 处理器、早于 Leptos 挂在 window 上的链接拦截）；`window.addEventListener('popstate', this.onPop)` 改为 `window.addEventListener('popstate', this.onPop, true)`；`disconnect()` 对应移除（同样的 target 与 capture 参数）。
     - `onPop` 签名改为 `(event: PopStateEvent)`，函数第一行 `event.stopImmediatePropagation();`（捕获阶段先于 Leptos 的 popstate 监听，阻止它绕过视图过渡直接改 URL）。
     - `onClick` 重写为：未连接（`!this.notify || !this.active`）、`event.defaultPrevented`、非左键或带 meta/ctrl/alt/shift → 直接返回；`anchor = target.closest('a[href]')`；以下任一成立则返回：无 anchor、`anchor.target` 非空且不是 `_self`、有 `download`、`anchor.relList.contains('external')`、`anchor.origin !== location.origin`、`anchor.pathname` 以 `NATIVE_PREFIXES = ['/api/', '/doc/', '/uploads/', '/og/', '/pkg/', '/note-media/', '/feed.', '/mcp']` 中任一开头；否则 `event.preventDefault()`，按原逻辑设置 `this.intent = { url, post: anchor.dataset.vtPostLink, returning: anchor.dataset.vtReturn === 'true' }`，然后 `this.request(url, 'push')`。删除 `intentTimer` 字段及所有 `clearTimeout(this.intentTimer)`。
     - 文件头与注释中“Dioxus”说明改为 Leptos 语义（JS 拥有 history，Rust 通过 `connect` 的 notify 读取地址栏）。`anchor-click.ts`、`hash-scroll.ts`、`index.ts` 只改注释，行为不变。
     - 更新 `route-transitions.test.ts` 与 `anchor-click.test.ts`：点击测试改为断言“文档冒泡阶段点击站内链接触发 push 并 preventDefault；`rel=external`、`/feed.xml`、带 `target=_blank`、已 `defaultPrevented` 的点击不拦截；popstate 监听在捕获阶段并调用 `stopImmediatePropagation`”。
   - **组件规则**：带 `on:click` 的 `<a>` 只做副作用，不调用 `prevent_default`（否则 JS 不接管、也就没有视图过渡意图）；需要取消导航时才 `prevent_default`。
4. **`src/theme.rs`**：以 `spike/leptos/src/theme.rs` 为起点，对照 BASE 补齐（`use_theme_provider` → `provide_theme()`，`use_theme()`/`use_resolved_theme()` 读 context，`ThemePreload`、`ThemeToggle`、`choose_theme` 的视图过渡调用 `__startThemeTransition` 保持）。`ThemePreload` 加进 `shell()` 的 `<head>`（在 `yggdrasil-core.js` 之后、`AutoReload` 之前）。
5. **`src/context.rs`**：`#[derive(Clone, Copy)] pub struct UserContext { pub user: RwSignal<Option<Arc<PublicUser>>>, pub checked: RwSignal<bool> }` + `UserContext::new()`。
6. **`src/app.rs` 完整版**：先把 `ShowcaseFilters`（`git show BASE:src/pages/components_showcase.rs` 第 32-38 行，结构体与 `Default`）移到 `src/context.rs`，Phase 4 从那里引用。`App` 依次 `provide_meta_context()`、`crate::theme::provide_theme()`、`provide_context(UserContext::new())`、`provide_context(RwSignal::new(ShowcaseFilters::default()))`、`crate::components::ui::provide_toaster()`；`let (is_routing, set_is_routing) = signal(false);`；`#[cfg(feature = "hydrate")] Effect::new(|_| document().document_element() 上 set_attribute("data-hydrated", "true"))`；视图：`<Title text="Yggdrasil Blog"/>`、`<Router set_is_routing><NavigationBridge/><RouteMarker is_routing><Routes fallback=..>`（完整路由表见“路由表”）`</Routes></RouteMarker></Router>`、`<ToastRegion/>`。删除 Phase 1 的 `MIGRATION_STUB` 注释。
7. **hooks 与桥接**：`hooks/{query,near_viewport,event_listener}.rs` 按对照表移植（签名改为 Leptos 信号：`use_paginated` 返回的 `PaginatedState<T>` 字段改为 `RwSignal`，`use_near_viewport(target_id: String, root_margin: &'static str) -> ReadSignal<bool>`，`use_event_listener` 用 `on_cleanup` 移除监听）；`bridges/library.rs` 用原型版本（Effect + spawn_local，不用 LocalResource）；`bridges/{tiptap,codemirror,xterm}.rs` 与 `utils/web_upload.rs` 按 BASE 完整移植（tiptap 以 spike 子集为起点补齐全部 variant），JS 句柄持有者用 `StoredValue::new_local`，`Drop` 里 destroy 的语义保留。
8. **组件**：
   - `components/mod.rs` 写成最终声明（去掉全部 `ygg_client_reader` 门控）：`empty_state, enhance_assets(pub(crate)), forms, ui, skeletons, admin_layout, admin_palette, footer, frontend_layout, header, nav, palette, search_palette, share_meta, assets, code_runner, comments, post, sql_result_table`。其中 Phase 2B 负责的 `admin_layout.rs`、`admin_palette.rs`、`sql_result_table.rs`、`assets/mod.rs`、`code_runner/mod.rs`、`comments/mod.rs`、`post/mod.rs` 先写成只含 `// MIGRATION_STUB` 注释的空文件（不声明子模块）。
   - 移植 `ui/*`（含 BASE 有而 spike 缺的 `menu.rs`、`surfaces.rs`、Toast：`provide_toaster()` 替代 `use_toast_provider()`，`use_toast()` = `expect_context::<Toaster>()`，自动消失用 `set_timeout`）、`forms.rs`、`empty_state.rs`、`skeletons/*`、`enhance_assets.rs`、`share_meta.rs`（`use_share_context()` 服务端 = `crate::request_ctx::request_extension::<ShareContext>().unwrap_or_default()`，客户端 = `ShareContext::default()`）、`nav.rs`、`header.rs`、`footer.rs`、`palette.rs`、`search_palette.rs`、`frontend_layout.rs`。
   - `ui/display.rs`、`ui/icons.rs`、`ui/menu.rs` 里用 `dioxus::ssr::render_element` 的测试改成 Leptos SSR：`Owner::new().with(|| view! { .. }.to_html())`，断言内容不变（tooltip 的 aria/定位属性、图标 `aria-hidden`/`role="img"`、关闭的菜单只渲染触发器）。纯数据测试原样保留。
   - `frontend_layout.rs`：`#[component] pub fn FrontendLayout(#[prop(optional)] children: Option<ChildrenFn>) -> impl IntoView`；有 children 时渲染 children，否则渲染 `<Outlet/>`；`<Outlet/>`/children 外层依次包 `<ErrorBoundary fallback=..>`（规则见“错误规则”）与 `<Suspense fallback=move || route_skeleton(&pathname)>`；`route_skeleton` 改为按路径字符串匹配（逻辑同 BASE 第 26-68 行，图鉴分支不再有 cfg）；宽度判断用 `routes::is_post_path`、`routes::is_showcase_path`；`FrontendShell` 保留（图鉴预览复用）。
   - `search_palette.rs`：`SearchPalette` 外壳留在主包；面板主体抽成 `#[leptos::lazy] fn search_palette_body(<具体参数>) -> AnyView`，在 `<Suspense>` 内 `move || open.get().then(|| Suspend::new(async move { search_palette_body(..).await }))` 首次打开时才加载分包。
   - `NotesRouteSkeleton`：从 `git show BASE:src/pages/notes.rs` 移到 `src/components/skeletons/notes_route_skeleton.rs`，参数改为 `path: String`（按路径区分列表/笔记本/单篇）。
   - 骨架屏依赖的页面头部组件 `TagIntro`（pages/tags.rs）、`ChangelogHeader`（pages/changelog.rs）、`FriendsHeader`（pages/friends.rs）、`WritingHeader`（pages/writing_guide.rs）在本阶段就移植到对应页面文件里（与第 9 步的桩共存），Phase 3 保留它们。
9. **页面**：`pages/mod.rs` 写最终声明（`about, admin, archives, changelog, components_showcase, friends, home, login, not_found, notes, post_detail, register, search, tags, writing_guide`，无 cfg）。`pages/not_found.rs` 完整移植（以 spike 为起点；`set_status(404)`；“返回首页”用 `navigation::push("/")`；推荐列表保留），并新增 `PageLoadFailed`。其余每个路由入口文件（路由表“文件”列，含 `pages/admin/mod.rs` 的 `AdminShellRoute` 与 `pages/admin/notes/notebooks.rs`）写成桩：定义路由表里的类型名，`#[lazy_route] impl LazyRoute`，`data()` 返回 `Self`，`view()` 渲染 `view! { <p>"迁移中"</p> }.into_any()`（`AdminShellRoute` 渲染 `<Outlet/>`），文件顶部 `// MIGRATION_STUB`。`pages/admin/mod.rs` 写最终模块声明：`assets, comments, dashboard, friends, logs, mcp, note_drafts(私有), note_editor, notes, posts, posts_trash, preview, profile, runner, settings, system, write`；桩文件**不**声明自己的子模块（例如桩 `write.rs` 不写 `mod cover;`），`note_drafts.rs` 写成空桩。
10. **验收**
    - 两条 clippy、`cargo test --locked`（含 `routes` 测试与改写后的 ui SSR 测试）、`cd libs && bun run --filter yggdrasil-core test` 全过。
    - `make regression-build` 后用冒烟库启动（命令同 Phase 1），在浏览器（Playwright 或 omp browser）打开 `/`：页头、页脚、导航、主题切换可用，`document.documentElement.dataset.hydrated === 'true'`，控制台无错误。
    - 在页面执行 `window.__marker = 1` 后点击页头“归档”链接：URL 变为 `/archives`，`window.__marker === 1`（无整页刷新），过渡期间 `<html>` 出现过 `is-route-transitioning`；再 `history.back()` 回到 `/`，内容随之切换。
    - `curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:18080/no-such-page` → `404`，页面带页头页脚。
    - 提交：`refactor(ui): 建立 Leptos 前端骨架、路由表与视图过渡导航桥接`。

### Phase 2A 追加：测试脚本适配（与上面同一提交）
- `scripts/test-public-performance.cjs`：`build` 常量改为服务端 `process.env.PUBLIC_TEST_SERVER || <root>/target/debug/yggdrasil`、站点根 `process.env.PUBLIC_TEST_SITE || <root>/target/site`；存在性断言改查服务端可执行文件；env 删除 `DIOXUS_ASSET_DIR`、新增 `LEPTOS_SITE_ROOT`；两处 `window.__dx_mainWasm` 等待（`:136`、`:159`）改为 `document.documentElement.dataset.hydrated === 'true'`。
- `scripts/test-view-transitions.cjs:403-412`：删除从 wasm 正文里抠 `/api/logout\d+` 的逻辑，`logoutEndpoint = '/api/logout'`。
- `scripts/test-components-showcase.cjs` 的 `SENSITIVE`/`NORMAL_FRONTEND_READS` 不用改（`apiName` 会去掉下划线并转小写，`/api/get_current_user` 仍映射为 `getcurrentuser`）。

## Phase 2B：共享业务组件（3 个并行切片，worktree）

通用要求：遵守“迁移约定”；DOM 结构、`id`、`class`、`data-*` 属性、可见文案与 BASE 保持一致（CSS、浏览器回归脚本依赖它们）；完成后删除各自文件里的 `MIGRATION_STUB`。

| 切片 ID | 文件归属 | 要点 |
|---|---|---|
| `admin-shell` | `src/components/admin_layout.rs`、`src/components/admin_palette.rs`、`src/pages/admin/mod.rs` | `AdminShellRoute::view` 渲染 `<AdminLayout/>`；`AdminLayout` 内部保留原登录态校验、侧栏、`SidebarControl`/`AdminActions` 上下文，`<Outlet/>` 外包 `<Suspense fallback=<按路径选 admin 骨架屏>>`（逻辑同 BASE 中按 `Route` 选骨架屏的 match，改为按路径字符串）；退出登录调用 `bridges::navigation::clear_admin_state()` 后 `navigation::push("/login")`；未登录跳转用 `navigation::replace("/login")`。 |
| `assets-sql` | `src/components/assets/*`、`src/components/sql_result_table.rs` | 上传用 `utils::web_upload`；上传池/选择器的 JS 句柄与定时器在 `on_cleanup` 里释放。 |
| `article` | `src/components/post/*`、`src/components/comments/*`、`src/components/code_runner/*` | 评论区按原型 `spike/leptos/src/components/comments/*` 起步，补齐 BASE 功能（图片上传走 `/api/comments/upload`、待审轮询、回复）；评论数据与登录态用 Effect + 镜像信号（数据加载规则 3）。代码运行器：静态代码块与 `use_near_viewport` 激活判断留在调用方所在包，激活后的编辑器 + 终端 + 执行逻辑抽成 `#[leptos::lazy] fn code_runner_body(<具体参数>) -> AnyView`，在 `<Suspense>` 内 `Suspend::new(async move { code_runner_body(..).await })` 加载；`.code-runner-static` 与 `.cm-editor` 的出现时机必须与 BASE 一致（`test-public-performance.cjs:141-152` 断言它们）。 |

合并后编排者验收：两条 clippy 与 `cargo test --locked` 通过；`make regression-build` 后在冒烟库上打开 `/admin`（先 `psql` 往 `sessions` 插入一条会话，token 的 SHA-256 写入 `token_hash`，与 `scripts/regression.cjs:90-92` 同法），从 `/admin` 点击侧栏进入 `/admin/posts`：先给侧栏根元素（带 `data-vt-shell` 的元素）挂 `el.__marker = 1`，跳转后同一元素仍带 `__marker`（验证懒加载父路由不重建）。提交：`refactor(ui): 移植后台外壳、素材与文章/评论/代码运行组件`。

## Phase 3：页面（6 个并行切片，worktree）

通用要求：每个路由类型的名称、路径、`SsrMode` 严格按“路由表”；SSR 数据在 `LazyRoute::data()` 里建 `Resource`；`<Title>` 文案与 BASE `document::Title` 一致；按历史记录保存的页面状态继续用 `bridges::navigation::{read_state, write_state, entry_id}`（键名不变：`archive-tags-open`、`notes`、`search`、`admin-notes`、`admin-posts`）；离开保护 `use_draft_protection` 继续用 `set_leave_guard/clear_leave_guard`；DOM/文案/`data-vt-*` 标记保持 BASE 原样；删除对应文件的 `MIGRATION_STUB`，并在入口文件里重新声明各自子模块。

| 切片 ID | 文件归属 | 特别说明 |
|---|---|---|
| `public` | `src/pages/{home,archives,tags,about,writing_guide,friends,changelog,post_detail}.rs` | `HomeRoute` 同时服务 `/` 与 `/page/:page`（无参数视为第 1 页）；首页 5 个 SSR 数据源都在 `data()` 里建；`TagRoute` 的“按标签重建”用 keyed 写法；保留 Phase 2A 已移植的 `TagIntro`/`ChangelogHeader`/`FriendsHeader`/`WritingHeader`。 |
| `notes-search` | `src/pages/{notes,search}.rs` | `NotesRoute`/`NotebookRoute`/`NoteRoute` 三个类型同在 `notes.rs`；`NoteRoute` 读 `use_query_map()` 的 `book`；`SearchRoute` 读 `q`，搜索请求留在客户端（Effect + 防抖），输入框聚焦用 `NodeRef`。 |
| `admin-misc` | `src/pages/{login,register}.rs`、`src/pages/admin/{dashboard,profile,friends,comments,runner}.rs`、`src/pages/admin/mcp.rs`+`mcp/*`、`src/pages/admin/logs.rs`+`logs/*` | 登录成功后更新 `UserContext` 并 `navigation::push("/admin")`；日志 SSE 的 `EventSource` 在 `on_cleanup` 里关闭；`AdminCommentsRoute` 读可选 `page` 参数。 |
| `admin-content` | `src/pages/admin/{write,preview,posts,posts_trash,assets}.rs`、`write/*`、`assets/*` | `use_paginated` 用 Phase 2A 移植版；`preview.rs` 里针对 Dioxus 挂起子树问题的绕路注释与写法删除，按数据加载规则 2 实现。 |
| `admin-notes` | `src/pages/admin/{notes,note_editor,note_drafts}.rs`、`notes/notebooks.rs`、`note_editor/*` | `NotebooksRoute` 在 `notes/notebooks.rs`，由 `admin/notes.rs` 声明 `pub mod notebooks;`。 |
| `admin-system` | `src/pages/admin/system/*`、`src/pages/admin/settings/*` | `use_future` 自动刷新（`system/mod.rs:78`）改为 Effect + `set_interval`，`on_cleanup` 清除；`SettingsBus` 用 `provide_context`；`settings/mod.rs:291-295` 的 `data-app-anchor` 保留。 |

合并后编排者验收：
- `grep -rn MIGRATION_STUB src` 只剩 `src/pages/components_showcase.rs`；`grep -rnE 'dioxus::|use dioxus|rsx!' src` 只剩 `src/pages/components_showcase*`（`src/cache.rs` 测试里的搜索词字符串 `"dioxus fullstack"` 不算框架引用，保留）。
- 两条 clippy、`cargo test --locked`、`bun run --sequential --filter '*' test`（在 `libs/`）通过。
- `make regression-build`，然后 `make test-browser BROWSER_SUITE=<套件>` 依次跑 `smoke admin notes backup runner slash lightbox` 全过；`make test-databases` 通过；`bun scripts/test-public-performance.cjs` 通过。
- 冒烟库上启动服务，`VT_BASE=http://127.0.0.1:18080 bun scripts/test-view-transitions.cjs` 通过（`APP_BASE_URL` 必须与 `VT_BASE` 同源，否则 CSRF 返回 403）。
- `curl -s http://127.0.0.1:18080/post/<冒烟库中任一已发布 slug>` 的 HTML `<head>` 内含该文章标题的 `<title>` 与 `og:title`；`/post/does-not-exist` 返回 404；`/archives` 的 HTML 里直接包含文章条目（不是骨架屏，验证 `InOrder`）。
- 提交：`refactor(pages): 前台与后台页面迁移到 Leptos`。

## Phase 4：组件图鉴（单代理）

文件归属：`src/pages/components_showcase.rs`、`src/pages/components_showcase/*`、`src/pages/components_showcase_data.json`。
1. `ShowcaseRoute`/`ShowcaseDetailRoute` 按路由表实现；`get_showcase_data` 的裸 `#[server]` 改为 `#[server(endpoint = "get_showcase_data", input = leptos::server_fn::codec::Json)]`；`ShowcaseFilters` 从 `crate::context` 引用；缺失组件时 `set_status(404)`。
2. 预览分派 `preview_route`/各 `previews` 模块返回 `Option<AnyView>`；所有预览继续复用生产组件 + 本地 fixture，不得发出鉴权/上传/评论/执行请求，卸载时清理浏览器实例（`on_cleanup`）。
3. JSON：每个组件的 `code` 片段改写为 Leptos `view!` 用法；`props` 表中的类型名按对照表更新（`EventHandler<T>` → `Callback<T>`、`Element` → `Children`/`ViewFn`、`Signal<T>` → `RwSignal<T>`/`Signal<T>`）；slug、group、`preview_mode`、`usage_kind` 不变。
4. 保留并通过 `#[cfg(test)]` 里 6 个目录测试（`components_showcase.rs:1828-2034`）；若迁移在 `src/components/` 下新增了 `pub fn` 大写组件，要么补目录条目，要么改为 `pub(crate)`。
5. 验收：`grep -rnE 'dioxus::|use dioxus|rsx!|MIGRATION_STUB' src` 为空；两条 clippy、`cargo test --locked`；`make regression-build && make test-browser BROWSER_SUITE=showcase`；冒烟库上 `SHOWCASE_BASE=http://127.0.0.1:18080 bun scripts/test-components-showcase.cjs` 通过。提交：`refactor(showcase): 组件图鉴迁移到 Leptos`。

## Phase 5：构建、发布、Docker 与 CI（并行切片 `release`，worktree）

Phase 2A 合入后开工（worktree 从当时的 `migrate/leptos` 拉出），与 Phase 2B/3/4 并行；文件与 UI 切片不重叠。按“阶段顺序”在 Phase 4 之后合入，完整验收放在 Phase 6。

文件归属：`Makefile`、`mk/dx-cache.mk`（删除）、`mk/docker.mk`、`scripts/build-release.cjs`（重写）、`scripts/test-release-split.cjs`（新）、待删除脚本、`Dockerfile`、`Dockerfile.cross`、`Dockerfile.dev`、`docker-compose.dev.yml`、`docker-compose.tools.yml`、`.dockerignore`、`.github/workflows/ci.yml`、`.env.example`、`libs/{mermaid-renderer,codemirror-editor,xterm-terminal}/vite.config.ts` 与 `libs/tiptap-editor/src/math.ts`（只改注释）。

1. **Makefile**
   - 新变量：`RELEASE_OUT ?= target/release-dist`；`LEPTOS_WASM_ENV := RUSTUP_TOOLCHAIN=$(WASM_NIGHTLY) CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS='--cfg getrandom_backend="wasm_js" -Zunstable-options -Cpanic=immediate-abort'`；`LEPTOS_WASM_ARGS := --lib-cargo-args=-Zbuild-std=std,panic_abort --lib-cargo-args=-Zbuild-std-features=optimize_for_size`。删除 `WASM_CARGO_ARGS`、`CLIENT_SERVER_ARGS`、`RELEASE_SERVER_ARGS`、`RELEASE_ASSEMBLY`。
   - 新目标 `build-frontend`：`check-wasm-toolchain` 后执行 `$(LEPTOS_WASM_ENV) cargo leptos build --release --split --frontend-only $(LEPTOS_WASM_ARGS)`。
   - `build`：`check-build-tools build-assets doc build-frontend` → `cargo build --release --locked --bin yggdrasil` → `bun scripts/build-release.cjs --out $(RELEASE_OUT) --server target/release/yggdrasil` → `$(MAKE) precompress` → `bun scripts/build-release.cjs --verify --out $(RELEASE_OUT)`。
   - `build-linux`：同 `build`，服务端改为 `cargo build --release --locked --bin yggdrasil --target x86_64-unknown-linux-musl`，`--server target/x86_64-unknown-linux-musl/release/yggdrasil`；删除第 151 行 `DIOXUS_ASSET_DIR` 提示，改为“运行时 `hash.txt` 必须与 `server` 同目录，`public/` 由 `LEPTOS_SITE_ROOT` 指定或放在可执行文件旁”。
   - `precompress`：查找根改为 `$(RELEASE_OUT)/public`（扩展名列表不变）。
   - 删除目标：`build-client`、`build-full-and-server`、`build-client-reader`、`snapshot-flavor`、`restore-webp`、`test-release-bootstrap`、`test-wasm-preload`；`test` 目标去掉 `test-release-bootstrap` 依赖；删除 `include mk/dx-cache.mk` 与该文件。`check-dev-tools` 删除 llvm-tools 检查（只有 dx 用它）。
   - 新目标 `test-release-split`：依赖 `build`，执行 `bun scripts/test-release-split.cjs`。
   - `doc` 目标的 `cargo doc` 加 `--lib`；删除 313-324 行“docs 由 Dioxus 静态目录托管”的注释，改为“`/doc` 由 `site_fallback` 的 ServeDir 从站点根提供”。
2. **`scripts/build-release.cjs` 重写**（保留 `#!/usr/bin/env bun`、原子替换与 `release-manifest.json` 结构思路）：
   - 参数：`--out`（默认 `target/release-dist`）、`--site`（默认 `target/site`）、`--hash`（默认 `target/release/hash.txt`）、`--server`（可选；给出时复制为 `<out>/server`）、`--verify`。
   - 组装：在 `<out>/.staging-<pid>` 里复制 `site` → `public/`、`hash` → `hash.txt`、可选 server；写 `public/release-manifest.json`：`{ release_id: git describe --tags --always --dirty, generated_at, source: { git_hash, dirty, cargo_lock_sha256, cargo_lock_bytes }, hash_txt: { js, wasm, manifest, split }, assets: [{ path: "/pkg/<file>", sha256, bytes }] }`（`assets` 覆盖 `public/pkg/` 下全部非 `.br` 文件，按路径排序，2 空格缩进 + 结尾换行）；然后按原脚本方式原子替换 `<out>` 下的 `public/`、`hash.txt`、`server`。`site` 中存在 `index.html` 时直接失败（防止混入旧 dx 产物）。
   - `--verify`：manifest 中每个资产存在且 sha256/字节数一致；`hash.txt` 的 `js`/`wasm` 指向的 `pkg/yggdrasil.<hash>.{js,wasm}` 存在；有 `manifest` 行时 `pkg/__wasm_split_manifest.<hash>.json` 存在，且其中引用的每个文件都在 `pkg/` 下；`pkg/` 里每个 `.js/.wasm/.json/.css` 都有 `.br` 旁车；任一失败 `process.exit(1)` 并列出原因。
3. **删除脚本**：`scripts/release-bootstrap.cjs`、`scripts/release-bootstrap.test.cjs`、`scripts/test-wasm-preload.cjs`、`scripts/test-dual-flavor.cjs`、`scripts/measure-release.cjs`、`scripts/test-wasm-bindgen-metadata.cjs`（全部是 dx 产物/双 flavor 专用）。
4. **`scripts/test-release-split.cjs`（新，Playwright + Bun）**，以 `spike/leptos/scripts/verify.cjs` 与 `measure.cjs` 为参考，基础设施抄 `scripts/regression.cjs`（临时 `postgres:18-alpine` 容器、`createdb`、空闲端口、`/readyz` 等待、退出清理）：
   - 启动 `<RELEASE_OUT>/server`（cwd 为临时目录，env：`DATABASE_URL`、`IP=127.0.0.1`、`PORT`、`APP_BASE_URL`、`COOKIE_SECURE=false`、`SSR_CACHE_SECS=60`、`LEPTOS_SITE_ROOT=<RELEASE_OUT>/public`、`MCP_TOKEN_ENC_KEY='42'.repeat(32)`、`ADMIN_*=''`、`YGG_PREVIOUS_PUBLIC_DIR=<临时目录>/previous-public`）。`previous-public/pkg/legacy_chunk.AbCdEfGhIjKlMnOpQrStUv.wasm` 预先写入任意 16 字节。
   - 种子数据（psql）：管理员用户 + 会话（同 `regression.cjs:90-92`）；3 篇已发布文章 `split-a`/`split-b`/`split-c`（带标签 `split`，正文含一个可运行代码块 ```` ```python ````）。
   - 断言（分包文件名由 `LazyRoute` 类型名的 snake_case 派生，原型实测 `HomeRoute` → `home_route_view.<hash>.wasm`、`PostDetailRoute` → `post_detail_route_view.<hash>.wasm`；主包 = `hash.txt` 的 `wasm` 行对应的 `yggdrasil.<hash>.wasm`。若实际文件名与下列子串不符，以 `pkg/__wasm_split_manifest.<hash>.json` 的键名为准改写子串，断言语义不变）：
     1. 直接打开 `/`：等待 `data-hydrated`；`.wasm` 请求含主包与文件名含 `home_route` 的分包，不含文件名含 `post_route`、`admin`、`login`、`showcase` 的分包；无 `pageerror`/console error；无 ≥400 响应。
     2. 在 `/` 设 `window.__marker = 1` 后点击 `split-a` 文章链接：URL 变为 `/post/split-a`，`__marker` 仍在，新增的 `.wasm` 请求含文件名含 `post_route` 的分包，且没有再次请求主包。
     3. `GET /post/split-a` 的 HTML `<head>` 含 `<title>` 文章标题；`GET /post/no-such` 状态 404；第二次 `GET /post/split-a` 带 `x-ssr-cache: HIT`，打开该缓存页仍能水合（`data-hydrated`）。
     4. 带会话 cookie 打开 `/admin/posts`：`.wasm` 请求含文件名含 `admin` 的分包；不带 cookie `GET /admin` 返回 302 且 `location: /login`。
     5. `GET /pkg/legacy_chunk.AbCdEfGhIjKlMnOpQrStUv.wasm` → 200、`x-previous-release: 1`、`cache-control` 含 `immutable`。
     6. **体积门槛**：对 `/`、`/post/split-a`、`/archives`、`/tags/split`、`/notes`、`/about`、`/friends`、`/changelog`、`/search` 各开一个全新 context 直接访问，等待 `data-hydrated` 与 network idle，收集所有 `/pkg/*.wasm` 请求，按 `<site>/pkg/<file>.br` 的字节数求和；每条 ≤ `594603`。结果写入 `target/regressions/<时间戳>-release-split/sizes.json`（`{ route: { files: [..], brotli_bytes } }`），超标时测试失败并打印各路由明细。
5. **Dockerfile**
   - 删除 dx 安装段（`ARG DX_VERSION` 及 125-145 行）、`binaryen` apt 包与相关注释、`ENV DX_HOME` 与 `make esbuild-cache wasm-bindgen-cache`。在 tailwind 安装之后新增 `RUN cargo install cargo-leptos --version 0.3.11 --locked`（独立层，便于缓存）。
   - 把 291-300 行的 dx 构建 RUN 改为（保留三个 cache mount）：`make build-frontend && cp target/release/hash.txt /build/hash.txt && bun scripts/build-release.cjs --out /build/dist --hash /build/hash.txt`（`--site` 用默认 `target/site`；hash 先拷出是因为 `/build/target` 是 cache mount）。
   - 302-312 行 `make doc DOC_OUT=dist/public/doc` 保留；322-326 行 brotli 保留；其后新增 `RUN bun scripts/build-release.cjs --verify --out /build/dist`。
   - musl 服务端构建命令改为 `cargo build --release --locked --target "$MUSL_TARGET" --bin yggdrasil`（默认 feature 即 server）；cargo-chef `cook` 的 feature 参数同步去掉 `--no-default-features --features server`。
   - 运行阶段新增 `COPY --from=builder --chown=65534:65534 /build/dist/hash.txt /app/hash.txt`；`ENV DIOXUS_PUBLIC_PATH=/app/public` 改为 `ENV LEPTOS_SITE_ROOT=/app/public`。
6. **Dockerfile.cross**：前端 stage 同上替换 dx 段；server stage 的 `cargo zigbuild` 改为 `--bin yggdrasil`、去掉 `--no-default-features --features server`；最终 stage 复制 `hash.txt` 到 `/app/hash.txt`、`ENV LEPTOS_SITE_ROOT=/app/public`；删除 359-362 行 `SERVER_FN_OVERRIDE_KEY` 说明。
7. **Dockerfile.dev / docker-compose**：`Dockerfile.dev` 把 dx 安装换成 `cargo install cargo-leptos --version 0.3.11 --locked`，删除 `DX_HOME` 与工具缓存步骤，`CMD ["make", "dev"]` 不变；`docker-compose.dev.yml` 的 watch sync 列表删除 `./Dioxus.toml`，删除 116-120 行 `SERVER_FN_OVERRIDE_KEY` 注释，端口额外暴露 `3001`（cargo-leptos 热重载 websocket）；`docker-compose.tools.yml` 与 `mk/docker.mk` 的 `--all-features` 全部去掉。
8. **CI（`.github/workflows/ci.yml`）**
   - `test` job：`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked`。
   - `check-wasm` job：两条 clippy 合并为一条 `cargo clippy --locked --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings`，更新上方注释。
   - `lint-js` job：删除 `make test-release-bootstrap` 步骤。
   - `browser` job：`components` 去掉 `llvm-tools-preview`；“Install browser build tools”删掉 dx 三行，新增 `taiki-e/install-action`（沿用文件里已固定的 SHA）安装 `cargo-leptos@0.3.11`；其余不变。
   - 新增 `release-split` job（`runs-on: ubuntu-latest`，`timeout-minutes: 60`，`needs: [select_runner]`，PR 也跑）：checkout、stable 工具链（targets wasm32）、`make wasm-toolchain`、rust-cache（`prefix-key: release-split`）、setup-bun、安装 tailwind/postgresql-client-18/brotli/cargo-leptos 与 `libs` 依赖及 Playwright chromium（同 browser job）、`make build`、`bun scripts/test-release-split.cjs`、上传 `target/regressions/`。
   - `build-amd64` 的 `needs` 加入 `release-split`。
9. **.env.example**：`YGG_PREVIOUS_PUBLIC_DIR` 说明改为“上一发布的 `public/` 目录，滚动发布期间为旧标签页提供已删除的 `/pkg/` 分包”；新增 `LEPTOS_SITE_ROOT` 说明（默认可执行文件旁的 `public/`，`hash.txt` 必须与可执行文件同目录）。
10. 切片自检：`bun scripts/build-release.cjs --help` 不报语法错误、`bash -n` 检查改动的 shell 片段、`actionlint`（若本机可用）通过；不在切片内跑 `make build`。提交：`build(release): 构建发布与 CI 切换到 cargo-leptos`。

## Phase 6：收尾与总验收（编排者）

1. 合入 `release` 切片后删除：`spike/` 整个目录、`Dioxus.toml`（若仍在）、`mk/dx-cache.mk`。
2. 残留检查（每条结果必须为空）：
   - `grep -rnE 'dioxus::|use dioxus|rsx!|MIGRATION_STUB|ygg_client_reader|client-reader|SERVER_FN_OVERRIDE_KEY|DIOXUS_|__dx_|target/dx|--all-features' --exclude-dir=target --exclude-dir=node_modules --exclude-dir=.git --exclude-dir=public --exclude='*.md' .`
   - `grep -n 'dioxus' Cargo.toml Cargo.lock`
3. 全量验收按下文“Verification”逐条执行。
4. 提交：`chore(migrate): 删除 Dioxus 原型与遗留构建脚本`。

## Critical files & anchors
- `src/startup.rs`：`build_router`（BASE 411-641 行）——layer 顺序、merge 顺序、各路由组的超时/body 上限/CSRF 必须原样保留，只替换 Dioxus 应用部分。
- `src/navigation.rs` + `libs/yggdrasil-core/src/route-transitions.ts`：`NavigationBridge`/`RouteMarker` 与 JS `connect/notify/rendered` 握手，是视图过渡唯一的接缝；`onClick`（BASE 273-299 行）、`onPop`（310-338 行）、`connect`（129-146 行）。
- `spike/leptos/src/`：`app.rs`（shell/Lazy 注册写法）、`pages/post_detail.rs`（LazyRoute 范式）、`components/comments/section.rs:229-256`（Effect + 镜像信号）、`bridges/library.rs`（不用 LocalResource 的库加载）。
- `src/middleware.rs`：`cache_control_for_path`（121-166 行）、`previous_asset_file`（340-358 行）——发布产物目录从 `/assets`、`/wasm` 换成 `/pkg` 的两个落点。
- `Cargo.toml` 的 `[package.metadata.leptos]` 与 feature 定义——cargo-leptos、CI、Docker 三处构建都依赖它。

## Verification
前置：Phase 0 的工具齐全；冒烟库 `ygg_leptos_spike` 可用；Docker 可运行（回归脚本用临时 PostgreSQL 容器）。在仓库根目录执行：
1. 静态检查：`cargo fmt -- --check`；`cargo clippy --locked --all-targets -- -D warnings`；`cargo clippy --locked --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings`；`cd libs && bun run lint && bun run typecheck`。
2. 单测：`cargo test --locked`（应包含新增的 `routes` 编码测试、改写后的 ui SSR 测试、`utils::error` 的 413 嗅探测试、`middleware` 的 `/pkg` 缓存与回退测试）；`cd libs && bun run --sequential --filter '*' test`。
3. 数据库回归：`make test-databases`（11 个 `#[ignore]` 测试中脚本覆盖的 4 组，含改用 `scope_with_request` 的 `post_write_service_database`）。
4. 浏览器回归：`make regression-build`，然后 `make test-browser BROWSER_SUITE=smoke`、`admin`、`notes`、`backup`、`runner`、`slash`、`showcase`、`lightbox` 逐个通过；`bun scripts/test-public-performance.cjs` 通过。
5. 视图过渡：冒烟库上启动 `target/debug/yggdrasil`（`APP_BASE_URL` 与 `VT_BASE` 同为 `http://127.0.0.1:18080`），`VT_BASE=http://127.0.0.1:18080 bun scripts/test-view-transitions.cjs` 通过；按 `scripts/README-view-transitions.md` 手工检查直开、SPA 导航、明暗主题、390 px 移动端、减少动态效果。
6. 发布与分割：`make test-release-split`（含 `make build` 与 `build-release.cjs --verify`）通过；`target/regressions/*-release-split/sizes.json` 中 9 条路由的 `brotli_bytes` 全部 ≤ 594,603。
7. 新行为抽查（release 产物，`LEPTOS_SITE_ROOT=target/release-dist/public target/release-dist/server`，冒烟库）：
   - `curl -sI http://127.0.0.1:18080/pkg/<hash.txt 中 wasm 行对应的文件>` → `cache-control: public, max-age=31536000, immutable`。
   - `curl -s -w '\n%{http_code}' -X POST -H 'content-type: application/json' -H 'Origin: http://127.0.0.1:18080' -d '{}' http://127.0.0.1:18080/api/get_db_schema`（需管理员、无参数，未带会话）→ 状态码非 2xx，响应体以 `ServerError|` 开头（server_fn 的错误编码），且不含 SQL、端口或连接串细节。
   - 临时移走 `target/release-dist/hash.txt` 后启动服务 → 进程以 `hash.txt 缺失` 的 panic 信息退出；放回后正常启动。
8. 视觉对照：`git worktree add ../ygg-dioxus-base ba1b918e`，在其中 `make build`，然后在 `../ygg-dioxus-base/target/dx/yggdrasil/release/web` 下以 `DATABASE_URL=$SMOKE_DB_URL IP=127.0.0.1 PORT=18081 APP_BASE_URL=http://127.0.0.1:18081 ./server` 启动；新版以 18080 端口连同一冒烟库。对 `/`、`/post/<slug>`、`/archives`、`/notes`、`/about`、`/admin/posts`（带会话 cookie）分别在 1280×800 与 390×844 下截图两版，存到 `target/regressions/parity/`，逐对查看；除字体抗锯齿外的布局、间距、颜色差异都按 bug 修复。完成后 `git worktree remove ../ygg-dioxus-base`。
9. Docker：`make docker-amd64` 构建成功；对构建出的镜像执行 `docker run --rm -e MIGRATE_STARTUP_TIMEOUT_SECS=2 <镜像>`，日志同时含 `running database migrations` 与 `failed to ensure target database exists`（与 CI 冒烟一致）。

## 假设与预案
- **视图过渡握手**：若 `NavigationBridge` 的 notify 方案（JS 写 history → Rust 设置 `BrowserUrl` 的 URL 信号）在 Phase 2A 验收或第 5 条验证中无法稳定通过，**停止并报告**（附失败的脚本输出与复现步骤），不要改用 Leptos 自带的 `<Routes transition=true>`——那会丢掉共享元素过渡、滚动恢复和离开保护。
- **懒加载父路由**：若 `<ParentRoute path=path!("admin") view={Lazy::<AdminShellRoute>::new()}>` 编译失败或 Phase 2B 的侧栏保留检查不通过，改为把每个后台子路由提升为顶层 `<Route path=path!("admin/…")>`，各自的 `view()` 外包 `<AdminLayout>`（`AdminLayout` 改为接收 `children` 而不是渲染 `<Outlet/>`），并在报告中注明“后台切换页面会重建侧栏”。
- **`SsrMode::InOrder`**：若某条 `InOrder` 路由的 SSR HTML 里只有骨架屏而没有数据内容，把该路由改为 `SsrMode::Async`。
- **nightly 前端构建**：若 `make build-frontend`（nightly + `-Zbuild-std` + `-Cpanic=immediate-abort` + `--split`）失败，去掉 `LEPTOS_WASM_ENV` 与 `LEPTOS_WASM_ARGS`，改用稳定工具链 `cargo leptos build --release --split --frontend-only`（与原型一致），Dockerfile/CI 同步，然后继续；体积门槛仍按 594,603 B 判定。
- **体积门槛**：`test-release-split.cjs` 任一路由超标时**停止并报告** `sizes.json` 明细，不自行追加懒加载点或裁剪功能。
- **cargo-leptos 安装**：CI 中 `taiki-e/install-action` 装不上 `cargo-leptos@0.3.11` 时改为 `cargo install cargo-leptos --version 0.3.11 --locked`。Docker 构建中 cargo-leptos 下载 wasm-bindgen/wasm-opt 失败（例如 `CN_MIRROR=true` 无法访问 GitHub）时，在镜像里先 `cargo install wasm-bindgen-cli --version <Cargo.lock 中 wasm-bindgen 的版本> --locked` 并 apt 安装 `binaryen`，让 cargo-leptos 使用 PATH 上的工具。
- **musl 下的 cdylib**：若 musl 静态构建因 `cdylib` crate type 报错（而不只是 “dropping unsupported crate type” 警告），停止并报告。
- **server fn URL 变化**：所有 server fn 地址从 `/api/<name><哈希>` 变为 `/api/<name>`，发布切换瞬间仍开着的旧 Dioxus 标签页会调用失败、需要刷新；这是一次性切换的已知代价，不做兼容路由。
- **非 Async 路由的错误状态码**：`InOrder`/默认模式的路由在流式输出开始后无法再改 HTTP 状态，加载失败时返回 200 + “页面加载失败”页面；只有 `Async` 路由能返回 404/500。这与 Dioxus 全量 SSR 的行为不同，按此接受。
- **URL 细节**：`Route` 的 href 在查询值为空时省略 `?key=`（例如 `/search` 而不是 `/search?q=`），其余编码与 Dioxus 一致。
