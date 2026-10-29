import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from 'vitest';
import { RouteTransitions } from './route-transitions';
import { beginTransition, skipActiveAnimation } from './view-transition-lifecycle';

function deferred() {
  let resolve!: () => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<void>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

interface NativeTransition {
  invoke: () => Promise<void>;
  ready: ReturnType<typeof deferred>;
  done: ReturnType<typeof deferred>;
  finished: ReturnType<typeof deferred>;
  skip: ReturnType<typeof vi.fn>;
  updated: boolean;
}

const post = (id = '42', cover = false) => `
  <h1 data-vt-post-id="${id}" data-vt-role="title">Article ${id}</h1>
  ${cover ? `<div class="blur-img" data-vt-post-id="${id}" data-vt-role="cover"><img class="blur-img-full" src="/cover.png"></div>` : ''}`;

let routes: RouteTransitions;
let native: NativeTransition[];
let destination: string;
let autoRender: boolean;
let notify: Mock<() => void>;

function paint(
  html: string,
  route = routes.currentRoute(),
  id = routes.navigationId(),
): HTMLElement {
  document.body.innerHTML = `<div data-vt-route="${route}" data-vt-navigation="${id}">${html}</div>`;
  for (const element of document.querySelectorAll<HTMLElement>('[data-vt-post-id]')) {
    vi.spyOn(element, 'getBoundingClientRect').mockReturnValue(new DOMRect(20, 100, 400, 120));
  }
  for (const image of document.querySelectorAll<HTMLImageElement>('img')) {
    Object.defineProperties(image, {
      complete: { configurable: true, value: true },
      naturalWidth: { configurable: true, value: 800 },
      naturalHeight: { configurable: true, value: 400 },
    });
  }
  return document.querySelector<HTMLElement>('[data-vt-route]')!;
}

function render(html = destination): void {
  paint(html);
  routes.rendered(routes.navigationId(), routes.currentRoute());
}

/** 模拟站内链接点击：协调器在 document 冒泡阶段自己接管并发起 push。 */
function click(
  route: string,
  options: {
    post?: string;
    returning?: boolean;
    rel?: string;
    target?: string;
    prevented?: boolean;
  } = {},
): MouseEvent {
  const anchor = document.createElement('a');
  anchor.href = route;
  if (options.post) anchor.dataset.vtPostLink = options.post;
  if (options.returning) anchor.dataset.vtReturn = 'true';
  if (options.rel) anchor.rel = options.rel;
  if (options.target) anchor.target = options.target;
  if (options.prevented) anchor.addEventListener('click', (event) => event.preventDefault());
  document.body.append(anchor);
  const event = new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 });
  anchor.dispatchEvent(event);
  return event;
}

async function flush(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve();
}

async function finish(index = native.length - 1): Promise<void> {
  native[index].finished.resolve();
  await flush();
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'performance', 'Date'] });
  vi.stubGlobal('innerWidth', 1280);
  vi.stubGlobal('innerHeight', 800);
  vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: false } as MediaQueryList);
  vi.spyOn(window, 'scrollTo').mockImplementation(() => {});
  history.replaceState({ existing: 'keep' }, '', '/');
  document.documentElement.className = '';
  native = [];
  destination = '<main>Destination</main>';
  autoRender = true;
  Object.defineProperty(document, 'startViewTransition', {
    configurable: true,
    value: vi.fn((callback: () => void | Promise<void>) => {
      const item: NativeTransition = {
        ready: deferred(),
        done: deferred(),
        finished: deferred(),
        skip: vi.fn(),
        updated: false,
        invoke: async () => {
          await callback();
          item.updated = true;
          item.done.resolve();
          item.ready.resolve();
        },
      };
      native.push(item);
      return {
        ready: item.ready.promise,
        updateCallbackDone: item.done.promise,
        finished: item.finished.promise,
        skipTransition: item.skip,
        types: new Set<string>(),
      } as ViewTransition;
    }),
  });
  routes = new RouteTransitions();
  notify = vi.fn(() => {
    if (autoRender) render();
  });
  routes.connect(notify, null);
  render('<main data-vt-list>Source</main>');
});

