import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { initPageReveal, markPageEntryHandled } from './page-entry';

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
  document.body.innerHTML =
    '<main data-vt-route="/about"><div class="animate-section-enter"></div></main>';
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
    expect(
      document.querySelector('.animate-section-enter')?.hasAttribute('data-vt-entry-captured'),
    ).toBe(true);
  });

  it('keeps ordinary first loads and unsupported pagereveal events unchanged', () => {
    reveal();
    window.dispatchEvent(new Event('pagereveal'));
    expect(document.querySelector('[data-vt-enter-handled]')).toBeNull();
    expect(document.querySelector('[data-vt-entry-captured]')).toBeNull();
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

  it('only captures currently mounted section panels, preserving later tab remounts', () => {
    const wrapper = document.querySelector<HTMLElement>('main')!;
    markPageEntryHandled(wrapper);
    const laterPanel = document.createElement('div');
    laterPanel.className = 'animate-section-enter';
    wrapper.append(laterPanel);
    expect(wrapper.firstElementChild?.hasAttribute('data-vt-entry-captured')).toBe(true);
    expect(laterPanel.hasAttribute('data-vt-entry-captured')).toBe(false);
  });

  it('preserves section animations inside dialogs and modal panels', () => {
    const wrapper = document.querySelector<HTMLElement>('main')!;
    wrapper.insertAdjacentHTML(
      'beforeend',
      '<div role="dialog"><div class="animate-section-enter" id="dialog-section"></div></div><div class="animate-modal-panel-enter"><div class="animate-section-enter" id="modal-section"></div></div>',
    );
    markPageEntryHandled(wrapper);
    expect(wrapper.firstElementChild?.hasAttribute('data-vt-entry-captured')).toBe(true);
    expect(document.getElementById('dialog-section')?.hasAttribute('data-vt-entry-captured')).toBe(
      false,
    );
    expect(document.getElementById('modal-section')?.hasAttribute('data-vt-entry-captured')).toBe(
      false,
    );
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
