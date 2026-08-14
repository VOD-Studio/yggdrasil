# Repository Guidelines

Yggdrasil is a fullstack blog/CMS built with **Dioxus 0.7**. A single Rust crate (`yggdrasil`, Cargo edition 2021) compiles to **two targets from one codebase**: a WASM frontend (feature `web`) and a native Axum server (feature `server`). Stack: PostgreSQL via tokio-postgres + deadpool, Tailwind CSS v4, Argon2 passwords, moka cache, syntect code highlighting, katex-rs math, mimalloc allocator. The blog also runs user-submitted code in isolated Docker containers.

## Architecture & Data Flow

**The `server` feature gate is the central organizing principle.** Nearly every module gates real DB/IO/Axum logic under `#[cfg(feature = "server")]` and provides a compiling stub under `#[cfg(not(feature = "server"))]` for the WASM build. `default = ["web", "server"]` (fullstack); override to build one target only (see Development Commands).

- **Two endpoint kinds**: (1) Dioxus server functions `#[server(Name, "/api")]` (auth, posts, comments, settings, database, code_runner); (2) manual Axum routers merged into the app router in `src/main.rs` (upload, image serving, health, SSE stream).
- **Boot** (`src/main.rs`, server branch): dotenvy → tracing → build_info → hard-check `DATABASE_URL` → `validate_database_url()` → CSRF warn → a *throwaway* multi-thread tokio runtime runs `ensure_database()` + migrations + env seeders (`BACKUP_*` backup settings, `ADMIN_*` initial admin) + port pre-probe, then is dropped → `dioxus::server::serve` returns the Axum router with background tasks (session cleanup, post purge, image-cache cleanup, IP purge, sysinfo sampler, backup scheduler, code_runner readiness probe). mimalloc is the global allocator under `cfg(all(feature="server", not(wasm32)))`.
- **Read flow** (e.g. `GET /post/:slug`): middleware stack `[version_headers → ssr_generation → add_cache_control → csrf → optional compression → 30s Timeout → admin_guard]` → Dioxus IncrementalRenderer checks persisted `static/<route>/index/<hash>.html` (TTL `SSR_CACHE_SECS`, default 3600s). HIT → serve cached HTML. MISS → SSR renders `PostDetail` → `use_server_future(get_post_by_slug(slug))` → Dioxus deserializes to a server fn call → `cache::get_post_by_slug` (moka, 600s TTL) → miss → `get_conn()` from deadpool pool → query → cache set → return.
- **Write flow** (e.g. create post): admin client → POST `/api/CreatePost` → CSRF validates Origin → `get_current_admin_user()` → validate → `spawn_blocking(render_markdown_enhanced)` → BEGIN TXN → INSERT post + `sync_tags` → COMMIT → invalidate matching moka caches **and** `ssr_cache::invalidate_ssr_*` (physical dir deletion) → return.
- **Auth**: cookie-session (HttpOnly, SameSite=Lax, optional Secure), Argon2 hashing in `spawn_blocking`, moka-cached sessions re-checked against `users.session_generation` on every hit. First registered user becomes admin (atomic `INSERT ... ON CONFLICT`); alternatively `ADMIN_*` env vars sync an initial admin at boot (`sync_admin_from_env` in `src/api/auth.rs`, env wins over DB on every boot).
- **Code execution**: ` ```lang runnable ``` ` blocks and `/admin/runner` run code in Docker containers (bollard, `src/infra/docker.rs`) — read-only rootfs + tmpfs `/code`, UID 1000, resource/cap-limited, `ContainerGuard` cleanup. SSE streams output at `GET /api/exec/stream`.

## Key Directories

