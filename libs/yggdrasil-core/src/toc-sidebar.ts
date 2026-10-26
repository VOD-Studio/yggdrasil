/**
 * 侧边目录 scroll-spy：IntersectionObserver「探测带」追踪当前阅读节。
 *
 * 同时维护阅读进度：目录根（`[data-toc-root]`，没有时为 nav）上写 `--toc-progress`
 * （0–1），并把百分比与当前小节名写进根内的 `[data-toc-percent]` / `[data-toc-current]`。
 * 这两类元素在 Rust 端不渲染子节点，由这里独占文本。
 *
 * 幂等重入：调用方用以 slug 为 key 的单元素列表强制 remount PostToc，
 * PostToc 的 use_effect 每次挂载重调本函数——入口先 dispose 上一次的
 * observer 与激活态，再重新扫描。
 */

const BAND_ROOT_MARGIN = '-80px 0px -70% 0px'; // 带：视口顶 80px（避开 64px sticky header）~ 30vh

const DEFAULT_NAV = 'nav.toc-sidebar:not([id^="showcase-"])';
const disposers = new Map<string, () => void>();

export function disposeTocSidebar(navSelector = DEFAULT_NAV): void {
  disposers.get(navSelector)?.();
  disposers.delete(navSelector);
}

export function initTocSidebar(
  navSelector = DEFAULT_NAV,
  contentSelector?: string,
  scrollSelector?: string,
): void {
  disposeTocSidebar(navSelector);

  const nav = document.querySelector<HTMLElement>(navSelector);
  if (!nav) return;
  const body = nav.querySelector<HTMLElement>('.toc-sidebar-body');
  if (!body) return;
  const content = contentSelector ? document.querySelector<HTMLElement>(contentSelector) : null;
  const scrollRoot = scrollSelector ? document.querySelector<HTMLElement>(scrollSelector) : null;
  const root = nav.closest<HTMLElement>('[data-toc-root]') ?? nav;
  const stopProgress = trackProgress(
    root,
    content ?? document.querySelector<HTMLElement>('.post-content'),
    scrollRoot,
  );

  // 按文档序收集 (目录链接 → 标题元素) 对；href 是 #id，id 可能含 CJK，
  // getAttribute 返回原始字符，location.hash 才是百分号编码——这里 decodeURIComponent
  // 兜底两种形态（与 hash-scroll.ts 的双 fallback 同款）。
  const items: Array<{ id: string; el: HTMLElement; link: HTMLAnchorElement }> = [];
  for (const link of body.querySelectorAll<HTMLAnchorElement>('a[href^="#"]')) {
    const raw = link.getAttribute('href')!.slice(1);
    if (!raw) continue;
    const id = decodeURIComponent(raw);
    const el = content
      ? Array.from(content.querySelectorAll<HTMLElement>('[id]')).find((node) => node.id === id)
      : (document.getElementById(id) ?? document.getElementById(raw));
    if (el) items.push({ id: raw, el, link });
  }
  if (items.length === 0) {
    disposers.set(navSelector, stopProgress);
    return;
  }

  let activeLink: HTMLAnchorElement | null = null;

  const setActive = (link: HTMLAnchorElement | null): void => {
    if (link === activeLink) return;
    activeLink?.classList.remove('active');
    activeLink?.removeAttribute('aria-current');
    activeLink = link;
    for (const label of root.querySelectorAll('[data-toc-current]')) {
      label.textContent = link?.textContent?.trim() ?? '';
    }
    if (link) {
      link.classList.add('active');
      link.setAttribute('aria-current', 'true');
      // 目录列表自身滚动，让激活项保持可见；用户正悬浮浏览列表时不抢滚动。
      if (!nav.matches(':hover') && body.scrollHeight > body.clientHeight) {
        const top =
          link.getBoundingClientRect().top -
          body.getBoundingClientRect().top +
          body.scrollTop -
          body.clientHeight / 2;
        body.scrollTo({ top, behavior: 'smooth' });
      }
    }
  };

  // 初始激活：同步扫一遍，取「顶线（80px）之上最后一个标题」；全文在顶线之下则为 null。
  const pickAboveLine = (): HTMLAnchorElement | null => {
    const lineTop = scrollRoot ? scrollRoot.getBoundingClientRect().top + 20 : 80;
    let last: HTMLAnchorElement | null = null;
    for (const it of items) {
      if (it.el.getBoundingClientRect().top <= lineTop) last = it.link;
      else break; // 文档序单调，越过即停
    }
    return last;
  };
  setActive(pickAboveLine());

  if (typeof IntersectionObserver === 'undefined') {
    disposers.set(navSelector, () => {
      stopProgress();
      setActive(null);
    });
    return; // 旧环境：只留初始态
  }

  const visible = new Set<string>();
  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        const id = (e.target as HTMLElement).id;
        if (e.isIntersecting) visible.add(id);
        else visible.delete(id);
      }
      if (visible.size > 0) {
        // 带内可能有多个标题：取文档序最靠上的（items 即文档序）。
        const hit = items.find((it) => visible.has(it.el.id));
        setActive(hit ? hit.link : null);
      } else {
        // 带内无标题（处于长节中部）：保持/回退到顶线之上最后一个。
        setActive(pickAboveLine());
      }
    },
    {
      root: scrollRoot,
      rootMargin: scrollRoot ? '-20px 0px -70% 0px' : BAND_ROOT_MARGIN,
      threshold: 0,
    },
  );
  for (const it of items) io.observe(it.el);

  disposers.set(navSelector, () => {
    io.disconnect();
    stopProgress();
    setActive(null);
  });
}

