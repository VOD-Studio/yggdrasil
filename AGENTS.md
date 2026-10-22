# Repository Guidelines

## Project Overview

Yggdrasil is a self-hosted blog + CMS + notes/knowledge base: Rust 2021, Dioxus 0.7 fullstack (SSR + hydrated WASM), Axum, PostgreSQL. It also exposes an MCP server (`/mcp`) for AI clients and a Docker-sandboxed code runner. UI text, docs, code comments, and commit summaries are written in Chinese.

## Architecture & Data Flow

- **One crate, three builds** via Cargo features (`default = ["web", "server"]`):
  - native server: `server` (Axum + SSR + DB)
  - full browser client: `--no-default-features --features web` (admin, login, showcase, reader routes)
  - reader client: `web,client-reader` — `build.rs` emits `cfg(ygg_client_reader)` only when `client-reader` is on and `server` is off; admin/login/showcase routes are compiled out. `--all-features` therefore does not exercise reader cfg.
  - Gate server-only code with `#[cfg(feature = "server")]`, browser APIs with `#[cfg(target_arch = "wasm32")]`, full-only UI with `#[cfg(not(ygg_client_reader))]`.
- **Bootstrap** (`src/main.rs` → `src/startup.rs::run`): dotenvy → tracing (+ `api::logs::capture::CaptureLayer`) → validate `DATABASE_URL`/`DB_POOL_SIZE` → `db::pool::ensure_database` → `db::migrate::run_on_conn` → `api::settings::bootstrap_startup_settings` → `api::auth::sync_admin_from_env`. Debug uses `dioxus::server::serve(|| build_router)` (called repeatedly on hot reload; background tasks guarded by `BACKGROUND_TASKS_STARTED: OnceLock`); release uses `axum::serve`.
- **Router** (`startup::build_router`): Dioxus server functions + static assets, SSR fallback chosen per flavor (`router::is_reader_path` → `FlavorStates{reader, full}`), plus hand-written Axum routes: uploads (`/api/upload`, `/api/{comments,notes,mcp}/upload`), backups (`/api/database/*`), SSE (`/api/exec/stream`, `/api/logs/stream`), `/uploads/*`, `/note-media/{id}`, `/og/*`, `/feed.{xml,json}`, `/healthz`, `/readyz`, `/mcp`. Layers: SSR HTML cache, SSR generation, cache-control, `csrf_middleware`, optional compression, 30 s timeout, `admin_guard` (`src/middleware.rs`).
- **Request flow**: page (`use_server_future` for public SSR pages; `hooks::query::use_paginated` / `use_effect + spawn` for admin CSR pages) → `#[server(Name, "/api")]` fn in `src/api/**` → service module (e.g. `api/posts/service.rs`) → moka cache (`src/cache.rs`, `CacheKey`) → `db::pool::DB_POOL` (`LazyLock<deadpool_postgres::Pool>`) → PostgreSQL. Writes must invalidate the moka cache and bump SSR generation; cross-instance invalidation goes through `LISTEN ygg_content_changed` (`src/tasks/cache_sync.rs`).
- **SSR HTML cache** (`src/ssr_cache.rs`) is whitelist-only (`/`, `/archives`, `/friends`, `/about`, `/changelog`, `/post/*`, `/page/*`, `/tags/*`; no query/cookie/auth). New public routes stay uncached until added to `cacheable_request`. `/notes` uses a separate uncached state.
- **Settings model**: runtime config lives in the DB `settings` table. Env vars such as `APP_BASE_URL`, `COOKIE_SECURE`, `TRUSTED_PROXY_COUNT`, `RATE_LIMIT_*`, `CODE_RUNNER_*` only *seed* missing keys on first boot; afterwards the admin panel wins. Tier A settings are read live (moka); Tier B are `OnceLock`s in `src/config.rs` and need a restart (`src/models/settings.rs`).
- **Auth**: UUID session token, stored SHA-256, `session` cookie (`src/auth/session.rs`, `api/auth.rs::get_user_by_token`, `get_current_admin_user`). CSRF = Origin / `Sec-Fetch-Site` check (`src/api/csrf.rs`).
- **MCP** (`src/mcp/`): rmcp `StreamableHttpService`; `YggMcpServer` tool groups in `mcp/tools/`; token scopes `read < write < admin` plus per-notebook `NoteGrant` (`models/mcp_token.rs`); tokens encrypted with `MCP_TOKEN_ENC_KEY`.
- **Markdown**: `src/api/markdown/` (pulldown-cmark two-pass → `RenderedContent{html, toc_html}`), with KaTeX/mhchem and an HTML sanitizer; rendered HTML is stored with the post.
- **Browser libraries**: `libs/` packages build IIFE bundles into `public/<pkg>/`; Rust reaches them through window globals (`TiptapEditor`, `CodeMirrorEditor`, `Lightbox`, `XtermTerminal`, `YggdrasilCore`, `YggdrasilEnhance`, `MermaidRenderer`) via `src/bridges/*` and lazy `bridges::library::use_browser_library`.
- **Domain terms**: post / draft / trash; note `Moment` (随记) vs `Topic` (主题笔记), revisions, notebooks with ordering, `published_version` vs `knowledge_version`, `Access{Public, Owner, Knowledge}` (`models/note.rs`); asset references by `content_hash`; flavor (reader/full).

