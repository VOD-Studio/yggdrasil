# JavaScript workspace context

This context covers the packages under `libs/`, their browser-side IIFE bundles, and generated assets under `public/`.

## Current vocabulary

- **IIFE bundle**: a self-contained package build written directly to a `public/<package>/` directory and consumed by the Rust/WASM side through browser globals.
- **shared package**: compile-time-shared TypeScript constants inlined into each IIFE; packages do not import one another at runtime.
- **lightbox session**: `lightbox/src/controller.ts` owns one open viewer. Its resource scopes dispose image callbacks, listeners, timers and animation frames before replacement or unmount; `images.ts` owns image discovery and lazy-loading bindings. The public window bridge remains in `index.ts`.

The lightbox's DOM/geometry regressions run with its Vitest suite. `bun scripts/test-lightbox.cjs` (from the repository root) uses the existing Playwright installation and the built IIFE to check actual browser animations, themes, mobile gestures, reduced motion and SPA cleanup. Set `PLAYWRIGHT_MODULE` or `CHROMIUM_PATH` when their installed locations differ.

- **editor command UI**: `tiptap-editor/src/slash-command.ts` keeps the extension and public exports. The sibling directory owns commands, popup, runnable info, language selector and modal. Popup/modal listeners are tied to editor destruction; closing a modal cancels deferred keyboard registration.

Keep JavaScript-specific domain decisions and scoped ADRs under this context when they are resolved.
