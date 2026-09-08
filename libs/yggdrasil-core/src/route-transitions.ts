/** Browser history adapter for Dioxus. Only the VT update callback publishes a new route. */
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
const STATE_KEY = '__yggdrasilNavigation';
const MAX_ENTRIES = 40;

interface Entry {
  id: string;
  url: string;
  values: Map<string, string>;
  scroll?: ScrollSnapshot;
  origin?: { id: string; url: string; post: string };
}
interface LinkIntent {
  url: string;
  post?: string;
  returning: boolean;
}
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
  cancelled: boolean;
  owner: TransitionOwner;
  names: Map<HTMLElement, string>;
  oldRoles: Set<string>;
  shell: boolean;
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
  private intentTimer: number | undefined;
  private hydrated = false;
  private restoreCleanup: (() => void) | undefined;
  private restoring = false;
  private oldRestoration: ScrollRestoration = 'auto';

  connect(notify: () => void, prefix: string | null): void {
    if (this.notify) {
      this.notify = notify;
      return;
    }
    this.notify = notify;
    this.prefix = prefix?.replace(/\/$/, '') ?? '';
    this.displayed = this.locationRoute();
    this.active = this.newEntry(this.displayed);
    this.writeHistory(this.active, 'replace');
    this.oldRestoration = history.scrollRestoration;
    history.scrollRestoration = 'manual';
    window.addEventListener('popstate', this.onPop);
    window.addEventListener('hashchange', this.onHash);
    window.addEventListener('click', this.onClick, true);
    window.addEventListener('pagehide', this.onPageHide);
  }

  disconnect(): void {
    this.navigation?.owner.finish();
    if (this.navigation) this.cancel(this.navigation);
    this.restoreCleanup?.();
    clearTimeout(this.intentTimer);
    window.removeEventListener('popstate', this.onPop);
    window.removeEventListener('hashchange', this.onHash);
    window.removeEventListener('click', this.onClick, true);
    window.removeEventListener('pagehide', this.onPageHide);
    history.scrollRestoration = this.oldRestoration;
    this.notify = undefined;
    this.hydrated = false;
    this.navigation = undefined;
    this.entries.clear();
    this.active = undefined;
  }

  currentRoute = (): string => this.displayed;
  currentPrefix = (): string | null => this.prefix || null;
  navigationId = (): number => this.sequence;
  entryId = (): string => this.active?.id ?? '';
  isRestoring = (): boolean => this.restoring;
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

  /** Called after Dioxus flushes the layout's actual DOM, including fallback/error branches. */
  rendered = (id: number, route: string): void => {
    if (page(route) !== page(this.displayed) || id !== this.sequence) return;
    this.hydrated = true;
    const nav = this.navigation;
    if (!nav?.committed || nav.id !== id) return;
    nav.rendered = true;
    this.correctScroll(nav);
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
    const value = { ...state, [STATE_KEY]: entry.id };
    const url = this.prefix + entry.url;
    if (kind === 'push') history.pushState(value, '', url);
    else history.replaceState(value, '', url);
  }

  private onClick = (event: MouseEvent): void => {
    clearTimeout(this.intentTimer);
    this.intent = undefined;
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey)
      return;
    const anchor =
      event.target instanceof Element ? event.target.closest<HTMLAnchorElement>('a[href]') : null;
    if (
      !anchor ||
      anchor.target === '_blank' ||
      anchor.hasAttribute('download') ||
      anchor.origin !== location.origin
    )
      return;
    const url = anchor.pathname.slice(this.prefix.length) + anchor.search + anchor.hash;
    const intent = {
      url,
      post: anchor.dataset.vtPostLink,
      returning: anchor.dataset.vtReturn === 'true',
    };
    this.intent = intent;
    // Native event dispatch can run a microtask checkpoint between capture and
    // Dioxus's delegated listener. Keep the intent through the entire event task.
    this.intentTimer = window.setTimeout(() => {
      if (this.intent === intent) this.intent = undefined;
    }, 0);
  };
  private onPageHide = (): void => {
    if (this.active) this.active.scroll = captureScroll();
    this.restoreCleanup?.();
    if (this.navigation) this.cancel(this.navigation);
  };
  private onHash = (): void => {
    if (this.locationRoute() === this.displayed) return;
    if (!this.navigation || this.navigation.cancelled || this.navigation.committed) this.syncHash();
  };
  private onPop = (): void => {
    const route = this.locationRoute();
    if (this.active) this.active.scroll = captureScroll();
    const id: unknown = history.state?.[STATE_KEY];
    const entry = typeof id === 'string' ? this.entries.get(id) : undefined;
    const target = entry ?? this.newEntry(route);
    target.url = route;
    if (!entry) this.writeHistory(target, 'replace');
    this.start(target, 'pop', target.scroll);
  };

  private request(route: string, kind: 'push' | 'replace'): void {
    if (!this.notify || !this.active) return;
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
    clearTimeout(this.intentTimer);
    const origin = this.active.origin;
    const source =
      intent?.returning && origin && page(origin.url) === page(target)
        ? this.entries.get(origin.id)
        : undefined;
    const entry = this.newEntry(target, source);
    if (page(target) === page(this.displayed)) {
      // Fragment navigation keeps the page mounted, including its captured entry ID.
      // Share live page state while retaining independent history/scroll positions.
      entry.values = this.active.values;
      entry.origin = this.active.origin;
    }
    if (intent?.post) entry.origin = { id: this.active.id, url: this.displayed, post: intent.post };
    this.start(entry, kind, source?.scroll);
  }

  private start(entry: Entry, kind: Navigation['kind'], restore?: ScrollSnapshot): void {
    if (!this.active || !this.notify) return;
    this.restoreCleanup?.();
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
    const animate =
      this.hydrated &&
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
    this.assignShell(nav);
    if (nav.post) {
      for (const role of ['title', 'cover']) {
        const element = this.sharedElement(nav.post, role);
        if (element && (role !== 'cover' || this.coverReady(element))) {
          this.name(nav, element, `vt-post-${role}`);
          nav.oldRoles.add(role);
        }
      }
    }
    try {
      const vt = document.startViewTransition(async () => {
        if (!owner.isCurrent() || nav.cancelled) return;
        this.commit(nav);
        await this.waitForDestination(nav);
      });
      nav.vt = vt;
      owner.attach(vt);
      void vt.ready.catch(() => {});
      void vt.updateCallbackDone?.catch(() => {});
      void vt.finished.then(
        () => this.finish(nav),
        () => this.finish(nav),
      );
    } catch {
      this.cleanNames(nav);
      document.documentElement.classList.remove('is-route-transitioning');
      this.commit(nav);
      this.startScrollRestoration(nav);
      owner.finish();
    }
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
      const timer = window.setTimeout(complete, Math.max(0, nav.deadline - performance.now()));
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
  private assignShell(nav: Navigation): void {
    if (!nav.shell) return;
    for (const element of document.querySelectorAll<HTMLElement>('[data-vt-shell]')) {
      const key = element.dataset.vtShell;
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
    if (this.navigation === nav)
      document.documentElement.classList.remove('vt-shared-title', 'vt-shared-cover');
  }
  private cancel(nav: Navigation): void {
    if (nav.cancelled) return;
    nav.cancelled = true;
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
  }
}

export const routeTransitions = new RouteTransitions();