- `src/` — Rust source (single crate). `main.rs` (entry), `router.rs` (Dioxus routes), `middleware.rs` (axum layers: compression, cache-control, admin_guard), `cache.rs` (moka, many domain caches with distinct TTLs), `ssr_cache.rs` (physical SSR cache invalidation), `theme.rs` (light/dark/system), `highlight.rs` (syntect), `context.rs` (`UserContext` global login state).
  - `src/api/` — endpoints: `auth.rs`, `posts/` (create/update/delete/trash/list/read/search/stats/tags/rebuild/helpers/types), `assets/` (list/delete/rebuild/types), `comments/`, `settings.rs`, `database/` (admin: sql_console/export/backup/tasks), `code_runner/` (execute/readiness/languages/sse/progress), `upload.rs`, `image.rs`, `health.rs`, `feed.rs`, `friends.rs`, `changelog.rs`, `mcp_tokens.rs`. Cross-cutting: `error.rs` (`AppError`), `csrf.rs`, `rate_limit.rs`, `sanitizer.rs`, `slug.rs`, `markdown.rs` (`render_markdown_enhanced`), `katex.rs`, `mhchem.rs`.
  - `src/db/` — `pool.rs` (`DB_POOL: LazyLock<deadpool>`, `get_conn()` runtime fast-fail, `get_conn_for_startup()` retry), `migrate.rs` (`MIGRATIONS` array + runner), `retry.rs`, `mod.rs` (`format_with_sources`, `DummyPool` stub).
  - `src/models/` — `post.rs`, `user.rs`, `comment.rs`, `settings.rs`, `asset.rs`, `friend_link.rs`, `mcp_token.rs` (serde DTOs shared across SSR/cache/API).
  - `src/mcp/` — MCP server (`#[cfg(server)]`): `auth.rs` (bearer→principal middleware + rate limit + audit), `crypto.rs` (AES-GCM token encryption), `server.rs` (rmcp ServerHandler composing all tool routers), `router.rs` (StreamableHttpService mount), `config.rs` (client config generation), `tools/{read,posts,comments,tags,media,settings,runner}.rs` (tool groups) + `tools/common.rs` (shared helpers). See "MCP Server" section below.
  - `src/pages/` — route components. **`post_detail.rs` header docs are the canonical guide for `use_server_future` + route-subscription gotchas — read before editing pages.**
  - `src/tasks/` — server-only background loops spawned in `serve()`: `session_cleanup.rs`, `post_purge.rs`, `image_cache_cleanup.rs`, `ip_purge.rs`, `backup.rs` (scheduled backup). Sysinfo sampling runs from `src/sysinfo_sampler.rs`.
  - `src/infra/` — `docker.rs` (bollard), `runner_config.rs`.
  - `src/hooks/`, `src/utils/` — query/event hooks; text/time/html helpers.
  - `src/bin/generate_highlight_css.rs` — build-tool binary (regenerates `public/highlight.css` from syntect themes; `required-features = ["server"]`).
  - `src/*_bridge.rs` — wasm-bindgen bridges for JS editors/terminal (`tiptap_bridge`, `codemirror_bridge`, `xterm_bridge`).
- `libs/` — pnpm JS workspace, packages named `@yggdrasil/*`. Each builds to a self-contained IIFE bundle written **directly into `public/<dir>/`** and consumed by the Rust side via window globals (`js_sys::Reflect::get` on object-literal modules, or global `__init*` functions via a typed `invoke_optional_global` helper).
  - `tiptap-editor` → `public/tiptap/` (rich-text Markdown editor), `codemirror-editor` → `public/codemirror/` (code-runner source editor), `lightbox` → `public/lightbox/`, `xterm-terminal` → `public/xterm/`, `yggdrasil-core` → `public/yggdrasil-core/`, `mermaid-renderer` (dynamically script-injected by yggdrasil-core on viewport visibility), `shared` (cross-lib constants: `ThemeName`, `THEME_CHANGE_EVENT` — inlined into each IIFE, not bundled).
- `migrations/` — 22 numbered SQL files (`NNN_desc.sql`); each must also be registered in the `MIGRATIONS` array in `src/db/migrate.rs` (enforced by the `migrations_match_files_on_disk` test).
- `themes/` — Catppuccin Latte (light) / Mocha (dark) `.tmTheme` for syntect.
- `docker/` — `build-runners.sh` + `runner-base/` + `runner-{python,node,go,rust,bun}/` (sandbox images). The app `Dockerfile` and `Dockerfile.cross` are at the **repo root**.
- `docs/` — `agents/domain.md`, `agents/issue-tracker.md` (agent context docs). Repo-root `DEVELOPMENT.md` (perf benchmarking + highlighting guide). Repo-root `CHANGELOG.md` (Keep a Changelog v1.1.0, SemVer) is `include_str!`-embedded and served at `/changelog` (`src/api/changelog.rs`).
- `scripts/` — `xun.fish` (full-deploy pipeline to the `xun` server — build all images, scp, rolling-restart `app` only, verify).
- `static/` — Dioxus IncrementalRenderer persists SSR HTML here at runtime (gitignored output, not source).

## Development Commands

Prerequisites: Rust 1.95+, `wasm32-unknown-unknown` target, `dx` CLI (v0.7.10), `tailwindcss` CLI v4, PostgreSQL, Node 20+ / pnpm.

