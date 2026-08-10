# JavaScript workspace context

This context covers the packages under `libs/`, their browser-side IIFE bundles, and generated assets under `public/`.

## Current vocabulary

- **IIFE bundle**: a self-contained package build written directly to a `public/<package>/` directory and consumed by the Rust/WASM side through browser globals.
- **shared package**: compile-time-shared TypeScript constants inlined into each IIFE; packages do not import one another at runtime.

Keep JavaScript-specific domain decisions and scoped ADRs under this context when they are resolved.
