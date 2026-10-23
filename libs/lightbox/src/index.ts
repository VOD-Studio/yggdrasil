import { type LightboxController, openLightbox } from './controller';
import { collectImages, initLazyLoad } from './images';
import './style.css';

declare global {
  interface Window {
    __initLightbox: (selectors: string | string[]) => void;
    __closeLightboxFor: (owner: string) => void;
    __lightboxSelectors?: string[];
    /** P5 §8.2：core 建立的增强调用队列 flush（yggdrasil-core 类型，此处可选）。 */
    __flushEnhanceCalls?: () => void;
  }
}

const clickBindings = new WeakMap<HTMLElement, (event: MouseEvent) => void>();
let active: LightboxController | null = null;

function open(node: HTMLElement, gallery: HTMLElement[], index: number | null): void {
  active?.close(true);
  const controller = openLightbox(node, gallery, index, () => {
    if (active === controller) active = null;
  });
  active = controller;
}

// leptos_meta 的 <Script> 不按 src 去重：组件重新挂载会再次执行本 IIFE。
// 全局函数已安装则保留首次实例（clickBindings / active 的模块状态随之保持唯一），
// 否则新实例的空 WeakMap 查不到旧监听，同一节点会叠加点击处理器。
if (!window.__initLightbox) {
  window.__initLightbox = (selectors: string | string[]): void => {
    // selectors 可以是字符串、字符串数组
    const sels = Array.isArray(selectors) ? selectors : [selectors];
    const roots: Element[] = [];
    for (const sel of sels) {
      const found = document.querySelectorAll(sel);
      for (const el of found) roots.push(el);
    }

    // 先对所有图片做懒加载（图集与单张都做）
    const collected = collectImages(roots);
    for (const node of collected.gallery.concat(collected.singles)) {
      initLazyLoad(node);
    }

    // Replace a reused SPA node's handler with its current gallery membership.
    // Each node still has exactly one click listener after repeated initialization.
    const bindClick = (node: HTMLElement, handler: (e: MouseEvent) => void): void => {
      const previous = clickBindings.get(node);
      if (previous) node.removeEventListener('click', previous);
      clickBindings.set(node, handler);
      node.setAttribute('data-lb-bound', 'true');
      node.addEventListener('click', handler);
    };

    // Each closure captures its own node and current index.
    const gallery = collected.gallery;
    gallery.forEach((node, idx) => {
      bindClick(node, (e: MouseEvent) => {
        e.preventDefault();
        open(node, gallery, idx);
      });
    });
    // 单张图（封面）：index = null，gallery 传空数组表示单张
    for (const node of collected.singles) {
      bindClick(node, (e: MouseEvent) => {
        e.preventDefault();
        open(node, [], null);
      });
    }
  };

  // 图鉴预览卸载时只关闭自己打开的灯箱，不影响文章或素材页的浮层。
  window.__closeLightboxFor = (owner: string): void => {
    const activeOwner = active?.originNode
      .closest('[data-showcase-lightbox-owner]')
      ?.getAttribute('data-showcase-lightbox-owner');
    if (activeOwner === owner) active?.close(true);
  };
}

// ============ 自启动 ============
// The Rust bridge may configure selectors before or after this IIFE loads.
// 1) Rust 内联 eval 先跑（常态）：设 __lightboxSelectors，此时 __initLightbox 可能未定义 → 只设配置；
//    lightbox.js 后加载完 → 读到配置 → 这里自启动。
// 2) lightbox.js 先加载完：__initLightbox 就绪但无配置 → 不自启动；
//    Rust eval 后跑 → 设配置 + 兜底 if(__initLightbox) 显式调用 → 初始化。
if (Array.isArray(window.__lightboxSelectors)) {
  window.__initLightbox(window.__lightboxSelectors);
}

// P5 §8.2：本 IIFE 就绪后补执行队列里等待 __initLightbox 的迟到调用
// （SPA 首次按需注入时 effect 先于脚本执行）。与上方自启动双保险并存，
// data-lb-bound 守卫保证重复初始化幂等。
window.__flushEnhanceCalls?.();
