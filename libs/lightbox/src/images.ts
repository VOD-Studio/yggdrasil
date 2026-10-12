import { originalUrl } from './geometry';
import { ResourceScope } from './resources';

const RETRY_DELAYS_MS = [1000, 2000, 4000];

// 默认错误文案。容器可用 data-error-text 定制（素材页传「本地文件已丢失」）；
// 标 is-error 时补默认属性，卡片 CSS 经 attr() 显示。
const DEFAULT_ERROR_TEXT = '图片加载失败';

function markLoadError(container: Element): void {
  container.classList.add('is-error');
  if (!container.hasAttribute('data-error-text')) {
    container.setAttribute('data-error-text', DEFAULT_ERROR_TEXT);
  }
}

// 取节点错误文案：data-error-text 定制优先（素材页），缺省通用文案。
export function errorTextOf(node: HTMLElement): string {
  return node.getAttribute('data-error-text') || DEFAULT_ERROR_TEXT;
}

// ============ 懒加载 ============

// 为单个 .blur-img 容器初始化高清图懒加载。
// IO 进入视口后把 data-src 写入 src，加载完成加 is-loaded 触发 CSS 淡入。
// Only unfinished requests need removal tracking. One observer serves all thumbnails.
const pendingLoads = new Map<Element, () => void>();
let removalObserver: MutationObserver | null = null;

function trackRemoval(container: Element, cleanup: () => void): void {
  pendingLoads.set(container, cleanup);
  if (removalObserver) return;
  removalObserver = new MutationObserver((records) => {
    if (records.every((record) => record.removedNodes.length === 0)) return;
    for (const [node, stop] of pendingLoads) {
      if (!node.isConnected) stop();
    }
  });
  removalObserver.observe(document.body, { childList: true, subtree: true });
}

function stopTracking(container: Element): void {
  pendingLoads.delete(container);
  if (pendingLoads.size === 0) {
    removalObserver?.disconnect();
    removalObserver = null;
  }
}

export function initLazyLoad(container: Element): void {
  const fullImg = container.querySelector('.blur-img-full');
  if (!(fullImg instanceof HTMLImageElement)) return;
  if (container.getAttribute('data-blur-init')) return;
  const fullSrc = fullImg.getAttribute('data-src');
  if (!fullSrc) return;
  container.setAttribute('data-blur-init', 'true');

  const scope = new ResourceScope();
  let io: IntersectionObserver | null = null;
  let retryCount = 0;
  let retryTimer: number | undefined;
  const cleanup = (): void => {
    scope.dispose();
    io?.disconnect();
    stopTracking(container);
    // A keyed SPA node may be detached and mounted again. It must be able to resume.
    if (!container.isConnected) container.removeAttribute('data-blur-init');
  };
  trackRemoval(container, cleanup);

  const onFullLoaded = (): void => {
    if (!container.isConnected) {
      cleanup();
      return;
    }
    // Keep a server-measured ratio; client-mounted images acquire one after loading.
    if (
      container instanceof HTMLElement &&
      !container.style.getPropertyValue('--ar') &&
      fullImg.naturalWidth > 0 &&
      fullImg.naturalHeight > 0
    ) {
      container.style.setProperty('--ar', `${fullImg.naturalWidth} / ${fullImg.naturalHeight}`);
    }
    container.classList.add('is-loaded');
    // Avoid a stale opacity:0 compositing layer while hiding the placeholder.
    fullImg.style.transition = 'none';
    fullImg.style.opacity = '1';
    cleanup();
  };
  const onFullError = (): void => {
    if (!container.isConnected) {
      cleanup();
      return;
    }
    if (retryTimer !== undefined) return;
    if (retryCount >= RETRY_DELAYS_MS.length) {
      markLoadError(container);
      cleanup();
      return;
    }
    const delay = RETRY_DELAYS_MS[retryCount++];
    retryTimer = scope.timeout(() => {
      retryTimer = undefined;
      if (!container.isConnected) {
        cleanup();
        return;
      }
      fullImg.src = fullSrc;
    }, delay);
  };
  scope.listen(fullImg, 'load', onFullLoaded);
  scope.listen(fullImg, 'error', onFullError);

  if (fullImg.getAttribute('src') && fullImg.complete) {
    if (fullImg.naturalWidth > 0) onFullLoaded();
    else onFullError();
  }
  if (scope.disposed) return;

  if ('IntersectionObserver' in window) {
    io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          if (!container.isConnected) {
            cleanup();
            return;
          }
          fullImg.src = fullSrc;
          io?.unobserve(container);
        }
      },
      { rootMargin: '200px' },
    );
    io.observe(container);
  } else {
    fullImg.src = fullSrc;
  }
}

// ============ 图像收集 ============

// 从节点提取原图 URL 与 alt 描述（同时兼容 .blur-img 容器与原生外链 <img> 节点）。
export function getImageDetails(
  originNode: HTMLElement,
): { origSrc: string; altText: string } | null {
  if (originNode instanceof HTMLImageElement) {
    const src = originNode.getAttribute('data-src') || originNode.getAttribute('src') || '';
    if (!src) return null;
    return {
      origSrc: originalUrl(src),
      altText: originNode.getAttribute('alt') || '',
    };
  }
  const fullImgEl = originNode.querySelector('.blur-img-full');
  if (fullImgEl instanceof HTMLImageElement) {
    const dataSrc = fullImgEl.getAttribute('data-src') || fullImgEl.getAttribute('src') || '';
    if (!dataSrc) return null;
    return {
      origSrc: originalUrl(dataSrc),
      altText: fullImgEl.getAttribute('alt') || '',
    };
  }
  return null;
}

// 收集所有 selectors 命中的 .blur-img 节点与原生外链 <img> 节点。
// gallery: 正文图（组成图集）；singles: 带 lightbox-single class 的单张图（如封面）。
export function collectImages(roots: Element[]): {
  gallery: HTMLElement[];
  singles: HTMLElement[];
} {
  const gallery: HTMLElement[] = [];
  const singles: HTMLElement[] = [];
  const seen = new Set<HTMLElement>();
  for (const root of roots) {
    const nodes = root.querySelectorAll('.blur-img, img');
    for (const n of nodes) {
      if (!(n instanceof HTMLElement)) continue;
      if (n instanceof HTMLImageElement && n.closest('.blur-img')) continue;
      if (seen.has(n)) continue;
      seen.add(n);
      if (n.classList.contains('lightbox-single')) {
        singles.push(n);
      } else {
        gallery.push(n);
      }
    }
  }
  return { gallery, singles };
}
