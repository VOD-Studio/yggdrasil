/**
 * 拦截文章正文/目录里的 hash 锚点点击，阻止 Dioxus 接管导致整页刷新。
 *
 * 背景：Dioxus 0.7 的 NativeInterpreter 通过事件委托在根节点上监听 click。
 * 只要 app 任意位置用了 onclick（本项目 post_nav_links.rs 有），Dioxus 就启用
 * 全局 click 委托。点击 dangerous_inner_html 注入的原生 <a href="#id"> 时：
 *   1. click 冒泡到根 → handleEvent 同步 POST /__events
 *   2. 目标 <a> 没有 Dioxus onclick → response 不含 preventDefault
 *   3. 兜底分支 handleClickNavigate: event.preventDefault() + browser_open IPC
 *   4. browser_open 走 History::external → window.location.set_href("#id") → 整页刷新
 *
 * 结果：原生 fragment-scroll 被 preventDefault 拦掉，IPC 又把 hash 当外部 URL
 * 整页加载，表现为「点击锚点刷新文章页且不滚动」。WASM 未下载完时是纯 SSR HTML，
 * 走浏览器原生 fragment-scroll 正常——所以问题只在 hydration 后出现。
 *
 * 修复：在 capture 阶段（早于 Dioxus 的 bubble 委托监听器）拦截指向同页 hash 的
 * <a> 点击，stopPropagation 阻止事件到达 Dioxus 的委托监听器，自行滚动到标题
 * （scrollToHeading 手动扣除 header 高度）并用 history.replaceState 更新 URL hash
 * （不触发 popstate/hashchange，避免抖动）。
 */

import { scrollToHeading } from './scroll-to-heading';

let installed = false;

/**
 * 安装全局 hash 锚点点击拦截器。幂等：重复调用安全。
 *
 * 用 capture 阶段 + 在 window 上注册，保证在 Dioxus 的根监听器之前执行；
 * stopPropagation 阻止事件继续传播，Dioxus 的 handleEvent/handleClickNavigate
 * 不会收到这个事件，browser_open IPC 不触发，整页刷新被避免。
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

      const href = anchor.getAttribute('href');
      // 只接管同页 hash 锚点：href 以 # 开头且非空（排除纯 "#" 和 "#top" 之外的真锚点）。
      // 外链、路径链接、空 hash 都交给浏览器/Dioxus 原行为。
      if (!href?.startsWith('#') || href === '#') return;

      const id = decodeURIComponent(href.slice(1));
      const el = document.getElementById(id);
      if (!el) return; // 目标不存在：放行，交还原生行为（不滚动但不刷新）

      // 阻止事件继续传播到 Dioxus 的委托监听器 → 避免 handleClickNavigate。
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
    true, // capture：在 Dioxus 的 bubble 监听器之前执行
  );
}
