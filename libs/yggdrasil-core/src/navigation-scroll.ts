/** Scroll restoration belongs to a history entry, including the admin's nested viewport. */
import { scrollToHeading } from './scroll-to-heading';

export interface ScrollSnapshot {
  window: [number, number];
  containers: Record<string, [number, number]>;
}

export function captureScroll(): ScrollSnapshot {
  const containers: ScrollSnapshot['containers'] = {};
  for (const element of document.querySelectorAll<HTMLElement>('[data-vt-scroll]')) {
    containers[element.dataset.vtScroll!] = [element.scrollLeft, element.scrollTop];
  }
  return { window: [window.scrollX, window.scrollY], containers };
}

export function restoreScroll(snapshot: ScrollSnapshot | undefined, hash: string): void {
  if (snapshot) {
    window.scrollTo({ left: snapshot.window[0], top: snapshot.window[1], behavior: 'instant' });
    for (const element of document.querySelectorAll<HTMLElement>('[data-vt-scroll]')) {
      const [left, top] = snapshot.containers[element.dataset.vtScroll!] ?? [0, 0];
      element.scrollTo({ left, top, behavior: 'instant' });
    }
    return;
  }
  if (hash) {
    let id = hash.slice(1);
    try {
      id = decodeURIComponent(id);
    } catch {
      /* A malformed fragment is still a valid URL; try its literal ID. */
    }
    const target = document.getElementById(id);
    if (target) scrollToHeading(target, false);
    return;
  }
  window.scrollTo({ top: 0, left: 0, behavior: 'instant' });
  for (const element of document.querySelectorAll<HTMLElement>('[data-vt-scroll]')) {
    element.scrollTo({ left: 0, top: 0, behavior: 'instant' });
  }
}

/** Correct late layout changes for a bounded period; never fight the user's own scrolling. */
export function stabilizeNavigationScroll(correct: () => void, done: () => void): () => void {
  let stopped = false;
  let queued = false;
  const observer = new MutationObserver(schedule);
  const resize = typeof ResizeObserver === 'function' ? new ResizeObserver(schedule) : undefined;
  const events = ['wheel', 'touchmove', 'pointerdown', 'keydown'] as const;
  function schedule(): void {
    if (queued || stopped) return;
    queued = true;
    queueMicrotask(() => {
      queued = false;
      if (!stopped) correct();
    });
  }
  function stop(): void {
    if (stopped) return;
    stopped = true;
    observer.disconnect();
    resize?.disconnect();
    clearTimeout(timeout);
    for (const event of events) window.removeEventListener(event, stop, true);
    done();
  }
  // Only structural changes matter; observing style would observe our own corrections.
  observer.observe(document.body, { childList: true, subtree: true });
  resize?.observe(document.body);
  for (const element of document.querySelectorAll('[data-vt-scroll] > *')) resize?.observe(element);
  for (const event of events)
    window.addEventListener(event, stop, { capture: true, passive: true });
  const timeout = window.setTimeout(stop, 2000);
  return stop;
}