afterEach(() => {
  routes.disconnect();
  beginTransition('route').finish();
  delete (document as unknown as { startViewTransition?: unknown }).startViewTransition;
  document.body.innerHTML = '';
  document.documentElement.className = '';
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('unsaved draft navigation guard', () => {
  it('blocks push and replace before URL, history, DOM or transition changes', () => {
    const guard = vi.fn(() => false);
    routes.setLeaveGuard(guard);
    const state = history.state;
    click('/admin/notes');
    routes.replace('/about');
    expect(guard).toHaveBeenCalledTimes(2);
    expect(routes.currentRoute()).toBe('/');
    expect(location.pathname).toBe('/');
    expect(history.state).toEqual(state);
    expect(native).toHaveLength(0);
    expect(notify).not.toHaveBeenCalled();
  });

  it('allows fragment changes without asking and respects registration ownership', async () => {
    const stale = routes.setLeaveGuard(() => true);
    const guard = vi.fn(() => false);
    const current = routes.setLeaveGuard(guard);
    routes.clearLeaveGuard(stale);
    routes.push('/about');
    expect(native).toHaveLength(0);
    routes.push('/#section');
    // Hash-only navigation commits without a native transition.
    expect(routes.currentRoute()).toBe('/#section');
    expect(guard).toHaveBeenCalledTimes(1);
    routes.clearLeaveGuard(current);
    routes.push('/about');
    await native[0].invoke();
    await finish();
    expect(routes.currentRoute()).toBe('/about');
  });

  it('undoes cancelled back navigation without losing the forward entry or prompting twice', async () => {
    const first = history.state;
    routes.push('/admin/notes');
    await native[0].invoke();
    await finish();
    const second = history.state;
    const guard = vi.fn(() => false);
    routes.setLeaveGuard(guard);
    const go = vi.spyOn(history, 'go').mockImplementation(() => {});
    const push = vi.spyOn(history, 'pushState');
    const pop = (state: unknown, url: string) => {
      history.replaceState(state, '', url);
      window.dispatchEvent(new PopStateEvent('popstate', { state }));
    };
    pop(first, '/');
    expect(go).toHaveBeenCalledWith(1);
    expect(routes.currentRoute()).toBe('/admin/notes');
    routes.push('/about'); // No extra navigation while rollback is in flight.
    expect(native).toHaveLength(1);
    pop(second, '/admin/notes');
    expect(guard).toHaveBeenCalledTimes(1);
    expect(push).not.toHaveBeenCalled();
    guard.mockReturnValue(true);
    pop(first, '/');
    await native[1].invoke();
    await finish();
    expect(routes.currentRoute()).toBe('/');
    pop(second, '/admin/notes');
    await native[2].invoke();
    await finish();
    expect(routes.currentRoute()).toBe('/admin/notes');
  });

  it('retains the draft route for unmarked history and fails closed if the guard throws', () => {
    routes.setLeaveGuard(() => {
      throw new Error('unavailable');
    });
    history.replaceState(null, '', '/about');
    window.dispatchEvent(new PopStateEvent('popstate'));
    expect(location.pathname).toBe('/');
    expect(routes.currentRoute()).toBe('/');
    expect(native).toHaveLength(0);
  });

  it('cancels an uncommitted transition before undoing rejected history navigation', async () => {
    const first = history.state;
    routes.push('/admin/notes');
    await native[0].invoke();
    await finish();
    const second = history.state;
    const guard = vi.fn(() => true);
    routes.setLeaveGuard(guard);
    routes.push('/about');
    guard.mockReturnValue(false);
    vi.spyOn(history, 'go').mockImplementation(() => {});
    history.replaceState(first, '', '/');
    window.dispatchEvent(new PopStateEvent('popstate'));
    await native[1].invoke();
    expect(routes.currentRoute()).toBe('/admin/notes');
    history.replaceState(second, '', '/admin/notes');
    window.dispatchEvent(new PopStateEvent('popstate'));
    await finish();
    expect(routes.currentRoute()).toBe('/admin/notes');
    expect(guard).toHaveBeenCalledTimes(2);
  });
});

describe('route publication and rendering', () => {
  it('preserves the old URL, route and DOM until the native update callback', async () => {
    routes.push('/about');
    expect(routes.currentRoute()).toBe('/');
    expect(location.pathname).toBe('/');
    expect(document.body.textContent).toContain('Source');
    expect(notify).not.toHaveBeenCalled();
    await native[0].invoke();
    expect(routes.currentRoute()).toBe('/about');
    expect(location.pathname).toBe('/about');
    expect(history.state.existing).toBe('keep');
    expect(notify).toHaveBeenCalledOnce();
    expect(document.querySelector('[data-vt-enter-handled]')).not.toBeNull();
  });

  it('waits for the matching DOM commit even after the route has been published', async () => {
    autoRender = false;
    routes.push('/about');
    const update = native[0].invoke();
    routes.rendered(routes.navigationId() - 1, '/about');
    await flush();
    expect(native[0].updated).toBe(false);
    // A matching notification alone must not accept a stale layout wrapper.
    routes.rendered(routes.navigationId(), '/about');
    await flush();
    expect(native[0].updated).toBe(false);
    render();
    await update;
    expect(native[0].updated).toBe(true);
    expect(native[0].skip).not.toHaveBeenCalled();
    expect(document.documentElement.classList.contains('is-route-pending')).toBe(false);
  });

  it('counts the 300ms budget from navigation start, including snapshot scheduling', async () => {
    autoRender = false;
    routes.push('/about');
    await vi.advanceTimersByTimeAsync(200);
    const update = native[0].invoke();
    await vi.advanceTimersByTimeAsync(99);
    expect(native[0].updated).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await update;
    expect(native[0].updated).toBe(true);
    expect(performance.now()).toBe(300);
  });

  describe('destination chunk arrives after the budget', () => {
    const pending = () => document.documentElement.classList.contains('is-route-pending');
    const suspend = async (route = '/about'): Promise<void> => {
      autoRender = false;
      routes.push(route);
      const update = native[native.length - 1].invoke();
      await vi.advanceTimersByTimeAsync(300);
      await update;
    };

    it('skips the empty transition and fades the old page until the late render', async () => {
      await suspend();
      expect(native[0].skip).toHaveBeenCalledOnce();
      expect(native[0].updated).toBe(true);
      expect(pending()).toBe(true);
      await finish(0);
      expect(document.documentElement.classList.contains('is-route-transitioning')).toBe(false);
      expect(pending()).toBe(true);
      render();
      expect(pending()).toBe(false);
      expect(document.querySelector('[data-vt-enter-handled]')).toBeNull();
      expect(document.startViewTransition).toHaveBeenCalledOnce();
    });

    it('keeps fading across a newer navigation and clears before its snapshot', async () => {
      await suspend();
      await finish(0);
      autoRender = false;
      routes.push('/');
      const update = native[1].invoke();
      await flush();
      expect(pending()).toBe(true);
      render();
      expect(pending()).toBe(false);
      await update;
      expect(native[1].skip).not.toHaveBeenCalled();
    });

    it('is cleared by pagehide', async () => {
      await suspend();
      window.dispatchEvent(new Event('pagehide'));
      expect(pending()).toBe(false);
    });

    it('is cleared by disconnect', async () => {
      await suspend();
      routes.disconnect();
      expect(pending()).toBe(false);
    });
  });

  it('a late cancelled callback cannot commit or clean a newer navigation', async () => {
    routes.push('/about');
    routes.push('/archives');
    expect(native[0].skip).toHaveBeenCalled();
    await native[1].invoke();
    await native[0].invoke();
    await finish(0);
    expect(routes.currentRoute()).toBe('/archives');
    expect(location.pathname).toBe('/archives');
    expect(notify).toHaveBeenCalledOnce();
    expect(document.documentElement.classList.contains('is-route-transitioning')).toBe(true);
    expect(document.documentElement.dataset.vtSection).toBe('forward');
    await finish(1);
    expect(document.documentElement.classList.contains('is-route-transitioning')).toBe(false);
    expect(document.documentElement.dataset.vtSection).toBeUndefined();
  });

  it('a theme interruption preserves the pending route and its callback commits only once', async () => {
    routes.push('/about');
    const theme = beginTransition('theme');
    expect(document.documentElement.dataset.vtSection).toBeUndefined();
    expect(routes.currentRoute()).toBe('/about');
    expect(notify).toHaveBeenCalledOnce();
    await native[0].invoke();
    await finish();
    expect(notify).toHaveBeenCalledOnce();
    expect(theme.isCurrent()).toBe(true);
  });

  it('browser pop keeps the displayed page until capture and restores entry state', async () => {
    const first = history.state;
    routes.writeState(routes.entryId(), 'search', '{"query":"rust"}');
    routes.push('/about');
    await native[0].invoke();
    await finish();
    history.replaceState(first, '', '/');
    window.dispatchEvent(new PopStateEvent('popstate', { state: first }));
    expect(location.pathname).toBe('/');
    expect(routes.currentRoute()).toBe('/about');
    await native[1].invoke();
    expect(routes.currentRoute()).toBe('/');
    expect(routes.readState('search')).toBe('{"query":"rust"}');
    expect(notify).toHaveBeenCalledTimes(2);
  });

  it.each(['unsupported', 'reduced motion', 'same-page hash'])(
    '%s completes without a native transition',
    (mode) => {
      if (mode === 'unsupported')
        delete (document as unknown as { startViewTransition?: unknown }).startViewTransition;
      if (mode === 'reduced motion')
        vi.mocked(window.matchMedia).mockReturnValue({ matches: true } as MediaQueryList);
      const target = mode === 'same-page hash' ? '/#section' : '/about';
      routes.push(target);
      expect(routes.currentRoute()).toBe(target);
      expect(notify).toHaveBeenCalledOnce();
      expect(native).toHaveLength(0);
      expect(document.documentElement.dataset.vtSection).toBeUndefined();
    },
  );

  it('repeated URLs neither add history nor replay an animation', async () => {
    routes.push('/');
    expect(native).toHaveLength(0);
    routes.push('/about');
    await native[0].invoke();
    await finish();
    const id = routes.entryId();
    routes.push('/about');
    expect(routes.entryId()).toBe(id);
    expect(native).toHaveLength(1);
    expect(notify).toHaveBeenCalledOnce();
  });

  it('a same-page fragment preserves state owned by the still-mounted component', () => {
    const mountedEntry = routes.entryId();
    routes.writeState(mountedEntry, 'search', '{"query":"rust"}');
    routes.push('/#section');
    expect(native).toHaveLength(0);
    expect(routes.readState('search')).toBe('{"query":"rust"}');
    // Rust captures this ID at mount; fragment navigation does not remount its page.
    routes.writeState(mountedEntry, 'search', '{"query":"updated"}');
    expect(routes.readState('search')).toBe('{"query":"updated"}');
  });

  it('fresh navigation resets scroll once despite repeated destination notifications', async () => {
    routes.push('/about');
    await native[0].invoke();
    routes.rendered(routes.navigationId(), routes.currentRoute());
    routes.rendered(routes.navigationId(), routes.currentRoute());
    document.querySelector('main')!.append(document.createElement('p'));
    await vi.advanceTimersByTimeAsync(10);
    expect(window.scrollTo).toHaveBeenCalledOnce();
  });
});

describe('section navigation', () => {
  it.each([
    ['/', -535, 65, '0px', '-600px', '0px'],
    ['/', 65, -535, '0px', '600px', '600px'],
    // 滚动后的文章页回到栏目：页头不单独捕获；旧正文比新 main 宽，按原位置留在页头下方。
    ['/post/article', -2335, 65, '-180px', '-2400px', '0px'],
  ])(
    'keeps a scrolled source aligned and the live header clear (%s: %s → %s)',
    async (source, from, to, x, y, clip) => {
      if (source !== '/') {
        routes.push(source);
        await native[0].invoke();
        await finish();
      }
      const wide = source.startsWith('/post/');
      const content = '<header data-vt-shell="frontend-header"></header><main>Section</main>';
      render(content);
      const header = document.querySelector<HTMLElement>('header')!;
      vi.spyOn(document.querySelector('main')!, 'getBoundingClientRect').mockReturnValue(
        wide
          ? new DOMRect(100, Number(from), 1240, 9000)
          : new DOMRect(280, Number(from), 880, 2000),
      );
      autoRender = false;
      routes.push('/archives');
      expect(header.style.viewTransitionName).toBe('');
      const update = native[native.length - 1].invoke();
      paint(content);
      vi.spyOn(document.querySelector('main')!, 'getBoundingClientRect').mockReturnValue(
        new DOMRect(280, Number(to), 880, 2000),
      );
      vi.spyOn(document.querySelector('header')!, 'getBoundingClientRect').mockReturnValue(
        new DOMRect(0, 0, 1280, 65),
      );
      routes.rendered(routes.navigationId(), routes.currentRoute());
      await update;
      const style = document.documentElement.style;
      expect(style.getPropertyValue('--vt-section-old-x')).toBe(x);
      expect(style.getPropertyValue('--vt-section-old-y')).toBe(y);
      expect(style.getPropertyValue('--vt-section-clip-top')).toBe(clip);
      await finish();
      expect(style.getPropertyValue('--vt-section-old-x')).toBe('');
      expect(style.getPropertyValue('--vt-section-old-y')).toBe('');
      expect(style.getPropertyValue('--vt-section-clip-top')).toBe('');
    },
  );

  it.each([
    ['/', '/archives', 'forward'],
    ['/', '/notes', 'forward'],
    ['/notes', '/archives', 'forward'],
    ['/notes/example', '/', 'backward'],
    ['/notes', '/notes/example', 'fade'],
    ['/notes/book/1', '/notes/example', 'fade'],
    ['/archives', '/friends', 'forward'],
    ['/friends', '/about', 'forward'],
    ['/about', '/search', 'forward'],
    ['/search', '/', 'backward'],
    ['/about', '/friends', 'backward'],
    ['/page/2', '/archives', 'forward'],
    ['/tags/Rust', '/', 'backward'],
    ['/archives', '/tags/Rust', 'fade'],
    ['/', '/page/2', 'fade'],
    ['/', '/post/article', 'fade'],
    ['/post/article', '/archives', 'fade'],
    ['/post/article', '/post/other', 'fade'],
    ['/about', '/changelog', 'fade'],
    ['/about', '/admin', undefined],
    ['/about', '/login', undefined],
  ])('%s → %s captures only the content, sliding between sections', async (from, to, direction) => {
    if (from !== '/') {
      routes.push(from!);
      await native[native.length - 1].invoke();
      await finish();
    }
    routes.push(to!);
    expect(document.documentElement.dataset.vtSection).toBe(direction);
    await native[native.length - 1].invoke();
    expect(document.documentElement.dataset.vtSection).toBe(direction);
    await finish();
    expect(document.documentElement.dataset.vtSection).toBeUndefined();
  });

  it('history navigation uses the displayed section, even when the URL has already changed', async () => {
    const home = history.state;
    routes.push('/search');
    await native[0].invoke();
    await finish();
    history.replaceState(home, '', '/');
    window.dispatchEvent(new PopStateEvent('popstate', { state: home }));
    expect(document.documentElement.dataset.vtSection).toBe('backward');
    await native[1].invoke();
    await finish();
    expect(document.documentElement.dataset.vtSection).toBeUndefined();
  });

  it('an overlay opened mid-animation skips it without dropping the navigation', async () => {
    routes.push('/archives');
    skipActiveAnimation();
    expect(native[0].skip).toHaveBeenCalledOnce();
    // A skipped native transition still runs its update callback.
    await native[0].invoke();
    expect(routes.currentRoute()).toBe('/archives');
    expect(location.pathname).toBe('/archives');
    await finish();
    expect(document.documentElement.classList.contains('is-route-transitioning')).toBe(false);
    expect(document.documentElement.dataset.vtSection).toBeUndefined();
    skipActiveAnimation();
    expect(native[0].skip).toHaveBeenCalledOnce();
  });

  describe('nav pill', () => {
    const header = (current?: string) => {
      const link = (href: string, label: string) =>
        `<a class="nav-item" href="${href}">${label}${
          href === current ? '<span class="nav-indicator"></span>' : ''
        }</a>`;
      return `<header data-vt-shell="frontend-header"><nav class="site-nav">${link('/', '首页')}${link('/about', '关于')}</nav></header><main>Section</main>`;
    };
    const paintHeader = (current?: string, route = routes.currentRoute()): void => {
      paint(header(current), route);
      const pill = document.querySelector('.nav-indicator');
      if (pill)
        vi.spyOn(pill, 'getBoundingClientRect').mockReturnValue(new DOMRect(600, 12, 64, 30));
    };
    const withAnimate = async (
      body: (animate: Mock, done: ReturnType<typeof deferred>) => Promise<void>,
    ) => {
      const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'animate');
      const done = deferred();
      const animate = vi.fn(() => ({ finished: done.promise }) as unknown as Animation);
      Object.defineProperty(HTMLElement.prototype, 'animate', {
        configurable: true,
        writable: true,
        value: animate,
      });
      try {
        await body(animate, done);
      } finally {
        if (original) Object.defineProperty(HTMLElement.prototype, 'animate', original);
        else delete (HTMLElement.prototype as unknown as { animate?: unknown }).animate;
      }
    };
    // Settle on /about with the pill on its link.
    const settleOnAbout = async (): Promise<void> => {
      routes.push('/about');
      await native[native.length - 1].invoke();
      await finish();
      paintHeader('/about');
    };
    const navigateTo = async (route: string, html: string): Promise<void> => {
      autoRender = false;
      routes.push(route);
      const update = native[native.length - 1].invoke();
      paint(html);
      const pill = document.querySelector('.nav-indicator');
      if (pill)
        vi.spyOn(pill, 'getBoundingClientRect').mockReturnValue(new DOMRect(600, 12, 64, 30));
      routes.rendered(routes.navigationId(), routes.currentRoute());
      await update;
      await flush();
    };

    it('fades out in place of the old link when entering /search', async () => {
      await withAnimate(async (animate, done) => {
        await settleOnAbout();
        await navigateTo('/search', header());
        expect(animate).toHaveBeenCalledOnce();
        expect(animate.mock.calls[0][0]).toEqual([{ opacity: 1 }, { opacity: 0 }]);
        expect(
          document.querySelector('.site-nav a[href="/about"] .nav-indicator[aria-hidden="true"]'),
        ).not.toBeNull();
        done.resolve();
        await flush();
        expect(document.querySelector('.nav-indicator')).toBeNull();
      });
    });

    it.each(['/search', '/post/article'])(
      'fades in on the new link when leaving %s',
      async (source) => {
        await withAnimate(async (animate) => {
          routes.push(source);
          await native[native.length - 1].invoke();
          await finish();
          await navigateTo('/', header('/'));
          expect(animate).toHaveBeenCalledOnce();
          expect(animate.mock.calls[0][0]).toEqual([{ opacity: 0 }, { opacity: 1 }]);
          expect(animate.mock.contexts[0]).toBe(document.querySelector('.nav-indicator'));
        });
      },
    );

    it('leaves the pill alone for navigation outside the sections', async () => {
      await withAnimate(async (animate) => {
        await settleOnAbout();
        await navigateTo('/changelog', header('/about'));
        expect(animate).not.toHaveBeenCalled();
      });
    });

    it('slides from the old section when the destination renders late', async () => {
      await withAnimate(async (animate) => {
        await settleOnAbout();
        autoRender = false;
        routes.push('/');
        const update = native[native.length - 1].invoke();
        await vi.advanceTimersByTimeAsync(300);
        await update;
        expect(animate).not.toHaveBeenCalled();
        paint(header('/'));
        const pill = document.querySelector('.nav-indicator')!;
        vi.spyOn(pill, 'getBoundingClientRect').mockReturnValue(new DOMRect(500, 12, 64, 30));
        routes.rendered(routes.navigationId(), routes.currentRoute());
        expect(animate).toHaveBeenCalledOnce();
        expect(animate.mock.calls[0][0]).toEqual([
          { left: '100px', right: '-100px' },
          { left: '0px', right: '0px' },
        ]);
      });
    });
  });
});

describe('navigation while a transition is in flight', () => {
  const pending = () => document.documentElement.classList.contains('is-route-pending');
  const playArchives = async (): Promise<void> => {
    routes.push('/archives');
    await native[0].invoke();
    await flush();
    autoRender = false;
  };

  it('relays into the playing animation instead of skipping and recapturing it', async () => {
    await playArchives();
    routes.push('/friends');
    expect(native).toHaveLength(1);
    expect(native[0].skip).not.toHaveBeenCalled();
    expect(routes.currentRoute()).toBe('/friends');
    expect(location.pathname).toBe('/friends');
    // The page still on screen must not replay its entrance (blank flash) before the swap.
    expect(
      document.querySelector('[data-vt-route="/archives"]')!.hasAttribute('data-vt-enter-handled'),
    ).toBe(true);
    render();
    // The relayed animation keeps its section styling until it ends.
    expect(document.documentElement.dataset.vtSection).toBe('forward');
    await finish(0);
    expect(document.documentElement.classList.contains('is-route-transitioning')).toBe(false);
    expect(document.documentElement.dataset.vtSection).toBeUndefined();
    expect(notify).toHaveBeenCalledTimes(2);
  });

  it('holds the clicked link pressed only while the snapshot hides its hover', async () => {
    const press = (href: string) => {
      click(href);
      return document.querySelector<HTMLAnchorElement>(`a[href="${href}"]`)!;
    };
    const pressed = (link: HTMLAnchorElement) => link.hasAttribute('data-vt-pressed');
    const about = press('/about');
    expect(pressed(about)).toBe(true);
    // A newer click before the snapshot supersedes the first one.
    const archives = press('/archives');
    expect(pressed(about)).toBe(false);
    expect(pressed(archives)).toBe(true);
    await native[1].invoke();
    await flush();
    expect(pressed(archives)).toBe(true);
    // Relaying reuses the playing animation: no new snapshot, so nothing to hold.
    autoRender = false;
    const friends = press('/friends');
    expect(pressed(archives)).toBe(false);
    expect(pressed(friends)).toBe(false);
    render();
    await finish(1);
    expect(document.querySelector('[data-vt-pressed]')).toBeNull();
    expect(routes.currentRoute()).toBe('/friends');
  });

  it('fades the old page when the relayed destination misses the budget', async () => {
    await playArchives();
    routes.push('/friends');
    await vi.advanceTimersByTimeAsync(299);
    expect(pending()).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(pending()).toBe(true);
    expect(native[0].skip).toHaveBeenCalledOnce();
    // The late destination enters normally after the fade.
    expect(document.querySelector('[data-vt-enter-handled]')).toBeNull();
    render();
    expect(pending()).toBe(false);
  });

  it('holds navigation requested while the browser waits for the previous page, then relays it', async () => {
    autoRender = false;
    routes.push('/archives');
    const update = native[0].invoke();
    await flush();
    routes.push('/about');
    routes.push('/friends');
    expect(native[0].skip).not.toHaveBeenCalled();
    expect(routes.currentRoute()).toBe('/archives');
    render();
    await update;
    await flush();
    expect(native).toHaveLength(1);
    expect(routes.currentRoute()).toBe('/friends');
    expect(notify).toHaveBeenCalledTimes(2);
  });

  it('drops a held request when a newer history navigation starts', async () => {
    const home = history.state;
    autoRender = false;
    routes.push('/archives');
    const update = native[0].invoke();
    await flush();
    routes.push('/friends');
    history.replaceState(home, '', '/');
    window.dispatchEvent(new PopStateEvent('popstate', { state: home }));
    render();
    await update;
    const back = native[1].invoke();
    await flush();
    render();
    await back;
    await flush();
    expect(routes.currentRoute()).toBe('/');
  });

  it('recovers shell link clicks that the transition hit-tests to <html>', async () => {
    autoRender = false;
    routes.push('/archives');
    const update = native[0].invoke();
    await flush();
    document.body.insertAdjacentHTML(
      'afterbegin',
      '<header data-vt-shell="frontend-header"><a href="/friends">友链</a></header>',
    );
    vi.spyOn(document.querySelector('header a')!, 'getBoundingClientRect').mockReturnValue(
      new DOMRect(600, 12, 64, 30),
    );
    // dispatchEvent is false once the coordinator takes the click over.
    const press = (clientX: number, clientY: number) =>
      document.documentElement.dispatchEvent(
        new MouseEvent('click', { bubbles: true, cancelable: true, button: 0, clientX, clientY }),
      );
    expect(press(10, 400)).toBe(true);
    expect(press(620, 20)).toBe(false);
    render();
    await update;
    await flush();
    expect(routes.currentRoute()).toBe('/friends');
  });
});

describe('shared article snapshots', () => {
  it('intercepts a bubbling in-site link click, prevents the default and pushes', async () => {
    render(`<main data-vt-list>${post()}</main>`);
    destination = `<main data-vt-detail>${post()}</main>`;
    const event = click('/post/article', { post: '42' });
    expect(event.defaultPrevented).toBe(true);
    await flush();
    expect(native).toHaveLength(1);
    expect(
      document.querySelector<HTMLElement>('[data-vt-role="title"]')!.style.viewTransitionName,
    ).toBe('vt-post-title');
    await native[0].invoke();
    expect(
      document.querySelector<HTMLElement>('[data-vt-role="title"]')!.style.viewTransitionName,
    ).toBe('vt-post-title');
  });

  it('lets external, native-path, targeted and already-handled clicks through', async () => {
    // window 上最后一个监听器：记录协调器是否已接管，并取消 happy-dom 的真实跳转。
    const seen: boolean[] = [];
    const record = (event: MouseEvent) => {
      seen.push(event.defaultPrevented);
      event.preventDefault();
    };
    window.addEventListener('click', record);
    const cases = [
      { route: '/about', rel: 'external' },
      { route: '/feed.xml' },
      { route: '/api/anything' },
      { route: '/uploads/a.png' },
      { route: '/about', target: '_blank' },
      { route: '/about', prevented: true },
    ];
    for (const options of cases) click(options.route, options);
    window.removeEventListener('click', record);
    // 只有已被组件 preventDefault 的点击在到达 window 时仍是 defaultPrevented；其余未被协调器接管。
    expect(seen).toEqual(cases.map((options) => options.prevented === true));
    await flush();
    expect(native).toHaveLength(0);
  });

  it('hands in-site link clicks to native navigation, away from window listeners, until connected', () => {
    // Leptos 的链接拦截挂在 window 上且在水合时就生效；未连接时它若接管，路由器会在水合中途换页。
    routes.disconnect();
    const pending = new RouteTransitions();
    // 注册晚于协调器的 document 监听：记录协调器是否已接管，并取消 happy-dom 的真实跳转。
    const prevented: boolean[] = [];
    const cancel = (event: MouseEvent) => {
      prevented.push(event.defaultPrevented);
      event.preventDefault();
    };
    const reachedWindow = vi.fn();
    document.addEventListener('click', cancel);
    window.addEventListener('click', reachedWindow);
    click('/archives');
    expect(prevented).toEqual([false]);
    expect(reachedWindow).not.toHaveBeenCalled();
    expect(notify).not.toHaveBeenCalled();
    // 连接后照常接管。
    pending.connect(notify, null);
    click('/archives');
    document.removeEventListener('click', cancel);
    window.removeEventListener('click', reachedWindow);
    expect(prevented).toEqual([false, true]);
    pending.disconnect();
  });

  it('listens for popstate in the capture phase and stops it from reaching other listeners', () => {
    const bubbling = vi.fn();
    window.addEventListener('popstate', bubbling);
    const event = new PopStateEvent('popstate', { cancelable: true });
    const stop = vi.spyOn(event, 'stopImmediatePropagation');
    window.dispatchEvent(event);
    window.removeEventListener('popstate', bubbling);
    expect(stop).toHaveBeenCalled();
    expect(bubbling).not.toHaveBeenCalled();
  });

  it('intercepts popstate ahead of listeners that register after construction, and yields when disconnected', () => {
    // Leptos 的 popstate 监听在脚本加载之后才注册；window 上按注册顺序触发，协调器必须更早。
    const late = vi.fn();
    window.addEventListener('popstate', late);
    window.dispatchEvent(new PopStateEvent('popstate'));
    expect(late).not.toHaveBeenCalled();
    routes.disconnect();
    window.dispatchEvent(new PopStateEvent('popstate'));
    expect(late).toHaveBeenCalledTimes(1);
    window.removeEventListener('popstate', late);
  });

  it('matches title and loaded cover, then restores prior inline names', async () => {
    render(`<main data-vt-list>${post('42', true)}</main>`);
    const source = document.querySelector<HTMLElement>('[data-vt-role="title"]')!;
    source.style.viewTransitionName = 'previous-title';
    destination = `<main data-vt-detail>${post('42', true)}</main>`;
    click('/post/article', { post: '42' });
    expect(source.style.viewTransitionName).toBe('vt-post-title');
    // 标题需要在整页快照里飞行，不能只捕获正文。
    expect(document.documentElement.dataset.vtSection).toBeUndefined();
    await native[0].invoke();
    expect(
      document.querySelector<HTMLElement>('[data-vt-role="title"]')!.style.viewTransitionName,
    ).toBe('vt-post-title');
    expect(
      document.querySelector<HTMLElement>('[data-vt-role="cover"]')!.style.viewTransitionName,
    ).toBe('vt-post-cover');
    expect(document.documentElement.classList.contains('vt-shared-title')).toBe(true);
    expect(document.documentElement.classList.contains('vt-shared-cover')).toBe(true);
    await finish();
    expect(document.documentElement.classList.contains('vt-shared-title')).toBe(false);
    expect(document.documentElement.classList.contains('vt-shared-cover')).toBe(false);
    expect(source.style.viewTransitionName).toBe('previous-title');
    expect(
      document.querySelector<HTMLElement>('[data-vt-role="title"]')!.style.viewTransitionName,
    ).toBe('');
  });

  it('duplicate visible article titles fall back without duplicate transition names', async () => {
    render(`<main data-vt-list>${post()}${post()}</main>`);
    destination = `<main data-vt-detail>${post()}</main>`;
    click('/post/article', { post: '42' });
    expect(
      [...document.querySelectorAll<HTMLElement>('[data-vt-role]')].every(
        (node) => !node.style.viewTransitionName,
      ),
    ).toBe(true);
    await native[0].invoke();
    expect(
      document.querySelector<HTMLElement>('[data-vt-role="title"]')!.style.viewTransitionName,
    ).toBe('');
  });

  it('offscreen source elements fall back to the page transition', async () => {
    render(`<main data-vt-list>${post()}</main>`);
    const title = document.querySelector<HTMLElement>('[data-vt-role="title"]')!;
    vi.mocked(title.getBoundingClientRect).mockReturnValue(new DOMRect(20, 2000, 400, 120));
    destination = `<main data-vt-detail>${post()}</main>`;
    click('/post/article', { post: '42' });
    expect(title.style.viewTransitionName).toBe('');
    await native[0].invoke();
    expect(
      document.querySelector<HTMLElement>('[data-vt-role="title"]')!.style.viewTransitionName,
    ).toBe('');
  });

  it('ambiguous destination titles never receive duplicate transition names', async () => {
    render(`<main data-vt-list>${post()}</main>`);
    destination = `<main data-vt-detail>${post()}${post()}</main>`;
    click('/post/article', { post: '42' });
    await native[0].invoke();
    expect(
      [...document.querySelectorAll<HTMLElement>('[data-vt-role="title"]')].every(
        (node) => !node.style.viewTransitionName,
      ),
    ).toBe(true);
    expect(document.documentElement.classList.contains('vt-shared-title')).toBe(false);
  });

  it('image completion does not bypass an outstanding decode before the snapshot', async () => {
    render(`<main data-vt-list>${post('42', true)}</main>`);
    autoRender = false;
    click('/post/article', { post: '42' });
    const update = native[0].invoke();
    paint(`<main data-vt-detail>${post('42', true)}</main>`);
    const image = document.querySelector<HTMLImageElement>('img')!;
    const decoded = deferred();
    Object.defineProperties(image, {
      complete: { configurable: true, value: false },
      naturalWidth: { configurable: true, value: 0 },
      decode: { configurable: true, value: vi.fn(() => decoded.promise) },
    });
    routes.rendered(routes.navigationId(), routes.currentRoute());
    Object.defineProperties(image, {
      complete: { configurable: true, value: true },
      naturalWidth: { configurable: true, value: 800 },
    });
    // Another content commit can arrive between resource completion and decoding.
    routes.rendered(routes.navigationId(), routes.currentRoute());
    await flush();
    expect(native[0].updated).toBe(false);
    decoded.resolve();
    await update;
    expect(
      document.querySelector<HTMLElement>('[data-vt-role="cover"]')!.style.viewTransitionName,
    ).toBe('vt-post-cover');
  });

  it.each(['pending', 'failed'])(
    '%s destination covers preserve the title transition within 300ms',
    async (mode) => {
      render(`<main data-vt-list>${post('42', true)}</main>`);
      autoRender = false;
      click('/post/article', { post: '42' });
      const update = native[0].invoke();
      paint(`<main data-vt-detail>${post('42', true)}</main>`);
      const image = document.querySelector<HTMLImageElement>('img')!;
      const decoded = deferred();
      image.removeAttribute('src');
      image.dataset.src = '/cover.png';
      Object.defineProperties(image, {
        complete: { configurable: true, value: false },
        naturalWidth: { configurable: true, value: 0 },
        decode: { configurable: true, value: vi.fn(() => decoded.promise) },
      });
      routes.rendered(routes.navigationId(), routes.currentRoute());
      expect(image.getAttribute('src')).toBe('/cover.png');
      if (mode === 'failed') decoded.reject(new Error('image unavailable'));
      await vi.advanceTimersByTimeAsync(299);
      expect(native[0].updated).toBe(mode === 'failed');
      await vi.advanceTimersByTimeAsync(1);
      await update;
      expect(
        document.querySelector<HTMLElement>('[data-vt-role="title"]')!.style.viewTransitionName,
      ).toBe('vt-post-title');
      expect(
        document.querySelector<HTMLElement>('[data-vt-role="cover"]')!.style.viewTransitionName,
      ).toBe('');
      await finish();
      if (mode === 'pending') decoded.resolve();
      image.dispatchEvent(new Event('load'));
      await flush();
      expect(native).toHaveLength(1);
      expect(
        document.querySelector<HTMLElement>('[data-vt-role="title"]')!.style.viewTransitionName,
      ).toBe('');
    },
  );

  it('explicit return restores source state and plays the reverse title match', async () => {
    render(`<main data-vt-list>${post()}</main>`);
    const sourceEntry = routes.entryId();
    routes.writeState(sourceEntry, 'search', '{"query":"rust","results":[42]}');
    destination = `<main data-vt-detail>${post()}</main>`;
    click('/post/article', { post: '42' });
    await native[0].invoke();
    await finish();
    // A late source effect writes to the captured source, never the active detail.
    routes.writeState(sourceEntry, 'search', '{"query":"rust","results":[42,43]}');
    expect(routes.readState('search')).toBeNull();
    destination = `<main data-vt-list>${post()}</main>`;
    click('/', { returning: true });
    await native[1].invoke();
    expect(routes.readState('search')).toBe('{"query":"rust","results":[42,43]}');
    expect(
      document.querySelector<HTMLElement>('[data-vt-role="title"]')!.style.viewTransitionName,
    ).toBe('vt-post-title');
  });

  it('normal navigation menus start a fresh entry instead of restoring source filters', async () => {
    render(`<main data-vt-list>${post()}</main>`);
    routes.writeState(routes.entryId(), 'search', '{"query":"rust"}');
    destination = `<main data-vt-detail>${post()}</main>`;
    click('/post/article', { post: '42' });
    await native[0].invoke();
    await finish();
    destination = '<main data-vt-list>Fresh</main>';
    click('/');
    await native[1].invoke();
    expect(routes.readState('search')).toBeNull();
  });

  it('detail fragments retain the article source for explicit return and reverse matching', async () => {
    render(`<main data-vt-list>${post()}</main>`);
    routes.writeState(routes.entryId(), 'search', '{"query":"rust"}');
    destination = `<main data-vt-detail>${post()}</main>`;
    click('/post/article', { post: '42' });
    await native[0].invoke();
    await finish();
    routes.push('/post/article#section');
    expect(native).toHaveLength(1);
    destination = `<main data-vt-list>${post()}</main>`;
    click('/', { returning: true });
    await native[1].invoke();
    expect(routes.readState('search')).toBe('{"query":"rust"}');
    expect(
      document.querySelector<HTMLElement>('[data-vt-role="title"]')!.style.viewTransitionName,
    ).toBe('vt-post-title');
  });
});

describe('entry scroll and private state', () => {
  it('fragment history entries share component state while preserving their own scroll positions', async () => {
    const first = history.state;
    routes.writeState(routes.entryId(), 'search', '{"query":"rust"}');
    vi.stubGlobal('scrollY', 120);
    routes.push('/#section');
    const fragment = history.state;
    expect(fragment.__yggdrasilNavigation).not.toBe(first.__yggdrasilNavigation);
    vi.stubGlobal('scrollY', 450);
    routes.push('/about');
    await native[0].invoke();
    await finish();
    history.replaceState(first, '', '/');
    window.dispatchEvent(new PopStateEvent('popstate', { state: first }));
    await native[1].invoke();
    await finish();
    expect(window.scrollTo).toHaveBeenLastCalledWith({ left: 0, top: 120, behavior: 'instant' });
    expect(routes.readState('search')).toBe('{"query":"rust"}');
    vi.stubGlobal('scrollY', 120);
    history.replaceState(fragment, '', '/#section');
    window.dispatchEvent(new PopStateEvent('popstate', { state: fragment }));
    expect(native).toHaveLength(2);
    expect(window.scrollTo).toHaveBeenLastCalledWith({ left: 0, top: 450, behavior: 'instant' });
    expect(routes.readState('search')).toBe('{"query":"rust"}');
  });

  it('restores the admin scroll container on return and stops correcting after user input', async () => {
    const scroll = vi.spyOn(HTMLElement.prototype, 'scrollTo');
    routes.disconnect();
    history.replaceState({}, '', '/admin/posts');
    routes.connect(notify, null);
    const list = `<main data-vt-list data-vt-scroll="admin-main">${post()}</main>`;
    render(list);
    document.querySelector<HTMLElement>('[data-vt-scroll]')!.scrollTop = 650;
    destination = `<main data-vt-detail data-vt-scroll="admin-main">${post()}</main>`;
    click('/admin/preview/article', { post: '42' });
    await native[0].invoke();
    await finish();
    destination = list;
    click('/admin/posts', { returning: true });
    await native[1].invoke();
    const container = document.querySelector<HTMLElement>('[data-vt-scroll]')!;
    expect(container.scrollTop).toBe(650);
    // Settings and other internal panes use CSS smooth scrolling; snapshots need
    // their restored position synchronously, before the transition begins.
    expect(scroll).toHaveBeenLastCalledWith({ left: 0, top: 650, behavior: 'instant' });
    expect(routes.isRestoring()).toBe(true);
    window.dispatchEvent(new Event('wheel'));
    expect(routes.isRestoring()).toBe(false);
    container.scrollTop = 120;
    container.append(document.createElement('div'));
    await vi.advanceTimersByTimeAsync(10);
    expect(container.scrollTop).toBe(120);
  });

  it('logout clears saved admin filters while retaining public search state', async () => {
    routes.writeState(routes.entryId(), 'search', '{"query":"public"}');
    const first = history.state;
    routes.push('/admin/posts');
    await native[0].invoke();
    await finish();
    routes.writeState(routes.entryId(), 'admin-posts', '{"page":3}');
    routes.clearAdminState();
    expect(routes.readState('admin-posts')).toBeNull();
    history.replaceState(first, '', '/');
    window.dispatchEvent(new PopStateEvent('popstate', { state: first }));
    await native[1].invoke();
    expect(routes.readState('search')).toBe('{"query":"public"}');
  });
});

describe('link prefetch', () => {
  let prefetch: Mock<(route: string, withData: boolean) => void>;

  beforeEach(() => {
    routes.disconnect();
    prefetch = vi.fn();
    routes.connect(notify, null, prefetch);
    render('<main data-vt-list>Source</main>');
  });

  function link(href: string, options: { rel?: string } = {}): HTMLAnchorElement {
    const anchor = document.createElement('a');
    anchor.href = href;
    if (options.rel) anchor.rel = options.rel;
    anchor.innerHTML = '<span>title</span>';
    document.body.append(anchor);
    return anchor;
  }

  function pointer(type: string, target: Element, init: { pointerType?: string; to?: Node } = {}) {
    const event = new MouseEvent(type, {
      bubbles: true,
      relatedTarget: init.to ?? null,
    });
    Object.defineProperty(event, 'pointerType', { value: init.pointerType ?? 'mouse' });
    target.dispatchEvent(event);
  }

  function setConnection(value: { saveData?: boolean; effectiveType?: string } | undefined) {
    Object.defineProperty(navigator, 'connection', { configurable: true, value });
  }

  afterEach(() => {
    delete (navigator as unknown as { connection?: unknown }).connection;
  });

  it('prefetches after the hover intent delay with the route and data flag', () => {
    pointer('pointerover', link('/post/a'));
    vi.advanceTimersByTime(64);
    expect(prefetch).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(prefetch).toHaveBeenCalledExactlyOnceWith('/post/a', true);
  });

  it('cancels when the pointer leaves the link but keeps going inside it', () => {
    const left = link('/post/a');
    pointer('pointerover', left);
    pointer('pointerout', left, { to: document.body });
    vi.advanceTimersByTime(100);
    expect(prefetch).not.toHaveBeenCalled();

    const inside = link('/post/b');
    pointer('pointerover', inside);
    pointer('pointerout', inside, { to: inside.querySelector('span')! });
    vi.advanceTimersByTime(100);
    expect(prefetch).toHaveBeenCalledExactlyOnceWith('/post/b', true);
  });

  it('does not prefetch when a click follows the hover', () => {
    const anchor = link('/post/a');
    pointer('pointerover', anchor);
    anchor.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 }));
    vi.advanceTimersByTime(200);
    expect(prefetch).not.toHaveBeenCalled();
    expect(native).toHaveLength(1);
  });

  it('prefetches on touchstart immediately and ignores touch pointerover', () => {
    pointer('pointerover', link('/post/a'), { pointerType: 'touch' });
    vi.advanceTimersByTime(200);
    expect(prefetch).not.toHaveBeenCalled();
    link('/post/b').dispatchEvent(new Event('touchstart', { bubbles: true }));
    expect(prefetch).toHaveBeenCalledExactlyOnceWith('/post/b', true);
  });

  it('prefetches on keyboard focus after the intent delay', () => {
    link('/post/a').dispatchEvent(new Event('focusin', { bubbles: true }));
    vi.advanceTimersByTime(65);
    expect(prefetch).toHaveBeenCalledExactlyOnceWith('/post/a', true);
  });

  it('skips external, native, cross-origin and current-page links', () => {
    for (const anchor of [
      link('/post/a', { rel: 'external' }),
      link('/api/x'),
      link('https://example.org/post/a'),
      link('/#h'),
    ]) {
      pointer('pointerover', anchor);
      vi.advanceTimersByTime(100);
      pointer('pointerout', anchor, { to: document.body });
    }
    expect(prefetch).not.toHaveBeenCalled();
  });

  it('throttles per page for two seconds regardless of the hash', () => {
    const hover = (href: string) => {
      const anchor = link(href);
      pointer('pointerover', anchor);
      vi.advanceTimersByTime(65);
      pointer('pointerout', anchor, { to: document.body });
    };
    hover('/post/a');
    hover('/post/a#x');
    expect(prefetch).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(2000);
    hover('/post/a');
    expect(prefetch).toHaveBeenCalledTimes(2);
    expect(prefetch).toHaveBeenLastCalledWith('/post/a', true);
  });

  it('is disabled by save-data and 2g connections', () => {
    setConnection({ saveData: true });
    link('/post/a').dispatchEvent(new Event('touchstart', { bubbles: true }));
    setConnection({ effectiveType: '2g' });
    link('/post/b').dispatchEvent(new Event('touchstart', { bubbles: true }));
    expect(prefetch).not.toHaveBeenCalled();
    setConnection({ effectiveType: '4g' });
    link('/post/c').dispatchEvent(new Event('touchstart', { bubbles: true }));
    expect(prefetch).toHaveBeenCalledExactlyOnceWith('/post/c', true);
  });

  it('swallows prefetch errors and still navigates', async () => {
    prefetch.mockImplementation(() => {
      throw new Error('boom');
    });
    const anchor = link('/post/a');
    expect(() => anchor.dispatchEvent(new Event('touchstart', { bubbles: true }))).not.toThrow();
    anchor.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 }));
    expect(native).toHaveLength(1);
    await native[0].invoke();
    await finish();
    expect(routes.currentRoute()).toBe('/post/a');
  });

  it('stops prefetching after disconnect', () => {
    const anchor = link('/post/a');
    routes.disconnect();
    pointer('pointerover', anchor);
    anchor.dispatchEvent(new Event('touchstart', { bubbles: true }));
    vi.advanceTimersByTime(200);
    expect(prefetch).not.toHaveBeenCalled();
  });
});

