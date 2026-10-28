/**
 * anchor-click 测试:钉住 hash 锚点点击的拦截、滚动与 URL 更新。
 * 黑盒驱动:加载站点 bundle 后直接派发真实 DOM 事件，不依赖文章组件初始化。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import './index';

// 模拟路由器监听器:挂在 document 的 bubble 阶段 click
// 会被触发时的副作用（preventDefault + 整页跳转）。测试用它验证拦截器确实阻止了冒泡。
let routerInterceptorCalled = false;

describe('initAnchorClick', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    history.replaceState(null, '', '#');
    vi.restoreAllMocks();
    routerInterceptorCalled = false;
    // 模拟 Leptos 路由器在冒泡阶段的 click 监听器。
    // 拦截器在 window capture 阶段 stopPropagation 后，这个监听器不应被触发。
    document.addEventListener(
      'click',
      () => {
        routerInterceptorCalled = true;
      },
      false,
    );
  });
  afterEach(() => {
    document.body.innerHTML = '';
    history.replaceState(null, '', '#');
  });

  it('首次加载首页即可平滑开始阅读，相同 hash 的重复点击仍生效', () => {
    document.body.innerHTML = `
      <a class="home-reading-link" href="#home-posts">开始阅读 <span>↓</span></a>
      <section id="home-posts" tabindex="-1">文章列表</section>`;
    const scroll = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    const sync = vi.spyOn(window.__routeTransitions, 'syncHash');
    const arrow = document.querySelector<HTMLElement>('.home-reading-link span')!;

    arrow.click();
    expect(scroll).toHaveBeenCalledExactlyOnceWith({ top: 0, behavior: 'smooth' });
    expect(window.location.hash).toBe('#home-posts');
    expect(routerInterceptorCalled).toBe(false);

    // 回到首屏后再次点击：hash 没变也必须启动新的滚动。
    scroll.mockClear();
    arrow.click();
    expect(scroll).toHaveBeenCalledExactlyOnceWith({ top: 0, behavior: 'smooth' });
    expect(sync).toHaveBeenCalledOnce();
    expect(routerInterceptorCalled).toBe(false);
  });

  it('减少动态效果时开始阅读立即到达列表', () => {
    document.body.innerHTML =
      '<a href="#home-posts">开始阅读</a><section id="home-posts"></section>';
    const media = window.matchMedia('(prefers-reduced-motion: reduce)');
    vi.spyOn(window, 'matchMedia').mockReturnValue({ ...media, matches: true });
    const scroll = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});

    document.querySelector<HTMLAnchorElement>('a')!.click();

    expect(scroll).toHaveBeenCalledExactlyOnceWith({ top: 0, behavior: 'instant' });
    expect(routerInterceptorCalled).toBe(false);
  });

  it('点击 hash 锚点滚动到目标并阻止 路由器监听器', () => {
    const heading = document.createElement('h2');
    heading.id = 'ru-men-zhi-nan';
    document.body.appendChild(heading);

    const anchor = document.createElement('a');
    anchor.setAttribute('href', '#ru-men-zhi-nan');
    document.body.appendChild(anchor);

    const spy = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    anchor.click();

    expect(spy).toHaveBeenCalled();
    // 关键:路由器监听器未被触发，hash 锚点不会被当作路由请求。
    expect(routerInterceptorCalled).toBe(false);
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

  it('局部目录只滚动自己的容器，不修改图鉴 URL hash', () => {
    document.body.innerHTML = `
      <div data-local-anchor-scroll id="sample-root">
        <a href="#sample-heading">章节</a><h2 id="sample-heading">正文</h2>
      </div>`;
    const root = document.getElementById('sample-root')!;
    const scroll = vi.fn();
    root.scrollTo = scroll;
    const windowScroll = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});

    root.querySelector('a')!.click();

    expect(scroll).toHaveBeenCalledOnce();
    expect(windowScroll).not.toHaveBeenCalled();
    expect(window.location.hash).toBe('');
    expect(routerInterceptorCalled).toBe(false);
  });

  it('data-app-anchor 内的锚点完全放行给组件自己的 click 处理', () => {
    document.body.innerHTML = `
      <nav data-app-anchor="true"><a href="#settings-sec-ratelimit">接口限流</a></nav>
      <section id="settings-sec-ratelimit"></section>`;
    const anchor = document.querySelector('a')!;
    // 模拟组件 onclick 的 prevent_default；否则 happy-dom 的 HTMLAnchorElement.dispatchEvent
    // 会在未阻止默认行为时 window.open(href, '_self') 改写 location，污染 hash 断言。
    anchor.addEventListener('click', (event) => event.preventDefault());
    const windowScroll = vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
    anchor.click();
    expect(windowScroll).not.toHaveBeenCalled();
    expect(window.location.hash).toBe('');
    expect(routerInterceptorCalled).toBe(true);
  });

  it('目标元素不存在时放行，不阻止路由器原行为', () => {
    const anchor = document.createElement('a');
    anchor.setAttribute('href', '#no-such-heading');
    document.body.appendChild(anchor);

    anchor.click();

    // 目标不存在:拦截器不接管，事件继续冒泡到路由器监听器。
    expect(routerInterceptorCalled).toBe(true);
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

    expect(routerInterceptorCalled).toBe(true);
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
    expect(routerInterceptorCalled).toBe(true);
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
