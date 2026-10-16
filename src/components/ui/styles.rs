//! Shared class names and spinner markup.
// ===========================================================================
// 样式常量
// ===========================================================================

/// 后台卡片：单层发丝线边框的白卡（`.panel`，12px 圆角）。
pub const ADMIN_CARD_CLASS: &str = "panel";

/// 后台表格容器：与卡片同外观，裁掉表格溢出的圆角。
pub const ADMIN_TABLE_CLASS: &str = "panel overflow-hidden";

/// 行内加载 spinner：环形渐变 + 自旋动画，用 currentColor 继承文字色。
///
/// 内联 SVG（含 `@keyframes`），通过 `dangerous_inner_html` 注入；尺寸由外层
/// Tailwind 类（如 `w-3.5 h-3.5`）控制。源文件 `public/icons/90-ring-with-gradient.svg`。
pub const SPINNER_SVG: &str = r#"<svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg"><defs><linearGradient id="yggSpinnerGrad"><stop offset="0%" stop-color="currentColor" stop-opacity="1"/><stop offset="100%" stop-color="currentColor" stop-opacity="0.25"/></linearGradient></defs><style>@keyframes yggSpin { to { transform: rotate(360deg); } } .ygg-spinner-circle { transform-origin: 50% 50%; stroke: url(#yggSpinnerGrad); fill: none; animation: yggSpin .5s infinite linear; }</style><circle cx="10" cy="10" r="8" class="ygg-spinner-circle" stroke-width="2"/></svg>"#;

/// 徽章底座（`.badge`，中性色）；语气追加 `badge-clay` / `badge-success` /
/// `badge-warning` / `badge-danger`。
#[allow(dead_code)]
pub const BADGE_BASE: &str = "badge";
/// 素材卡片上的等宽小徽章（格式、尺寸）。
pub const MEDIA_BADGE_BASE: &str = "badge badge-mono";

// --- 按钮 ---
//
// 样式定义在 styles/components/ui.css（`.btn` 系列）。新代码优先用 `Button` /
// `IconButton` 组件；这些常量供 `Link`、`label` 等非按钮元素与旧页面沿用，
// 页面重写时逐步收敛。

/// 批量通过、批量恢复：小号次要按钮。
pub const BTN_SOLID_GREEN: &str = "btn btn-sm btn-secondary";
/// 批量标为垃圾：小号次要按钮。
pub const BTN_SOLID_AMBER: &str = "btn btn-sm btn-secondary";
/// 批量删除、批量彻底删除：小号浅红按钮。
pub const BTN_SOLID_RED: &str = "btn btn-sm btn-danger-soft";

/// 表格行内的次要文字操作。
#[allow(dead_code)]
pub const BTN_TEXT_AMBER: &str = "btn btn-xs btn-ghost";
/// 表格行内的危险文字操作。
#[allow(dead_code)]
pub const BTN_TEXT_RED: &str = "btn btn-xs btn-ghost text-danger";

/// 弱化按钮（弹窗「取消」等）：透明底，悬停浅灰底。
pub const BTN_GHOST: &str = "btn btn-sm btn-ghost";

/// 次要按钮：浅灰底。
pub const BTN_SECONDARY: &str = "btn btn-secondary";

/// 主操作按钮：陶土底（用于 `Link`、无 loading 态的静态按钮）。
pub const BTN_PRIMARY: &str = "btn btn-primary";

/// 小号主操作按钮：工具栏场景（刷新 / 导出 / 创建备份）。
pub const BTN_PRIMARY_SM: &str = "btn btn-sm btn-primary";

/// 描边按钮：发丝线描边，透明底。
pub const BTN_OUTLINE: &str = "btn btn-outline";

/// 危险操作的弱化形式（清空回收站等）：浅红底红字。
pub const BTN_DANGER_OUTLINE: &str = "btn btn-danger-soft";

/// 关闭图标按钮（× 关闭提示条）。
pub const BTN_CLOSE_ICON: &str = "btn btn-xs btn-icon btn-ghost";

/// 方形图标按钮（步进 −/+ 等）。
pub const BTN_ICON: &str = "btn btn-icon btn-ghost";

// ===========================================================================
// 组件
