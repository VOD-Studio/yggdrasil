import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import './index';

function scrollTo(y: number): void {
  Object.defineProperty(window, 'scrollY', { value: y, configurable: true });
  window.dispatchEvent(new Event('scroll'));
}

function openImage(load = true): {
  origin: HTMLImageElement;
  img: HTMLImageElement;
  overlay: HTMLElement;
} {
  document.body.innerHTML =
    '<div class="post-content"><img src="/photo.webp"><img src="/next.webp"></div>';
  const origin = document.querySelector('img')!;
  // 页面里的原图会随真实页面滚动改变视口坐标。
  vi.spyOn(origin, 'getBoundingClientRect').mockImplementation(
    () => new DOMRect(200, 850 - window.scrollY, 400, 300),
  );
  window.__initLightbox('.post-content');
  origin.click();
  const img = document.querySelector<HTMLImageElement>('.lightbox-img')!;
  const overlay = document.querySelector<HTMLElement>('.lightbox-overlay')!;
  if (load) {
    Object.defineProperty(img, 'naturalWidth', { value: 800 });
    Object.defineProperty(img, 'naturalHeight', { value: 600 });
    img.dispatchEvent(new Event('load'));
    vi.advanceTimersByTime(300);
    vi.spyOn(img, 'getBoundingClientRect').mockReturnValue(new DOMRect(100, 100, 800, 600));
  }
  return { origin, img, overlay };
}

function wheel(el: Element, options: WheelEventInit = {}): WheelEvent {
  const event = new WheelEvent('wheel', {
    deltaY: -100,
    clientX: 500,
    clientY: 400,
    bubbles: true,
    cancelable: true,
    ...options,
  });
  // happy-dom 的 WheelEvent 继承 UIEvent，需补浏览器从 MouseEvent 继承的字段。
  Object.defineProperties(event, {
    ctrlKey: { value: options.ctrlKey ?? false },
    metaKey: { value: options.metaKey ?? false },
    clientX: { value: 500 },
    clientY: { value: 400 },
  });
  el.dispatchEvent(event);
  return event;
}

