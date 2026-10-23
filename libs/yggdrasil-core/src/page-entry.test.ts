import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { initPageReveal } from './page-entry';

let dispose: () => void;

function reveal(transition: ViewTransition | null = null): void {
  const event = new Event('pagereveal');
  Object.defineProperty(event, 'viewTransition', { value: transition });
  window.dispatchEvent(event);
}

function nativeTransition(): ViewTransition {
  return {
    ready: Promise.resolve(),
    updateCallbackDone: Promise.resolve(),
    finished: Promise.resolve(),
    skipTransition: vi.fn(),
    types: new Set<string>(),
  } as ViewTransition;
}

beforeEach(() => {
  document.documentElement.removeAttribute('data-vt-enter-handled');
  document.body.innerHTML = '<main data-vt-route="/about"></main>';
  vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: false } as MediaQueryList);
  dispose = initPageReveal();
});

afterEach(() => {
  dispose();
  document.documentElement.removeAttribute('data-vt-enter-handled');
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('page entry snapshot handling', () => {
  it('suppresses a cross-document reveal synchronously before its new snapshot', () => {
    reveal(nativeTransition());
    expect(document.querySelector('main')?.hasAttribute('data-vt-enter-handled')).toBe(true);
  });

  it('keeps ordinary first loads and unsupported pagereveal events unchanged', () => {
    reveal();
    window.dispatchEvent(new Event('pagereveal'));
    expect(document.querySelector('[data-vt-enter-handled]')).toBeNull();
  });

  it('retains the reduced-motion fallback without marking its entry', () => {
    vi.mocked(window.matchMedia).mockReturnValue({ matches: true } as MediaQueryList);
    reveal(nativeTransition());
    expect(document.querySelector('[data-vt-enter-handled]')).toBeNull();
  });

  it('native completion does not reenable this mount and replay its entrance', async () => {
    const transition = nativeTransition();
    reveal(transition);
    await transition.finished;
    expect(document.querySelector('main')?.hasAttribute('data-vt-enter-handled')).toBe(true);
  });

  it('marks the document before a streaming route wrapper exists', () => {
    document.body.innerHTML = '';
    reveal(nativeTransition());
    expect(document.documentElement.hasAttribute('data-vt-enter-handled')).toBe(true);
  });

  it('is idempotent across repeated registration and can be disposed', () => {
    const wrapper = document.querySelector<HTMLElement>('main')!;
    const mark = vi.spyOn(wrapper, 'setAttribute');
    expect(initPageReveal()).toBe(dispose);
    reveal(nativeTransition());
    expect(mark).toHaveBeenCalledOnce();
    dispose();
    wrapper.removeAttribute('data-vt-enter-handled');
    reveal(nativeTransition());
    expect(wrapper.hasAttribute('data-vt-enter-handled')).toBe(false);
  });
});
