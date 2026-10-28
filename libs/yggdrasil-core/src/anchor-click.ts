/**
 * 拦截文章正文/目录里的 hash 锚点点击，避免被 Leptos 路由器或视图过渡协调器当成站内导航。
 *
 * 背景：Leptos 路由器在 window 上监听 click 拦截站内链接，视图过渡协调器
 * （route-transitions.ts）在 document 冒泡阶段接管站内链接。点击 inner_html 注入的
 * 原生 <a href="#id"> 时，两者都会把它当作一次路由请求，原生 fragment-scroll 被
 * preventDefault 掉，表现为「点击锚点不滚动」或整页重绘。
 *
 * 修复：在 capture 阶段（早于上述两个冒泡监听器）拦截指向同页 hash 的 <a> 点击，
 * stopPropagation 阻止事件到达它们，自行滚动到标题（scrollToHeading 手动扣除 header
 * 高度）并用 history.replaceState 更新 URL hash（不触发 popstate/hashchange，避免抖动）。
 *
 * 例外：带 data-app-anchor 祖先的链接不接管，由所在组件的 on:click 自行滚动。
 */

import { scrollToHeading } from './scroll-to-heading';

let installed = false;

/**
 * 安装全局 hash 锚点点击拦截器。幂等：重复调用安全。
 *
 * 用 capture 阶段 + 在 window 上注册，保证在路由器与协调器的监听器之前执行；
 * stopPropagation 阻止事件继续传播，路由器不会收到这个事件。
 */
export function initAnchorClick(): void {
  if (installed) return;
  installed = true;

  window.addEventListener(
    'click',
    (event) => {
      // 只处理无修饰键的左键点击（中键/ctrl/cmd 点击交给浏览器新窗口行为）。
      if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
        return;
      }

      const target = event.target;
      if (!(target instanceof Element)) return;

      // 从点击点向上找最近的 <a>（点击可能落在 <a> 内的子元素上）。
      const anchor = target.closest('a');
      if (!anchor) return;

      // 组件自行处理点击（on:click + prevent_default）的锚点：完全放行，
      // 不 stopPropagation，否则组件的 handler 收不到事件。
      if (anchor.closest('[data-app-anchor]')) return;

      const href = anchor.getAttribute('href');
      // 只接管同页 hash 锚点：href 以 # 开头且非空（排除纯 "#" 和 "#top" 之外的真锚点）。
      // 外链、路径链接、空 hash 都交给浏览器/路由器原行为。
      if (!href?.startsWith('#') || href === '#') return;

      const id = decodeURIComponent(href.slice(1));
      const localRoot = anchor.closest<HTMLElement>('[data-local-anchor-scroll]');
      if (localRoot) {
        const localTarget = Array.from(localRoot.querySelectorAll<HTMLElement>('[id]')).find(
          (node) => node.id === id,
        );
        if (!localTarget) return;
        event.stopPropagation();
        event.preventDefault();
        const top =
          localRoot.scrollTop +
          localTarget.getBoundingClientRect().top -
          localRoot.getBoundingClientRect().top;
        localRoot.scrollTo({ top, behavior: 'smooth' });
        return;
      }
      const el = document.getElementById(id);
      if (!el) return; // 目标不存在：放行，交还原生行为（不滚动但不刷新）

      // 阻止事件继续传播到路由器与协调器的监听器。
      event.stopPropagation();
      event.preventDefault();

      // 平滑滚动到目标标题，手动扣除 sticky header 高度（见 scroll-to-heading.ts）。
      scrollToHeading(el, true);

      // 更新 URL hash 但不产生历史记录抖动：replaceState 不触发 popstate/hashchange。
      // 这样地址栏显示当前章节，刷新页面也能 scrollToHash 回到原位。
      if (window.location.hash !== href) {
        history.replaceState(history.state, '', href);
        window.__routeTransitions?.syncHash();
      }
    },
    true, // capture：在路由器的冒泡监听器之前执行
  );
}