## Key Directories

- `src/api/`: server functions and services by domain (`posts/`, `notes/`, `comments/`, `settings/`, `database/backup/`, `code_runner/`, `markdown/`, `assets/`).
- `src/db/`: pool, migration runner, retry; `src/mcp/`: MCP server; `src/models/`: types shared by server and client.
- `src/pages/`, `src/components/` (`ui/` primitives), `src/hooks/`, `src/bridges/` (JS interop), `src/context.rs` (`UserContext`), `src/theme.rs`, `src/navigation.rs`.
- `src/tasks/`: background jobs (purges, backups, cache sync, sysinfo); `src/infra/`: Docker runner, WebP.
- `libs/`: Bun workspace — `@yggdrasil/{shared, core, tiptap-editor, codemirror-editor, lightbox, xterm-terminal, mermaid-renderer}`. `shared` is inlined at compile time; packages never import each other at runtime. See `libs/CONTEXT.md`.
- `migrations/`: `NNN_description.sql` (latest `030_note_asset_refs_asset_index.sql`).
- `styles/` + `input.css`: Tailwind v4 CSS-first (no tailwind config file); import order matters.
- `scripts/`: Bun `.cjs` browser regressions, release assembly (`build-release.cjs`, `release-bootstrap.cjs`, `measure-release.cjs`), `test-databases.sh`, asset generators.
- `docker/`: deployment compose files and code-runner images (`docker/build-runners.sh`).
- `docs/`: design/plan notes (`v1.0-重设计方案.md`, `plan/`, `backup-regressions.md`, `code-audit-2026-10-22.md`).

## Development Commands

Run bare `make` to list annotated targets. Prefer Makefile targets: they clear `RUSTC_WRAPPER` for `dx` (sccache conflict), set `SSR_CACHE_SECS=0` in dev, and restore source WebP files after release builds (dx re-encodes and drops animation).

```sh
make check-dev-tools          # verify dx / tailwindcss / bun / llvm-tools
make dev                      # incremental assets + dx serve (features dev-logger)
make dev-assets               # refresh changed assets for a running server
make build-libs | make css    # libs IIFE bundles | Tailwind → public/style.css
make build                    # full release: assets, docs, full + reader clients, merge, brotli, verify
make wasm-toolchain           # install pinned nightly + rust-src + wasm32
make test | make lint | make fmt | make fix
```

WASM checks (native Clippy does not see wasm-gated code):

```sh
cargo check  --locked --target wasm32-unknown-unknown --no-default-features --features web
cargo clippy --locked --target wasm32-unknown-unknown --no-default-features --features web,client-reader -- -D warnings
```

Docker fallbacks only when local tools are missing: `make docker-dev` (port 8080), `make docker-test`, `make docker-lint`, `make docker-fmt`, `make docker-run CMD='<cmd>'`, `make docker-dev-down`.

