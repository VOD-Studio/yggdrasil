/**
 * 滚动到标题锚点，手动扣除 sticky header 高度。
 *
 * 为什么不用 scrollIntoView + CSS scroll-margin-top：
 * 浏览器原生 fragment-scroll（直接访问 #hash 链接时）对 scroll-margin-top 的
 * 应用时序不稳定——常在标题 DOM 就绪后延迟触发，覆盖我们 scrollToHash 的落点，
 * 导致标题被 sticky header 遮住。改用手动 getBoundingClientRect 计算绝对滚动
 * 位置并 window.scrollTo，两个场景（直接访问 / 点击）行为确定一致。
 */

/** 呼吸空间：标题顶部与 header 下沿之间留出的空白。 */
const HEADING_OFFSET_PX = 16;

/**
 * 动态测量 sticky header 高度。读 .md-content 最近的 sticky 祖先 header，
 * 避免硬编码 80px（响应式/主题变化时 header 高度可能变）。
 */
function measureStickyHeaderHeight(): number {
  const header = document.querySelector('header.sticky');
  return header ? header.getBoundingClientRect().height : 0;
}

/**
 * 平滑滚动到指定元素，落点上移 header 高度 + 固定呼吸空间。
 * 元素不存在时安全跳过。
 */
export function scrollToHeading(el: Element, smooth = true): void {
  // scrollIntoView 配合 scroll-margin-top 在点击场景能工作，但直接访问 #hash 时
  // 浏览器原生 fragment-scroll 会争抢落点。统一用手动计算绕开时序竞态。
  const top =
    el.getBoundingClientRect().top +
    window.scrollY -
    measureStickyHeaderHeight() -
    HEADING_OFFSET_PX;
  window.scrollTo({
    top: Math.max(top, 0),
    // 'auto' inherits the document's scroll-behavior:smooth; snapshots need an instant landing.
    behavior: smooth ? 'smooth' : 'instant',
  });
}
