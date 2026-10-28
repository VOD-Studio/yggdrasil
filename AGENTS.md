# Repository Guidelines

## Project Overview

Yggdrasil is a self-hosted blog + CMS + notes/knowledge base: Rust 2021, Leptos 0.8 (SSR + hydrated WASM, route-level WASM splitting via cargo-leptos `--split`), Axum, PostgreSQL. It also exposes an MCP server (`/mcp`) for AI clients and a Docker-sandboxed code runner. UI text, docs, code comments, and commit summaries are written in Chinese.

## Architecture & Data Flow

- **Lib + bin crate, two builds** via Cargo features (`default = ["server"]`; `server` and `hydrate` are mutually exclusive, never enable all features):
  - native server: `server` (Axum + `leptos_axum` SSR + DB), binary `yggdrasil` (`src/main.rs` → `yggdrasil::startup::run`).
  - browser WASM: `--no-default-features --features hydrate` (cdylib, built by cargo-leptos; `hydrate_lazy` entry in `src/lib.rs`).
  - Gate server-only code with `#[cfg(feature = "server")]`, browser APIs with `#[cfg(target_arch = "wasm32")]` (only inside effects/handlers/bridges, never to pick a different view branch: hydration must match SSR).
  - Admin, login, showcase and every public page are `Lazy::<T>` routes (`LazyRoute`), each split into its own WASM chunk. There is no reader/full flavor.
  - Both sides compile with `--cfg erase_components` (server: `.cargo/config.toml` `[build] rustflags`, re-added wherever `RUSTFLAGS` is exported, e.g. Dockerfiles; WASM: `always-erase-components` in `[package.metadata.leptos]`). A mismatch leaves pages silently unhydrated.
  - `SPLIT_DATA_SLACK` in `src/lib.rs` lets wasm-split move route-only `.rodata` out of the main bundle; `build-release.ts` strips `producers`/`target_features` from every `.wasm`. Both are needed for the per-route size gate.
  - Release frontends need cargo-leptos 0.3.11 built against the chunk-merge fork of `wasm_split_cli_support` (`make install-cargo-leptos`, rev `WASM_SPLIT_REV` in Makefile, duplicated in both Dockerfiles and `ci.yml`); stock wasm-split emits ~4× more tiny chunks. `make build-frontend` refuses a stock binary; `test-release-split` caps `.wasm` files per route.
  - `build-release.ts` renames the entry trio (`yggdrasil.<h>.{js,wasm}`, `__wasm_split.<h>.js`) by their combined final content and writes `entry: content-addressed` to `hash.txt`; only then does `middleware::cache_control_for_path` serve the entries `immutable`.
  - `app::HydrationScriptsWithoutSplitPreload` drops Leptos's per-chunk `<link rel="preload">` flood; only the main wasm and the split loader are preloaded.
