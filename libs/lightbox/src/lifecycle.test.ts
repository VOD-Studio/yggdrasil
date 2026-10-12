import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fitCentered, transformFor } from './geometry';
import './index';

function mount(): HTMLImageElement[] {
  document.body.innerHTML =
    '<main class="post-content" data-showcase-lightbox-owner="lifecycle">' +
    '<img src="/uploads/first.webp" alt="第一张">' +
    '<img src="https://images.example/second.webp?token=preview" alt="第二张">' +
    '</main>';
  window.__initLightbox('.post-content');
  return [...document.querySelectorAll<HTMLImageElement>('.post-content img')];
}

function key(value: string): void {
  document.dispatchEvent(new KeyboardEvent('keydown', { key: value }));
}

function currentImage(): HTMLImageElement {
  return document.querySelector<HTMLImageElement>('.lightbox-img')!;
}

function ready(img: HTMLImageElement, width = 800, height = 600): void {
  Object.defineProperties(img, {
    naturalWidth: { value: width, configurable: true },
    naturalHeight: { value: height, configurable: true },
    complete: { value: false, configurable: true },
  });
  img.dispatchEvent(new Event('load'));
}

describe('lightbox session and SPA lifecycle', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    window.__closeLightboxFor('lifecycle');
    vi.runOnlyPendingTimers();
    vi.restoreAllMocks();
    vi.useRealTimers();
    document.body.innerHTML = '';
  });

  it('replaces a closing session immediately and its old callbacks leave the new overlay intact', () => {
    const [first, second] = mount();
    first.click();
    const oldImg = currentImage();
    ready(oldImg);
    vi.advanceTimersByTime(300);
    key('Escape');
    second.click();
    const newImg = currentImage();
    expect(newImg).not.toBe(oldImg);
    expect(document.querySelectorAll('.lightbox-overlay')).toHaveLength(1);
    oldImg.dispatchEvent(new Event('load'));
    oldImg.dispatchEvent(new Event('transitionend'));
    vi.advanceTimersByTime(300);
    expect(currentImage()).toBe(newImg);
    expect(newImg.getAttribute('src')).toBe(second.getAttribute('src'));
    expect(document.querySelectorAll('.lightbox-overlay')).toHaveLength(1);
  });

  it('owner unmount disposes a closing session without waiting for its animation', () => {
    const [first] = mount();
    first.click();
    key('Escape');
    document.querySelector('.post-content')!.remove();
    window.__closeLightboxFor('lifecycle');
    expect(document.querySelector('.lightbox-overlay')).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('switches source, accessible description and download behavior with the current gallery image', () => {
    const [first, second] = mount();
    first.click();
    const download = document.querySelector<HTMLAnchorElement>('[aria-label="下载原图"]')!;
    expect(download.getAttribute('download')).toBe('first.webp');
    key('ArrowRight');
    vi.advanceTimersByTime(150);
    expect(currentImage().getAttribute('src')).toBe(second.getAttribute('src'));
    expect(currentImage().alt).toBe('第二张');
    expect(download.getAttribute('href')).toBe(second.getAttribute('src'));
    expect(download.hasAttribute('download')).toBe(false);
    expect(download.getAttribute('target')).toBe('_blank');
    expect(download.getAttribute('rel')).toBe('noopener');
    key('ArrowLeft');
    vi.advanceTimersByTime(150);
    expect(download.getAttribute('href')).toBe('/uploads/first.webp');
    expect(download.getAttribute('download')).toBe('first.webp');
    expect(download.hasAttribute('target')).toBe(false);
  });

  it('does not replay the pending first-image opening animation after a new image loads', () => {
    const [first, second] = mount();
    vi.spyOn(first, 'getBoundingClientRect').mockReturnValue(new DOMRect(100, 200, 300, 200));
    first.click();
    const img = currentImage();
    key('ArrowRight');
    vi.advanceTimersByTime(150);
    expect(img.getAttribute('src')).toBe(second.getAttribute('src'));
    ready(img, 500, 1000);
    vi.advanceTimersByTime(300);
    const fit = fitCentered(500, 1000, window.innerWidth, window.innerHeight);
    expect(img.style.width).toBe(`${fit.w}px`);
    expect(img.style.height).toBe(`${fit.h}px`);
    expect(img.style.transform).toBe(transformFor(fit, fit.w, fit.h));
    expect(img.style.transition).toBe('opacity 150ms ease-out');
  });

  it('cancels already scheduled opening frames when switching to another image', () => {
    const [first] = mount();
    first.click();
    const img = currentImage();
    ready(img, 800, 600);
    key('ArrowRight');
    vi.advanceTimersByTime(150);
    ready(img, 400, 1000);
    vi.advanceTimersByTime(300);
    const fit = fitCentered(400, 1000, window.innerWidth, window.innerHeight);
    expect(img.style.transform).toBe(transformFor(fit, fit.w, fit.h));
    expect(img.style.transition).toBe('opacity 150ms ease-out');
  });

  it('reinitialization refreshes gallery indexes for keyed DOM nodes without duplicate entries', () => {
    const [first, second] = mount();
    const root = document.querySelector('.post-content')!;
    root.insertBefore(second, first);
    const third = document.createElement('img');
    third.src = '/uploads/third.webp';
    root.appendChild(third);
    window.__initLightbox(['.post-content', 'main']);
    first.click();
    expect(document.querySelector('.lightbox-counter')!.textContent).toBe('2 / 3');
    expect(document.querySelectorAll('.lightbox-overlay')).toHaveLength(1);
    key('ArrowRight');
    vi.advanceTimersByTime(150);
    expect(currentImage().getAttribute('src')).toBe('/uploads/third.webp');
  });

  it('cancels a thumbnail retry on unmount and allows the same DOM node to initialize again', async () => {
    mount();
    const root = document.querySelector('.post-content')!;
    const node = document.createElement('span');
    node.className = 'blur-img';
    node.innerHTML = '<img class="blur-img-full" data-src="/uploads/retry.webp">';
    root.appendChild(node);
    window.__initLightbox('.post-content');
    const img = node.querySelector<HTMLImageElement>('img')!;
    const assignSource = vi.spyOn(img, 'src', 'set');
    img.dispatchEvent(new Event('error'));
    node.remove();
    await Promise.resolve();
    vi.advanceTimersByTime(1000);
    expect(assignSource).not.toHaveBeenCalled();
    expect(node.hasAttribute('data-blur-init')).toBe(false);
    expect(node.classList.contains('is-error')).toBe(false);
    root.appendChild(node);
    window.__initLightbox('.post-content');
    ready(img, 900, 600);
    expect(node.classList.contains('is-loaded')).toBe(true);
    expect(node.style.getPropertyValue('--ar')).toBe('900 / 600');
  });

  it('removes document and viewport listeners when a session closes', () => {
    const [first] = mount();
    first.click();
    const img = currentImage();
    ready(img);
    vi.advanceTimersByTime(300);
    window.__closeLightboxFor('lifecycle');
    const style = img.style.cssText;
    key('r');
    window.dispatchEvent(new Event('resize'));
    window.dispatchEvent(new Event('scroll'));
    img.dispatchEvent(new MouseEvent('dblclick'));
    vi.advanceTimersByTime(3000);
    expect(img.style.cssText).toBe(style);
    expect(document.querySelector('.lightbox-overlay')).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
  });
});
