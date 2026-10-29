/**
 * 浏览器 history 与视图过渡协调器。JS 拥有 history：只有视图过渡的 update 回调才发布新路由，
 * Rust 侧（Leptos）通过 `connect` 的 notify 回调读取地址栏并渲染。
 */
import { prefersReducedMotion } from '@yggdrasil/shared';
import {
  captureScroll,
  restoreScroll,
  type ScrollSnapshot,
  stabilizeNavigationScroll,
} from './navigation-scroll';
import { markPageEntryHandled } from './page-entry';
import { beginTransition, type TransitionOwner } from './view-transition-lifecycle';

const WAIT_MS = 300;
/** 鼠标停留多久算“有意点击”（instant.page 的默认值）。 */
const HOVER_INTENT_MS = 65;
/** 同一地址两次预取的最小间隔，挡住指针抖动；数据新鲜度由 Rust 缓存自己判断。 */
const PREFETCH_THROTTLE_MS = 2000;
/** 触屏设备视口预取：每次空闲最多处理的地址数、重新扫描链接的防抖、已预取集合的上限。 */
const VIEWPORT_BATCH = 4;
const VIEWPORT_SCAN_MS = 300;
const VIEWPORT_MEMORY = 500;
const PENDING_CLASS = 'is-route-pending';
const STATE_KEY = '__yggdrasilNavigation';
const POSITION_KEY = '__yggdrasilPosition';
const MAX_ENTRIES = 40;

interface Entry {
  id: string;
  position: number;
  url: string;
  values: Map<string, string>;
  scroll?: ScrollSnapshot;
  origin?: { id: string; url: string; post: string };
}
interface LinkIntent {
  url: string;
  anchor: HTMLAnchorElement;
  post?: string;
  returning: boolean;
}

/** 这些路径由服务端直接处理，不是 SPA 路由，点击交给浏览器原生导航。 */
const NATIVE_PREFIXES = [
  '/api/',
  '/doc/',
  '/uploads/',
  '/og/',
  '/pkg/',
  '/note-media/',
  '/feed.',
  '/mcp',
];
interface Navigation {
  id: number;
  entry: Entry;
  kind: 'push' | 'replace' | 'pop';
  restore?: ScrollSnapshot;
  post?: string;
  reverse: boolean;
  deadline: number;
  committed: boolean;
  rendered: boolean;
  /** 等待到期时目标页仍未提交：已跳过 VT，旧正文淡出，等迟到的 rendered 收尾。 */
  late: boolean;
  /** 接力导航：上一次过渡播放中发起，沿用它的原生过渡，在实时的新快照里直接换页。 */
  relay: boolean;
  /**
   * 原生过渡所处阶段：capture 为截图前，update 为回调已提交、等目标页渲染（浏览器暂停绘制），
   * animate 为 ready 之后的动画播放。结束、跳过或取消后为 undefined。
   */
  phase?: 'capture' | 'update' | 'animate';
  cancelled: boolean;
  owner: TransitionOwner;
  names: Map<HTMLElement, string>;
  oldRoles: Set<string>;
  shell: boolean;
  /** 正文过渡（`data-vt-section`）专用：截图前 `main` 的视口位置，旧正文快照据此留在原处。 */
  sectionFrom?: DOMRect;
  /**
   * 被点击的链接。截图起到 ready 后浏览器重新命中测试前，悬停都落在 `<html>` 上；
   * 截图那一帧会把它画成未悬停色并冻结到 ready。`data-vt-pressed` 维持悬停外观，
   * 过渡结束或取消时撤掉。
   */
  pressed?: HTMLAnchorElement;
  /**
   * Content transitions only: the desktop pill before the update and its nav link. `source`
   * marks a relay: the header may still show that pill, so sliding waits until it changes.
   */
  pill?: { from?: DOMRect; href?: string; source?: HTMLElement | null };
  attempt?: () => void;
  release?: () => void;
  vt?: ViewTransition;
  imageCleanups: Array<() => void>;
  scrollStarted: boolean;
  scrollCancelled: boolean;
  scrolled: boolean;
}

function page(url: string): string {
  return url.split('#')[0];
}

/** 省流量模式或 2G 网络下不预取。TS 的 DOM lib 没有 `navigator.connection`，用局部类型。 */
function constrainedNetwork(): boolean {
  // Network Information API 不在 TS 的 DOM lib 里，且并非所有浏览器都有：按可选字段声明。
  const nav: Navigator & { connection?: { saveData?: boolean; effectiveType?: string } } =
    navigator;
  const connection = nav.connection;
  return connection?.saveData === true || /2g$/.test(connection?.effectiveType ?? '');
}

function sectionIndex(url: string): number {
  const path = page(url).split('?')[0];
  if (/^\/page\/\d+$/.test(path)) return 0;
  if (/^\/notes(?:\/|$)/.test(path)) return 1;
  if (/^\/tags(?:\/|$)/.test(path)) return 2;
  return ['/', '/notes', '/archives', '/friends', '/about', '/search'].indexOf(path);
}
/** Desktop section pill; the mobile nav is hidden and measures zero width. */
const NAV_PILL = '.site-nav .nav-indicator';
/** Minified CSS may serialize time tokens in seconds (`.41s`). */
function cssTime(value: string): number {
  const time = value.trim();
  return Number.parseFloat(time) * (time.endsWith('ms') ? 1 : 1000);
}
function shell(url: string): string {
  const path = page(url).split('?')[0];
  return /^\/admin(?:\/|$)/.test(path)
    ? 'admin'
    : /^\/(?:login|register)$/.test(path)
      ? path
      : 'frontend';
}
function visible(element: HTMLElement): boolean {
  const rect = element.getBoundingClientRect();
  const style = getComputedStyle(element);
  if (
    rect.width <= 0 ||
    rect.height <= 0 ||
    style.visibility === 'hidden' ||
    style.display === 'none'
  )
    return false;
  let left = 0;
  let top = 0;
  let right = window.innerWidth;
  let bottom = window.innerHeight;
  // An admin row can intersect the window but lie outside its clipped scrolling card.
  for (let parent = element.parentElement; parent; parent = parent.parentElement) {
    const css = getComputedStyle(parent);
    if (/(auto|scroll|hidden|clip)/.test(`${css.overflow} ${css.overflowY} ${css.overflowX}`)) {
      const box = parent.getBoundingClientRect();
      left = Math.max(left, box.left);
      right = Math.min(right, box.right);
      top = Math.max(top, box.top);
      bottom = Math.min(bottom, box.bottom);
    }
  }
  return rect.bottom > top && rect.top < bottom && rect.right > left && rect.left < right;
}