## Code Conventions & Common Patterns

- Rust: rustfmt defaults; `snake_case` modules/fns, `PascalCase` types and components. Never run routine `dx fmt` (it can corrupt RSX).
- Frontend (Biome): 2 spaces, single quotes, semicolons, 100 cols; kebab-case filenames; TS strict, `verbatimModuleSyntax`.
- Errors: server code returns `api::error::AppError` (`Unauthorized`, `Forbidden`, `NotFound`, `BadRequest`, `DbConn`, `Query`, `Transaction`, `Internal`), converted to `ServerFnError` with DB details masked. UI must show human-readable Chinese messages, never raw `error running server function…`.
- State/DI: no `AppState` struct. Server uses globals (`DB_POOL`, moka caches, `config` `OnceLock`s) plus Axum `Extension`. Client uses Dioxus signals and `use_context_provider` contexts (`UserContext`, toast `use_toast`, `SettingsBus`, `CommentContext`, `SidebarControl`).
- Migrations: add `migrations/NNN_description.sql` (zero-padded, contiguous) and append `("NNN", include_str!(...))` to `MIGRATIONS` in `src/db/migrate.rs` in ascending order. Tests enforce sorting, uniqueness, and file/array parity; each file runs in one transaction under an advisory lock.
- Generated sources: `src/components/ui/icons_data.rs`, `styles/motion.css`, and code themes come from `cd libs/shared && bun run {icons,motion-css,code-themes}`; regenerate them, don't hand-edit.
- Components showcase: every `pub fn PascalCase` component under `src/components` (except layouts/header/footer) must have an entry in `src/pages/components_showcase_data.json` (unique slug, known group, non-empty `code`, `preview_route`; enforced by tests in `src/pages/components_showcase.rs`). Previews reuse production display code with fixtures and local callbacks, must not send auth/upload/comment/exec requests, and must clean up browser instances on unmount.
- `CHANGELOG.md` is embedded via `include_str!` (`api/changelog.rs`) and rendered at `/changelog`.
- Global style/script URLs in `Dioxus.toml` carry `?v=N`; bump it when their content changes.
- Typography: `text-autospace`, no CJK italics, `tabular-nums` for dynamic numbers.

## Important Files

- `src/main.rs`, `src/startup.rs`, `src/router.rs` (`Route`, `is_reader_path`), `src/middleware.rs`, `src/ssr_cache.rs`, `src/cache.rs`, `src/config.rs`, `build.rs`.
- `Cargo.toml` (features; `[profile.wasm-release]`: `opt-level="z"`, fat LTO, `strip=false` because stripping removes wasm-bindgen metadata), `.cargo/config.toml` (`SERVER_FN_OVERRIDE_KEY` keeps server-fn URLs stable; wasm `getrandom` backend), `Dioxus.toml`.
- `Makefile`, `mk/assets.mk`, `mk/dx-cache.mk`, `mk/docker.mk`; `.github/workflows/ci.yml`.
- `libs/package.json`, `libs/bunfig.toml`, `libs/biome.json`, `libs/tsconfig.base.json`.
- `.env.example`; `Dockerfile`, `Dockerfile.cross`.

## Runtime/Tooling Preferences

