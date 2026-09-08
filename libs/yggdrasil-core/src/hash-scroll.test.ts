/**
 * hash-scroll 测试:钉住锚点跳转的解码与手动偏移滚动行为。
 * 黑盒驱动:只通过 window.__scrollToHash 入口。
 *
 * 现在用 window.scrollTo 手动扣除 sticky header 高度（不再依赖 scrollIntoView +
 * scroll-margin-top，因为浏览器原生 fragment-scroll 对后者的应用时序不稳定）。
 *
 * 布局稳定期（ResizeObserver）测试：钉住异步布局位移（mermaid 懒加载注入 SVG、
 * 图片/字体加载撑高内容）后落点会重新校正，且用户主动滚动后停止校正。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import './index';

// --- ResizeObserver mock：happy-dom 不实现，手动提供回调钩子 ---
let resizeCallback:
  | ((entries: { target: Element; contentRect: { width: number; height: number } }[]) => void)
  | null = null;
const roDisconnect = vi.fn();
vi.stubGlobal(
  'ResizeObserver',
  class {
    constructor(cb: typeof resizeCallback) {
      resizeCallback = cb;
    }
    observe() {}
    disconnect() {
      roDisconnect();
    }
  },
);

describe('scrollToHash', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    history.replaceState(null, '', '#');
    vi.restoreAllMocks();
    resizeCallback = null;
    roDisconnect.mockClear();
  });
  afterEach(() => {
    document.body.innerHTML = '';
    history.replaceState(null, '', '#');
  });

  it('历史返回恢复阅读位置时不会重新跳到旧 hash', () => {
    document.body.innerHTML = '<h2 id="chapter">Chapter</h2>';
    history.replaceState(null, '', '#chapter');
    vi.spyOn(window.__routeTransitions, 'isRestoring').mockReturnValue(true);
    const scroll = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    window.__scrollToHash();
    expect(scroll).not.toHaveBeenCalled();
  });

  it('hash 为 CJK 百分号编码时解码后命中并滚动', () => {
    const heading = document.createElement('h2');
    heading.id = '三-五-零法则';
    document.body.appendChild(heading);

    const spy = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    history.replaceState(null, '', `#${encodeURIComponent(heading.id)}`);

    window.__scrollToHash();

    expect(spy).toHaveBeenCalled();
  });

  it('hash 为纯 ASCII 时直接命中', () => {
    const heading = document.createElement('h2');
    heading.id = 'getting-started';
    document.body.appendChild(heading);

    const spy = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    history.replaceState(null, '', '#getting-started');

    window.__scrollToHash();

    expect(spy).toHaveBeenCalled();
  });

  it('滚动落点扣除 sticky header 高度', () => {
    // 模拟 sticky header：getBoundingClientRect 返回 height 80。
    const header = document.createElement('header');
    header.className = 'sticky';
    header.getBoundingClientRect = () =>
      ({
        top: 0,
        left: 0,
        right: 0,
        bottom: 80,
        width: 1000,
        height: 80,
        x: 0,
        y: 0,
        toJSON() {},
      }) as DOMRect;
    document.body.appendChild(header);

    // 目标标题：放在 scrollY=1000 处（getBoundingClientRect.top 反映相对视口位置）。
    const heading = document.createElement('h2');
    heading.id = 'mid';
    heading.getBoundingClientRect = () =>
      ({
        top: 1000,
        left: 0,
        right: 0,
        bottom: 1050,
        width: 800,
        height: 50,
        x: 0,
        y: 1000,
        toJSON() {},
      }) as DOMRect;
    document.body.appendChild(heading);

    // scrollY 参与 scrollTo 目标计算：top = rect.top + scrollY - headerH - offset。
    Object.defineProperty(window, 'scrollY', { value: 500, writable: true, configurable: true });

    const spy = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    history.replaceState(null, '', '#mid');

    window.__scrollToHash();

    // 第一帧调用（smooth）：top = 1000 + 500 - 80 - 16 = 1404
    expect(spy).toHaveBeenCalled();
    const firstCall = spy.mock.calls[0][0] as ScrollToOptions;
    expect(firstCall.top).toBe(1000 + 500 - 80 - 16);
    expect(firstCall.behavior).toBe('smooth');
  });

  it('hash 为空时不滚动也不报错', () => {
    const spy = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    history.replaceState(null, '', '#');
    expect(() => window.__scrollToHash()).not.toThrow();
    expect(spy).not.toHaveBeenCalled();
  });

  it('目标元素不存在时不报错', () => {
    const spy = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    history.replaceState(null, '', '#no-such-heading');
    expect(() => window.__scrollToHash()).not.toThrow();
    expect(spy).not.toHaveBeenCalled();
  });

  // ---------- 布局稳定期（ResizeObserver） ----------
  // 复现 bug 根因：mermaid 懒加载等异步位移源发生在两帧校正之后，撑高内容把标题
  // 推离落点。稳定期监听内容容器 resize，位移后重新校正一次。

  /** 构造一个带 .post-content 容器和带 id 标题的 DOM，用于稳定期测试。 */
  function setupPostWithHeading(headingId: string): { root: HTMLElement; heading: HTMLElement } {
    const root = document.createElement('div');
    root.className = 'post-content';
    const heading = document.createElement('h2');
    heading.id = headingId;
    heading.getBoundingClientRect = () =>
      ({
        top: 1000,
        left: 0,
        right: 0,
        bottom: 1050,
        width: 800,
        height: 50,
        x: 0,
        y: 0,
        toJSON() {},
      }) as DOMRect;
    root.appendChild(heading);
    document.body.appendChild(root);
    Object.defineProperty(window, 'scrollY', { value: 500, writable: true, configurable: true });
    return { root, heading };
  }

  it('内容容器 resize 后再次校正落点（异步布局位移修正）', async () => {
    vi.useFakeTimers();
    const spy = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    setupPostWithHeading('mermaid-tu-biao');
    history.replaceState(null, '', '#mermaid-tu-biao');

    window.__scrollToHash();
    // 推进两帧校正的 rAF（fake rAF 周期 ~16ms），不触发 2s 超时。
    await vi.advanceTimersByTimeAsync(40);
    spy.mockClear();

    expect(resizeCallback).not.toBeNull();
    // 模拟 mermaid 注入 SVG 撑高内容：触发一次 resize。
    resizeCallback!([
      {
        target: document.querySelector('.post-content')!,
        contentRect: { width: 800, height: 2000 },
      },
    ]);
    // rAF 合并的校正在下一帧执行。
    await vi.advanceTimersByTimeAsync(20);

    expect(spy).toHaveBeenCalled();
    // 推进超时，让稳定器在本测试内自清理，避免泄漏到后续测试。
    await vi.advanceTimersByTimeAsync(2000);
    vi.useRealTimers();
  });

  it('用户主动滚动（wheel）后停止校正，避免与用户交互打架', () => {
    vi.useFakeTimers();
    vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    setupPostWithHeading('h2');
    history.replaceState(null, '', '#h2');

    window.__scrollToHash();
    expect(roDisconnect).not.toHaveBeenCalled();

    // 用户主动滚动：派发 wheel，应触发 dispose（RO 断开、监听移除、超时清除）。
    window.dispatchEvent(new WheelEvent('wheel'));
    expect(roDisconnect).toHaveBeenCalled();

    // dispose 后 resize 不再触发新的稳定器：RO 已断开，回调不会再被调用。
    expect(resizeCallback).not.toBeNull();
    vi.useRealTimers();
  });

  it('重入安全：再次 scrollToHash 时旧稳定器被清理（不泄漏/不叠加）', () => {
    vi.useFakeTimers();
    vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    setupPostWithHeading('h3');
    history.replaceState(null, '', '#h3');

    window.__scrollToHash();
    expect(roDisconnect).not.toHaveBeenCalled();

    // 第二次调用（模拟主题切换触发 use_effect 重跑）。
    window.__scrollToHash();
    expect(roDisconnect).toHaveBeenCalled();
    vi.useRealTimers();
  });
});