export class RouteTransitions {
  private notify: (() => void) | undefined;
  private prefix = '';
  private displayed = '/';
  private sequence = 0;
  private entrySequence = 0;
  private entries = new Map<string, Entry>();
  private active: Entry | undefined;
  private navigation: Navigation | undefined;
  private intent: LinkIntent | undefined;
  private restoreCleanup: (() => void) | undefined;
  private restoring = false;
  private oldRestoration: ScrollRestoration = 'auto';
  private leaveGuard: { id: number; check: () => boolean } | undefined;
  private guardSequence = 0;
  private rollingBack = false;
  private prefetch: ((route: string, withData: boolean) => void) | undefined;
  private prefetched = new Map<string, number>(); // 预取地址（不含哈希）→ 上次时间
  private intentTimer: number | undefined;
  private intentAnchor: HTMLAnchorElement | undefined;
  private viewportObserver: IntersectionObserver | undefined;
  private mutationObserver: MutationObserver | undefined;
  private observedAnchors = new WeakSet<HTMLAnchorElement>();
  private viewportQueue: string[] = [];
  private viewportCodePrefetched = new Set<string>();
  private scanTimer: number | undefined;
  private cancelIdle: (() => void) | undefined;
  /** 原生过渡 update 阶段（浏览器暂停绘制）收到的站内跳转，等 ready 后接力，只留最新一次。 */
  private queued: { route: string; kind: 'push' | 'replace'; intent?: LinkIntent } | undefined;

  constructor() {
    // 两个监听都必须早于 Leptos 生效：Leptos 的 BrowserUrl 在水合时就在 window 上挂了
    // popstate 与链接点击监听，而 Rust 要等整棵树水合完毕才 connect。
    // popstate：window 上的监听按注册顺序触发（捕获/冒泡不分先后），只能在脚本加载时注册；
    // 未连接时 onPop 直接放行，由 Leptos 自己处理。
    window.addEventListener('popstate', this.onPop, true);
    // click：document 冒泡阶段晚于元素自身的 Leptos 处理器（可 preventDefault），早于 window；
    // 未连接时 onClick 截住冒泡、交给浏览器整页跳转，避免路由器在水合中途换页。
    document.addEventListener('click', this.onClick);
  }

  connect(
    notify: () => void,
    prefix: string | null,
    prefetch?: (route: string, withData: boolean) => void,
  ): void {
    this.prefetch = prefetch;
    if (this.notify) {
      this.notify = notify;
      return;
    }
    this.notify = notify;
    this.prefix = prefix?.replace(/\/$/, '') ?? '';
    this.displayed = this.locationRoute();
    this.active = this.newEntry(this.displayed);
    this.active.position = this.historyPosition() ?? 0;
    this.writeHistory(this.active, 'replace');
    this.oldRestoration = history.scrollRestoration;
    history.scrollRestoration = 'manual';
    // popstate 与 click 监听在构造时已注册（见 constructor）。
    window.addEventListener('hashchange', this.onHash);
    window.addEventListener('pagehide', this.onPageHide);
    document.addEventListener('pointerover', this.onPointerOver, { passive: true });
    document.addEventListener('pointerout', this.onPointerOut, { passive: true });
    document.addEventListener('focusin', this.onFocusIn);
    document.addEventListener('focusout', this.onPointerOut);
    document.addEventListener('touchstart', this.onTouchStart, { capture: true, passive: true });
    this.startViewportPrefetch();
  }

  disconnect(): void {
    this.navigation?.owner.finish();
    if (this.navigation) this.cancel(this.navigation);
    this.restoreCleanup?.();
    window.removeEventListener('hashchange', this.onHash);
    window.removeEventListener('pagehide', this.onPageHide);
    document.removeEventListener('pointerover', this.onPointerOver);
    document.removeEventListener('pointerout', this.onPointerOut);
    document.removeEventListener('focusin', this.onFocusIn);
    document.removeEventListener('focusout', this.onPointerOut);
    document.removeEventListener('touchstart', this.onTouchStart, { capture: true });
    this.cancelIntent();
    this.prefetch = undefined;
    this.prefetched.clear();
    this.stopViewportPrefetch();
    this.queued = undefined;
    this.clearPending();
    history.scrollRestoration = this.oldRestoration;
    this.notify = undefined;
    this.navigation = undefined;
    this.entries.clear();
    this.active = undefined;
    this.leaveGuard = undefined;
    this.rollingBack = false;
  }

  currentRoute = (): string => this.displayed;
  currentPrefix = (): string | null => this.prefix || null;
  navigationId = (): number => this.sequence;
  entryId = (): string => this.active?.id ?? '';
  isRestoring = (): boolean => this.restoring;
  /** Registration IDs prevent an outgoing page from clearing the next page's guard. */
  setLeaveGuard = (check: () => boolean): number => {
    const id = ++this.guardSequence;
    this.leaveGuard = { id, check };
    return id;
  };
  clearLeaveGuard = (id: number): void => {
    if (this.leaveGuard?.id === id) this.leaveGuard = undefined;
  };
  readState = (key: string): string | null => this.active?.values.get(key) ?? null;
  writeState = (id: string, key: string, value: string): void => {
    this.entries.get(id)?.values.set(key, value);
  };
  clearAdminState = (): void => {
    for (const entry of this.entries.values()) {
      if (shell(entry.url) === 'admin') {
        entry.values.clear();
        entry.scroll = undefined;
        // A late effect from an outgoing admin component cannot recreate cleared state.
        this.entries.delete(entry.id);
      }
      if (entry.origin && shell(entry.origin.url) === 'admin') entry.origin = undefined;
    }
  };
  back = (): void => {
    history.back();
  };
  forward = (): void => {
    history.forward();
  };
  external = (url: string): boolean => {
    try {
      window.location.assign(url);
      return true;
    } catch {
      return false;
    }
  };

