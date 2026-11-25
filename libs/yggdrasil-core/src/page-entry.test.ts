import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import earlyNavigation from './early-navigation.js?raw';

// 使用独立文档执行实际内联脚本，每个用例都有新的事件监听环境。
const testWindow = () =>
  Object.assign(new EventTarget(), {
    matchMedia: vi.fn(() => ({ matches: false })),
    __yggNavigationEvents: undefined as Window['__yggNavigationEvents'],
  });
let win: ReturnType<typeof testWindow>;
let doc: Document;
const install = () => new Function('window', 'document', earlyNavigation)(win, doc);

function reveal(transition: ViewTransition | null = null): void {
  const event = new Event('pagereveal');
  Object.defineProperty(event, 'viewTransition', { value: transition });
  win.dispatchEvent(event);
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
  win = testWindow();
  doc = document.implementation.createHTMLDocument('');
  doc.documentElement.removeAttribute('data-vt-enter-handled');
  doc.body.innerHTML = '<main data-vt-route="/about"></main>';
  install();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('page entry snapshot handling', () => {
  it('suppresses a cross-document reveal synchronously before its new snapshot', () => {
    reveal(nativeTransition());
    expect(doc.querySelector('main')?.hasAttribute('data-vt-enter-handled')).toBe(true);
  });

  it('keeps ordinary first loads and unsupported pagereveal events unchanged', () => {
    reveal();
    win.dispatchEvent(new Event('pagereveal'));
    expect(doc.querySelector('[data-vt-enter-handled]')).toBeNull();
  });

  it('retains the reduced-motion fallback without marking its entry', () => {
    vi.mocked(win.matchMedia).mockReturnValue({ matches: true } as MediaQueryList);
    reveal(nativeTransition());
    expect(doc.querySelector('[data-vt-enter-handled]')).toBeNull();
  });

  it('native completion does not reenable this mount and replay its entrance', async () => {
    const transition = nativeTransition();
    reveal(transition);
    await transition.finished;
    expect(doc.querySelector('main')?.hasAttribute('data-vt-enter-handled')).toBe(true);
  });

  it('marks the document before a streaming route wrapper exists', () => {
    doc.body.innerHTML = '';
    reveal(nativeTransition());
    expect(doc.documentElement.hasAttribute('data-vt-enter-handled')).toBe(true);
  });

  it('is idempotent across repeated registration', () => {
    const wrapper = doc.querySelector<HTMLElement>('main')!;
    const mark = vi.spyOn(wrapper, 'setAttribute');
    install();
    reveal(nativeTransition());
    expect(mark).toHaveBeenCalledOnce();
  });

  it('在导航代码加载前保留原生事件，之后交给同一组监听器', () => {
    const link = doc.createElement('a');
    doc.body.appendChild(link);
    const click = () =>
      link.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    expect(click()).toBe(true);
    const events = win.__yggNavigationEvents!;
    events.click = (event) => event.preventDefault();
    events.pop = vi.fn();
    expect(click()).toBe(false);
    win.dispatchEvent(new Event('popstate'));
    expect(events.pop).toHaveBeenCalledOnce();
  });
});
