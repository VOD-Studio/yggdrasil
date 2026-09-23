/**
 * toc-sidebar 测试：钉住 scroll-spy 的配对、激活切换、回退与幂等重入。
 * happy-dom 无 IntersectionObserver，vi.stubGlobal 捕获回调手动注入。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { disposeTocSidebar, initTocSidebar } from './toc-sidebar';

const disconnect = vi.fn();
const observe = vi.fn();
let intersectCallback:
  | ((entries: Array<{ target: HTMLElement; isIntersecting: boolean }>) => void)
  | null = null;

function stubIo(): void {
  vi.stubGlobal(
    'IntersectionObserver',
    class {
      constructor(cb: (entries: Array<{ target: HTMLElement; isIntersecting: boolean }>) => void) {
        intersectCallback = cb;
      }
      observe(el: Element) {
        observe(el);
      }
      disconnect() {
        disconnect();
      }
    },
  );
}

function setupDom(): {
  h2a: HTMLElement;
  h2b: HTMLElement;
  linkA: HTMLAnchorElement;
  linkB: HTMLAnchorElement;
} {
  document.body.innerHTML = `
    <nav class="toc-sidebar">
      <div class="toc-sidebar-body">
        <ul>
          <li><a href="#a">章节 A</a></li>
          <li><a href="#b">章节 B</a></li>
        </ul>
      </div>
    </nav>
    <article class="post-content">
      <h2 id="a">章节 A</h2>
      <h2 id="b">章节 B</h2>
    </article>`;
  return {
    h2a: document.getElementById('a')!,
    h2b: document.getElementById('b')!,
    linkA: document.querySelector<HTMLAnchorElement>('a[href="#a"]')!,
    linkB: document.querySelector<HTMLAnchorElement>('a[href="#b"]')!,
  };
}

/** 钉住元素的视口顶距（happy-dom 不做布局，rect 全零需手动给）。 */
function mockTop(el: HTMLElement, top: number): void {
  el.getBoundingClientRect = () =>
    ({
      top,
      bottom: top,
      left: 0,
      right: 0,
      width: 0,
      height: 0,
      x: 0,
      y: top,
      toJSON: () => ({}),
    }) as DOMRect;
}

describe('initTocSidebar', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    observe.mockClear();
    disconnect.mockClear();
    intersectCallback = null;
    stubIo();
  });
  afterEach(() => {
    document.body.innerHTML = '';
    // 无 nav 分支：dispose 上一次并复位模块级 disposePrev。
    initTocSidebar();
    document.body.innerHTML = '';
  });

  it('无 nav.toc-sidebar 时不抛错、不建 observer', () => {
    expect(() => initTocSidebar()).not.toThrow();
    expect(observe).not.toHaveBeenCalled();
    expect(intersectCallback).toBeNull();
  });

  it('正确配对目录链接与标题，初始无激活项（全文在顶线之下）', () => {
    const { h2a, h2b, linkA, linkB } = setupDom();
    mockTop(h2a, 500);
    mockTop(h2b, 900);

    initTocSidebar();

    expect(observe).toHaveBeenCalledTimes(2);
    expect(intersectCallback).not.toBeNull();
    expect(linkA.classList.contains('active')).toBe(false);
    expect(linkB.classList.contains('active')).toBe(false);
  });

  it('IO 回调驱动激活切换并维护 aria-current', () => {
    const { h2a, h2b, linkA, linkB } = setupDom();
    mockTop(h2a, 500);
    mockTop(h2b, 900);
    initTocSidebar();

    intersectCallback!([{ target: h2b, isIntersecting: true }]);
    expect(linkB.classList.contains('active')).toBe(true);
    expect(linkB.getAttribute('aria-current')).toBe('true');
    expect(linkA.classList.contains('active')).toBe(false);

    // b 离开探测带、a 进入 → 激活移到 a，b 的 aria-current 被移除。
    intersectCallback!([
      { target: h2b, isIntersecting: false },
      { target: h2a, isIntersecting: true },
    ]);
    expect(linkA.classList.contains('active')).toBe(true);
    expect(linkA.getAttribute('aria-current')).toBe('true');
    expect(linkB.classList.contains('active')).toBe(false);
    expect(linkB.getAttribute('aria-current')).toBeNull();
  });

  it('带内无标题时回退到顶线之上最后一个标题', () => {
    const { h2a, h2b, linkA, linkB } = setupDom();
    mockTop(h2a, -200); // 已滚过顶线
    mockTop(h2b, 500); // 顶线之下
    initTocSidebar();
    // 初始同步激活即应落在 a。
    expect(linkA.classList.contains('active')).toBe(true);

    // 两个标题都不在带内（处于长节中部）→ 保持回退到 a。
    intersectCallback!([
      { target: h2a, isIntersecting: false },
      { target: h2b, isIntersecting: false },
    ]);
    expect(linkA.classList.contains('active')).toBe(true);
    expect(linkB.classList.contains('active')).toBe(false);
  });

  it('幂等重入：连续两次 init 先 dispose 上一次', () => {
    const { h2a, h2b } = setupDom();
    mockTop(h2a, 500);
    mockTop(h2b, 900);

    initTocSidebar();
    initTocSidebar();

    expect(disconnect).toHaveBeenCalledTimes(1);
    // 第二次 init 重建 observer，只观察当前 DOM 的 2 个标题（第一次的 observe 记录已含 2 次）。
    expect(observe).toHaveBeenCalledTimes(4);
  });

  it('局部目录初始化和清理不接管页面目录', () => {
    setupDom();
    document.body.insertAdjacentHTML(
      'beforeend',
      `<div id="sample-scroll"><nav id="sample-toc" class="toc-sidebar"><div class="toc-sidebar-body"><a href="#sample-a">样例</a></div></nav><article id="sample-content"><h2 id="sample-a">样例</h2></article></div>`,
    );
    initTocSidebar();
    expect(observe).toHaveBeenCalledTimes(2);

    initTocSidebar('#sample-toc', '#sample-content', '#sample-scroll');
    expect(observe).toHaveBeenCalledTimes(3);
    expect(disconnect).not.toHaveBeenCalled();

    disposeTocSidebar('#sample-toc');
    expect(disconnect).toHaveBeenCalledTimes(1);
    initTocSidebar();
    expect(disconnect).toHaveBeenCalledTimes(2);
  });

  it('页面目录初始化不会选中排在前面的图鉴目录', () => {
    const { h2a, h2b } = setupDom();
    document.body.insertAdjacentHTML(
      'afterbegin',
      '<nav id="showcase-post-toc-nav-card" class="toc-sidebar"><div class="toc-sidebar-body"><a href="#sample-a">样例</a></div></nav><h2 id="sample-a">样例</h2>',
    );
    mockTop(h2a, 500);
    mockTop(h2b, 900);
    initTocSidebar();
    expect(observe.mock.calls.map(([element]) => element.id)).toEqual(['a', 'b']);
  });

  it('IntersectionObserver 不存在时不抛错，初始同步激活仍生效', () => {
    const { h2a, h2b, linkA } = setupDom();
    mockTop(h2a, -200);
    mockTop(h2b, 500);
    vi.stubGlobal('IntersectionObserver', undefined);

    expect(() => initTocSidebar()).not.toThrow();
    expect(linkA.classList.contains('active')).toBe(true);
  });
});
