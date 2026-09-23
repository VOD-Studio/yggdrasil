/**
 * 侧边目录 scroll-spy：IntersectionObserver「探测带」追踪当前阅读节。
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
  if (items.length === 0) return;

  let activeLink: HTMLAnchorElement | null = null;

  const setActive = (link: HTMLAnchorElement | null): void => {
    if (link === activeLink) return;
    activeLink?.classList.remove('active');
    activeLink?.removeAttribute('aria-current');
    activeLink = link;
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
    disposers.set(navSelector, () => setActive(null));
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
    setActive(null);
  });
}
