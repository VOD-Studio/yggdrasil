# Repository Guidelines

## Project Structure & Module Organization

Yggdrasil is a Rust 2021/Dioxus fullstack blog and CMS with an Axum backend and PostgreSQL.

- `src/pages/` and `src/components/`: UI; `src/api/`, `src/db/`, and `src/mcp/`: backend; `src/models/`: shared types. Consult `src/CONTEXT.md` for domain terminology.
- `libs/`: pnpm workspace for editors, lightbox, terminal, and browser utilities. Consult `libs/CONTEXT.md` for bundle and shared-code conventions.
- `migrations/`: sequential SQL migrations (`NNN_description.sql`), applied at startup. Register each new SQL file in the `MIGRATIONS` array in `src/db/migrate.rs`, keeping versions in ascending order.
- `public/`: static assets and generated bundles. Edit `input.css` and `libs/` sources, then rebuild generated CSS/JS. Keep `public/style.css` untracked; it is ignored build output.
- `scripts/`: browser regression tooling; `docker/`: deployment and code-runner support.

## Build, Test, and Development Commands

Prefer the local toolchain; use Docker only when required local tools are missing. Native development needs Rust with the `llvm-tools` component, Dioxus CLI 0.7.10, and Tailwind CSS v4 CLI. Follow `engines.node` and `packageManager` in `libs/package.json` for Node and pnpm requirements. Run `make check-dev-tools` or `make check-build-tools` to check tool availability before development or release builds.

Prefer the Makefile targets for development and release builds: they clear `RUSTC_WRAPPER` for `dx` to avoid sccache conflicts, disable SSR caching during development with `SSR_CACHE_SECS=0`, and restore source WebP files after release builds to preserve animations.

- `make dev`: build assets and start native `dx serve`.
- `make build`: assemble release assets, documentation, and application; requires Brotli CLI.
- `make build-libs` / `make css`: rebuild frontend libraries / Tailwind CSS.
- `make test`: run Rust and frontend tests.
- `make lint`: run Biome, TypeScript checks, Clippy for the native target, and rustfmt checks.
- `cargo fmt` and `(cd libs && pnpm format)`: format Rust and frontend files.
- `cargo check --locked --target wasm32-unknown-unknown --no-default-features --features web`: verify the browser target.
- `cargo clippy --locked --target wasm32-unknown-unknown --no-default-features --features web -- -D warnings`: lint Rust for the browser target when changing WASM paths; native Clippy does not cover WASM-gated code.

Docker fallbacks: `make docker-dev` (port 8080), `make docker-test`, `make docker-lint`, and `make docker-fmt`. Use `make docker-run CMD='<command>'` for other checks; `make docker-dev-down` stops development containers.

## Coding Style & Naming Conventions

Use rustfmt defaults: four-space indentation, `snake_case` modules/functions, and `PascalCase` types/components. Keep server-only dependencies behind feature gates and browser APIs behind WASM gates.

Frontend Biome settings use two spaces, single quotes, semicolons, and 100-column lines. Follow existing kebab-case filenames. Avoid routine `dx fmt`; the Makefile documents RSX corruption risks.

## Testing Guidelines

Rust tests live in inline `#[cfg(test)] mod tests`, including Tokio async tests. Frontend tests use Vitest, with happy-dom for browser utilities, under each `libs/` package's `src/` directory (`**/*.test.{ts,js}` or `__tests__/`). Name tests after observable behavior; add regression coverage for fixes. No numeric coverage threshold is configured.

`make test` skips `#[ignore]` database regressions. Run relevant database regressions when changing their workflows, using the disposable databases required by each test: `ygg_mcp_test` for posts/uploads and `ygg_notes_test` for notes. See [database test commands](README.md#数据库回归测试) and [notes regression guidance](docs/notes.md#回归与待补验收); these tests can reset user and content fixtures.

For component showcase changes, follow [docs/components-showcase.md](docs/components-showcase.md). Keep catalog metadata and preview dispatch aligned, reuse production display code with fixtures and local callbacks, and clean up browser instances on unmount. Verify previews do not send authentication, upload, comment submission, or code execution requests with `scripts/test-components-showcase.cjs`.

For navigation changes, follow `scripts/README-view-transitions.md`; verify direct loads, SPA navigation, themes, mobile layouts, and reduced motion.

## Commit & Pull Request Guidelines

Use `type(scope): summary`, commonly with Chinese summaries, e.g. `fix(navigation): 恢复后台滚动位置`. After completing and validating each functional unit, create a scoped commit autonomously without asking for confirmation. Include only changes belonging to that unit; do not bundle unrelated work. Push only when requested. PRs should explain behavior, link relevant issues, report checks and limitations, and include screenshots for UI changes.

## Security & Production Configuration

Copy `.env.example` to `.env`; keep credentials untracked. Development Compose expects external PostgreSQL on `global-databases_default`. Production requires a TLS reverse proxy, `APP_BASE_URL`, `COOKIE_SECURE=true`, and an accurate `TRUSTED_PROXY_COUNT`.
