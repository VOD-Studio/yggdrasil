# Rust/Dioxus application context

This context covers the fullstack Yggdrasil blog/CMS in `src/`: Dioxus 0.7 pages and components, Axum/server functions, PostgreSQL persistence, SSR, authentication, admin workflows, and shared DTOs.

## Current vocabulary

- **素材**: an image registered in the `assets` table and stored below `uploads/`; `AssetDto` carries the image metadata and reference information used by admin views.
- **素材选择弹窗**: the reusable `AssetPickerModal` used by cover editors, avatar editors, and the post body's Tiptap slash command 「素材库」; it loads server-side paginated assets and returns picks to its parent as `Vec<AssetSelection>` — single-select mode (cover/avatar) yields exactly one element, `multi` mode (post body) yields the check-order batch whose serde shape `[{"src","alt"?}]` is the editor bridge contract for `insertImagesFromLibrary`.
- **友链**: a front-end friend-link record edited through `/admin/friends`.
- **分页列表**: a server-function response containing the current page's records and the total matching count; page numbers are one-based.
- **文章写入业务**: `api/posts/service.rs` receives an authenticated author ID and owns validation, transactions, tag/asset references and cache invalidation after commit. Browser server functions and MCP tools adapt their protocols to this module.
- **完整更新与部分更新**: the browser editor replaces a `PostDraft`; MCP sends a `PostPatch`. `FieldChange::{Keep, Set, Clear}` distinguishes omitted nullable fields from explicit clearing. MCP publication dates and `expected_updated_at` remain protocol-specific inputs.
- **编辑页状态**: `pages/admin/write/` separates the form, editor lifecycle, save snapshot and display components. `EditorHandle` owns browser callbacks and destroys Tiptap on unmount; editor readiness and save progress are separate state enums.

Keep new domain decisions and Rust-specific ADRs under this context when they are resolved.
