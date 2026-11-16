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
  backdrop: HTMLElement;
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
  const backdrop = document.querySelector<HTMLElement>('.lightbox-backdrop')!;
  if (load) {
    Object.defineProperty(img, 'naturalWidth', { value: 800 });
    Object.defineProperty(img, 'naturalHeight', { value: 600 });
    img.dispatchEvent(new Event('load'));
    vi.advanceTimersByTime(300);
    vi.spyOn(img, 'getBoundingClientRect').mockReturnValue(new DOMRect(100, 100, 800, 600));
  }
  return { origin, img, overlay, backdrop };
}

function corners(img: HTMLImageElement): { x: number; y: number }[] {
  const matrix = new DOMMatrixReadOnly(img.style.transform);
  const w = Number.parseFloat(img.style.width);
  const h = Number.parseFloat(img.style.height);
  return [
    [0, 0],
    [w, 0],
    [0, h],
    [w, h],
  ].map(([x, y]) => ({
    x: matrix.a * x + matrix.c * y + matrix.e,
    y: matrix.b * x + matrix.d * y + matrix.f,
  }));
}

function expectCorners(img: HTMLImageElement, expected: { x: number; y: number }[]): void {
  corners(img).forEach((point, index) => {
    expect(point.x).toBeCloseTo(expected[index].x, 1);
    expect(point.y).toBeCloseTo(expected[index].y, 1);
  });
}

function rectCorners(rect: DOMRect): { x: number; y: number }[] {
  return [
    { x: rect.left, y: rect.top },
    { x: rect.right, y: rect.top },
    { x: rect.left, y: rect.bottom },
    { x: rect.right, y: rect.bottom },
  ];
}