/** 阅读进度：正文顶部到达视口顶线记 0，正文底部进入视口底部记 1。 */
export function readingProgress(
  contentTop: number,
  contentHeight: number,
  viewportHeight: number,
  lineTop = 80,
): number {
  const range = contentHeight - (viewportHeight - lineTop);
  if (range <= 0) return contentTop <= lineTop ? 1 : 0;
  return Math.min(1, Math.max(0, (lineTop - contentTop) / range));
}

/** 最近的可滚动祖先（overflow-y 为 auto/scroll/overlay）；没有时为 null，表示窗口滚动。 */
function findScrollParent(el: HTMLElement | null): HTMLElement | null {
  for (let node = el?.parentElement ?? null; node; node = node.parentElement) {
    if (node === document.body || node === document.documentElement) return null;
    if (/^(auto|scroll|overlay)$/.test(getComputedStyle(node).overflowY)) return node;
  }
  return null;
}

/**
 * 跟随滚动更新进度；没有正文元素时按整页计算。返回清理函数。
 * 未指定滚动容器时取正文最近的可滚动祖先：后台预览在 `.admin-scroll` 内滚动，
 * 只听窗口会让进度停在初始值。
 */
function trackProgress(
  root: HTMLElement,
  content: HTMLElement | null,
  explicitScrollRoot: HTMLElement | null,
): () => void {
  const scrollRoot = explicitScrollRoot ?? findScrollParent(content);
  const target: EventTarget = scrollRoot ?? window;
  let frame = 0;
  const update = (): void => {
    frame = 0;
    const viewportTop = scrollRoot ? scrollRoot.getBoundingClientRect().top : 0;
    const viewportHeight = scrollRoot ? scrollRoot.clientHeight : window.innerHeight;
    const rect = (content ?? document.documentElement).getBoundingClientRect();
    const value = readingProgress(rect.top - viewportTop, rect.height, viewportHeight);
    root.style.setProperty('--toc-progress', value.toFixed(4));
    const percent = `${Math.round(value * 100)}%`;
    for (const label of root.querySelectorAll('[data-toc-percent]')) {
      if (label.textContent !== percent) label.textContent = percent;
    }
  };
  const schedule = (): void => {
    if (!frame) frame = requestAnimationFrame(update);
  };
  update();
  target.addEventListener('scroll', schedule, { passive: true });
  window.addEventListener('resize', schedule, { passive: true });
  return () => {
    if (frame) cancelAnimationFrame(frame);
    target.removeEventListener('scroll', schedule);
    window.removeEventListener('resize', schedule);
  };
}
