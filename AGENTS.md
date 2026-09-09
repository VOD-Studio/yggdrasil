# Repository Guidelines

## Project Structure & Module Organization

Yggdrasil is a Rust 2021/Dioxus fullstack blog and CMS with an Axum backend and PostgreSQL.

- `src/pages/` and `src/components/`: UI; `src/api/`, `src/db/`, and `src/mcp/`: backend; `src/models/`: shared types. Consult `src/CONTEXT.md` for domain terminology.
- `libs/`: pnpm workspace for editors, lightbox, terminal, and browser utilities.
- `migrations/`: sequential SQL migrations (`NNN_description.sql`), applied at startup.
- `public/`: static assets and generated bundles. Edit `input.css` and `libs/` sources, then rebuild generated CSS/JS.
- `scripts/`: browser regression tooling; `docker/`: deployment and code-runner support.

## Build, Test, and Development Commands

Prefer the local toolchain; use Docker only when required local tools are missing. Native development needs Rust, Dioxus CLI, Tailwind CLI, Node, and the pnpm version in `libs/package.json`.

- `make dev`: build assets and start native `dx serve`.
- `make build`: assemble release assets, documentation, and application; requires Brotli CLI.
- `make build-libs` / `make css`: rebuild frontend libraries / Tailwind CSS.
- `make test`: run Rust and frontend tests.
- `make lint`: run Biome, TypeScript checks, Clippy, and rustfmt checks.
- `cargo fmt` and `(cd libs && pnpm format)`: format Rust and frontend files.
- `cargo check --locked --target wasm32-unknown-unknown --no-default-features --features web`: verify the browser target.

Docker fallbacks: `make docker-dev` (port 8080), `make docker-test`, `make docker-lint`, and `make docker-fmt`. Use `make docker-run CMD='<command>'` for other checks; `make docker-dev-down` stops development containers.

## Coding Style & Naming Conventions

Use rustfmt defaults: four-space indentation, `snake_case` modules/functions, and `PascalCase` types/components. Keep server-only dependencies behind feature gates and browser APIs behind WASM gates.

Frontend Biome settings use two spaces, single quotes, semicolons, and 100-column lines. Follow existing kebab-case filenames. Avoid routine `dx fmt`; the Makefile documents RSX corruption risks.

## Testing Guidelines

Rust tests live in inline `#[cfg(test)] mod tests`, including Tokio async tests. Frontend tests use Vitest, with happy-dom for browser utilities, in `src/**/*.test.{ts,js}` or `src/__tests__/`. Name tests after observable behavior; add regression coverage for fixes. No numeric coverage threshold is configured.

For navigation changes, follow `scripts/README-view-transitions.md`; verify direct loads, SPA navigation, themes, mobile layouts, and reduced motion.

## Commit & Pull Request Guidelines

Use `type(scope): summary`, commonly with Chinese summaries, e.g. `fix(navigation): 恢复后台滚动位置`. Keep commits scoped to validated logical changes; push only when requested. PRs should explain behavior, link relevant issues, report checks and limitations, and include screenshots for UI changes.

## Security & Production Configuration

Copy `.env.example` to `.env`; keep credentials untracked. Development Compose expects external PostgreSQL on `global-databases_default`. Production requires a TLS reverse proxy, `APP_BASE_URL`, `COOKIE_SECURE=true`, and an accurate `TRUSTED_PROXY_COUNT`.
