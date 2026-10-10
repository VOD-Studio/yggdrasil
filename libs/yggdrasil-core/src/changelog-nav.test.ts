import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { disposeChangelogNav, initChangelogNav } from './changelog-nav';
import { scrollToHeading } from './scroll-to-heading';

describe('changelog version navigation', () => {
  let scrollY: number;
  let horizontal: boolean;
  let reduced: boolean;
  let scroller: HTMLElement;
  let nav: HTMLElement;
  type Scroll = (options?: ScrollToOptions | number, y?: number) => void;
  let scroll: ReturnType<typeof vi.fn<Scroll>>;

  beforeEach(() => {
    vi.useFakeTimers();
    scrollY = 0;
    horizontal = false;
    reduced = false;
    document.body.innerHTML = `
      <header class="sticky"></header>
      <nav class="changelog-nav"><div class="changelog-nav-panel">
        <div class="changelog-nav-scroll"><div class="changelog-nav-items">
          ${['1.2', '1.1', '1.0'].map((v) => `<a class="changelog-nav-link" href="#v${v}">${v}</a>`).join('')}
        </div></div><span data-nav-position></span>
      </div></nav>
      ${['1.2', '1.1', '1.0'].map((v) => `<article id="v${v}"></article>`).join('')}`;
    nav = document.querySelector('.changelog-nav')!;
    scroller = document.querySelector('.changelog-nav-scroll')!;
    vi.spyOn(window, 'matchMedia').mockImplementation(
      (query) =>
        ({ matches: query.includes('reduced-motion') ? reduced : horizontal }) as MediaQueryList,
    );
    vi.spyOn(window, 'scrollY', 'get').mockImplementation(() => scrollY);
    vi.spyOn(window, 'innerHeight', 'get').mockReturnValue(800);
    vi.spyOn(document.documentElement, 'scrollHeight', 'get').mockReturnValue(4000);
    vi.spyOn(nav, 'matches').mockReturnValue(false);
    vi.spyOn(document.querySelector('header')!, 'getBoundingClientRect').mockReturnValue({
      height: 64,
    } as DOMRect);
    vi.spyOn(scroller, 'getBoundingClientRect').mockReturnValue({ top: 80, left: 0 } as DOMRect);
    vi.spyOn(scroller, 'clientHeight', 'get').mockReturnValue(88);
    vi.spyOn(scroller, 'scrollHeight', 'get').mockReturnValue(148);
    vi.spyOn(scroller, 'clientWidth', 'get').mockReturnValue(172);
    vi.spyOn(scroller, 'scrollWidth', 'get').mockReturnValue(340);
    scroll = vi.fn<Scroll>();
    scroller.scrollTo = scroll;
    document.querySelectorAll<HTMLElement>('article').forEach((card, i) => {
      vi.spyOn(card, 'getBoundingClientRect').mockImplementation(
        () => ({ top: 200 + i * 1000 - scrollY }) as DOMRect,
      );
    });
    nav.querySelectorAll<HTMLAnchorElement>('a').forEach((link, i) => {
      vi.spyOn(link, 'offsetTop', 'get').mockReturnValue(8 + i * 44);
      vi.spyOn(link, 'offsetLeft', 'get').mockReturnValue(8 + (horizontal ? i * 100 : 0));
      vi.spyOn(link, 'offsetWidth', 'get').mockReturnValue(172);
      vi.spyOn(link, 'getBoundingClientRect').mockImplementation(
        () =>
          ({
            top: 88 + i * 44 - scroller.scrollTop,
            height: 40,
            left: 8 + i * 100,
            width: 92,
          }) as DOMRect,
      );
    });
  });

  afterEach(() => {
    disposeChangelogNav();
    vi.restoreAllMocks();
    vi.useRealTimers();
    document.body.innerHTML = '';
  });

  const current = () => nav.querySelector('a[aria-current]')?.textContent;
  const tick = () => vi.advanceTimersByTime(20);

  it('uses the current card on a direct deep link and moves the directory independently', () => {
    scrollY = 1150;
    initChangelogNav();
    expect(current()).toBe('1.1');
    expect(nav.querySelector('[data-nav-position]')?.textContent).toBe('02 / 03');
    expect(scroll).toHaveBeenCalledWith({ top: 28, behavior: 'smooth' });
    expect(scrollY).toBe(1150);
    expect(scroller.hasAttribute('data-overflow-end')).toBe(true);
  });

  it('keeps the current version through long cards and selects the final card at the page bottom', () => {
    initChangelogNav();
    scrollY = 1800;
    window.dispatchEvent(new Event('scroll'));
    tick();
    expect(current()).toBe('1.1');
    scrollY = 3200;
    window.dispatchEvent(new Event('scroll'));
    tick();
    expect(current()).toBe('1.0');
    expect(nav.querySelector('.changelog-nav-panel')?.getAttribute('style')).toContain(
      '--nav-progress: 1',
    );
  });

  it('does not recenter while the user browses the directory with a pointer or keyboard', () => {
    vi.mocked(nav.matches).mockReturnValue(true);
    scrollY = 2150;
    initChangelogNav();
    expect(scroll).not.toHaveBeenCalled();
    vi.mocked(nav.matches).mockReturnValue(false);
    const focusedLink = nav.querySelector('a')!;
    focusedLink.focus();
    vi.spyOn(focusedLink, 'matches').mockReturnValue(true);
    window.dispatchEvent(new Event('resize'));
    tick();
    expect(scroll).not.toHaveBeenCalled();
    (document.activeElement as HTMLElement).blur();
    nav.dispatchEvent(new Event('pointerleave'));
    tick();
    expect(scroll).toHaveBeenCalled();
  });

  it('updates overflow fades without changing the selected card on directory scroll', () => {
    initChangelogNav();
    scroll.mockClear();
    scroller.scrollTop = 60;
    scroller.dispatchEvent(new Event('scroll'));
    expect(scroller.hasAttribute('data-overflow-start')).toBe(true);
    expect(scroller.hasAttribute('data-overflow-end')).toBe(false);
    expect(current()).toBe('1.2');
    expect(scroll).not.toHaveBeenCalled();
  });

  it('follows on the horizontal axis on small screens and honors reduced motion', () => {
    horizontal = true;
    reduced = true;
    scrollY = 2200;
    initChangelogNav();
    expect(scroll).toHaveBeenCalledWith({ left: 168, behavior: 'instant' });
    const pageScroll = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    scrollToHeading(document.getElementById('v1.0')!);
    expect(pageScroll).toHaveBeenCalledWith({ top: 2120, behavior: 'instant' });
  });

  it('leaves room for the sticky mobile directory when jumping to a version', () => {
    horizontal = true;
    scrollY = 2200;
    vi.spyOn(nav, 'getBoundingClientRect').mockReturnValue({ height: 88 } as DOMRect);
    const card = document.getElementById('v1.0')!;
    card.className = 'changelog-version';
    const pageScroll = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    scrollToHeading(card);
    expect(pageScroll).toHaveBeenCalledWith({ top: 2024, behavior: 'smooth' });
  });

  it('continues following after a pointer click leaves a link focused', () => {
    const focusedLink = nav.querySelector('a')!;
    focusedLink.focus();
    vi.spyOn(focusedLink, 'matches').mockReturnValue(false);
    scrollY = 2200;
    initChangelogNav();
    expect(scroll).toHaveBeenCalled();
  });

  it('cleans up listeners and pending frames on disposal and can mount again', () => {
    initChangelogNav();
    scrollY = 2200;
    window.dispatchEvent(new Event('scroll'));
    disposeChangelogNav();
    tick();
    expect(current()).toBe('1.2');
    window.dispatchEvent(new Event('scroll'));
    tick();
    expect(current()).toBe('1.2');
    initChangelogNav();
    expect(current()).toBe('1.0');
    scroll.mockClear();
    initChangelogNav();
    scroll.mockClear();
    window.dispatchEvent(new Event('scroll'));
    tick();
    expect(scroll).toHaveBeenCalledTimes(1);
  });
});