```bash
# Dev server (builds libs + highlight.css + katex.css first; runs pnpm install via build-libs)
make dev

# Full release build (client WASM + native server)
make build

# Linux cross-build (musl static binary)
make build-linux

# CSS
make css           # input.css -> public/style.css (one-shot)
make css-watch     # with --watch

# Lint (JS Biome + Rust clippy, no writes)
make lint

# Auto-fix (Biome -> cargo fix -> cargo fmt; 不含 dx fmt——见 Makefile 注释）
make fix

# Tests
make test          # cargo test (default features incl. server) + pnpm JS tests
cargo test --features server highlight_code_swift -- --nocapture   # single highlight test w/ output

# Docs (rustdoc, --no-deps --document-private-items, ayu theme)
make doc           # -> public/doc/ (copies target/doc)
make doc-open      # cargo doc --open (no copy)

# Build JS libs
make build-libs
make build-editor      # pnpm --filter @yggdrasil/tiptap-editor run build (others: make build-{codemirror,lightbox,core,xterm,mermaid})

# Regenerate highlight.css (only when adding new syntect scope types)
cargo run --features server --bin generate_highlight_css

# WASM-only build (e.g. to check the web target)
cargo build --no-default-features --features web --target wasm32-unknown-unknown
# Server-only build (as the Dockerfile does)
cargo build --no-default-features --features server

# Docker
make docker              # native arch, load into local daemon
make docker-amd64        # x86_64 via Dockerfile.cross (no QEMU; Apple Silicon uses Rosetta)
# make docker-apple      # removed (no recipe exists; was Apple Container CLI placeholder)
make docker-multiarch IMAGE=ghcr.io/owner/yggdrasil:latest   # amd64+arm64, push to registry
```

## Important Configuration

**Feature model** (`Cargo.toml`): `default = ["web", "server"]`. `web` = `dioxus/web` (WASM); `server` = all native deps (tokio, axum, tokio-postgres, deadpool, argon2, moka, syntect, katex-rs, mimalloc, governor, bollard, …). Most deps are `optional = true` and gated behind the `server` feature list. Release profile: `opt-level=3`, `lto="thin"`, `codegen-units=1`, `strip=symbols`, **`panic="abort"`** (shed WASM unwind metadata; server relies on systemd/k8s restart — design around `Result + ?`, never panic-driven control flow). The WASM client additionally has `[profile.wasm-release]` (`inherits="release"`, `opt-level="z"`, `lto="fat"`): dx's web release build always uses the `wasm-release` profile name and injects `opt-level="s"` only when the profile is absent from `Cargo.toml` — declaring it reclaims rustc-side size control (2.72 MB vs 3.47 MB raw). `Dioxus.toml` sets `[web] pre_compress = true` (build-time brotli `.br` sidecars; dioxus-server serves them via `ServeFile::precompressed_br()` at zero runtime CPU — wire size ~732 KB) and pins `[web.wasm_opt] level = "z"`.

**`.cargo/config.toml`**: sets `--cfg getrandom_backend="wasm_js"` for `wasm32-unknown-unknown` only (workaround for a Dioxus 0.7.10 cfg leak into the server build). The musl linker block (`[target.x86_64-unknown-linux-musl]`) is **active**; the FreeBSD block is a commented template.

**`build.rs`**: injects `YGG_BUILD_GIT_DESCRIBE/HASH/COMMIT_DATE` + rustc version + build time via `cargo:rustc-env` (read by `src/build_info.rs` through `env!`). Git fields use 3-tier fallback (env var → local `git` → `"unknown"`); rustc version falls back to `"unknown"`, build time to `"0"` on clock failure. `rerun-if-changed=.git/HEAD` + `.git/index`. std-only (no build-deps).

**Self-contained binary**: migrations (`src/db/migrate.rs` `include_str!`), custom syntaxes (`src/highlight.rs` `include_str!`), and `public/highlight.css` (pre-generated at build time) are embedded — the runtime image (`alpine:3.22` + `postgresql16-client` for `pg_dump`/`psql` backup support) needs only the binary + `public/` + `uploads/`.

**Key env vars** (see `.env.example` for the full reference; no mailer, no `LISTEN_ADDR` — uses `IP`/`PORT`, no `UPLOAD` dir env, no session-lifetime env; `ADMIN_*` is the startup initial-admin sync, see Bootstrap below):

