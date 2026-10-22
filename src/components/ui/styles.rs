//! Shared class names and spinner markup.
// ===========================================================================
// 样式常量
// ===========================================================================

/// 行内加载 spinner：环形渐变 + 自旋动画，用 currentColor 继承文字色。
///
/// 内联 SVG（含 `@keyframes`），通过 `dangerous_inner_html` 注入；尺寸由外层
/// Tailwind 类（如 `w-3.5 h-3.5`）控制。源文件 `public/icons/90-ring-with-gradient.svg`。
pub const SPINNER_SVG: &str = r#"<svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg"><defs><linearGradient id="yggSpinnerGrad"><stop offset="0%" stop-color="currentColor" stop-opacity="1"/><stop offset="100%" stop-color="currentColor" stop-opacity="0.25"/></linearGradient></defs><style>@keyframes yggSpin { to { transform: rotate(360deg); } } .ygg-spinner-circle { transform-origin: 50% 50%; stroke: url(#yggSpinnerGrad); fill: none; animation: yggSpin .5s infinite linear; }</style><circle cx="10" cy="10" r="8" class="ygg-spinner-circle" stroke-width="2"/></svg>"#;

/// 徽章底座（`.badge`，中性色）；语气追加 `badge-clay` / `badge-success` /
/// `badge-warning` / `badge-danger`。
#[allow(dead_code)]
pub const BADGE_BASE: &str = "badge";

// --- 按钮 ---
//
// 样式定义在 styles/components/ui.css（`.btn` 系列）。新代码优先用 `Button` /
// `IconButton` 组件；这些常量供 `Link`、`label` 等非按钮元素与旧页面沿用，
// 页面重写时逐步收敛。

/// 次要按钮：浅灰底。
pub const BTN_SECONDARY: &str = "btn btn-secondary";

// ===========================================================================
// 组件
