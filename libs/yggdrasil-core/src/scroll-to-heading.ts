/**
 * 滚动到标题锚点，手动扣除吸顶层高度。
 *
 * 为什么不用 scrollIntoView + CSS scroll-margin-top：
 * 浏览器原生 fragment-scroll（直接访问 #hash 链接时）对 scroll-margin-top 的
 * 应用时序不稳定——常在标题 DOM 就绪后延迟触发，覆盖我们 scrollToHash 的落点，
 * 导致标题被 sticky header 遮住。改用手动 getBoundingClientRect 计算绝对滚动
 * 位置再 scrollTo，两个场景（直接访问 / 点击）行为确定一致。
 *
 * 前台滚动窗口；后台（含文章预览）滚动 `[data-vt-scroll]` 容器，窗口本身不动。
 */

/** 呼吸空间：标题顶部与 header 下沿之间留出的空白。 */
const HEADING_OFFSET_PX = 16;

/** 吸顶页头：前台为 `.site-header`，后台布局仍用 Tailwind 的 `sticky`。 */
export const STICKY_HEADER_SELECTOR = 'header.site-header, header.sticky';

/**
 * 动态测量 sticky header 高度，避免硬编码（响应式/主题变化时 header 高度可能变）。
 */
function measureStickyHeaderHeight(): number {
  const header = document.querySelector(STICKY_HEADER_SELECTOR);
  return header ? header.getBoundingClientRect().height : 0;
}

/** 后台吸顶层：顶栏与文章预览条，都贴在滚动容器顶部。 */
const CONTAINER_STICKY_SELECTOR = '.admin-topbar, .preview-bar-slot';

/** 标题所在的滚动容器；不在容器内或容器不滚动（如 `.admin-scroll-fixed`）时为 null，表示窗口。 */
function scrollContainer(el: Element): HTMLElement | null {
  const container = el.closest<HTMLElement>('[data-vt-scroll]');
  if (!container) return null;
  return /^(auto|scroll|overlay)$/.test(getComputedStyle(container).overflowY) ? container : null;
}

/** 容器内吸顶层贴住后的下沿，相对容器顶部：sticky 的 top 加自身高度，取最低者。 */
function containerStickyBottom(container: HTMLElement): number {
  let bottom = 0;
  for (const node of container.querySelectorAll<HTMLElement>(CONTAINER_STICKY_SELECTOR)) {
    const { height } = node.getBoundingClientRect();
    if (height === 0) continue;
    bottom = Math.max(bottom, (Number.parseFloat(getComputedStyle(node).top) || 0) + height);
  }
  return bottom;
}

/**
 * 平滑滚动到指定元素，落点上移 header 高度 + 固定呼吸空间。
 * 元素不存在时安全跳过。
 */
export function scrollToHeading(el: Element, smooth = true): void {
  // 'auto' inherits the document's scroll-behavior:smooth; snapshots need an instant landing.
  const behavior =
    smooth && !window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'smooth' : 'instant';
  const container = scrollContainer(el);
  if (container) {
    const top =
      container.scrollTop +
      el.getBoundingClientRect().top -
      container.getBoundingClientRect().top -
      containerStickyBottom(container) -
      HEADING_OFFSET_PX;
    container.scrollTo({ top: Math.max(top, 0), behavior });
    return;
  }
  // The compact changelog directory sticks below the header on small screens.
  const directory = el.matches('.changelog-version')
    ? document.querySelector('.changelog-nav')
    : null;
  const directoryHeight =
    directory && window.matchMedia('(max-width: 1023px)').matches
      ? directory.getBoundingClientRect().height + 8
      : 0;
  // scrollIntoView 配合 scroll-margin-top 在点击场景能工作，但直接访问 #hash 时
  // 浏览器原生 fragment-scroll 会争抢落点。统一用手动计算绕开时序竞态。
  const top =
    el.getBoundingClientRect().top +
    window.scrollY -
    measureStickyHeaderHeight() -
    directoryHeight -
    HEADING_OFFSET_PX;
  window.scrollTo({ top: Math.max(top, 0), behavior });
}
