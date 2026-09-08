import { prefersReducedMotion } from '@yggdrasil/shared';

/** Keep this mount static after native snapshots, without disabling later keyed panels. */
export function markPageEntryHandled(wrapper: HTMLElement): void {
  wrapper.setAttribute('data-vt-enter-handled', 'true');
  for (const element of wrapper.querySelectorAll('.animate-section-enter')) {
    if (element.closest('[role="dialog"], .animate-modal-panel-enter')) continue;
    element.setAttribute('data-vt-entry-captured', 'true');
  }
}

/** Register before first paint; WASM history hydration can follow pagereveal. */
export function initPageReveal(): () => void {
  // The development HTML can include the IIFE twice. Share registration on window
  // so those independent module instances still install just one listener.
  const pageWindow = window as Window & { __yggdrasilPageRevealCleanup?: () => void };
  if (pageWindow.__yggdrasilPageRevealCleanup) return pageWindow.__yggdrasilPageRevealCleanup;

  const onReveal = (event: Event) => {
    const transition = (event as Event & { viewTransition?: ViewTransition | null }).viewTransition;
    if (!transition || prefersReducedMotion()) return;
    // Streaming SSR can reveal before its route wrapper has finished parsing.
    // An html marker also covers later parsed descendants and is removed by the
    // next actual SPA page commit, just like a wrapper marker.
    const wrapper =
      document.querySelector<HTMLElement>('[data-vt-route]') ?? document.documentElement;
    markPageEntryHandled(wrapper);
    // Do not remove the marker on finished: that would restart CSS entrances.
  };
  window.addEventListener('pagereveal', onReveal);
  const cleanup = () => {
    window.removeEventListener('pagereveal', onReveal);
    if (pageWindow.__yggdrasilPageRevealCleanup === cleanup)
      delete pageWindow.__yggdrasilPageRevealCleanup;
  };
  pageWindow.__yggdrasilPageRevealCleanup = cleanup;
  return cleanup;
}