- Pins: Dioxus CLI 0.7.10, Tailwind 4.3.3 **standalone** binary (the npm CLI can't resolve `tailwindcss` without a root `node_modules`), Bun 1.4.2 (`packageManager`), WASM nightly `nightly-2026-10-19`, Rust with `llvm-tools`. There is no `rust-toolchain.toml`.
- Bun only. No Node.js or pnpm; `scripts/*.cjs` have a `#!/usr/bin/env bun` shebang, so run them with `bun scripts/<name>.cjs`.
- Changing frontend deps: `cd libs && BUN_CONFIG_REGISTRY=https://registry.npmjs.org/ bun install`; keep per-package registry URLs out of `bun.lock`. `bunfig.toml` enforces a 1-day `minimumReleaseAge`.
- Generated output is never committed: `public/style.css`, `public/highlight.css`, `public/katex/`, `public/doc/`, every `public/<lib>/` bundle dir, and release `assets/`, `wasm/`, `flavors/`, `release-manifest.json`, `*.br`. Tracked: `public/icons/`, `public/images/`, `public/skills/`, favicons, `manifest.webmanifest`.
- Env: `DATABASE_URL` (required), `MCP_TOKEN_ENC_KEY` (64 hex, required for `/mcp`), `ADMIN_{USERNAME,EMAIL,PASSWORD}` (re-synced on every boot), `SSR_CACHE_SECS` (default 60), `COMPRESSION_ALGORITHMS` (default `off`), `DB_POOL_SIZE`, `STATEMENT_TIMEOUT_SECS`, `DIOXUS_PUBLIC_PATH`, `IP`/`PORT`, `YGG_PREVIOUS_PUBLIC_DIR` (old release `public/` kept during rolling deploys so stale tabs can load dropped hashed assets).
- Production: TLS reverse proxy, correct `APP_BASE_URL`, `COOKIE_SECURE=true`, and an accurate `TRUSTED_PROXY_COUNT` (set in the panel after first boot). Dev compose expects external PostgreSQL on `global-databases_default`. Code runner images: `yggdrasil-runner-{base,python,node,go,rust,bun}`, run with read-only rootfs, no network, and dropped capabilities.

## Testing & QA

- Rust: inline `#[cfg(test)] mod tests` (server-only: `#[cfg(all(test, feature = "server"))]`), `#[tokio::test]`, `serial_test::serial` for env/global state. No `tests/` dir, no coverage threshold.
  - One test: `cargo test --locked api::database::backup`.
- DB regressions are `#[ignore]` and destructive (they truncate fixtures). They run `*_impl` async fns on `db::TEST_DATABASE_RUNTIME` under `TEST_DATABASE_LOCK` and assert `current_database()`. Use disposable DBs: `ygg_mcp_test` (posts/uploads/MCP), `ygg_notes_test` (notes), `ygg_backup_test` via `YGG_BACKUP_TEST_DATABASE_URL` (needs `pg_dump`/`psql`).
  - `make test-databases`, or `bash scripts/test-databases.sh` with `PG*` env.
  - Single: `DATABASE_URL=postgres://…/ygg_notes_test cargo test --locked --all-features <name> -- --ignored --exact --test-threads=1`.
- Frontend: Vitest + happy-dom, co-located `src/**/*.test.{ts,js}` (some packages use `__tests__/`); `// @vitest-environment node` for pure tests; stub missing observers with `vi.stubGlobal`.
  - All: `cd libs && bun run --sequential --filter '*' test`.
  - One: `cd libs/yggdrasil-core && bun run test -- src/anchor-click.test.ts -t 'initAnchorClick'`.
- Browser (Playwright via Bun): `make regression-build`, then `make test-browser BROWSER_SUITE=smoke|admin|notes|backup|runner|slash|lightbox|showcase`. Standalone scripts include `bun scripts/test-components-showcase.cjs` (`SHOWCASE_BASE`, `SHOWCASE_ONLY`) and `bun scripts/test-view-transitions.cjs` (`VT_BASE`, which must be same-origin with `APP_BASE_URL`, otherwise CSRF returns 403).
- Navigation changes: follow `scripts/README-view-transitions.md`; verify direct load, SPA navigation, themes, 390 px mobile, and reduced motion.
- Release: `make test-release-bootstrap`, `make test-wasm-preload`, `bun scripts/test-public-performance.cjs`. Reader-flavor Brotli size must stay ≥35% smaller than full.
- CI (`ci.yml`) runs fmt, clippy `--all-targets --all-features -D warnings`, `cargo test --all-features`, Biome/typecheck/Vitest, both WASM clippy variants, `test-databases.sh`, all browser suites, then an amd64 image boot smoke test.

## Commits

`type(scope): 中文摘要` (`feat|fix|refactor|docs|test|style|chore`), e.g. `fix(navigation): 恢复后台滚动位置`. After each validated functional unit, commit autonomously with only that unit's changes. Push only when asked.