| Category | Var | Purpose (defaults) |
|---|---|---|
| Database | `DATABASE_URL` | PostgreSQL connection string (required) |
| Database | `DB_POOL_SIZE` | deadpool pool size (20) |
| Database | `STATEMENT_TIMEOUT_SECS` | per-query SQL timeout (30) |
| Database | `MIGRATE_STARTUP_TIMEOUT_SECS` | startup DB-connect retry window (30) |
| Server | `RUST_LOG` | tracing filter (`info`) |
| Server | `IP` / `PORT` | bind address (set in Dockerfile `0.0.0.0:3000`) |
| Server | `DIOXUS_PUBLIC_PATH` | public assets path (Dockerfile `/app/public`) |
| Bootstrap | `ADMIN_USERNAME` / `ADMIN_EMAIL` / `ADMIN_PASSWORD` | startup initial-admin sync (create or overwrite-password + ensure admin role on every boot; unset/empty = disabled) |
| Backup | `BACKUP_AUTO_ENABLED` / `BACKUP_TIME_UTC` / `BACKUP_RETENTION_COUNT` / `BACKUP_INCLUDE_UPLOADS` | scheduled backup settings (seeded by `BACKUP_*` env on first boot only; thereafter admin panel/DB) |
| Perf | `SSR_CACHE_SECS` | SSR page cache TTL (3600) |
| Perf | `COMPRESSION_ALGORITHMS` | response compression — gzip/brotli/deflate/zstd/`all`/`off` (**off**) |
| Perf | `TOKIO_WORKER_THREADS` | tokio workers (read by runtime, not app code) |
| Perf | `SYSINFO_SAMPLE_SECS` | `/admin/system` metric interval (0.5) |
| Security | `APP_BASE_URL` | CSRF trusted origin (prod strongly recommended; else Host-header fallback) |
| Security | `COOKIE_SECURE` | add `Secure` to session cookie (false) |
| Security | `TRUSTED_PROXY_COUNT` | reverse-proxy hop count for real-IP from XFF (0) |
| Security | `EXPOSE_VERSION_HEADERS` | attach Server/X-Yggdrasil-Version/Git/Hash headers (true) |
| Security | `MAX_SESSIONS_PER_USER` | concurrent-session cap w/ LRU evict (5) |
| MCP | `MCP_TOKEN_ENC_KEY` | AES-GCM-256 token encryption key (hex 32 bytes) |
| Images | `WEBP_QUALITY` / `WEBP_METHOD` | WebP encode quality (85) / method (2) |
| Images | `MAX_IMAGE_DIMENSION` / `MAX_IMAGE_PIXELS` | max edge px (8192) / total pixels (50M) |
| Images | `IMAGE_DISK_CACHE_MAX_MB` / `_MAX_AGE_HOURS` | `uploads/.cache` cap (1024) / retention (168 = 7d) |
| Images | `IMAGE_DIMENSIONS_CACHE_TTL_SECS` | image-dimensions moka cache TTL |
| Upload | `UPLOAD_CONCURRENCY` | concurrent image upload limit |
| Rate limit | `RATE_LIMIT_{STRICT,UPLOAD,IMAGE,COMMENT,CODE_EXEC,UNKNOWN,MCP,MCP_UPLOAD}_PER_SEC/_BURST`, `RATE_LIMIT_CODE_EXEC_DAILY`, `RATE_LIMIT_GC_INTERVAL_SECS` | governor buckets keyed by client IP (MCP/MCP_UPLOAD keyed by token) |
| Runners | `CODE_RUNNER_ALLOW_NETWORK` / `_MAX_CONCURRENT` / `_MAX_CPU_CORES` / `_MAX_MEMORY_MB` / `_MAX_TIMEOUT_SECS` / `_MAX_OUTPUT_BYTES` / `_MAX_SOURCE_BYTES` / `_QUEUE_TIMEOUT_SECS` / `_TASK_TTL_SECS` | sandbox limits |
| Runners | `CODE_RUNNER_LANGUAGES` | optional allow-list (default: all registered) |
| Runners | `DOCKER_SOCKET_PATH` | docker.sock for bollard (`/var/run/docker.sock`) |

**Production deployment**: the app does NOT do TLS — a reverse proxy (nginx/Caddy) is mandatory. **MUST set** `APP_BASE_URL`, `COOKIE_SECURE=true`, `TRUSTED_PROXY_COUNT` (exact proxy hop count — a wrong value lets attackers spoof XFF to bypass rate limits or makes all users share one proxy-IP bucket). nginx: `client_max_body_size 12m` (app hard-limits 10 MiB), `proxy_read/send_timeout 360s` (image transcoding up to 300s). Bind `127.0.0.1:3000:3000`, not `0.0.0.0`. Health: `/healthz` (liveness), `/readyz` (readiness, `SELECT 1`).

## Code Conventions & Common Patterns

