/**
 * anchor-click 测试:钉住 hash 锚点点击的拦截、滚动与 URL 更新。
 * 黑盒驱动:只通过 window.__initAnchorClick 入口 + 真实 DOM 事件派发。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import './index';

// 模拟 Dioxus 委托监听器:挂在 document 的 bubble 阶段 click，模拟 handleClickNavigate
// 会被触发时的副作用（preventDefault + 整页跳转）。测试用它验证拦截器确实阻止了冒泡。
let dioxusInterceptorCalled = false;

describe('initAnchorClick', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    history.replaceState(null, '', '#');
    vi.restoreAllMocks();
    dioxusInterceptorCalled = false;
    // 模拟 Dioxus 在 document bubble 阶段的 click 委托监听器。
    // 拦截器在 window capture 阶段 stopPropagation 后，这个监听器不应被触发。
    document.addEventListener(
      'click',
      () => {
        dioxusInterceptorCalled = true;
      },
      false,
    );
    window.__initAnchorClick();
  });
  afterEach(() => {
    document.body.innerHTML = '';
    history.replaceState(null, '', '#');
  });

  it('点击 hash 锚点滚动到目标并阻止 Dioxus 委托监听器', () => {
    const heading = document.createElement('h2');
    heading.id = 'ru-men-zhi-nan';
    document.body.appendChild(heading);

    const anchor = document.createElement('a');
    anchor.setAttribute('href', '#ru-men-zhi-nan');
    document.body.appendChild(anchor);

    const spy = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    anchor.click();

    expect(spy).toHaveBeenCalled();
    // 关键:Dioxus 委托监听器未被触发 → handleClickNavigate/browser_open 不会执行。
    expect(dioxusInterceptorCalled).toBe(false);
  });

  it('点击后 URL hash 更新为锚点 id（不触发整页刷新）', () => {
    const heading = document.createElement('h2');
    heading.id = 'section-2';
    document.body.appendChild(heading);

    const anchor = document.createElement('a');
    anchor.setAttribute('href', '#section-2');
    document.body.appendChild(anchor);

    anchor.click();

    expect(window.location.hash).toBe('#section-2');
  });

  it('目标元素不存在时放行，不阻止 Dioxus 原行为', () => {
    const anchor = document.createElement('a');
    anchor.setAttribute('href', '#no-such-heading');
    document.body.appendChild(anchor);

    anchor.click();

    // 目标不存在:拦截器不接管，事件继续冒泡到 Dioxus 监听器。
    expect(dioxusInterceptorCalled).toBe(true);
  });

  it('锚点替换保留导航记录及其它 history state', () => {
    history.replaceState({ __yggdrasilNavigation: 'entry-7', unrelated: 42 }, '', '#');
    document.body.innerHTML = '<h2 id="chapter">Chapter</h2><a href="#chapter">Jump</a>';
    const sync = vi.spyOn(window.__routeTransitions, 'syncHash');
    document.querySelector<HTMLAnchorElement>('a')!.click();
    expect(history.state).toEqual({ __yggdrasilNavigation: 'entry-7', unrelated: 42 });
    expect(sync).toHaveBeenCalledOnce();
  });

  it('非 hash 链接不拦截（外链/路径链接交给原行为）', () => {
    const anchor = document.createElement('a');
    anchor.setAttribute('href', '/post/other');
    document.body.appendChild(anchor);

    anchor.click();

    expect(dioxusInterceptorCalled).toBe(true);
  });

  it('带修饰键的点击不拦截（新窗口等交给浏览器）', () => {
    const heading = document.createElement('h2');
    heading.id = 'target';
    document.body.appendChild(heading);

    const anchor = document.createElement('a');
    anchor.setAttribute('href', '#target');
    document.body.appendChild(anchor);

    const spy = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});

    // 模拟 ctrl+click（新标签页）:dispatchEvent 携带 ctrlKey。
    const event = new MouseEvent('click', { bubbles: true, ctrlKey: true });
    anchor.dispatchEvent(event);

    expect(spy).not.toHaveBeenCalled();
    expect(dioxusInterceptorCalled).toBe(true);
  });

  it('重复调用 initAnchorClick 幂等，不会重复注册监听器', () => {
    const heading = document.createElement('h2');
    heading.id = 'dup-target';
    document.body.appendChild(heading);

    const anchor = document.createElement('a');
    anchor.setAttribute('href', '#dup-target');
    document.body.appendChild(anchor);

    // 再次调用（模拟 PostContent 多次挂载）。
    window.__initAnchorClick();
    window.__initAnchorClick();

    const spy = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    anchor.click();

    // 即使重复调用，scrollTo 仍只触发一次（监听器只注册一份）。
    expect(spy).toHaveBeenCalledOnce();
  });
});
