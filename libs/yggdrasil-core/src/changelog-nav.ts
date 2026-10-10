import { initAnchorClick } from './anchor-click';
import { scrollToHash } from './hash-scroll';

let disposeActive: (() => void) | undefined;

export function disposeChangelogNav(): void {
  disposeActive?.();
  disposeActive = undefined;
}

/** Follow the reading position without taking over a directory being browsed. */
export function initChangelogNav(): void {
  disposeChangelogNav();
  const nav = document.querySelector<HTMLElement>('.changelog-nav');
  const scroller = nav?.querySelector<HTMLElement>('.changelog-nav-scroll');
  const panel = nav?.querySelector<HTMLElement>('.changelog-nav-panel');
  if (!nav || !scroller || !panel) return;

  const items = [...nav.querySelectorAll<HTMLAnchorElement>('.changelog-nav-link')]
    .map((link) => ({ link, card: document.getElementById(link.hash.slice(1)) }))
    .filter((item): item is { link: HTMLAnchorElement; card: HTMLElement } => !!item.card);
  if (!items.length) return;

  initAnchorClick();
  if (items.some(({ link }) => link.hash === window.location.hash)) scrollToHash();

  const position = nav.querySelector<HTMLElement>('[data-nav-position]');
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
  let active = -1;
  let frame = 0;

  function updateEdges(): void {
    const horizontal = window.matchMedia('(max-width: 1023px)').matches;
    const offset = horizontal ? scroller!.scrollLeft : scroller!.scrollTop;
    const size = horizontal ? scroller!.clientWidth : scroller!.clientHeight;
    const extent = horizontal ? scroller!.scrollWidth : scroller!.scrollHeight;
    scroller!.toggleAttribute('data-overflow-start', offset > 2);
    scroller!.toggleAttribute('data-overflow-end', offset + size < extent - 2);
  }

  function follow(link: HTMLAnchorElement): void {
    // A focused link or pointer over the directory means the user is choosing a version.
    const keyboardBrowsing =
      nav!.contains(document.activeElement) && document.activeElement?.matches(':focus-visible');
    if (nav!.matches(':hover') || keyboardBrowsing) return;
    const horizontal = window.matchMedia('(max-width: 1023px)').matches;
    const viewport = scroller!.getBoundingClientRect();
    const rect = link.getBoundingClientRect();
    const start = horizontal ? rect.left - viewport.left : rect.top - viewport.top;
    const size = horizontal ? rect.width : rect.height;
    const available = horizontal ? scroller!.clientWidth : scroller!.clientHeight;
    if (start >= 16 && start + size <= available - 16) return;
    const offset = start - (available - size) / 2;
    scroller!.scrollTo({
      ...(horizontal
        ? { left: scroller!.scrollLeft + offset }
        : { top: scroller!.scrollTop + offset }),
      behavior: reducedMotion.matches ? 'instant' : 'smooth',
    });
  }

  function update(): void {
    frame = 0;
    panel!.style.setProperty(
      '--nav-scroll-height',
      `${Math.max(40, window.innerHeight - Math.max(80, nav!.getBoundingClientRect().top) - 124)}px`,
    );
    const header = document.querySelector('header.sticky');
    const horizontal = window.matchMedia('(max-width: 1023px)').matches;
    const directoryHeight = horizontal ? nav!.getBoundingClientRect().height + 8 : 0;
    const line = (header?.getBoundingClientRect().height ?? 64) + directoryHeight + 24;
    let index = 0;
    for (let i = 0; i < items.length; i++) {
      if (items[i].card.getBoundingClientRect().top <= line) index = i;
      else break;
    }
    if (window.scrollY + window.innerHeight >= document.documentElement.scrollHeight - 4) {
      index = items.length - 1;
    }

    const link = items[index].link;
    if (active !== index) {
      for (const item of items) item.link.removeAttribute('aria-current');
      link.setAttribute('aria-current', 'location');
      active = index;
      if (position) {
        position.textContent = `${String(index + 1).padStart(2, '0')} / ${String(items.length).padStart(2, '0')}`;
      }
      panel!.style.setProperty('--nav-progress', String((index + 1) / items.length));
    }
    panel!.style.setProperty('--nav-active-x', `${link.offsetLeft - 8}px`);
    panel!.style.setProperty('--nav-active-y', `${link.offsetTop - 8}px`);
    panel!.style.setProperty('--nav-active-width', `${link.offsetWidth}px`);
    nav!.setAttribute('data-ready', '');
    follow(link);
    updateEdges();
  }

  function schedule(): void {
    if (!frame) frame = requestAnimationFrame(update);
  }

  update();
  const resize = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(schedule);
  resize?.observe(scroller);
  for (const { card } of items) resize?.observe(card);
  window.addEventListener('scroll', schedule, { passive: true });
  window.addEventListener('resize', schedule);
  scroller.addEventListener('scroll', updateEdges, { passive: true });
  nav.addEventListener('pointerleave', schedule);
  nav.addEventListener('focusout', schedule);
  disposeActive = () => {
    cancelAnimationFrame(frame);
    resize?.disconnect();
    window.removeEventListener('scroll', schedule);
    window.removeEventListener('resize', schedule);
    scroller.removeEventListener('scroll', updateEdges);
    nav.removeEventListener('pointerleave', schedule);
    nav.removeEventListener('focusout', schedule);
  };
}