  push = (route: string): void => {
    this.request(route, 'push');
  };
  replace = (route: string): void => {
    this.request(route, 'replace');
  };

  /** Called after Leptos flushes the layout's actual DOM, including fallback/error branches. */
  rendered = (id: number, route: string): void => {
    if (page(route) !== page(this.displayed) || id !== this.sequence) return;
    const nav = this.navigation;
    if (!nav?.committed || nav.id !== id) return;
    nav.rendered = true;
    this.clearPending();
    this.correctScroll(nav);
    if (nav.late || nav.relay) this.slidePill(nav);
    if (nav.cancelled) this.startScrollRestoration(nav);
    nav.attempt?.();
  };

  /** Anchor-click uses replaceState, which emits neither popstate nor hashchange. */
  syncHash = (): void => {
    this.restoreCleanup?.();
    this.displayed = this.locationRoute();
    if (this.active) this.active.url = this.displayed;
  };

  private locationRoute(): string {
    let path = window.location.pathname;
    if (this.prefix && (path === this.prefix || path.startsWith(`${this.prefix}/`)))
      path = path.slice(this.prefix.length);
    return (path || '/') + window.location.search + window.location.hash;
  }
  private normalize(route: string): string {
    const url = new URL(route, window.location.origin + this.displayed);
    if (url.origin !== window.location.origin) throw new Error('External navigation');
    return url.pathname + url.search + url.hash;
  }
  private newEntry(url: string, restore?: Entry): Entry {
    const entry: Entry = {
      id: `vt-${Date.now().toString(36)}-${++this.entrySequence}`,
      position: this.active?.position ?? 0,
      url,
      values: new Map(restore?.values),
      scroll: restore?.scroll,
    };
    this.entries.set(entry.id, entry);
    if (this.entries.size > MAX_ENTRIES) {
      const oldest = [...this.entries.keys()].find((id) => id !== this.active?.id);
      if (oldest) this.entries.delete(oldest);
    }
    return entry;
  }
  private writeHistory(entry: Entry, kind: 'push' | 'replace'): void {
    const previous: unknown = history.state;
    const state =
      previous && typeof previous === 'object' && !Array.isArray(previous) ? { ...previous } : {};
    const value = { ...state, [STATE_KEY]: entry.id, [POSITION_KEY]: entry.position };
    const url = this.prefix + entry.url;
    if (kind === 'push') history.pushState(value, '', url);
    else history.replaceState(value, '', url);
  }