1. **Dual-target gating.** Any code touching DB/IO/Axum must gate impl under `#[cfg(feature = "server")]` and provide a compiling `#[cfg(not(feature = "server"))]` stub. Never put server-only deps in code reachable by the web build. The `DummyPool` stub in `src/db/mod.rs` exists for this — do not delete it.
2. **Server functions.** `#[server(FnName, "/api")] pub async fn name(args...) -> Result<T, ServerFnError>`. Args/return serde-serializable. Re-export from the module's `mod.rs` (`pub use create::create_post;`).
3. **Error handling.** Never `?` a raw DB error into `ServerFnError`. Use `AppError` constructors (`db_conn`/`query`/`tx`) which log the full chain via `db::format_with_sources` but expose a generic message — they never leak SQL. Map domain failures via `AppError::Unauthorized/Forbidden/NotFound/BadRequest/Internal` then `.into()`. Validation/business rejections return `Ok(Response{success:false,...})`, **not** `Err`.
4. **Component purity (Dioxus 0.7).** `#[component]` bodies and `rsx!` must be **pure** — no `signal.set`, `spawn`, DOM calls, or side effects in the render body. Derive data inline or via signals; do effects in `use_effect`. Don't store derivable data in `use_signal`. (See the `dioxus-render-purity` skill.)
5. **Async data in pages.** Use `use_server_future(move || { ... })?`. To re-run on route-param change you MUST read the router state **inside the closure** via `router().current::<Route>()` (it subscribes via `ReactiveContext`) — a moved `String` prop is a frozen snapshot that won't re-trigger. To force a child remount on identity change (e.g. slug), wrap it in a single-element `for x in std::iter::once(...) { Comp { key: "{x}" } }` — a bare `key` on a non-list element is ignored by Dioxus's diff. See `src/pages/post_detail.rs` header docs.
6. **Auth guard.** Every admin server fn starts with `let user = get_current_admin_user().await?;`. The SSR `admin_guard` middleware is a fast-path 302 (fail-OPEN on DB error); the client `AdminLayout` is the backstop. Don't rely solely on the middleware for security decisions in server fns.
7. **CPU-bound work** (Argon2, syntect/markdown render) MUST go in `tokio::task::spawn_blocking` — never on the async worker.
8. **Caching.** Read-through on reads (`cache::get` → miss → db → `cache::set`); on writes call the matching `cache::invalidate_*` **and** `ssr_cache::invalidate_ssr_*` before returning. Use the `CacheKey` enum; don't hand-roll keys. Note: `ssr_cache::GLOBAL_GENERATION` / `X-SSR-Generation` is **observability only** — real SSR freshness is physical dir deletion + `SSR_CACHE_SECS` TTL.
9. **Migrations.** Create `migrations/NNN_desc.sql` **and** append `("NNN", include_str!("../../migrations/NNN_desc.sql"))` to the `MIGRATIONS` array in `src/db/migrate.rs` (the `#[test] migrations_match_files_on_disk` guards file/array parity). Each migration runs in its own transaction; write them idempotent-safe.
10. **DB connections.** Runtime path = `get_conn()` (fast-fail — do NOT retry pool-full `Timeout`, to avoid avalanche); startup path = `get_conn_for_startup()`. `statement_timeout` is injected globally via libpq options — don't add per-query timeouts.
11. **Markdown/HTML rendering** (`render_markdown_enhanced`): pulldown-cmark + syntect classed highlighting + TOC + heading anchors, CPU-bound → `spawn_blocking`. **Article HTML is rendered once at save time and stored in `posts.content_html`** — modifying syntaxes does not auto-refresh existing posts; rebuild via the `/admin/posts` "rebuild all" button (`rebuild_content_html`, batch size 500).
12. **Code highlighting** (`src/highlight.rs`): syntect `ClassedHTMLGenerator` emits CSS classes paired with `public/highlight.css`. To add/fix a language: edit `syntaxes/<Lang>.sublime-syntax` (the `expression` context's `include` order matters — multi-token rules before single-token ones), validate the YAML, add a test asserting CSS classes, run `cargo test --features server highlight_code_<lang> -- --nocapture`, and regenerate `public/highlight.css` only if a new scope type was added (`cargo run --features server --bin generate_highlight_css`). See `DEVELOPMENT.md` for the full guide.
13. **WebP**: the `image` crate's `"webp"` feature is **intentionally excluded** — all WebP encode/decode goes through zenwebp (`src/webp.rs`). Do NOT add it.
14. **JS libs** (`libs/`): pnpm workspace, TypeScript strict (target ES2020, `verbatimModuleSyntax` ⟹ use `import type`), Biome formatter (2-space, single quotes, semicolons, `trailingCommas: all`, line width 100), Vite 8 IIFE bundles written into `../../public/<dir>/`. `@yggdrasil/shared` is inlined into each IIFE — IIFEs cannot import each other at runtime. Use `make build-libs` or `make build-<name>` (`pnpm --filter`).
15. **Heavy `//!` module docs explain WHY.** Read a module's top doc comment before editing it. User-facing strings are predominantly Chinese.
16. **No `unwrap()` in non-test code.** `panic = "abort"` (see Profile config) means any panic kills the whole process — there is no unwind, no recovery, just an immediate crash. This is incompatible with `unwrap()`'s "I'll deal with it later" semantics. Rules:
    - **Default to `?` / `Result`** for fallible operations (DB, IO, parsing, header construction, regex compilation at call sites). Map failures through `AppError` (server) or return `Option` (WASM).
    - **`.expect("reason")` is permitted only for true invariants** — cases where a `None`/`Err` would indicate a code bug, not a runtime condition. The message MUST explain *why* it cannot fail (e.g. `"val.max(1) 保证非零"`, `"etag 仅含 ASCII hex"`, `"静态 302 响应必然构造成功"`). A bare `.expect("TODO")` or `.expect("unreachable")` is equivalent to `unwrap` and is not acceptable.
    - **`LazyLock` / `OnceLock` initialization of compile-time constants** (static `Regex`, `NonZeroU32::new(val.max(1))`, syntect's built-in `Plain Text` syntax) may use `.expect()` with an explanatory message — these run at most once and a failure means the source constant itself is wrong, which should surface immediately at startup, not silently degrade.
    - **WASM browser-context calls** (`web_sys::window()`, `Reflect::get` on a known global) may use `.expect()` only inside `#[cfg(target_arch = "wasm32")]` / `#[component]` / `use_effect` scopes where a missing `window` proves the code is running outside a browser — a deployment bug, not a runtime input.
    - **`unreachable!()` is permitted ONLY in `#[cfg(not(feature = "server"))]` stubs** of server functions — these branches are compiled out of the real server build and exist solely to satisfy the WASM target's type checker.
    - **Build-tool binaries** (`src/bin/*`) and **`#[cfg(test)]` modules** are exempt — `unwrap`/`expect`/`panic` are idiomatic in tests and one-shot codegen tools.
    - **If clippy's `unwrap_used` / `expect_used` lints are later wired in** (`[lints]` table in `Cargo.toml`), the exemptions above are the intended `allow` set; do not relax them further without a documented invariant.
17. **Reuse shared UI components — never hand-roll a duplicate.** Before writing any `rsx!` element with an inline Tailwind class string, scan `src/components/` for an existing component or style constant and reuse it. Two modules own this surface: `ui.rs` (display atoms — `Pagination`, `StatusBadge`, `Tooltip`, `Popover`, `FilterTabs`, `LoadingButton`, `CollapsibleSettingsCard`; plus class constants `BTN_PRIMARY`/`BTN_PRIMARY_SM`/`BTN_OUTLINE`/`BTN_DANGER_OUTLINE`/`BTN_SECONDARY`/`BTN_TEXT_*`/`BTN_SOLID_*`/`BTN_CLOSE_ICON`/`BTN_ICON`/`ADMIN_CARD_CLASS`/`ADMIN_TABLE_CLASS`/`ADMIN_ROW_HOVER`/`BADGE_BASE`/`CHECKBOX_CLASS`/`CHECKBOX_DANGER_CLASS`/`SPINNER_SVG`) and `forms.rs` (form controls — `FormInput`, `FormSelect`, `FormLabel`, `AlertBox`, `TimePicker`; plus class constants `INPUT_CLASS`/`INPUT_INLINE_CLASS`/`BUTTON_PRIMARY_CLASS`/`FORM_SELECT_COMPACT_CLASS`). Use `FormInput` with the right `class` prop (`INPUT_CLASS` = full-width form field, `INPUT_INLINE_CLASS` = flank a button / fill remaining width) instead of a bespoke `<input class="...">`; use `FormSelect` instead of a native `<select>` (OS-rendered popups can't follow the theme). For composite patterns (search bars, filter toolbars, action rows), grep for an existing instance and mirror its structure — e.g. a `FormInput { r#type: "search", ... }` flanked by `BTN_PRIMARY`/`BTN_OUTLINE` search bar lives in `asset_picker.rs` (using `INPUT_INLINE_CLASS`) and `assets.rs` (custom class string). Additional display components live under `src/components/` submodules (`post/`, `comments/`, `code_runner/`, `skeletons/`) — grep before creating a new one. A second convention beside an existing one is PROHIBITED — if the existing component is missing a feature, extend it rather than spawning a parallel one.

## MCP Server

The blog is also a **Model Context Protocol server** (`/mcp` endpoint, Streamable HTTP, stateless per SEP-2567; plus a bearer-authenticated `/api/mcp/upload` multipart endpoint for media uploads). The admin's AI clients (Claude Code / Cursor / Cline / Oh-My-Pi / OpenCode) connect via bearer token to query published posts as a knowledge base and perform nearly all backend operations.

- **Transport**: official `rmcp` crate **pinned to `=3.0.0-beta.3`** (NOT stable `0.2.1` — it lacks Origin validation, protocol-version guard, body-size cap, and stateless config, all of which the 3.x beta ships). Mounted via `StreamableHttpService` → `Router::nest_service("/mcp", service)` in `src/mcp/router.rs`, merged into the app router in `src/main.rs`. rmcp handles Origin→403, `MCP-Protocol-Version` 400, and 4MiB body cap internally.
- **Auth**: axum `from_fn` middleware (`src/mcp/auth.rs`) parses `Authorization: Bearer ygg_...`, SHA-256-hashes it, does a constant DB lookup on `mcp_tokens.token_hash`, and injects `McpPrincipal { user_id, scope, token_id }` into `request.extensions()`. Tools read it via the rmcp `Extension<http::request::Parts>` extractor. Token-keyed rate limiter (429), `last_used_at` throttle (≥60s), and audit `tracing::info!` live in the middleware.
- **Token storage**: AES-GCM-256 ciphertext (`token_enc`, hex of nonce‖ct‖tag) + SHA-256 hash (`token_hash`) in `mcp_tokens` table (migration 017). Plaintext never stored; admin can re-reveal via decryption (key from `MCP_TOKEN_ENC_KEY` env, hex 32 bytes). Token mgmt server fns in `src/api/mcp_tokens.rs`, admin UI at `/admin/mcp`.
- **Scopes**: `read` < `write` < `admin` (partial order via `TokenScope::grants()` in `src/models/mcp_token.rs`). read = published-only knowledge base; write = + posts/comments/tags/media (drafts visible); admin = + settings + code runner. Scope checked per-tool.
- **Tool composition**: each tool group (`src/mcp/tools/<x>.rs`) uses `#[tool_router(router = <x>_router, vis = "pub")] impl YggMcpServer { ... }` to emit a public router fn returning `ToolRouter<YggMcpServer>`; `src/mcp/server.rs` composes them with `+` into one `ServerHandler`. All tools impl on the **same** `YggMcpServer` type (required for `ToolRouter<S>` `Add`). To add a tool group: write `tools/<name>.rs` with a named pub router on `YggMcpServer`, register in `tools/mod.rs`, and add `+ YggMcpServer::<name>_router()` in `server.rs::combined_router()`.
- **Feature gating**: the entire `src/mcp/` module is `#[cfg(feature = "server")]` in `main.rs`. The WASM admin UI gets its DTOs/server fns from `src/api/mcp_tokens.rs` (not from MCP module code). `rmcp`/`aes-gcm` are server-only deps (`base64` is a transitive rmcp dep, deliberately not promoted to a direct dep — see `src/mcp/crypto.rs`). The WASM build never touches MCP code — no stubs needed.
- **Config generation**: `src/mcp/config.rs` + the `get_mcp_client_configs` server fn emit ready-to-paste JSON for Claude Code / Cursor / Cline / Oh-My-Pi / OpenCode / generic + a `claude mcp add` CLI one-liner.
## Workflow

- **每完成一个功能点立即提交**。Agent 自主判断提交时机——当一个逻辑完整的改动通过验证(编译通过 / 测试通过)后,无需等待用户指令,直接 `git add` + `git commit`。
- 提交粒度按"功能点"而非"文件":相关联的多文件改动合并为一个提交,不相关的改动拆成多个提交。
- 提交信息遵循现有风格:`type(scope): 简述`,正文(可选)说明动机与关键改动。常见 type:`feat` / `fix` / `docs` / `refactor` / `chore` / `perf`。
- 只在用户明确要求时才 `git push`。提交到本地即可,不主动推送。
- **修改完成后通知用户验证**：每次修改完代码后，AI 不要自己去验证，直接通知用户去验证。

## Testing & QA

- **Layout**: mostly inline `#[cfg(test)] mod tests` unit tests across `src/` (~58 modules) plus exactly one integration file `tests/post_detail_slug_rerun.rs` (a source-string guard asserting a Dioxus render-purity antipattern is absent).
- **Philosophy**: pure-function unit tests deliberately decoupled from DB/FS/cache. Inject dependencies (closures, temp dirs) instead of touching live state. **No test connects to live Postgres** — there is no test DB harness.
- **Feature gating**: server-touching tests use `#[cfg(all(test, feature = "server"))]`; pure-logic tests use plain `#[cfg(test)]`.
- **`serial_test`** serializes tests that mutate *process-global* state (moka cache singletons in `cache.rs`, env vars in `csrf`/`rate_limit`, in-process task maps in `progress.rs`, Docker in `infra/docker.rs`) — never DB rows.
- **Docker tests** auto-skip when the daemon (or the required image) is unavailable (`require_docker_with_image(image)` → `None` → `eprintln!("skip: ...")`).
- **Run**: `make test` (runs `cargo test` with default features incl. `server`, plus `pnpm` JS tests). For a single Rust highlight test: `cargo test --features server highlight_code_<lang> -- --nocapture`.
- **Highlighting tests** (`src/highlight.rs`, ~35 tests) are the canonical example of the test philosophy; include compile-consistency tests (`custom_syntax_list_matches_directory`, `migrations_match_files_on_disk`) that keep embedded arrays in sync with their on-disk directories.
- **Coverage**: no formal coverage target; tests defend invariants and guard footguns (migration/syntax array parity, render-purity anti-patterns).

## CI

GitHub Actions (`.github/workflows/ci.yml`), **nine** jobs following the 2026 native multi-runner pattern (zero QEMU): three run on **every push, PR, and manual trigger** — `test` (cargo fmt/clippy + `cargo test --all-features`), `lint-js` (Biome on `libs/`), and `check-wasm` (wasm32 target check). `build-amd64` (native x86_64 runner, **plain `Dockerfile`** at repo root with cargo-chef + GHA layer cache; calls `docker buildx` directly — NOT `make docker-amd64` — to inject `--cache-from/to type=gha`; includes a container-boot smoke test that runs the image without Postgres and greps the startup banner) runs on **`main`/`master` push, tag push, or manual** (PRs and non-trunk branches rely on `test` only — saving CI minutes and GHA cache quota). `build-arm64` (native `ubuntu-24.04-arm` runner, plain Dockerfile builds `aarch64-unknown-linux-musl` natively — 4-8× faster than QEMU), `publish-ghcr` (merges amd64+arm64 into a multi-arch manifest → GHCR; `deploy` pulls the `latest` manifest from here — GHCR is now the deploy image source), `publish-runners` (builds the 6 Code Runner sandbox images → GHCR, **skipped automatically when `docker/` is unchanged since the previous tag**), `release` (three GitHub Release assets: `yggdrasil-amd64.tar.gz`, `yggdrasil-arm64.tar.gz`, `yggdrasil-x86_64-musl.tar.gz`; body extracted from `CHANGELOG.md`), and `deploy` (SSH to xun → pulls app + 6 runner images from GHCR with 3× retry, tags runners to the `LANGUAGES`-registry short names `yggdrasil-runner-*:latest` → `docker compose up -d app` rolling restart, postgres/data volumes untouched → polls `/readyz` via `nginx-proxy`; runner pulls are best-effort — a failed pull keeps the existing tag, the app deploy is never blocked; `scripts/xun.fish` remains a local-build+scp fallback when GHCR is unreachable) all run **only on tag push (`v*`) or `workflow_dispatch`**. **Note**: CI builds with the plain `Dockerfile` (repo root) natively on each arch's runner; `Dockerfile.cross` (repo root, two-builder glibc+zig cross-compile) is the **local-only** path used by `make docker-amd64` / `scripts/xun.fish` on Apple Silicon — CI never invokes it. Required `deploy` secrets: `XUN_SSH_KEY`, `XUN_HOST`, `XUN_USER`; optional `XUN_PORT`, `XUN_DEPLOY_DIR` (default `/root/docker/yggdrasil`), `XUN_KNOWN_HOSTS` (server fingerprint — recommended over the `ssh-keyscan` TOFU fallback). The deploy key's public half goes in xun's `~/.ssh/authorized_keys`.
## Agent skills

### Issue tracker

GitHub Issues at `VOD-Studio/yggdrasil` (uses `gh` CLI). See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical roles (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Multi-context layout: `CONTEXT-MAP.md` points to the Rust/Dioxus and JavaScript workspace contexts. See `docs/agents/domain.md`.
