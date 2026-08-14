# Rust/Dioxus application context

This context covers the fullstack Yggdrasil blog/CMS in `src/`: Dioxus 0.7 pages and components, Axum/server functions, PostgreSQL persistence, SSR, authentication, admin workflows, and shared DTOs.

## Current vocabulary

- **素材**: an image registered in the `assets` table and stored below `uploads/`; `AssetDto` carries the image metadata and reference information used by admin views.
- **素材选择弹窗**: the reusable `AssetPickerModal` used by cover editors, avatar editors, and the post body's Tiptap slash command 「素材库」; it loads server-side paginated assets and returns picks to its parent as `Vec<AssetSelection>` — single-select mode (cover/avatar) yields exactly one element, `multi` mode (post body) yields the check-order batch whose serde shape `[{"src","alt"?}]` is the editor bridge contract for `insertImagesFromLibrary`.
- **友链**: a front-end friend-link record edited through `/admin/friends`.
- **分页列表**: a server-function response containing the current page's records and the total matching count; page numbers are one-based.

Keep new domain decisions and Rust-specific ADRs under this context when they are resolved.