  private onClick = (event: MouseEvent): void => {
    if (event.defaultPrevented) return;
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey)
      return;
    const routable = this.routableAnchor(this.clickTarget(event));
    if (!routable) return;
    const { anchor } = routable;
    if (!this.notify || !this.active) {
      // 未连接（水合中、断开后）：不让冒泡到 window 上 Leptos 的链接拦截，浏览器整页跳转。
      event.stopPropagation();
      return;
    }
    // 点击已经开始导航：不能等到点击后才预取，否则资源已发出请求，预取条目没人消费。
    this.cancelIntent();
    event.preventDefault();
    const url = routable.url;
    this.intent = {
      url,
      anchor,
      post: anchor.dataset.vtPostLink,
      returning: anchor.dataset.vtReturn === 'true',
    };
    this.request(url, 'push');
  };

  /**
   * 路由过渡期间，原生过渡把命中测试全部交给 `<html>`（截图后到 ready 之间，以及 root
   * 过渡的整段动画），页头链接的点击会被吞掉。按坐标在外壳（页头、页脚、后台侧栏）里找回
   * 被点的链接；外壳在过渡中保持原位，实时 DOM 的几何就是用户看到的位置。
   */
  private clickTarget(event: MouseEvent): EventTarget | null {
    const root = document.documentElement;
    if (event.target !== root || !root.classList.contains('is-route-transitioning'))
      return event.target;
    const { clientX: x, clientY: y } = event;
    for (const anchor of document.querySelectorAll<HTMLAnchorElement>('[data-vt-shell] a[href]')) {
      const box = anchor.getBoundingClientRect();
      if (x >= box.left && x < box.right && y >= box.top && y < box.bottom && visible(anchor))
        return anchor;
    }
    return event.target;
  }

  /** 站内 SPA 链接 → 去掉前缀的“路径 + 查询 + 哈希”；不该由协调器接管的返回 undefined。 */
  private routableAnchor(
    target: EventTarget | null,
  ): { anchor: HTMLAnchorElement; url: string } | undefined {
    const anchor = target instanceof Element ? target.closest<HTMLAnchorElement>('a[href]') : null;
    if (
      !anchor ||
      (anchor.target !== '' && anchor.target !== '_self') ||
      anchor.hasAttribute('download') ||
      anchor.relList.contains('external') ||
      anchor.origin !== location.origin ||
      NATIVE_PREFIXES.some((prefix) => anchor.pathname.startsWith(prefix))
    )
      return undefined;
    return { anchor, url: anchor.pathname.slice(this.prefix.length) + anchor.search + anchor.hash };
  }

  private cancelIntent(): void {
    clearTimeout(this.intentTimer);
    this.intentTimer = undefined;
    this.intentAnchor = undefined;
  }

  /** 悬停/聚焦：停留 HOVER_INTENT_MS 后才预取；同一链接的子元素会反复触发，直接忽略。 */
  private beginIntent(target: EventTarget | null): void {
    const routable = this.routableAnchor(target);
    if (!routable || routable.anchor === this.intentAnchor) return;
    this.cancelIntent();
    this.intentAnchor = routable.anchor;
    this.intentTimer = window.setTimeout(() => {
      this.intentTimer = undefined;
      this.requestPrefetch(routable.url, true);
    }, HOVER_INTENT_MS);
  }

  private onPointerOver = (event: PointerEvent): void => {
    // 触屏交给 touchstart。
    if (event.pointerType !== 'mouse' && event.pointerType !== 'pen') return;
    this.beginIntent(event.target);
  };
  private onPointerOut = (event: PointerEvent | FocusEvent): void => {
    const related = event.relatedTarget;
    if (related instanceof Node && this.intentAnchor?.contains(related)) return;
    this.cancelIntent();
  };
  private onFocusIn = (event: FocusEvent): void => {
    this.beginIntent(event.target);
  };
  private onTouchStart = (event: TouchEvent): void => {
    // 按下到抬起约 100ms，就是用这段时间。
    const routable = this.routableAnchor(event.target);
    if (routable) this.requestPrefetch(routable.url, true);
  };

  private requestPrefetch(url: string, withData: boolean): void {
    if (!this.prefetch || !this.active || constrainedNetwork()) return;
    const target = page(url);
    if (target === page(this.displayed)) return; // 当前页（含页内锚点）不预取
    // 只预取分包的请求单独节流：触屏按下随后还要预取数据，不能被视口预取挡住。
    const key = withData ? target : `${target}\0code`;
    const now = performance.now();
    if (now - (this.prefetched.get(key) ?? -Infinity) < PREFETCH_THROTTLE_MS) return;
    if (this.prefetched.size > 200) this.prefetched.clear();
    this.prefetched.set(key, now);
    try {
      this.prefetch(target, withData);
    } catch {
      // 预取失败不能影响导航；Rust 回调出错时这里吞掉。
    }
  }

  /** 没有悬停能力的设备：链接进入视口（外扩 200px）后在空闲期预取分包，不预取数据。 */
  private startViewportPrefetch(): void {
    if (typeof IntersectionObserver !== 'function' || !window.matchMedia('(hover: none)').matches)
      return;
    this.viewportObserver = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          this.viewportObserver?.unobserve(entry.target);
          const routable = this.routableAnchor(entry.target);
          if (!routable) continue;
          const target = page(routable.url);
          if (this.viewportCodePrefetched.has(target) || this.viewportQueue.includes(target))
            continue;
          this.viewportQueue.push(target);
        }
        this.scheduleFlush();
      },
      { rootMargin: '200px' },
    );
    // Suspense 内容晚到、“加载更多”追加的链接也要覆盖。
    this.mutationObserver = new MutationObserver(() => this.scheduleScan());
    this.mutationObserver.observe(document.body, { childList: true, subtree: true });
    this.scheduleScan();
  }

  private stopViewportPrefetch(): void {
    this.viewportObserver?.disconnect();
    this.mutationObserver?.disconnect();
    this.viewportObserver = undefined;
    this.mutationObserver = undefined;
    clearTimeout(this.scanTimer);
    this.scanTimer = undefined;
    this.cancelIdle?.();
    this.cancelIdle = undefined;
    this.observedAnchors = new WeakSet();
    this.viewportQueue = [];
    this.viewportCodePrefetched.clear();
  }

  private scheduleScan(): void {
    if (!this.viewportObserver) return;
    clearTimeout(this.scanTimer);
    this.scanTimer = window.setTimeout(() => {
      this.scanTimer = undefined;
      this.scanAnchors();
    }, VIEWPORT_SCAN_MS);
  }

  private scanAnchors(): void {
    const observer = this.viewportObserver;
    if (!observer) return;
    for (const anchor of document.querySelectorAll<HTMLAnchorElement>('a[href]')) {
      if (this.observedAnchors.has(anchor) || !this.routableAnchor(anchor)) continue;
      this.observedAnchors.add(anchor);
      observer.observe(anchor);
    }
  }

  private scheduleFlush(): void {
    if (this.cancelIdle || this.viewportQueue.length === 0) return;
    const run = () => {
      this.cancelIdle = undefined;
      this.flushViewport();
    };
    if (typeof window.requestIdleCallback === 'function') {
      const handle = window.requestIdleCallback(run, { timeout: 2000 });
      this.cancelIdle = () => window.cancelIdleCallback(handle);
    } else {
      const handle = window.setTimeout(run, 200);
      this.cancelIdle = () => clearTimeout(handle);
    }
  }

  private flushViewport(): void {
    if (constrainedNetwork()) {
      this.viewportQueue = [];
      return;
    }
    if (this.viewportCodePrefetched.size > VIEWPORT_MEMORY) this.viewportCodePrefetched.clear();
    for (const target of this.viewportQueue.splice(0, VIEWPORT_BATCH)) {
      if (target === page(this.displayed)) continue;
      this.viewportCodePrefetched.add(target);
      this.requestPrefetch(target, false);
    }
    this.scheduleFlush();
  }
  private onPageHide = (): void => {
    if (this.active) this.active.scroll = captureScroll();
    this.queued = undefined;
    this.clearPending();
    this.restoreCleanup?.();
    if (this.navigation) this.cancel(this.navigation);
  };
  private onHash = (): void => {
    if (this.rollingBack) return;
    if (this.locationRoute() === this.displayed) return;
    if (!this.navigation || this.navigation.cancelled || this.navigation.committed) this.syncHash();
  };
  private onPop = (event: PopStateEvent): void => {
    // 未连接（水合前、断开后）放行，由 Leptos 自己处理；连接后在 Leptos 之前拦下，
    // 阻止它绕过视图过渡直接改 URL。
    if (!this.notify || !this.active) return;
    event.stopImmediatePropagation();
    const route = this.locationRoute();
    const position = this.historyPosition();
    if (this.rollingBack) {
      if (route === this.displayed && position === this.active?.position) {
        this.rollingBack = false;
      } else {
        this.restoreHistory(position);
      }
      return;
    }
    if (!this.canLeave(route)) {
      const pending = this.navigation;
      if (pending && !pending.committed && !pending.cancelled) {
        this.cancel(pending);
        pending.owner.finish();
      }
      this.restoreHistory(position);
      return;
    }
    if (this.active) this.active.scroll = captureScroll();
    const id: unknown = history.state?.[STATE_KEY];
    const entry = typeof id === 'string' ? this.entries.get(id) : undefined;
    const target = entry ?? this.newEntry(route);
    target.position = position ?? target.position;
    target.url = route;
    if (!entry) this.writeHistory(target, 'replace');
    this.start(target, 'pop', target.scroll);
  };

  private request(route: string, kind: 'push' | 'replace'): void {
    if (!this.notify || !this.active || this.rollingBack) return;
    if (this.navigation?.phase === 'update') {
      // 浏览器正暂停绘制、等上一页渲染：此时跳过只会让它截一张旧页面再冻住一次。
      // 记下来，等 ready 后在播放中的过渡里接力。
      this.queued = { route, kind, intent: this.intent };
      this.intent = undefined;
      return;
    }
    let target: string;
    try {
      target = this.normalize(route);
    } catch {
      this.external(route);
      return;
    }
    if (target === this.displayed) {
      const pending = this.navigation;
      if (pending && !pending.committed && !pending.cancelled) {
        this.cancel(pending);
        pending.owner.finish();
      }
      return;
    }
    const intent = this.intent?.url === target ? this.intent : undefined;
    this.intent = undefined;
    if (!this.canLeave(target)) return;
    const origin = this.active.origin;
    const source =
      intent?.returning && origin && page(origin.url) === page(target)
        ? this.entries.get(origin.id)
        : undefined;
    const entry = this.newEntry(target, source);
    entry.position = this.active.position + (kind === 'push' ? 1 : 0);
    if (page(target) === page(this.displayed)) {
      // Fragment navigation keeps the page mounted, including its captured entry ID.
      // Share live page state while retaining independent history/scroll positions.
      entry.values = this.active.values;
      entry.origin = this.active.origin;
    }
    if (intent?.post) entry.origin = { id: this.active.id, url: this.displayed, post: intent.post };
    this.start(entry, kind, source?.scroll, intent?.anchor);
  }

  private canLeave(target: string): boolean {
    if (page(target) === page(this.displayed) || !this.leaveGuard) return true;
    try {
      return this.leaveGuard.check();
    } catch {
      return false;
    }
  }

  private historyPosition(): number | undefined {
    const value: unknown = history.state?.[POSITION_KEY];
    return typeof value === 'number' && Number.isSafeInteger(value) ? value : undefined;
  }

  private restoreHistory(position: number | undefined): void {
    if (!this.active) return;
    if (position !== undefined && position !== this.active.position) {
      this.rollingBack = true;
      history.go(this.active.position - position);
    } else {
      // Unmarked third-party history cannot be traversed reliably. Retain the draft
      // and URL, at the cost of replacing that untracked forward branch.
      this.rollingBack = false;
      this.writeHistory(this.active, 'push');
    }
  }

  private start(
    entry: Entry,
    kind: Navigation['kind'],
    restore?: ScrollSnapshot,
    pressed?: HTMLAnchorElement,
  ): void {
    if (!this.active || !this.notify) return;
    // A newer navigation supersedes one waiting for the previous update callback.
    this.queued = undefined;
    this.restoreCleanup?.();
    // 上一次过渡还在播放：跳过它会让刚进来的页面跳回全亮、再被截成旧快照冻住，随后又淡出
    // 一次。改为接力：保留它的原生动画（新快照是实时画面），在其中直接换页。
    const running = this.navigation;
    const relay =
      running?.phase === 'animate' &&
      !running.cancelled &&
      running.vt !== undefined &&
      page(entry.url) !== page(this.displayed)
        ? running
        : undefined;
    // Hand the running animation over before the new owner would skip it.
    relay?.owner.finish();
    // Commit interrupted theme changes before capturing geometry or starting the next route.
    const owner = beginTransition('route');
    const previous = this.active;
    previous.scroll = captureScroll();
    const reverse =
      previous.origin?.id === entry.id ||
      (restore !== undefined && previous.origin?.url === entry.url);
    const post = reverse
      ? previous.origin?.post
      : entry.origin?.id === previous.id
        ? entry.origin.post
        : undefined;
    const nav: Navigation = {
      id: ++this.sequence,
      entry,
      kind,
      restore,
      post,
      reverse,
      deadline: performance.now() + WAIT_MS,
      committed: false,
      rendered: false,
      late: false,
      relay: relay !== undefined,
      cancelled: false,
      owner,
      names: new Map(),
      oldRoles: new Set(),
      imageCleanups: [],
      scrollStarted: false,
      scrollCancelled: false,
      scrolled: false,
      shell: shell(previous.url) === shell(entry.url),
    };
    this.navigation = nav;
    owner.onCancel((nextKind) => {
      // Theme changes may skip a page animation, but must not lose its navigation.
      if (nextKind === 'theme') this.commit(nav);
      this.cancel(nav);
    });
    window.dispatchEvent(new Event('yggdrasil:navigation-start'));
    if (relay) {
      this.relayFrom(nav, relay);
      return;
    }
    const animate =
      page(entry.url) !== page(this.displayed) &&
      typeof document.startViewTransition === 'function' &&
      !prefersReducedMotion();
    if (!animate) {
      this.commit(nav);
      this.startScrollRestoration(nav);
      owner.finish();
      return;
    }
    document.documentElement.classList.add('is-route-transitioning');
    if (pressed) {
      pressed.dataset.vtPressed = '';
      nav.pressed = pressed;
    }
    if (nav.post) {
      for (const role of ['title', 'cover']) {
        const element = this.sharedElement(nav.post, role);
        if (element && (role !== 'cover' || this.coverReady(element))) {
          this.name(nav, element, `vt-post-${role}`);
          nav.oldRoles.add(role);
        }
      }
    }
    // 前台外壳内没有共享元素的导航只捕获正文，页头保持实时。整页（root）过渡里页头只显示新快照：
    // 新页在顶部时页头透明，旧页滚动后原本被页头遮住的正文会在导航文字下方露出来。
    const main = document.querySelector('main');
    if (main && nav.shell && shell(entry.url) === 'frontend' && nav.oldRoles.size === 0) {
      const fromSection = sectionIndex(previous.url);
      const toSection = sectionIndex(entry.url);
      // 两个不同栏目之间按栏目顺序横向滑动，其余（文章、同栏目内、栏目外页面）原地淡入淡出。
      document.documentElement.dataset.vtSection =
        fromSection < 0 || toSection < 0 || fromSection === toSection
          ? 'fade'
          : toSection > fromSection
            ? 'forward'
            : 'backward';
      nav.sectionFrom = main.getBoundingClientRect();
      const pill = document.querySelector<HTMLElement>(NAV_PILL);
      nav.pill = {
        from: pill?.getBoundingClientRect(),
        href: pill?.closest('.nav-item')?.getAttribute('href') ?? undefined,
      };
    }
    this.assignShell(nav);
    try {
      nav.phase = 'capture';
      const vt = document.startViewTransition(async () => {
        if (!owner.isCurrent() || nav.cancelled) return;
        nav.phase = 'update';
        this.commit(nav);
        await this.waitForDestination(nav);
      });
      nav.vt = vt;
      owner.attach(vt);
      void vt.ready.then(
        () => {
          if (!nav.cancelled && !nav.late) {
            nav.phase = 'animate';
            this.slidePill(nav);
          }
          this.flushQueued();
        },
        () => {
          if (!nav.cancelled) nav.phase = undefined;
          this.flushQueued();
        },
      );
      void vt.updateCallbackDone?.catch(() => {});
      const done = () => {
        if (!nav.cancelled) nav.phase = undefined;
        this.finish(nav);
      };
      void vt.finished.then(done, done);
    } catch {
      nav.phase = undefined;
      this.cleanNames(nav);
      document.documentElement.classList.remove('is-route-transitioning');
      this.commit(nav);
      this.startScrollRestoration(nav);
      owner.finish();
    }
  }

  /** Continue `from`'s native transition: its new snapshot is live, so the page swaps inside it. */
  private relayFrom(nav: Navigation, from: Navigation): void {
    const vt = from.vt as ViewTransition;
    from.cancelled = true;
    from.phase = undefined;
    for (const cleanup of from.imageCleanups.splice(0)) cleanup();
    // ready 之后栏目过渡的页头已恢复真实命中测试，root 过渡的页头只显示快照，不再需要保持。
    from.pressed?.removeAttribute('data-vt-pressed');
    // Snapshot names must persist until the native animation ends; this navigation restores them.
    nav.names = from.names;
    from.names = new Map();
    nav.vt = vt;
    nav.phase = 'animate';
    nav.owner.attach(vt);
    const pill = document.querySelector<HTMLElement>(NAV_PILL);
    nav.pill = {
      from: pill?.getBoundingClientRect(),
      href: pill?.closest('.nav-item')?.getAttribute('href') ?? undefined,
      source: pill,
    };
    const done = () => {
      if (!nav.cancelled) nav.phase = undefined;
      this.finish(nav);
    };
    void vt.finished.then(done, done);
    // 旧页面要一直显示到新路由渲染：保留进场抑制，否则它会重播进场、先闪空一下。
    const handled = document.querySelector<HTMLElement>('[data-vt-enter-handled]');
    this.commit(nav);
    if (handled) markPageEntryHandled(handled);
    // The header usually switches its pill before the next frame; rendered() retries otherwise.
    requestAnimationFrame(() => this.slidePill(nav));
    window.setTimeout(
      () => {
        if (!nav.cancelled && !nav.rendered && this.navigation === nav) this.defer(nav);
      },
      Math.max(0, nav.deadline - performance.now()),
    );
  }

  private flushQueued(): void {
    const queued = this.queued;
    if (!queued || this.navigation?.phase === 'update') return;
    this.queued = undefined;
    this.intent = queued.intent;
    this.request(queued.route, queued.kind);
  }

  private commit(nav: Navigation): void {
    if (nav.cancelled || nav.committed || this.navigation !== nav) return;
    nav.committed = true;
    if (nav.kind !== 'pop') this.writeHistory(nav.entry, nav.kind);
    const changedPage = page(this.displayed) !== page(nav.entry.url);
    this.active = nav.entry;
    this.displayed = nav.entry.url;
    this.restoring = nav.restore !== undefined;
    // The same layout element survives navigation; entrance suppression is per navigation.
    if (changedPage)
      document.querySelector('[data-vt-enter-handled]')?.removeAttribute('data-vt-enter-handled');
    this.startScrollRestoration(nav);
    this.notify?.();
  }

  private waitForDestination(nav: Navigation): Promise<void> {
    return new Promise((resolve) => {
      const observer = new MutationObserver(() => nav.attempt?.());
      let settled = false;
      const release = () => {
        if (settled) return;
        settled = true;
        observer.disconnect();
        clearTimeout(timer);
        nav.release = undefined;
        nav.attempt = undefined;
        resolve();
      };
      const complete = () => {
        if (nav.cancelled) {
          release();
          return;
        }
        const wrapper = this.wrapper(nav);
        if (nav.rendered && wrapper) markPageEntryHandled(wrapper);
        this.correctScroll(nav);
        if (nav.sectionFrom !== undefined) {
          // Keep the outgoing snapshot where it was (scroll position, column width and offset)
          // without moving it over the live header.
          const to = document.querySelector('main')?.getBoundingClientRect();
          const top = to?.top ?? 0;
          const header = document.querySelector('[data-vt-shell="frontend-header"]');
          const style = document.documentElement.style;
          style.setProperty('--vt-section-old-x', `${nav.sectionFrom.left - (to?.left ?? 0)}px`);
          style.setProperty('--vt-section-old-y', `${nav.sectionFrom.top - top}px`);
          style.setProperty(
            '--vt-section-clip-top',
            `${Math.max(0, (header?.getBoundingClientRect().bottom ?? 0) - top)}px`,
          );
        }
        this.assignShell(nav);
        if (nav.post && nav.rendered) {
          for (const role of nav.oldRoles) {
            const element = this.sharedElement(nav.post, role);
            if (
              element &&
              (role !== 'cover' || (decoded.has(element) && this.coverReady(element)))
            ) {
              this.name(nav, element, `vt-post-${role}`);
              document.documentElement.classList.add(`vt-shared-${role}`);
            }
          }
        }
        this.startScrollRestoration(nav);
        release();
      };
      const preparing = new WeakSet<HTMLElement>();
      const decoded = new WeakSet<HTMLElement>();
      const failed = new WeakSet<HTMLElement>();
      nav.attempt = () => {
        if (nav.cancelled) {
          release();
          return;
        }
        if (!nav.rendered || !this.wrapper(nav)) return;
        this.correctScroll(nav);
        if (!nav.post || nav.oldRoles.size === 0) {
          complete();
          return;
        }
        const target = this.sharedElement(nav.post, 'title');
        if (this.wrapper(nav)?.querySelector('[data-vt-list-pending]')) return;
        const terminal = this.wrapper(nav)?.querySelector(
          '[data-vt-list], [data-vt-detail], [role="alert"]',
        );
        if (!target) {
          if (terminal) complete();
          return;
        }
        if (nav.oldRoles.has('cover')) {
          const cover = this.sharedElement(nav.post, 'cover', false);
          if (
            cover &&
            (visible(cover) || cover.getBoundingClientRect().height === 0) &&
            !failed.has(cover) &&
            !decoded.has(cover)
          ) {
            if (!preparing.has(cover)) {
              preparing.add(cover);
              this.prepareCover(nav, cover, (ok) => {
                if (!ok) failed.add(cover);
                else decoded.add(cover);
                nav.attempt?.();
              });
            }
            return;
          }
        }
        complete();
      };
      nav.release = release;
      const expire = () => {
        if (!nav.cancelled && !nav.rendered) {
          this.defer(nav);
          release();
          return;
        }
        complete();
      };
      const timer = window.setTimeout(expire, Math.max(0, nav.deadline - performance.now()));
      // Observe DOM commits and image readiness, without relying on rAF while VT pauses rendering.
      observer.observe(document.body, {
        childList: true,
        subtree: true,
        attributes: true,
        attributeFilter: ['class', 'src', 'data-vt-navigation'],
      });
      nav.attempt();
    });
  }

  private defer(nav: Navigation): void {
    nav.late = true;
    nav.phase = undefined;
    document.documentElement.classList.add(PENDING_CLASS);
    // 接力时保留的进场抑制撤掉：旧正文淡出后，迟到的新页面照常进场。
    if (nav.relay)
      document.querySelector('[data-vt-enter-handled]')?.removeAttribute('data-vt-enter-handled');
    // 新快照只会是旧页面：跳过动画，恢复实时渲染，让旧正文按 CSS 淡出。
    nav.vt?.skipTransition();
  }

  private clearPending(): void {
    document.documentElement.classList.remove(PENDING_CLASS);
  }

  private wrapper(nav: Navigation): HTMLElement | null {
    const element = document.querySelector<HTMLElement>('[data-vt-route]');
    return element?.dataset.vtNavigation === String(nav.id) &&
      page(element.dataset.vtRoute ?? '') === page(nav.entry.url)
      ? element
      : null;
  }
  private sharedElement(id: string, role: string, requireVisible = true): HTMLElement | undefined {
    const matches = [
      ...document.querySelectorAll<HTMLElement>('[data-vt-post-id][data-vt-role]'),
    ].filter(
      (element) =>
        element.dataset.vtPostId === id &&
        element.dataset.vtRole === role &&
        (!requireVisible || visible(element)),
    );
    return matches.length === 1 ? matches[0] : undefined;
  }
  private coverReady(element: HTMLElement): boolean {
    const image = element.querySelector<HTMLImageElement>('.blur-img-full');
    return !!image?.getAttribute('src') && image.complete && image.naturalWidth > 0;
  }
  private prepareCover(
    nav: Navigation,
    element: HTMLElement,
    changed: (ok: boolean) => void,
  ): void {
    const image = element.querySelector<HTMLImageElement>('.blur-img-full');
    if (!image) return;
    const ready = () => {
      if (nav.cancelled || this.navigation !== nav) return;
      if (image.naturalWidth > 0) {
        const box = image.closest<HTMLElement>('.blur-img');
        box?.style.setProperty('--ar', `${image.naturalWidth} / ${image.naturalHeight}`);
        box?.classList.add('is-loaded');
      }
      changed(image.naturalWidth > 0);
    };
    // IntersectionObserver may not run during the snapshot pause. Load only the selected cover.
    if (!image.getAttribute('src') && image.dataset.src) image.src = image.dataset.src;
    const failed = () => {
      if (!nav.cancelled && this.navigation === nav) changed(false);
    };
    if (typeof image.decode === 'function') void image.decode().then(ready, failed);
    else {
      image.addEventListener('load', ready, { once: true });
      image.addEventListener('error', failed, { once: true });
      nav.imageCleanups.push(() => {
        image.removeEventListener('load', ready);
        image.removeEventListener('error', failed);
      });
      if (image.complete && image.getAttribute('src')) {
        if (image.naturalWidth > 0) ready();
        else failed();
      }
    }
  }
  private name(nav: Navigation, element: HTMLElement, name: string): void {
    if (!nav.names.has(element)) nav.names.set(element, element.style.viewTransitionName);
    element.style.viewTransitionName = name;
  }
  // The pill slides in the live header: a captured snapshot would cover the nav text above it
  // and block clicks that interrupt the animation.
  private slidePill(nav: Navigation): void {
    const state = nav.pill;
    if (!state || nav.cancelled || this.navigation !== nav) return;
    const pill = document.querySelector<HTMLElement>(NAV_PILL);
    // Relay: the header has not switched sections yet; retry on the next call.
    if (state.source !== undefined && pill === state.source) return;
    // One slide per navigation: late and relayed destinations may render repeatedly.
    nav.pill = undefined;
    const motion = getComputedStyle(document.documentElement);
    // The destination has no pill (entering /search): fade out the old one.
    if (!pill) {
      if (state.from?.width) this.fadeOutPill(state.href, motion);
      return;
    }
    const to = pill.getBoundingClientRect();
    if (!to.width) return;
    // The source had no pill element (leaving /search): fade the new one in place. A mobile
    // pill exists with zero width, so `from` is defined and the check above already returned.
    if (!state.from) {
      pill.animate([{ opacity: 0 }, { opacity: 1 }], {
        duration: cssTime(motion.getPropertyValue('--spring-snappy-duration')),
        easing: motion.getPropertyValue('--ease-spring-snappy').trim(),
      });
      return;
    }
    const from = state.from;
    if (!from.width || (from.left === to.left && from.right === to.right)) return;
    pill.animate(
      [
        { left: `${from.left - to.left}px`, right: `${to.right - from.right}px` },
        { left: '0px', right: '0px' },
      ],
      {
        duration: cssTime(motion.getPropertyValue('--spring-snappy-duration')),
        easing: motion.getPropertyValue('--ease-spring-snappy').trim(),
      },
    );
  }
  // The old pill is gone after the update; fade a temporary copy inside the old live link.
  private fadeOutPill(href: string | undefined, motion: CSSStyleDeclaration): void {
    const item = [...document.querySelectorAll<HTMLElement>('.site-nav .nav-item')].find(
      (link) => link.getAttribute('href') === href,
    );
    if (!item || item.querySelector('.nav-indicator')) return;
    const ghost = document.createElement('span');
    ghost.className = 'nav-indicator';
    ghost.setAttribute('aria-hidden', 'true');
    item.append(ghost);
    const remove = () => ghost.remove();
    void ghost
      .animate([{ opacity: 1 }, { opacity: 0 }], {
        duration: 140,
        easing: motion.getPropertyValue('--ease-out-expo').trim(),
        fill: 'forwards',
      })
      .finished.then(remove, remove);
  }
  private assignShell(nav: Navigation): void {
    if (!nav.shell) return;
    for (const element of document.querySelectorAll<HTMLElement>('[data-vt-shell]')) {
      const key = element.dataset.vtShell;
      // Keep the section header live so another click can interrupt the animation.
      if (nav.sectionFrom !== undefined && key === 'frontend-header') continue;
      if (key === 'frontend-header' || key === 'frontend-footer' || key === 'admin-sidebar')
        this.name(nav, element, `vt-${key}`);
    }
  }
  private correctScroll(nav: Navigation): void {
    if (!nav.rendered || nav.scrollCancelled || !this.wrapper(nav)) return;
    const hash = new URL(nav.entry.url, location.origin).hash;
    // A fresh page only needs one top reset. Repeating scrollTo on every article
    // mutation forces expensive layouts while editors and images initialize.
    if (!nav.restore && !hash && nav.scrolled) return;
    restoreScroll(nav.restore, hash);
    nav.scrolled = true;
  }
  private startScrollRestoration(nav: Navigation): void {
    if (nav.scrollStarted) return;
    this.restoreCleanup?.();
    nav.scrollStarted = true;
    this.restoring = nav.restore !== undefined;
    if (!nav.restore && !new URL(nav.entry.url, location.origin).hash) return;
    this.restoreCleanup = stabilizeNavigationScroll(
      () => {
        if (this.navigation === nav && nav.committed) this.correctScroll(nav);
      },
      () => {
        nav.scrollCancelled = true;
        this.restoring = false;
        this.restoreCleanup = undefined;
      },
    );
  }
  private cleanNames(nav: Navigation): void {
    for (const [element, original] of nav.names) element.style.viewTransitionName = original;
    nav.names.clear();
    for (const cleanup of nav.imageCleanups.splice(0)) cleanup();
    nav.pressed?.removeAttribute('data-vt-pressed');
    nav.pressed = undefined;
    if (this.navigation === nav) {
      document.documentElement.classList.remove('vt-shared-title', 'vt-shared-cover');
      delete document.documentElement.dataset.vtSection;
      document.documentElement.style.removeProperty('--vt-section-old-x');
      document.documentElement.style.removeProperty('--vt-section-old-y');
      document.documentElement.style.removeProperty('--vt-section-clip-top');
    }
  }
  private cancel(nav: Navigation): void {
    if (nav.cancelled) return;
    nav.cancelled = true;
    nav.phase = undefined;
    nav.vt?.skipTransition();
    nav.release?.();
    this.cleanNames(nav);
    if (this.navigation === nav)
      document.documentElement.classList.remove('is-route-transitioning');
  }
  private finish(nav: Navigation): void {
    if (nav.cancelled || this.navigation !== nav) return;
    this.cleanNames(nav);
    document.documentElement.classList.remove('is-route-transitioning');
    nav.owner.finish();
    this.scheduleScan();
  }
}

export const routeTransitions = new RouteTransitions();
