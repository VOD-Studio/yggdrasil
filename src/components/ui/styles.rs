//! Shared class names and spinner markup.
// ===========================================================================
// 样式常量
// ===========================================================================

/// Admin 卡片容器：内容档圆角（16px），作为主面板内的内容卡片，与外壳 32px 形成层次。
///
/// 用裸 `transition`（Tailwind v4 默认列表含 colors/transform/box-shadow/opacity）而非
/// `transition-colors`：编译产物中 `.transition-colors` 排在 `.transition-all` 之后，
/// 同层同优先级会覆盖组件追加的 `transition-all`，导致 hover 位移/阴影瞬时跳变。
pub const ADMIN_CARD_CLASS: &str = "bg-[var(--color-paper-entry)] rounded-2xl shadow-sm border border-transparent hover:border-[var(--color-paper-border)] transition";

/// Admin 表格容器：内容档圆角（16px），与卡片一致。
pub const ADMIN_TABLE_CLASS: &str = "bg-[var(--color-paper-entry)] rounded-2xl shadow-sm border border-transparent hover:border-[var(--color-paper-border)] transition overflow-hidden";

/// 行内加载 spinner：环形渐变 + 自旋动画，用 currentColor 继承文字色。
///
/// 内联 SVG（含 `@keyframes`），通过 `dangerous_inner_html` 注入；尺寸由外层
/// Tailwind 类（如 `w-3.5 h-3.5`）控制。源文件 `public/icons/90-ring-with-gradient.svg`。
pub const SPINNER_SVG: &str = r#"<svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg"><defs><linearGradient id="yggSpinnerGrad"><stop offset="0%" stop-color="currentColor" stop-opacity="1"/><stop offset="100%" stop-color="currentColor" stop-opacity="0.25"/></linearGradient></defs><style>@keyframes yggSpin { to { transform: rotate(360deg); } } .ygg-spinner-circle { transform-origin: 50% 50%; stroke: url(#yggSpinnerGrad); fill: none; animation: yggSpin .5s infinite linear; }</style><circle cx="10" cy="10" r="8" class="ygg-spinner-circle" stroke-width="2"/></svg>"#;

#[allow(dead_code)]
pub const BADGE_BASE: &str =
    "inline-flex items-center px-2 py-0.5 rounded text-xs font-medium whitespace-nowrap";
pub const MEDIA_BADGE_BASE: &str =
    "inline-flex items-center px-2 py-0.5 rounded-lg text-[10px] font-mono font-medium whitespace-nowrap";

// --- 实心小按钮（批量操作栏：通过 / 垃圾 / 删除） ---

/// 绿色实心小按钮（批量通过、批量恢复）。
pub const BTN_SOLID_GREEN: &str =
    "px-4 py-1.5 text-sm font-medium bg-green-500/10 text-green-600 dark:text-green-400 rounded-full hover:bg-green-500/20 transition-colors cursor-pointer";
/// 琥珀色实心小按钮（批量标为垃圾）。
pub const BTN_SOLID_AMBER: &str =
    "px-4 py-1.5 text-sm font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400 rounded-full hover:bg-amber-500/20 transition-colors cursor-pointer";
/// 红色实心小按钮（批量删除、批量彻底删除）。
pub const BTN_SOLID_RED: &str =
    "px-4 py-1.5 text-sm font-medium bg-red-500/10 text-red-600 dark:text-red-400 rounded-full hover:bg-red-500/20 transition-colors cursor-pointer";

// --- 文字小按钮（表格行内操作：通过 / 垃圾 / 删除 / 恢复） ---

#[allow(dead_code)]
pub const BTN_TEXT_AMBER: &str = "text-xs text-amber-600 hover:text-amber-800 dark:text-amber-400 dark:hover:text-amber-300 transition-colors cursor-pointer";
#[allow(dead_code)]
pub const BTN_TEXT_RED: &str =
    "text-xs text-red-500 hover:text-red-700 dark:hover:text-red-300 transition-colors cursor-pointer";

/// 弱化文字按钮（弹窗「取消」等次要取消操作）：无描边、无填充，悬浮转主色。
pub const BTN_GHOST: &str =
    "px-3 py-1.5 text-xs text-paper-secondary hover:text-paper-primary transition-colors cursor-pointer";

// --- 次要按钮（Teal 第二色，ghost 描边风格，从属于主色 Green） ---

/// 次要按钮：极简风次要操作。
pub const BTN_SECONDARY: &str =
    "px-6 py-2.5 rounded-full text-sm font-medium text-center text-[var(--color-paper-secondary)] bg-[var(--color-paper-entry)] hover:bg-[var(--color-paper-border)] hover:text-[var(--color-paper-primary)] active:scale-[0.98] transition-all cursor-pointer";

// --- 主操作按钮（主题绿实心胶囊，全站统一 CTA） ---

/// 主操作按钮：主题绿实心胶囊（用于 `<Link>`、无 loading 态的静态按钮）。
pub const BTN_PRIMARY: &str =
    "inline-flex items-center justify-center px-5 py-2 text-sm font-medium text-[var(--color-paper-theme)] bg-[var(--color-paper-accent)] rounded-full shadow-sm hover:brightness-110 active:scale-[0.98] transition-all cursor-pointer";

/// 小号主操作按钮：工具栏场景（刷新 / 导出 / 创建备份）。
pub const BTN_PRIMARY_SM: &str =
    "inline-flex items-center justify-center px-4 py-1.5 text-sm font-medium text-[var(--color-paper-theme)] bg-[var(--color-paper-accent)] rounded-full hover:brightness-110 active:scale-[0.98] transition-all cursor-pointer";

// --- 描边按钮 ---

/// 描边次要按钮（posts 重建、system 刷新列表）：`relative` 以承载 spinner 叠加层。
pub const BTN_OUTLINE: &str =
    "relative px-4 py-2 rounded-full text-sm font-medium text-paper-primary border border-paper-border hover:border-paper-accent hover:text-paper-accent transition-all cursor-pointer";

/// 红色描边危险按钮（trash 清空回收站）。
pub const BTN_DANGER_OUTLINE: &str =
    "px-4 py-2 text-sm font-medium text-red-600 dark:text-red-400 border border-red-300 dark:border-red-900/50 rounded-full hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors cursor-pointer";

// --- 图标按钮 ---

/// 关闭图标按钮（× 关闭提示条）。
pub const BTN_CLOSE_ICON: &str =
    "shrink-0 text-red-400 hover:text-red-600 cursor-pointer text-lg leading-none";

/// 方形图标按钮（trash 步进 −/+）。
pub const BTN_ICON: &str =
    "w-9 h-9 flex items-center justify-center text-sm text-paper-secondary hover:text-paper-primary hover:bg-paper-theme cursor-pointer transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-paper-accent/40";

// ===========================================================================
// 组件
