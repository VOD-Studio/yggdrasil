import { type LightboxController, openLightbox } from './controller';
import { collectImages, initLazyLoad } from './images';
import './style.css';

declare global {
  interface Window {
    __initLightbox: (selectors: string | string[]) => void;
    __closeLightboxFor: (owner: string) => void;
    __lightboxSelectors?: string[];
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

// ============ 自启动 ============
// The Rust bridge may configure selectors before or after this IIFE loads.
// 1) Rust 内联 eval 先跑（常态）：设 __lightboxSelectors，此时 __initLightbox 可能未定义 → 只设配置；
//    lightbox.js 后加载完 → 读到配置 → 这里自启动。
// 2) lightbox.js 先加载完：__initLightbox 就绪但无配置 → 不自启动；
//    Rust eval 后跑 → 设配置 + 兜底 if(__initLightbox) 显式调用 → 初始化。
if (Array.isArray(window.__lightboxSelectors)) {
  window.__initLightbox(window.__lightboxSelectors);
}
