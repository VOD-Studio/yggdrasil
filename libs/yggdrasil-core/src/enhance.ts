import type { ThemeName } from '@yggdrasil/shared';
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
 * （如 anchor-click 的 installed 幂等位、主题 registry）会造成双监听或收不到通知，
 * 需要 core 能力的地方一律走 window 全局桥接。
 */
declare global {
  interface Window {
    __initPostContent: typeof initPostContent;
    __initMermaid: (selector: string, theme: ThemeName) => Promise<void>;
    __initTocSidebar: typeof initTocSidebar;
    __disposeTocSidebar: typeof disposeTocSidebar;
    __initChangelogNav: typeof initChangelogNav;
    __disposeChangelogNav: typeof disposeChangelogNav;
    __initShowcaseMasonry: typeof initShowcaseMasonry;
    __disposeShowcaseMasonry: typeof disposeShowcaseMasonry;
  }
}

window.__initPostContent = initPostContent;
window.__initMermaid = initMermaid;
window.__initTocSidebar = initTocSidebar;
window.__disposeTocSidebar = disposeTocSidebar;
window.__initChangelogNav = initChangelogNav;
window.__disposeChangelogNav = disposeChangelogNav;
window.__initShowcaseMasonry = initShowcaseMasonry;
window.__disposeShowcaseMasonry = disposeShowcaseMasonry;

// 本入口定义的函数已就绪，执行队列里等它们的调用。
window.__flushEnhanceCalls?.();