- **Bootstrap** (`src/main.rs` → `src/startup.rs::run`): dotenvy → tracing (+ `api::logs::capture::CaptureLayer`) → validate `DATABASE_URL`/`DB_POOL_SIZE` → `db::pool::ensure_database` → `db::migrate::run_on_conn` → `api::settings::bootstrap_startup_settings` → `api::auth::sync_admin_from_env` → `axum::serve` with graceful shutdown. Site assets come from `LEPTOS_SITE_ROOT` (default `<exe dir>/public`); `hash.txt` must sit next to the executable and every file it names must exist under `pkg/`, otherwise startup panics.
- **Router** (`startup::build_router`): `leptos_axum` routes from `app::App` + `site_fallback` (ServeDir with precompressed br, `/pkg/*` miss → 404, otherwise SSR render so unmatched routes get `NotFound`), plus hand-written Axum routes: uploads (`/api/upload`, `/api/{comments,notes,mcp}/upload`), backups (`/api/database/*`), SSE (`/api/exec/stream`, `/api/logs/stream`), `/uploads/*`, `/note-media/{id}`, `/og/*`, `/feed.{xml,json}`, `/healthz`, `/readyz`, `/mcp`. Layers: SSR HTML cache, SSR generation, cache-control, `csrf_middleware`, optional compression, 30 s timeout, `admin_guard` (`src/middleware.rs`). The route table lives in `src/app.rs`; `src/routes.rs::Route` only builds hrefs.
- **Request flow**: page (`hooks::ssr_resource::route_data` created in `LazyRoute::data()` for public SSR pages, with a `#[leptos::lazy]` fetcher returning `PageJson<T>`, so data types and server-fn clients stay out of the main bundle; never create resources in `view()`: SSR runs a lazy route's `view()` more than once; `hooks::query::use_paginated` / `Effect + spawn_local` for admin CSR pages) → `#[server(endpoint = "<fn name>", input = Json)]` fn in `src/api/**` (URL `/api/<fn name>`) → service module (e.g. `api/posts/service.rs`) → moka cache (`src/cache.rs`, `CacheKey`) → `db::pool::DB_POOL` (`LazyLock<deadpool_postgres::Pool>`) → PostgreSQL. Writes must invalidate the moka cache and bump SSR generation; cross-instance invalidation goes through `LISTEN ygg_content_changed` (`src/tasks/cache_sync.rs`).
- **SSR HTML cache** (`src/ssr_cache.rs`) is whitelist-only (`/`, `/archives`, `/friends`, `/about`, `/changelog`, `/post/*`, `/page/*`, `/tags/*`; no query/cookie/auth). New public routes stay uncached until added to `cacheable_request`. `/notes` uses a separate uncached state.
- **Settings model**: runtime config lives in the DB `settings` table. Env vars such as `APP_BASE_URL`, `COOKIE_SECURE`, `TRUSTED_PROXY_COUNT`, `RATE_LIMIT_*`, `CODE_RUNNER_*` only *seed* missing keys on first boot; afterwards the admin panel wins. Tier A settings are read live (moka); Tier B are `OnceLock`s in `src/config.rs` and need a restart (`src/models/settings.rs`).
- **Auth**: UUID session token, stored SHA-256, `session` cookie (`src/auth/session.rs`, `api/auth.rs::get_user_by_token`, `get_current_admin_user`). CSRF = Origin / `Sec-Fetch-Site` check (`src/api/csrf.rs`).
- **MCP** (`src/mcp/`): rmcp `StreamableHttpService`; `YggMcpServer` tool groups in `mcp/tools/`; token scopes `read < write < admin` plus per-notebook `NoteGrant` (`models/mcp_token.rs`); tokens encrypted with `MCP_TOKEN_ENC_KEY`.
- **Markdown**: `src/api/markdown/` (pulldown-cmark two-pass → `RenderedContent{html, toc_html}`), with KaTeX/mhchem and an HTML sanitizer; rendered HTML is stored with the post.
- **Browser libraries**: `libs/` packages build IIFE bundles into `public/<pkg>/`; Rust reaches them through window globals (`TiptapEditor`, `CodeMirrorEditor`, `Lightbox`, `XtermTerminal`, `YggdrasilCore`, `YggdrasilEnhance`, `MermaidRenderer`) via `src/bridges/*` and lazy `bridges::library::use_browser_library`.
- **Domain terms**: post / draft / trash; note `Moment` (随记) vs `Topic` (主题笔记), revisions, notebooks with ordering, `published_version` vs `knowledge_version`, `Access{Public, Owner, Knowledge}` (`models/note.rs`); asset references by `content_hash`.

## Key Directories

- `src/api/`: server functions and services by domain (`posts/`, `notes/`, `comments/`, `settings/`, `database/backup/`, `code_runner/`, `markdown/`, `assets/`).
- `src/db/`: pool, migration runner, retry; `src/mcp/`: MCP server; `src/models/`: types shared by server and client.
- `src/pages/`, `src/components/` (`ui/` primitives), `src/hooks/`, `src/bridges/` (JS interop), `src/context.rs` (`UserContext`), `src/theme.rs`, `src/navigation.rs` (programmatic navigation + view-transition handshake with `window.__routeTransitions`), `src/request_ctx.rs` (request headers/extensions/status in server fns and SSR).
- `src/tasks/`: background jobs (purges, backups, cache sync, sysinfo); `src/infra/`: Docker runner, WebP.
- `libs/`: Bun workspace — `@yggdrasil/{shared, core, tiptap-editor, codemirror-editor, lightbox, xterm-terminal, mermaid-renderer}`. `shared` is inlined at compile time; packages never import each other at runtime. See `libs/CONTEXT.md`.
- `migrations/`: `NNN_description.sql` (latest `031_session_generation_snapshot.sql`).
- `styles/` + `input.css`: Tailwind v4 CSS-first (no tailwind config file); import order matters.
- `scripts/`: Bun TypeScript browser regressions, release assembly (`build-release.ts`, `test-release-split.ts`), `test-databases.sh`, asset generators, `deploy.fish` (build the amd64 image here with CN mirrors and ship it over ssh to xun/rua, same role-aware flow and rollback as the CI deploy jobs).
- `docker/`: deployment compose files and code-runner images (`docker/build-runners.sh`).
- `docs/`: design/plan notes (`v1.0-重设计方案.md`, `plan/`, `backup-regressions.md`, `code-audit-2026-10-22.md`); `docs/双机主备/` is the production xun/rua primary–standby runbook plus the exact compose files, systemd units and scripts installed on the hosts (edit there first, then copy to the servers).

## Development Commands

Run bare `make` to list annotated targets. Prefer Makefile targets: they set `SSR_CACHE_SECS=0` in dev and wire cargo-leptos with the pinned WASM toolchain.

```sh
make check-dev-tools          # verify cargo-leptos 0.3.11 / tailwindcss / bun
make dev                      # incremental assets + cargo leptos watch
make dev-assets               # refresh changed assets for a running server
make build-libs | make css    # libs IIFE bundles | Tailwind → public/style.css
make build                    # full release: assets, docs, split WASM frontend, server, assemble target/release-dist, brotli, verify
make wasm-toolchain           # install pinned nightly + rust-src + wasm32
make test | make lint | make fmt | make fix
```

WASM checks (native Clippy does not see wasm-gated code):

```sh
cargo clippy --locked --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings
```

Docker fallbacks only when local tools are missing: `make docker-dev` (port 8080), `make docker-test`, `make docker-lint`, `make docker-fmt`, `make docker-run CMD='<cmd>'`, `make docker-dev-down`.

## Code Conventions & Common Patterns

- Rust: rustfmt defaults; `snake_case` modules/fns, `PascalCase` types and components (`#[component] fn X(..) -> impl IntoView` + `view!`). `#![recursion_limit = "256"]` is required in both `lib.rs` and `main.rs`.
- Frontend (Biome): 2 spaces, single quotes, semicolons, 100 cols; kebab-case filenames; TS strict, `verbatimModuleSyntax`.
- Errors: server fn、SSR 资源和 UI 消费端统一使用 `api::error::ApiError`，独立导入，不覆盖 Leptos 的 `ServerFnError` 名称。服务端业务错误保留 `AppError`，转换时 `DbConn` → `Unavailable`，`Query`/`Transaction` → `Internal("操作失败")`，不得泄露数据库详情；框架编解码等错误放入 `ApiError::Framework(ServerFnError::...)`。404 只按 `ApiError::NotFound` 类型判定，不解析错误文本。UI 经 `utils::error::{failure_message, error_reason}` 展示中文消息，页面加载失败使用 `PageLoadFailed`；MCP 将 `AppError` 转成 `ApiError` 后只展示人类可读消息，不输出序列化错误或调试详情。
- State/DI: no `AppState` struct. Server uses globals (`DB_POOL`, moka caches, `config` `OnceLock`s) plus Axum `Extension`. Client uses Leptos signals and `provide_context`/`expect_context` (`UserContext`, toast `use_toast`, `SettingsBus`, `CommentContext`, `SidebarControl`). Signal values must be `Send + Sync`; JS handles use `RwSignal::new_local` / `StoredValue::new_local`. Data that refreshes inside `<Suspense>` must not be a `Resource` (it would swap the DOM back to the fallback): use `Effect` + `spawn_local` mirrored into a signal.
- Navigation: internal links are plain `<a href>` (`rel="external"` for `/feed.*`, `/doc/`, `/uploads/`, `/og/`, `/api/`, downloads); programmatic navigation only through `crate::navigation::{push, replace, back, forward, external}`. Never use `leptos_router::<A>` or `use_navigate`: `libs/yggdrasil-core` owns history and view transitions.
- `SsrMode`: pages whose `<Title>`/`<Meta>`/`<Script>` or HTTP status depend on server data must be `Async`; `InOrder` cannot write to `<head>` after streaming starts.
- Migrations: add `migrations/NNN_description.sql` (zero-padded, contiguous) and append `("NNN", include_str!(...))` to `MIGRATIONS` in `src/db/migrate.rs` in ascending order. Tests enforce sorting, uniqueness, and file/array parity; each file runs in one transaction under an advisory lock.
- Generated sources: `src/components/ui/icons_data.rs`, `styles/motion.css`, and code themes come from `cd libs/shared && bun run {icons,motion-css,code-themes}`; regenerate them, don't hand-edit.
- Components showcase: every `pub fn PascalCase` component under `src/components` (except layouts/header/footer) must have an entry in `src/pages/components_showcase_data.json` (unique slug, known group, non-empty `code`, `preview_route`; enforced by tests in `src/pages/components_showcase.rs`). Previews reuse production display code with fixtures and local callbacks, must not send auth/upload/comment/exec requests, and must clean up browser instances on unmount.
- `CHANGELOG.md` is embedded via `include_str!` (`api/changelog.rs`) and rendered at `/changelog`.
- Global style/script URLs in `src/app.rs::shell` carry `?v=N`; bump it when their content changes.
- Typography: `text-autospace`, no CJK italics, `tabular-nums` for dynamic numbers.

## Important Files

- `src/lib.rs`, `src/main.rs`, `src/app.rs` (`shell`, route table), `src/routes.rs`, `src/startup.rs`, `src/middleware.rs`, `src/ssr_cache.rs`, `src/cache.rs`, `src/config.rs`, `build.rs`.
- `Cargo.toml` (features; `[package.metadata.leptos]`; `[profile.wasm-release]`: `opt-level="z"`, fat LTO, `strip=false` because stripping removes wasm-bindgen metadata), `.cargo/config.toml` (wasm `getrandom` backend).
- `Makefile`, `mk/assets.mk`, `mk/docker.mk`; `.github/workflows/ci.yml`.
- `libs/package.json`, `libs/bunfig.toml`, `libs/biome.json`, `libs/tsconfig.base.json`.
- `.env.example`; `Dockerfile`, `Dockerfile.cross`.

## Runtime/Tooling Preferences

- Pins: cargo-leptos 0.3.11 (chunk-merge patched, see above), Leptos 0.8.22, Tailwind 4.3.3 **standalone** binary (the npm CLI can't resolve `tailwindcss` without a root `node_modules`), Bun 1.4.2 (`packageManager`), WASM nightly `nightly-2026-10-19` (release frontend uses `-Zbuild-std`), Rust. There is no `rust-toolchain.toml`.
- Bun only. No Node.js or pnpm; `scripts/*.ts` use Bun directly (`bun scripts/<name>.ts`); run `cd libs && bun run typecheck:scripts` for strict type checking. Standalone CLI scripts have a `#!/usr/bin/env bun` shebang.
- Changing frontend deps: `cd libs && BUN_CONFIG_REGISTRY=https://registry.npmjs.org/ bun install`; keep per-package registry URLs out of `bun.lock`. `bunfig.toml` enforces a 1-day `minimumReleaseAge`.
- Generated output is never committed: `public/style.css`, `public/highlight.css`, `public/katex/`, `public/doc/`, every `public/<lib>/` bundle dir, `target/site`, `target/release-dist`, `release-manifest.json`, `*.br`. Tracked: `public/icons/`, `public/images/`, `public/skills/`, favicons, `manifest.webmanifest`.
- Env: `DATABASE_URL` (required), `MCP_TOKEN_ENC_KEY` (64 hex, required for `/mcp`), `ADMIN_{USERNAME,EMAIL,PASSWORD}` (re-synced on every boot), `SSR_CACHE_SECS` (default 60), `COMPRESSION_ALGORITHMS` (default `off`), `DB_POOL_SIZE`, `STATEMENT_TIMEOUT_SECS`, `LEPTOS_SITE_ROOT`, `IP`/`PORT`, `YGG_PREVIOUS_PUBLIC_DIR` (previous release `public/` kept during rolling deploys so stale tabs can load dropped `/pkg/` chunks).
- Production: TLS reverse proxy, correct `APP_BASE_URL`, `COOKIE_SECURE=true`, and an accurate `TRUSTED_PROXY_COUNT` (set in the panel after first boot). Dev compose expects external PostgreSQL on `global-databases_default`. Code runner images: `yggdrasil-runner-{base,python,node,go,rust,bun}`, run with read-only rootfs, no network, and dropped capabilities.

## Testing & QA

- Rust: inline `#[cfg(test)] mod tests` (server-only: `#[cfg(all(test, feature = "server"))]`), `#[tokio::test]`, `serial_test::serial` for env/global state. No `tests/` dir, no coverage threshold.
  - One test: `cargo test --locked api::database::backup`.
- DB regressions are `#[ignore]` and destructive (they truncate fixtures). They run `*_impl` async fns on `db::TEST_DATABASE_RUNTIME` under `TEST_DATABASE_LOCK` and assert `current_database()`. Use disposable DBs: `ygg_mcp_test` (posts/uploads/MCP), `ygg_notes_test` (notes), `ygg_backup_test` via `YGG_BACKUP_TEST_DATABASE_URL` (needs `pg_dump`/`psql`).
  - `make test-databases`, or `bash scripts/test-databases.sh` with `PG*` env.
  - Single: `DATABASE_URL=postgres://…/ygg_notes_test cargo test --locked <name> -- --ignored --exact --test-threads=1`.
- Frontend: Vitest + happy-dom, co-located `src/**/*.test.{ts,js}` (some packages use `__tests__/`); `// @vitest-environment node` for pure tests; stub missing observers with `vi.stubGlobal`.
  - All: `cd libs && bun run --sequential --filter '*' test`.
  - One: `cd libs/yggdrasil-core && bun run test -- src/anchor-click.test.ts -t 'initAnchorClick'`.
- Browser (Playwright via Bun): `make regression-build`, then `make test-browser BROWSER_SUITE=smoke|admin|notes|backup|runner|slash|lightbox|showcase`. Standalone scripts include `bun scripts/test-components-showcase.ts` (`SHOWCASE_BASE`, `SHOWCASE_ONLY`) and `bun scripts/test-view-transitions.ts` (`VT_BASE`, which must be same-origin with `APP_BASE_URL`, otherwise CSRF returns 403).
- Navigation changes: follow `scripts/README-view-transitions.md`; verify direct load, SPA navigation, themes, 390 px mobile, and reduced motion.
- Release: `make test-release-split` (builds, then asserts split chunks, SSR cache, previous-release fallback and per-route Brotli WASM ≤ 594,603 B), `bun scripts/test-public-performance.ts`.
- CI (`ci.yml`) runs fmt, clippy `--all-targets -D warnings`, `cargo test`, Biome/typecheck/Vitest, hydrate WASM clippy, `test-databases.sh`, all browser suites, `release-split`, then an amd64 image boot smoke test.

## Commits

`type(scope): 中文摘要` (`feat|fix|refactor|docs|test|style|chore`), e.g. `fix(navigation): 恢复后台滚动位置`. After each validated functional unit, commit autonomously with only that unit's changes. Push only when asked.
