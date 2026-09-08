/**
 * 锚点跳转：根据 URL hash 滚动到对应标题。
 *
 * 解决 SPA 异步取数时序问题：文章正文经 use_server_future 异步加载，骨架屏阶段
 * 标题 DOM 尚不存在，浏览器原生 fragment-scroll 找不到目标留在顶部。本函数在
 * 内容挂载后调用，弥补原生 scroll 丢失的窗口。
 *
 * 落点遮挡问题：直接访问 #hash 链接时，浏览器原生 fragment-scroll 会在我们
 * scrollToHash 之后触发，它不读 scroll-margin-top，把标题顶到 sticky header 下面。
 * 解法：用 requestAnimationFrame 延后两帧再做最终校正，确保我们的落点在原生
 * scroll 之后生效（原生 scroll 通常在下一帧完成）。
 *
 * 异步布局位移问题：两帧校正之后仍可能有异步内容撑高布局——mermaid 懒加载
 * 注入 SVG、图片/字体加载等发生在两帧之后，把目标标题推离已校准的落点，导致
 * 直接访问带 hash 的 URL 时标题停在屏幕中间。解法：两帧校正后开启一个 ResizeObserver
 * 布局稳定期（见 stabilizeScrollOnResize），位移后重新校正，直到超时或用户主动滚动。
 *
 * CJK 编码要点：location.hash 返回百分号编码形式（%E4%B8%89-...），而标题 id
 * 属性是原始字符（id="三-五-零法则"）。必须先 decodeURIComponent 还原，getElementById
 * 才能匹配。双 fallback（先解码、后原始）兼容两种情况。
 */

import { scrollToHeading } from './scroll-to-heading';

/** 布局稳定期时长：覆盖 mermaid bundle 加载+渲染、图片/字体加载等异步位移源。 */
const STABILIZE_WINDOW_MS = 2000;

/** 用户主动滚动的输入事件：收到任一即认为用户接管，停止自动校正。 */
const USER_INPUT_EVENTS: Array<keyof WindowEventMap> = ['wheel', 'touchmove', 'keydown'];

/** 当前活跃的稳定器；重入时先 dispose 上一次，避免泄漏/叠加。 */
let activeStabilizer: (() => void) | null = null;

window.addEventListener('yggdrasil:navigation-start', () => {
  activeStabilizer?.();
  activeStabilizer = null;
});

/**
 * 布局稳定期：在 STABILIZE_WINDOW_MS 内监听内容容器尺寸变化，位移后用 rAF 合并
 * 并重新校正落点。用户主动滚动或超时后自动停止。
 *
 * 为什么观察容器而非标题本身：标题自身尺寸不变（ResizeObserver 不触发），位移来自
 * 它上方的 mermaid/图片撑高。观察 .post-content 容器（标题最近的正文祖先）能覆盖
 * 所有可能撑高的内容，且与具体位移源（mermaid/图片/字体）解耦。
 */
function stabilizeScrollOnResize(el: Element): void {
  // 重入安全：dispose 上一次活跃的稳定器。
  activeStabilizer?.();
  activeStabilizer = null;

  if (typeof ResizeObserver === 'undefined') return;

  const target = el.closest('.post-content') ?? document.body;
  let rafId = 0;
  let disposed = false;

  const dispose = () => {
    if (disposed) return;
    disposed = true;
    ro.disconnect();
    window.clearTimeout(timerId);
    for (const evt of USER_INPUT_EVENTS) {
      window.removeEventListener(evt, onUserInput, { capture: true } as EventListenerOptions);
    }
    if (rafId) window.cancelAnimationFrame(rafId);
    if (activeStabilizer === dispose) activeStabilizer = null;
  };

  // rAF 合并：同帧多次 resize 只校正一次，避免抖动。
  const scheduleCorrect = () => {
    if (disposed) return;
    if (rafId) return;
    rafId = window.requestAnimationFrame(() => {
      rafId = 0;
      if (!disposed) scrollToHeading(el, false);
    });
  };

  // 用户主动滚动即停止校正：不与用户交互打架。
  // 用输入事件而非 scroll 事件——window.scrollTo 本身会触发 scroll，无法区分。
  const onUserInput = () => dispose();
  for (const evt of USER_INPUT_EVENTS) {
    window.addEventListener(evt, onUserInput, {
      capture: true,
      passive: true,
    } as AddEventListenerOptions);
  }

  const ro = new ResizeObserver(scheduleCorrect);
  ro.observe(target);

  // 超时兜底：无论如何 STABILIZE_WINDOW_MS 后停止，防止长期占用。
  const timerId = window.setTimeout(dispose, STABILIZE_WINDOW_MS);

  activeStabilizer = dispose;
}

export function scrollToHash(): void {
  // History traversal restores the actual reading position, which may be far below the hash.
  if (window.__routeTransitions?.isRestoring()) return;
  const hash = window.location.hash.slice(1); // 去掉前导 #
  if (!hash) return;

  const id = decodeURIComponent(hash);
  const el = document.getElementById(id) ?? document.getElementById(hash);
  if (!el) return;

  // 立即先滚一次（覆盖骨架屏阶段原生 scroll 失败的窗口）。
  scrollToHeading(el, true);

  // 延后两帧再校正：直接访问 #hash 时浏览器原生 fragment-scroll 会在此期间触发，
  // 把标题顶到 header 下面（它不读 scroll-margin-top）。两帧后原生 scroll 已完成，
  // 用即时滚动（非 smooth，避免与刚才的 smooth 动画叠加抖动）把落点拉回正确位置。
  requestAnimationFrame(() => {
    requestAnimationFrame(() => scrollToHeading(el, false));
  });

  // 布局稳定期：覆盖两帧之后的异步位移（mermaid 懒加载、图片/字体加载等），
  // 位移后重新校正。用户主动滚动或超时后自动停止。
  stabilizeScrollOnResize(el);
}
