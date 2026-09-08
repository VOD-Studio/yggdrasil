/**
 * theme-transition 测试。
 *
 * happy-dom 不提供 document.startViewTransition,天然覆盖降级路径(瞬切 dark class)。
 * 主路径通过 mock startViewTransition 验证:调用它、设 CSS 变量。
 *
 * 注意:startThemeTransition 只接收 (x, y),目标主题(亮/暗)从 DOM 的 dark class
 * 现状推导(取反),不依赖外部传入——避免与调用方状态不同步。
 *
 * 主题变更事件(THEME_CHANGE_EVENT):验证 VT 回调 + 降级路径都同步 dispatch,
 * 且事件在 applyDarkClass 之前触发(编辑器换肤先于 class 翻转,同一 reflow 捕获)。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { onThemeChange, THEME_CHANGE_EVENT } from './theme-transition';
import './index';
import { beginTransition } from './view-transition-lifecycle';

describe('startThemeTransition', () => {
  beforeEach(() => {
    document.documentElement.classList.remove('dark');
    document.documentElement.classList.remove('is-theme-transitioning');
    document.documentElement.style.cssText = '';
    vi.restoreAllMocks();
  });
  afterEach(() => {
    document.documentElement.classList.remove('dark');
    document.documentElement.classList.remove('is-theme-transitioning');
    document.documentElement.style.cssText = '';
  });

  it('降级:无 startViewTransition 时,亮→暗(无 dark class 时 add)', () => {
    expect(document.documentElement.classList.contains('dark')).toBe(false);

    window.__startThemeTransition(100, 200);

    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  it('降级:无 startViewTransition 时,暗→亮(有 dark class 时 remove)', () => {
    document.documentElement.classList.add('dark');

    window.__startThemeTransition(100, 200);

    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  it('主路径:有 startViewTransition 时调用它,注入变量,callback 切换 dark class', async () => {
    const cbRef: { cb: (() => Promise<void>) | null } = { cb: null };
    const readyP = Promise.resolve();
    const finishedP = Promise.resolve();
    const startVT = vi.fn((cb: () => Promise<void>) => {
      cbRef.cb = cb;
      return { ready: readyP, finished: finishedP, skipTransition: () => {} };
    });
    Object.defineProperty(document, 'startViewTransition', {
      value: startVT,
      configurable: true,
      writable: true,
    });

    window.__startThemeTransition(100, 200);

    expect(startVT).toHaveBeenCalledTimes(1);

    // CSS 变量应注入
    expect(document.documentElement.style.getPropertyValue('--tt-x')).toBe('100px');
    expect(document.documentElement.style.getPropertyValue('--tt-y')).toBe('200px');
    expect(document.documentElement.style.getPropertyValue('--tt-r')).toMatch(/^\d+(\.\d+)?px$/);

    // is-theme-transitioning 应在 VT 之前添加
    expect(document.documentElement.classList.contains('is-theme-transitioning')).toBe(true);

    // callback 里根据 DOM 现状(无 dark)切到 dark(callback 现为 async,需 await)
    await cbRef.cb?.();
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    // finished 后移除 is-theme-transitioning 和 CSS 变量
    await finishedP;
    await Promise.resolve();
    // happy-dom microtask 可能需要额外 tick
    await new Promise((r) => setTimeout(r, 0));
    expect(document.documentElement.classList.contains('is-theme-transitioning')).toBe(false);
    expect(document.documentElement.style.getPropertyValue('--tt-x')).toBe('');

    delete (document as unknown as { startViewTransition?: unknown }).startViewTransition;
  });

  it('reduced-motion:即使有 startViewTransition 也走降级(瞬切)', () => {
    const startVT = vi.fn(() => ({
      ready: Promise.resolve(),
      finished: Promise.resolve(),
      skipTransition: () => {},
    }));
    Object.defineProperty(document, 'startViewTransition', {
      value: startVT,
      configurable: true,
      writable: true,
    });
    vi.stubGlobal(
      'matchMedia',
      vi.fn((q: string) => ({
        matches: q.includes('reduce'),
        media: q,
        onchange: null,
        addEventListener: () => {},
        removeEventListener: () => {},
        addListener: () => {},
        removeListener: () => {},
        dispatchEvent: () => false,
      })),
    );

    window.__startThemeTransition(0, 0);

    expect(startVT).not.toHaveBeenCalled();
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    delete (document as unknown as { startViewTransition?: unknown }).startViewTransition;
  });

  it('主路径:VT callback 内 dispatch 主题变更事件,且先于 dark class 翻转', async () => {
    const cbRef: { cb: (() => Promise<void>) | null } = { cb: null };
    Object.defineProperty(document, 'startViewTransition', {
      value: (cb: () => Promise<void>) => {
        cbRef.cb = cb;
        return { ready: Promise.resolve(), finished: Promise.resolve(), skipTransition: () => {} };
      },
      configurable: true,
      writable: true,
    });

    // 记录事件触发时刻的 dark class 状态——验证事件在 applyDarkClass 之前 dispatch
    const eventSnapshots: { isDark: boolean; darkClassAtDispatch: boolean }[] = [];
    const listener = (e: Event) => {
      const detail = (e as CustomEvent).detail as { isDark: boolean };
      eventSnapshots.push({
        isDark: detail.isDark,
        darkClassAtDispatch: document.documentElement.classList.contains('dark'),
      });
    };
    window.addEventListener(THEME_CHANGE_EVENT, listener);

    // 亮→暗:无 dark class,isDark=true
    window.__startThemeTransition(0, 0);
    await cbRef.cb?.();

    expect(eventSnapshots).toHaveLength(1);
    expect(eventSnapshots[0].isDark).toBe(true);
    // 事件触发时 dark class 尚未翻转(仍是 light)——证明事件先于 applyDarkClass
    expect(eventSnapshots[0].darkClassAtDispatch).toBe(false);
    // callback 执行完后 dark class 已翻转
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    window.removeEventListener(THEME_CHANGE_EVENT, listener);
    delete (document as unknown as { startViewTransition?: unknown }).startViewTransition;
  });

  it('降级路径:无 VT 时也 dispatch 主题变更事件(亮→暗)', () => {
    const calls: boolean[] = [];
    const listener = (e: Event) => {
      calls.push((e as CustomEvent).detail.isDark);
    };
    window.addEventListener(THEME_CHANGE_EVENT, listener);

    // 亮→暗
    window.__startThemeTransition(0, 0);
    expect(calls).toEqual([true]);
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    // 暗→亮
    window.__startThemeTransition(0, 0);
    expect(calls).toEqual([true, false]);
    expect(document.documentElement.classList.contains('dark')).toBe(false);

    window.removeEventListener(THEME_CHANGE_EVENT, listener);
  });

  it('applyResolvedTheme:同步 dispatch 主题变更事件 + 翻 dark class', () => {
    const calls: { isDark: boolean; darkClassAtDispatch: boolean }[] = [];
    const listener = (e: Event) => {
      const detail = (e as CustomEvent).detail as { isDark: boolean };
      calls.push({
        isDark: detail.isDark,
        darkClassAtDispatch: document.documentElement.classList.contains('dark'),
      });
    };
    window.addEventListener(THEME_CHANGE_EVENT, listener);

    window.__applyResolvedTheme(true);
    expect(calls).toEqual([{ isDark: true, darkClassAtDispatch: false }]);
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    window.__applyResolvedTheme(false);
    expect(calls).toEqual([
      { isDark: true, darkClassAtDispatch: false },
      { isDark: false, darkClassAtDispatch: true },
    ]);
    expect(document.documentElement.classList.contains('dark')).toBe(false);

    window.removeEventListener(THEME_CHANGE_EVENT, listener);
  });

  it('onThemeChange:VT callback 等待 registry 注册的异步回调 Promise', async () => {
    const cbRef: { cb: (() => Promise<void>) | null } = { cb: null };
    Object.defineProperty(document, 'startViewTransition', {
      value: (cb: () => Promise<void>) => {
        cbRef.cb = cb;
        return { ready: Promise.resolve(), finished: Promise.resolve(), skipTransition: () => {} };
      },
      configurable: true,
      writable: true,
    });

    // registry 回调返回一个可控的 Promise,记录它是否在 callback resolve 前完成。
    // resolveAsync 用 mutable 容器包裹,绕过 TS 跨闭包的控制流收窄(否则在 ?.() 处
    // 被判定为 null → never)。
    const resolveAsync: { fn: (() => void) | null } = { fn: null };
    const asyncDone = { value: false };
    const off = onThemeChange((isDark) => {
      void isDark;
      return new Promise<void>((resolve) => {
        resolveAsync.fn = () => {
          asyncDone.value = true;
          resolve();
        };
      });
    });

    window.__startThemeTransition(0, 0);

    // callback 尚未 resolve(async 任务未完成)——VT callback 的 Promise 仍 pending
    const callbackPromise = cbRef.cb?.();
    expect(asyncDone.value).toBe(false);

    // 触发异步任务完成
    resolveAsync.fn?.();
    await callbackPromise;
    expect(asyncDone.value).toBe(true);

    off();
    delete (document as unknown as { startViewTransition?: unknown }).startViewTransition;
  });

  it('onThemeChange:降级路径(无 VT)不等 registry 异步回调', async () => {
    // 降级路径不 await notifyThemeChange,applyDarkClass 同步完成,registry 后台跑
    let registryCalled = false;
    const off = onThemeChange(() => {
      registryCalled = true;
      return new Promise<void>(() => {}); // 永不 resolve
    });

    // 无 startViewTransition → 降级路径
    window.__startThemeTransition(0, 0);

    // dark class 应已同步翻转(不等 registry)
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    // registry 回调被调用(同步触发),但 Promise 未被 await
    expect(registryCalled).toBe(true);

    off();
  });
});

function deferred() {
  let resolve!: () => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<void>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

describe('theme and route transition interruptions', () => {
  const nativeTransitions: Array<{
    update: () => Promise<void>;
    ready: ReturnType<typeof deferred>;
    done: ReturnType<typeof deferred>;
    finished: ReturnType<typeof deferred>;
    skip: ReturnType<typeof vi.fn>;
  }> = [];

  beforeEach(() => {
    vi.unstubAllGlobals();
    document.documentElement.className = '';
    document.documentElement.style.cssText = '';
    nativeTransitions.length = 0;
    Object.defineProperty(document, 'startViewTransition', {
      configurable: true,
      value: vi.fn((update: () => Promise<void>) => {
        const native = {
          update,
          ready: deferred(),
          done: deferred(),
          finished: deferred(),
          skip: vi.fn(),
        };
        nativeTransitions.push(native);
        return {
          ready: native.ready.promise,
          updateCallbackDone: native.done.promise,
          finished: native.finished.promise,
          skipTransition: native.skip,
        };
      }),
    });
  });

  afterEach(() => {
    beginTransition('route').finish();
    delete (document as unknown as { startViewTransition?: unknown }).startViewTransition;
    document.documentElement.className = '';
    document.documentElement.style.cssText = '';
    vi.unstubAllGlobals();
  });

  it('two clicks before the first update commit both toggles, and stale callbacks cannot revert the latest color', async () => {
    window.__startThemeTransition(10, 20);
    window.__startThemeTransition(30, 40);
    const [first, second] = nativeTransitions;
    expect(first.skip).toHaveBeenCalledOnce();
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    await second.update();
    await first.update();
    expect(document.documentElement.classList.contains('dark')).toBe(false);
    expect(document.documentElement.style.getPropertyValue('--tt-x')).toBe('30px');

    first.finished.resolve();
    await Promise.resolve();
    expect(document.documentElement.classList.contains('is-theme-transitioning')).toBe(true);
    second.finished.resolve();
    await Promise.resolve();
    expect(document.documentElement.classList.contains('is-theme-transitioning')).toBe(false);
  });

  it('navigation commits a pending theme before taking over its snapshots', async () => {
    window.__startThemeTransition(10, 20);
    const route = beginTransition('route');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(document.documentElement.classList.contains('is-theme-transitioning')).toBe(false);
    expect(document.documentElement.style.getPropertyValue('--tt-r')).toBe('');
    await nativeTransitions[0].update();
    nativeTransitions[0].finished.resolve();
    await Promise.resolve();
    expect(route.isCurrent()).toBe(true);
  });

  it('theme changes cancel route ownership before native capture', () => {
    const route = beginTransition('route');
    const cleanup = vi.fn();
    route.onCancel(cleanup);
    window.__startThemeTransition(10, 20);
    expect(cleanup).toHaveBeenCalledOnce();
    expect(route.isCurrent()).toBe(false);
  });

  it('system preference overrides pending manual callbacks', async () => {
    window.__startThemeTransition(10, 20);
    window.__applyResolvedTheme(false);
    await nativeTransitions[0].update();
    expect(document.documentElement.classList.contains('dark')).toBe(false);
    expect(document.documentElement.classList.contains('is-theme-transitioning')).toBe(false);
  });

  it('failed native promises clean scoped styles without unhandled rejections', async () => {
    window.__startThemeTransition(10, 20);
    await nativeTransitions[0].update();
    nativeTransitions[0].ready.reject(new Error('capture failed'));
    nativeTransitions[0].done.reject(new Error('update failed'));
    nativeTransitions[0].finished.reject(new Error('update failed'));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(document.documentElement.classList.contains('is-theme-transitioning')).toBe(false);
    expect(document.documentElement.style.getPropertyValue('--tt-x')).toBe('');
  });

  it('native setup failures still commit the theme and remove snapshot styles', () => {
    Object.defineProperty(document, 'startViewTransition', {
      configurable: true,
      value: () => {
        throw new Error('native setup failed');
      },
    });
    window.__startThemeTransition(10, 20);
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(document.documentElement.classList.contains('is-theme-transitioning')).toBe(false);
  });
});