describe('滚动缩回页面原图', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal('innerWidth', 1000);
    vi.stubGlobal('innerHeight', 800);
    scrollTo(500);
  });

  afterEach(() => {
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    vi.advanceTimersByTime(300);
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    scrollTo(0);
    vi.useRealTimers();
    document.body.innerHTML = '';
  });

  it('普通滚轮交还页面滚动，不再放大图片', () => {
    const { img } = openImage();
    const before = img.style.transform;
    expect(wheel(img).defaultPrevented).toBe(false);
    expect(img.style.transform).toBe(before);
  });

  it.each(['ctrlKey', 'metaKey'])('%s + 滚轮仍在光标处缩放，并阻止页面滚动', (key) => {
    const { img } = openImage();
    expect(wheel(img, { [key]: true }).defaultPrevented).toBe(true);
    expect(img.classList.contains('is-zoomed')).toBe(true);
  });

  it('工具栏上的滚轮不滚动页面或缩放', () => {
    const { img, overlay } = openImage();
    const before = img.style.transform;
    expect(wheel(overlay.querySelector('[aria-label="放大"]')!).defaultPrevented).toBe(true);
    expect(img.style.transform).toBe(before);
  });

  it('滚动进度插值到实时原图位置，有连续过渡且反向滚动可退回', () => {
    const { img, overlay } = openImage();
    const start = img.style.transform;
    scrollTo(530); // 1/4 行程；目标原图现在位于 (200, 320)。
    // 图的视觉 rect = (125, 155, 700, 525)，通过外层变换映射原来的 rect。
    expect(img.style.transform).toBe(`translate(37.5px, 67.5px) scale(0.875, 0.875) ${start}`);
    expect(img.style.transition).toContain('transform 100ms');
    expect(overlay.style.opacity).toBe('0.75');

    scrollTo(560); // 1/2 行程；目标继续上移到 (200, 290)。
    expect(img.style.transform).toBe(`translate(75px, 120px) scale(0.75, 0.75) ${start}`);
    expect(overlay.style.opacity).toBe('0.5');

    scrollTo(500);
    expect(img.style.transform).toBe(`translate(0px, 0px) scale(1, 1) ${start}`);
    expect(overlay.style.opacity).toBe('1');
    expect(overlay.isConnected).toBe(true);
  });

  it.each([120, -120])('滚动 %i px 后动画飞回再移除，焦点恢复不改变滚动位置', (dy) => {
    const { origin, img, overlay } = openImage();
    const focus = vi.spyOn(origin, 'focus');
    scrollTo(500 + dy);
    expect(overlay.isConnected).toBe(true);
    expect(img.style.transition).toContain('transform 250ms');
    expect(overlay.style.opacity).toBe('0');
    vi.advanceTimersByTime(300);
    expect(overlay.isConnected).toBe(false);
    expect(focus).toHaveBeenCalledWith({ preventScroll: true });
    expect(window.scrollY).toBe(500 + dy);
  });

  it('滚动途中按 Esc 从当前动画继续飞回，关闭后解除滚动监听', () => {
    const { img, overlay } = openImage();
    const start = img.style.transform;
    scrollTo(530);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(img.style.transform).toBe(`translate(150px, 270px) scale(0.5, 0.5) ${start}`);
    vi.advanceTimersByTime(300);
    const closed = img.style.transform;
    scrollTo(600);
    expect(img.style.transform).toBe(closed);
    expect(overlay.isConnected).toBe(false);
  });

  it('收尾动画期间继续滚动，飞回终点仍跟随原图', () => {
    const { img, overlay } = openImage();
    const start = img.style.transform;
    scrollTo(620);
    scrollTo(680); // 滚轮惯性仍在移动页面，原图现在位于 (200, 170)。
    expect(img.style.transform).toBe(`translate(150px, 120px) scale(0.5, 0.5) ${start}`);
    vi.advanceTimersByTime(300);
    const closed = img.style.transform;
    scrollTo(740);
    expect(img.style.transform).toBe(closed);
    expect(overlay.isConnected).toBe(false);
  });

  it('滚动到一半后可用工具栏重新缩放，下一次滚动重新起算', () => {
    const { overlay } = openImage();
    scrollTo(560);
    overlay.querySelector<HTMLButtonElement>('[aria-label="放大"]')!.click();
    expect(overlay.style.opacity).toBe('1');
    scrollTo(590);
    expect(overlay.style.opacity).toBe('0.75');
    expect(overlay.isConnected).toBe(true);
  });

  it('放大并旋转多圈后从当前视图缩回，旋转角不变', () => {
    const { img, overlay } = openImage();
    overlay.querySelector<HTMLButtonElement>('[aria-label="放大"]')!.click();
    const rotate = overlay.querySelector<HTMLButtonElement>('[aria-label="顺时针旋转 90 度"]')!;
    for (let i = 0; i < 5; i++) rotate.click();
    const start = img.style.transform;
    expect(start).toContain('rotate(450deg)');
    scrollTo(530);
    expect(img.style.transform).toBe(`translate(37.5px, 67.5px) scale(0.875, 0.875) ${start}`);
    scrollTo(620);
    expect(img.style.transform.endsWith(start)).toBe(true);
    vi.advanceTimersByTime(300);
    expect(overlay.isConnected).toBe(false);
  });

  it('中途切换图集后重新计算滚动行程，并飞回新图位置', () => {
    const { img, overlay } = openImage();
    const next = document.querySelector<HTMLImageElement>('.post-content img:nth-child(2)')!;
    vi.spyOn(next, 'getBoundingClientRect').mockImplementation(
      () => new DOMRect(600, 1000 - window.scrollY, 400, 300),
    );
    Object.defineProperty(img, 'complete', { value: true });
    scrollTo(560);
    overlay.querySelector<HTMLButtonElement>('.lightbox-next')!.click();
    vi.advanceTimersByTime(300);
    const start = img.style.transform;
    expect(overlay.querySelector('.lightbox-counter')?.textContent).toBe('2 / 2');
    expect(overlay.style.opacity).toBe('1');
    scrollTo(590);
    expect(overlay.style.opacity).toBe('0.75');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(img.style.transform).toBe(`translate(550px, 360px) scale(0.5, 0.5) ${start}`);
  });

  it('图片还在加载时也能滚动关闭，迟到的 load 不会重新播放打开动画', () => {
    const { img, overlay } = openImage(false);
    scrollTo(530);
    const closing = img.style.transform;
    img.dispatchEvent(new Event('load'));
    vi.advanceTimersByTime(64);
    expect(img.style.transform).toBe(closing);
    expect(overlay.style.opacity).toBe('0');
    vi.advanceTimersByTime(300);
    expect(overlay.isConnected).toBe(false);
  });

  it('减少动态效果偏好下滚动立即关闭', () => {
    vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: true } as MediaQueryList);
    const { overlay } = openImage();
    scrollTo(501);
    expect(overlay.isConnected).toBe(false);
  });
});