function expectProgress(
  img: HTMLImageElement,
  start: { x: number; y: number }[],
  origin: HTMLImageElement,
  progress: number,
): void {
  const end = rectCorners(origin.getBoundingClientRect());
  expectCorners(
    img,
    start.map((point, index) => ({
      x: point.x + (end[index].x - point.x) * progress,
      y: point.y + (end[index].y - point.y) * progress,
    })),
  );
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
    vi.advanceTimersByTime(400);
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

  it('滚动进度插值到实时原图位置，逐帧直写且反向滚动可退回', () => {
    const { origin, img, overlay, backdrop } = openImage();
    const start = corners(img);
    scrollTo(530); // 1/4 行程；目标原图现在位于 (200, 320)。
    expectProgress(img, start, origin, 0.25);
    expect(img.style.transition).toBe('none');
    expect(backdrop.style.opacity).toBe('0.75');

    scrollTo(560); // 1/2 行程；目标继续上移到 (200, 290)。
    expectProgress(img, start, origin, 0.5);
    expect(backdrop.style.opacity).toBe('0.5');

    scrollTo(500);
    expectCorners(img, start);
    expect(backdrop.style.opacity).toBe('1');
    expect(overlay.isConnected).toBe(true);
  });

  it.each([120, -120])('滚动 %i px 后动画飞回再移除，焦点恢复不改变滚动位置', (dy) => {
    const { origin, img, overlay, backdrop } = openImage();
    const focus = vi.spyOn(origin, 'focus');
    scrollTo(500 + dy);
    expect(overlay.isConnected).toBe(true);
    expect(img.style.transition).toBe('none');
    vi.advanceTimersByTime(150);
    const opacity = Number(backdrop.style.opacity);
    expect(opacity).toBeGreaterThan(0);
    expect(opacity).toBeLessThan(1);
    expect(overlay.isConnected).toBe(true);
    vi.advanceTimersByTime(200);
    expect(overlay.isConnected).toBe(false);
    expect(focus).toHaveBeenCalledWith({ preventScroll: true });
    expect(window.scrollY).toBe(500 + dy);
  });

  it('滚动途中按 Esc 从当前动画继续飞回，关闭后解除滚动监听', () => {
    const { origin, img, overlay } = openImage();
    scrollTo(530);
    const tracked = corners(img);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expectCorners(img, tracked);
    vi.advanceTimersByTime(320);
    expectCorners(img, rectCorners(origin.getBoundingClientRect()));
    expect(overlay.isConnected).toBe(false);
    const closed = img.style.transform;
    scrollTo(600);
    expect(img.style.transform).toBe(closed);
    expect(overlay.isConnected).toBe(false);
  });

  it('收尾动画期间继续滚动，飞回终点仍跟随原图', () => {
    const { origin, img, overlay } = openImage();
    scrollTo(620);
    scrollTo(680); // 滚轮惯性仍在移动页面，原图现在位于 (200, 170)。
    vi.advanceTimersByTime(320);
    const end = rectCorners(origin.getBoundingClientRect());
    expectCorners(img, end);
    expect(overlay.isConnected).toBe(true); // 终点先绘制一帧，不能提前恢复原图。
    vi.advanceTimersByTime(40);
    expect(overlay.isConnected).toBe(false);
    scrollTo(740);
    expectCorners(img, end);
    expect(overlay.isConnected).toBe(false);
  });

  it('滚动到一半后可用工具栏重新缩放，下一次滚动重新起算', () => {
    const { overlay, backdrop } = openImage();
    scrollTo(560);
    overlay.querySelector<HTMLButtonElement>('[aria-label="放大"]')!.click();
    expect(backdrop.style.opacity).toBe('1');
    scrollTo(590);
    expect(backdrop.style.opacity).toBe('0.75');
    expect(overlay.isConnected).toBe(true);
  });

  it('450° 从当前姿态连续归位到 360°，反向归零和回弹保留旋转状态', () => {
    const { origin, img, overlay } = openImage();
    overlay.querySelector<HTMLButtonElement>('[aria-label="放大"]')!.click();
    const rotate = overlay.querySelector<HTMLButtonElement>('[aria-label="向右旋转"]')!;
    for (let i = 0; i < 5; i++) rotate.click();
    const start = corners(img);
    scrollTo(530);
    const matrix = new DOMMatrixReadOnly(img.style.transform);
    expect((Math.atan2(matrix.b, matrix.a) * 180) / Math.PI).toBeCloseTo(67.5, 3);
    scrollTo(500);
    expectCorners(img, start);
    scrollTo(530);
    vi.advanceTimersByTime(160); // 短滚动回弹，不能提前清零 view.deg。
    expectCorners(img, start);
    scrollTo(560); // 回弹后重新以 530 为起点。
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    vi.advanceTimersByTime(320);
    expectCorners(img, rectCorners(origin.getBoundingClientRect()));
    expect(overlay.isConnected).toBe(false);
  });

  it('中途切换图集后重新计算滚动行程，并飞回新图位置', () => {
    const { img, overlay, backdrop } = openImage();
    const next = document.querySelector<HTMLImageElement>('.post-content img:nth-child(2)')!;
    vi.spyOn(next, 'getBoundingClientRect').mockImplementation(
      () => new DOMRect(600, 1000 - window.scrollY, 400, 300),
    );
    Object.defineProperty(img, 'complete', { value: true });
    scrollTo(560);
    overlay.querySelector<HTMLButtonElement>('.lightbox-next')!.click();
    vi.advanceTimersByTime(300);
    const start = corners(img);
    expect(overlay.querySelector('.lightbox-counter')?.textContent).toBe('2 / 2');
    expect(backdrop.style.opacity).toBe('1');
    scrollTo(590);
    expect(backdrop.style.opacity).toBe('0.75');
    expectProgress(img, start, next, 0.25);
    const before = corners(img);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expectCorners(img, before);
    vi.advanceTimersByTime(320);
    expectCorners(img, rectCorners(next.getBoundingClientRect()));
  });

  it('停手时进度不足阈值回弹到完全打开，继续滚动推迟收尾，回弹后从当前位置重新起算', () => {
    const { img, overlay, backdrop } = openImage();
    const start = corners(img);
    scrollTo(530);
    vi.advanceTimersByTime(100);
    scrollTo(535);
    vi.advanceTimersByTime(100);
    expect(backdrop.style.opacity).not.toBe('1'); // 计时器已重置，尚未收尾
    vi.advanceTimersByTime(60);
    expect(backdrop.style.opacity).toBe('1');
    expectCorners(img, start);
    expect(img.style.transition).toContain('transform 180ms');
    expect(overlay.isConnected).toBe(true);
    scrollTo(565); // 以 535 为新起点，1/4 行程
    expect(backdrop.style.opacity).toBe('0.75');
  });

  it('停手时进度超过阈值继续飞回关闭，落在原图实时位置', () => {
    const { origin, img, overlay } = openImage();
    scrollTo(560); // 进度 0.5
    vi.advanceTimersByTime(150);
    expect(overlay.isConnected).toBe(true);
    vi.advanceTimersByTime(320);
    expect(overlay.isConnected).toBe(false);
    expectCorners(img, rectCorners(origin.getBoundingClientRect()));
  });

  it('图片还在加载时也能滚动关闭，迟到的 load 不会重新播放打开动画', () => {
    const { img, overlay, backdrop } = openImage(false);
    scrollTo(530);
    const closing = img.style.transform;
    img.dispatchEvent(new Event('load'));
    vi.advanceTimersByTime(64);
    expect(img.style.transform).toBe(closing);
    expect(backdrop.style.opacity).toBe('0');
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
