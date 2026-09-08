import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from 'vitest';
import { RouteTransitions } from './route-transitions';
import { beginTransition } from './view-transition-lifecycle';

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

function click(route: string, options: { post?: string; returning?: boolean } = {}): void {
  const anchor = document.createElement('a');
  anchor.href = route;
  if (options.post) anchor.dataset.vtPostLink = options.post;
  if (options.returning) anchor.dataset.vtReturn = 'true';
  anchor.addEventListener('click', (event) => {
    event.preventDefault();
    routes.push(route);
  });
  document.body.append(anchor);
  anchor.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 }));
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
    await finish(1);
    expect(document.documentElement.classList.contains('is-route-transitioning')).toBe(false);
  });

  it('a theme interruption preserves the pending route and its callback commits only once', async () => {
    routes.push('/about');
    const theme = beginTransition('theme');
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

describe('shared article snapshots', () => {
  it('capture intent survives a microtask before the delegated navigation handler', async () => {
    render(`<main data-vt-list>${post()}</main>`);
    destination = `<main data-vt-detail>${post()}</main>`;
    const anchor = document.createElement('a');
    anchor.href = '/post/article';
    anchor.dataset.vtPostLink = '42';
    anchor.addEventListener('click', (event) => {
      event.preventDefault();
      queueMicrotask(() => routes.push('/post/article'));
    });
    document.body.append(anchor);
    anchor.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
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

  it('unconsumed click intent expires before a later unrelated programmatic navigation', async () => {
    render(`<main data-vt-list>${post()}</main>`);
    destination = `<main data-vt-detail>${post()}</main>`;
    const anchor = document.createElement('a');
    anchor.href = '/post/article';
    anchor.dataset.vtPostLink = '42';
    anchor.addEventListener('click', (event) => event.preventDefault());
    document.body.append(anchor);
    anchor.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    await vi.advanceTimersByTimeAsync(1);
    routes.push('/post/article');
    expect(
      document.querySelector<HTMLElement>('[data-vt-role="title"]')!.style.viewTransitionName,
    ).toBe('');
    await native[0].invoke();
  });

  it('matches title and loaded cover, then restores prior inline names', async () => {
    render(`<main data-vt-list>${post('42', true)}</main>`);
    const source = document.querySelector<HTMLElement>('[data-vt-role="title"]')!;
    source.style.viewTransitionName = 'previous-title';
    destination = `<main data-vt-detail>${post('42', true)}</main>`;
    click('/post/article', { post: '42' });
    expect(source.style.viewTransitionName).toBe('vt-post-title');
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
