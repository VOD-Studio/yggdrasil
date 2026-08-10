# Rust/Dioxus application context

This context covers the fullstack Yggdrasil blog/CMS in `src/`: Dioxus 0.7 pages and components, Axum/server functions, PostgreSQL persistence, SSR, authentication, admin workflows, and shared DTOs.

## Current vocabulary

- **素材**: an image registered in the `assets` table and stored below `uploads/`; `AssetDto` carries the image metadata and reference information used by admin views.
- **素材选择弹窗**: the reusable `AssetPickerModal` used by cover and avatar editors; it loads server-side paginated assets and returns the selected upload URL to its parent.
- **友链**: a front-end friend-link record edited through `/admin/friends`.
- **分页列表**: a server-function response containing the current page's records and the total matching count; page numbers are one-based.

Keep new domain decisions and Rust-specific ADRs under this context when they are resolved.
