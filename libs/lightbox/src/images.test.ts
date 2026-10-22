/**
 * 懒加载与既有加载的协调测试（P5 §8.2）。
 *
 * 按需加载后，lightbox 可能在路由协调器（prepareCover 已换 src 并挂起 decode）
 * 之后才初始化；对已就位的 src 重新赋值会重启加载、令 decode() 拒绝。钉住
 * 「src 已存在就接管、不重新赋值」的语义。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { initLazyLoad } from './images';

let ioCallback: IntersectionObserverCallback = () => {};

function blurContainer(): { container: HTMLElement; full: HTMLImageElement } {
  document.body.innerHTML = `
    <figure class="entry-cover">
      <span class="blur-img">
        <img class="blur-img-placeholder" src="/photo.webp?w=20">
        <img class="blur-img-full" data-src="/photo.webp?w=1200">
      </span>
    </figure>`;
  const container = document.querySelector<HTMLElement>('.blur-img')!;
  const full = container.querySelector<HTMLImageElement>('.blur-img-full')!;
  return { container, full };
}

describe('initLazyLoad 既有 src 的协调', () => {
  // happy-dom 的 IntersectionObserver 回调时序不确定：注入捕获型替身，
  // 由用例手动触发回调（observe 路径与无 IO 降级路径共用 adoptOrStart）。
  let dispatched = 0;
  beforeEach(() => {
    document.body.innerHTML = '';
    dispatched = 0;
    vi.stubGlobal(
      'IntersectionObserver',
      class {
        constructor(cb: IntersectionObserverCallback) {
          ioCallback = cb;
        }
        observe() {
          dispatched += 1;
        }
        unobserve() {}
        disconnect() {}
      },
    );
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });
  function intersect(): void {
    expect(dispatched).toBeGreaterThan(0);
    ioCallback([{ isIntersecting: true } as IntersectionObserverEntry], {} as IntersectionObserver);
  }
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('happy-dom 无 IntersectionObserver：无 src 时同步换上 data-src', () => {
    const { container, full } = blurContainer();
    initLazyLoad(container);
    intersect();
    expect(full.getAttribute('src')).toBe('/photo.webp?w=1200');
    expect(container.getAttribute('data-blur-init')).toBe('true');
  });

  it('src 已由外部（路由协调器）就位时保持原值，不重启加载', () => {
    const { container, full } = blurContainer();
    full.src = '/photo.webp?w=1200';
    const assignments: string[] = [];
    // 劫持 src setter：真实属性语义保留，同时记录每次赋值。
    const descriptor = Object.getOwnPropertyDescriptor(HTMLImageElement.prototype, 'src')!;
    Object.defineProperty(full, 'src', {
      ...descriptor,
      get: () => descriptor.get!.call(full),
      set: (value: string) => {
        assignments.push(value);
        descriptor.set!.call(full, value);
      },
    });

    initLazyLoad(container);
    intersect();

    // 初始已就位的 src 不算赋值；adoptOrStart 不得再碰 src。
    expect(assignments).toHaveLength(0);
    expect(full.getAttribute('src')).toBe('/photo.webp?w=1200');
    // 既有加载的结果由上方挂好的 load 监听接管：触发 load 进入 is-loaded。
    Object.defineProperty(full, 'naturalWidth', { value: 800, configurable: true });
    Object.defineProperty(full, 'naturalHeight', { value: 600, configurable: true });
    full.dispatchEvent(new Event('load'));
    expect(container.classList.contains('is-loaded')).toBe(true);
  });
});
