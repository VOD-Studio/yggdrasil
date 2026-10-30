import { disposeChangelogNav, initChangelogNav } from './changelog-nav';
import { initMermaid } from './mermaid';
import { initPostContent } from './post-content';
import { disposeShowcaseMasonry, initShowcaseMasonry } from './showcase-masonry';
import { disposeTocSidebar, initTocSidebar } from './toc-sidebar';

/**
 * P5 §8.2 增强入口：文章、图鉴、changelog 的增强函数，与 core 启动入口分离。
 *
 * 本 IIFE 由页面组件按需注入（SSR 直接烤进 head 带 defer，先于 WASM hydration；
 * SPA 首次注入为 async，迟到的 `__init*` 调用经 core 的 `__pendingEnhanceCalls`
 * 队列在尾部 flush 补上）。不 import core 模块：两个 IIFE 各持一份模块状态
 * （如 anchor-click 的 installed 幂等位）会造成双监听或状态分裂，
 * 需要 core 能力的地方一律走 window 全局桥接。
 */
declare global {
  interface Window {
    __initPostContent: typeof initPostContent;
    __initMermaid: typeof initMermaid;
    __initTocSidebar: typeof initTocSidebar;
    __disposeTocSidebar: typeof disposeTocSidebar;
    __initChangelogNav: typeof initChangelogNav;
    __disposeChangelogNav: typeof disposeChangelogNav;
    __initShowcaseMasonry: typeof initShowcaseMasonry;
    __disposeShowcaseMasonry: typeof disposeShowcaseMasonry;
  }
}

// leptos_meta 的 <Script> 不按 src 去重：组件重新挂载会再次执行本 IIFE。
// 全局函数已安装则保留首次实例——toc / changelog / masonry 的 dispose 句柄存在
// 模块状态里，被新实例替换后旧实例的监听就再也清理不掉。
if (!window.__initPostContent) {
  window.__initPostContent = initPostContent;
  window.__initMermaid = initMermaid;
  window.__initTocSidebar = initTocSidebar;
  window.__disposeTocSidebar = disposeTocSidebar;
  window.__initChangelogNav = initChangelogNav;
  window.__disposeChangelogNav = disposeChangelogNav;
  window.__initShowcaseMasonry = initShowcaseMasonry;
  window.__disposeShowcaseMasonry = disposeShowcaseMasonry;
}

// 本入口定义的函数已就绪，执行队列里等它们的调用。
window.__flushEnhanceCalls?.();