describe('viewport prefetch on touch devices', () => {
  class FakeObserver {
    static instances: FakeObserver[] = [];
    observed = new Set<Element>();
    disconnected = false;
    constructor(
      private callback: (entries: Partial<IntersectionObserverEntry>[]) => void,
      readonly options?: IntersectionObserverInit,
    ) {
      FakeObserver.instances.push(this);
    }
    observe(element: Element) {
      this.observed.add(element);
    }
    unobserve(element: Element) {
      this.observed.delete(element);
    }
    disconnect() {
      this.disconnected = true;
    }
    reveal(...elements: Element[]) {
      this.callback(elements.map((target) => ({ target, isIntersecting: true })));
    }
  }

  let prefetch: Mock<(route: string, withData: boolean) => void>;

  function link(href: string): HTMLAnchorElement {
    const anchor = document.createElement('a');
    anchor.href = href;
    document.body.append(anchor);
    return anchor;
  }

  function connect(hoverNone: boolean) {
    vi.mocked(window.matchMedia).mockImplementation(
      (query) => ({ matches: hoverNone && query === '(hover: none)' }) as MediaQueryList,
    );
    routes.disconnect();
    routes.connect(notify, null, prefetch);
  }

  const observer = () => FakeObserver.instances[FakeObserver.instances.length - 1];

  beforeEach(() => {
    FakeObserver.instances = [];
    prefetch = vi.fn();
    vi.stubGlobal('IntersectionObserver', FakeObserver);
    // 空闲回调换成 setTimeout(0)，便于用假定时器推进。
    vi.stubGlobal('requestIdleCallback', (callback: () => void) => setTimeout(callback, 0));
    vi.stubGlobal('cancelIdleCallback', (handle: number) => clearTimeout(handle));
  });

  it('prefetches code only, once per page, when a link reaches the viewport', () => {
    const anchor = link('/post/a');
    const again = link('/post/a#x');
    connect(true);
    vi.advanceTimersByTime(300);
    expect(observer().options).toEqual({ rootMargin: '200px' });
    expect(observer().observed).toEqual(new Set([anchor, again]));

    observer().reveal(anchor);
    vi.advanceTimersByTime(0);
    expect(prefetch).toHaveBeenCalledExactlyOnceWith('/post/a', false);

    vi.advanceTimersByTime(2000);
    observer().reveal(again);
    vi.advanceTimersByTime(0);
    expect(prefetch).toHaveBeenCalledTimes(1);
  });

  it('handles at most four addresses per idle slot', () => {
    const anchors = ['a', 'b', 'c', 'd', 'e'].map((slug) => link(`/post/${slug}`));
    connect(true);
    vi.advanceTimersByTime(300);
    observer().reveal(...anchors);
    vi.advanceTimersByTime(0);
    expect(prefetch).toHaveBeenCalledTimes(4);
    vi.advanceTimersByTime(1);
    expect(prefetch).toHaveBeenCalledTimes(5);
    expect(prefetch).toHaveBeenLastCalledWith('/post/e', false);
  });

  it('does not let a viewport prefetch suppress the data prefetch from a touch', () => {
    const anchor = link('/post/a');
    connect(true);
    vi.advanceTimersByTime(300);
    observer().reveal(anchor);
    vi.advanceTimersByTime(0);
    anchor.dispatchEvent(new Event('touchstart', { bubbles: true }));
    expect(prefetch.mock.calls).toEqual([
      ['/post/a', false],
      ['/post/a', true],
    ]);
  });

  it('skips the current page and respects save-data', () => {
    const here = link('/#top');
    const there = link('/post/a');
    connect(true);
    vi.advanceTimersByTime(300);
    Object.defineProperty(navigator, 'connection', {
      configurable: true,
      value: { saveData: true },
    });
    try {
      observer().reveal(here, there);
      vi.advanceTimersByTime(0);
    } finally {
      delete (navigator as unknown as { connection?: unknown }).connection;
    }
    expect(prefetch).not.toHaveBeenCalled();
    observer().reveal(here);
    vi.advanceTimersByTime(0);
    expect(prefetch).not.toHaveBeenCalled();
  });

  it('does not create an observer without hover: none', () => {
    link('/post/a');
    connect(false);
    vi.advanceTimersByTime(300);
    expect(FakeObserver.instances).toHaveLength(0);
  });

  it('picks up links inserted later after the scan debounce', async () => {
    connect(true);
    vi.advanceTimersByTime(300);
    const late = link('/post/late');
    await Promise.resolve();
    vi.advanceTimersByTime(299);
    expect(observer().observed.has(late)).toBe(false);
    vi.advanceTimersByTime(1);
    expect(observer().observed.has(late)).toBe(true);
  });

  it('disconnects both observers and drops queued work', () => {
    const anchor = link('/post/a');
    connect(true);
    vi.advanceTimersByTime(300);
    const current = observer();
    current.reveal(anchor);
    routes.disconnect();
    vi.advanceTimersByTime(5000);
    expect(current.disconnected).toBe(true);
    expect(prefetch).not.toHaveBeenCalled();
  });
});
