#!/usr/bin/env bun
/**
 * Native Chromium regression checks against a running, populated development site.
 * 不写入站点内容；慢响应和后台场景使用浏览器隔离的临时响应。
 *
 * PLAYWRIGHT_MODULE=/path/to/playwright CHROMIUM_PATH=/usr/bin/chromium \
 *   bun scripts/test-view-transitions.ts
 *   bun scripts/test-view-transitions.ts share
 * Optional: VT_BASE, VT_SEARCH_QUERY, VT_TAG_PATH, VT_TOC_POST_PATH, VT_DEBUG=1.
 * Supply VT_ADMIN_USERNAME and VT_ADMIN_PASSWORD to include login/preview/logout.
 */
import assert from 'node:assert/strict';
import type * as Playwright from 'playwright';

const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright') as typeof Playwright;
const BASE = (process.env.VT_BASE || 'http://127.0.0.1:8080').replace(/\/$/, '');
const SEARCH_QUERY = process.env.VT_SEARCH_QUERY || 'Rust';
const TAG_PATH = process.env.VT_TAG_PATH || '/tags/Rust';
const TOC_POST_PATH = process.env.VT_TOC_POST_PATH;

export interface RouteTransitions {
  push(route: string): void;
  entryId(): string;
  navigationId(): number;
  currentRoute(): string;
  readState(key: string): string | null;
  entries: Map<string, { values: Map<string, string> }>;
}
interface TransitionName {
  name: string;
  role?: string;
  id?: string;
}
interface TransitionCall {
  started: number;
  from: string;
  oldNames: TransitionName[];
  kind: string;
  callbackAt?: number;
  updateDoneAt?: number;
  to?: string;
  newNames?: TransitionName[];
  readyAt?: number;
  readyError?: string;
  finishedAt?: number;
  finishError?: string;
}
interface TransitionFrame {
  pending: boolean;
  old: number | null;
  events: string | null;
  slot: { display: boolean; opacity: number } | null;
  skeletonInView: boolean;
  detail: boolean;
}
interface LaunchOptions {
  viewport?: { width: number; height: number };
  reducedMotion?: 'reduce' | 'no-preference';
}
interface InstrumentOptions {
  unsupported?: boolean;
}
export interface BrowserState {
  browser: Playwright.Browser;
  context: Playwright.BrowserContext;
  page: Playwright.Page;
  errors: string[];
  consoleErrors: string[];
  failedRequests: { url: string; error: string | undefined }[];
  httpErrors: { url: string; status: number; body: string }[];
}
interface AdminPost {
  id: number;
  status: string;
  title: string;
}
interface NavigationHistoryState {
  __yggdrasilNavigation: string;
}
interface AdminPostsState {
  page: number;
  status: string;
  search_query: string;
  search_input: string;
}
declare global {
  var __routeTransitions: RouteTransitions;
  var __vtCalls: TransitionCall[];
  var __vtDocument: string;
  var __vtFrames: TransitionFrame[];
}

async function launch(options: LaunchOptions = {}): Promise<BrowserState> {
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
    args: ['--no-sandbox'],
  });
  const context = await browser.newContext({
    viewport: options.viewport || { width: 1440, height: 1000 },
    reducedMotion: options.reducedMotion || 'no-preference',
  });
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  // The dev toast imports Google Fonts; keep network variability out of navigation checks.
  await page.route('https://fonts.googleapis.com/**', (route) =>
    route.fulfill({ status: 200, contentType: 'text/css', body: '' }),
  );
  const errors: string[] = [];
  const consoleErrors: string[] = [];
  const failedRequests: { url: string; error: string | undefined }[] = [];
  const httpErrors: { url: string; status: number; body: string }[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') consoleErrors.push(message.text());
  });
  page.on('response', async (response) => {
    if (response.status() >= 400)
      httpErrors.push({
        url: response.url(),
        status: response.status(),
        body: (await response.text().catch(() => '')).slice(0, 600),
      });
  });
  page.on('requestfailed', (request) =>
    failedRequests.push({
      url: request.url(),
      error: request.failure()?.errorText,
    }),
  );
  return { browser, context, page, errors, consoleErrors, failedRequests, httpErrors };
}

async function snapshot(page: Playwright.Page) {
  return page.evaluate(() => ({
    url: location.href,
    title: document.title,
    heading: document.querySelector('main h1')?.textContent,
    startViewTransition: typeof document.startViewTransition,
    core: typeof window.__routeTransitions,
    historyState: history.state,
    readyState: document.readyState,
    shells: Array.from(
      document.querySelectorAll<HTMLElement>('[data-vt-shell]'),
      (e) => e.dataset.vtShell,
    ),
    details: Array.from(
      document.querySelectorAll<HTMLElement>('[data-vt-detail]'),
      (e) => e.dataset.vtDetail,
    ),
    listReady: Boolean(document.querySelector('[data-vt-list="true"]')),
    listPending: Boolean(document.querySelector('[data-vt-list-pending="true"]')),
    posts: Array.from(document.querySelectorAll<HTMLAnchorElement>('a[data-vt-post-link]'))
      .slice(0, 12)
      .map((link) => {
        const id = link.dataset.vtPostLink;
        const title = document.querySelector(`[data-vt-post-id="${id}"][data-vt-role="title"]`);
        const cover = document.querySelector<HTMLImageElement>(
          `[data-vt-post-id="${id}"][data-vt-role="cover"] img.blur-img-full`,
        );
        return {
          id,
          href: link.getAttribute('href'),
          title: title?.textContent?.trim(),
          cover: cover?.getAttribute('src'),
          coverReady: Boolean(cover?.complete && cover?.naturalWidth > 0),
        };
      }),
    tags: Array.from(document.querySelectorAll('a[href^="/tags/"]'))
      .slice(0, 12)
      .map((e) => e.getAttribute('href')),
    wasm: (performance.getEntriesByType('resource') as PerformanceResourceTiming[])
      .filter((r) => r.name.includes('.wasm'))
      .map((r) => ({
        url: r.name,
        duration: r.duration,
        size: r.transferSize,
      })),
  }));
}

async function instrument(page: Playwright.Page, { unsupported = false }: InstrumentOptions = {}) {
  await page.addInitScript(
    ({ unsupported }) => {
      window.__vtCalls = [];
      window.__vtDocument = Math.random().toString(36);
      if (unsupported) {
        Object.defineProperty(document, 'startViewTransition', {
          configurable: true,
          value: undefined,
        });
        return;
      }
      const start = document.startViewTransition?.bind(document);
      if (!start) return;
      const names = () =>
        Array.from(document.querySelectorAll<HTMLElement>('[data-vt-role], [data-vt-shell]'))
          .filter((e) => e.style.viewTransitionName)
          .map((e) => ({
            name: e.style.viewTransitionName,
            role: e.dataset.vtRole,
            id: e.dataset.vtPostId,
          }));
      document.startViewTransition = (update) => {
        const call: TransitionCall = {
          started: performance.now(),
          from: location.pathname,
          oldNames: names(),
          kind: document.documentElement.className,
        };
        window.__vtCalls.push(call);
        const transition = start(async () => {
          call.callbackAt = performance.now();
          if (typeof update === 'function') await update();
          else await update?.update?.();
          call.updateDoneAt = performance.now();
          call.to = location.pathname;
          call.newNames = names();
        });
        transition.ready.then(
          () => {
            call.readyAt = performance.now();
          },
          (error) => {
            call.readyError = error.name;
          },
        );
        transition.finished.then(
          () => {
            call.finishedAt = performance.now();
          },
          (error) => {
            call.finishError = error.name;
          },
        );
        return transition;
      };
    },
    { unsupported },
  );
}
async function settle(page: Playwright.Page) {
  await page.waitForFunction(
    () =>
      !document.documentElement.classList.contains('is-route-transitioning') &&
      !document.documentElement.classList.contains('is-theme-transitioning'),
  );
  await page.waitForTimeout(80);
}
async function ready(page: Playwright.Page, route: string) {
  await page.waitForURL(`${BASE}${route}`);
  await page.waitForFunction(
    (route) => document.querySelector<HTMLElement>('[data-vt-route]')?.dataset.vtRoute === route,
    route,
  );
  await settle(page);
}
async function push(page: Playwright.Page, route: string) {
  await page.evaluate((route) => window.__routeTransitions.push(route), route);
  await ready(page, route);
}
async function fresh(state: BrowserState, options: InstrumentOptions = {}) {
  await instrument(state.page, options);
  await state.page.goto(`${BASE}/`, { waitUntil: 'domcontentloaded', timeout: 90000 });
  await state.page.waitForFunction(
    () => window.__routeTransitions?.entryId() && document.querySelector('a[data-vt-post-link]'),
    null,
    { timeout: 90000 },
  );
  await settle(state.page);
}
function traceSummary(calls: TransitionCall[]) {
  return calls.map((c) => ({
    from: c.from,
    to: c.to,
    old: c.oldNames.map((n) => n.name),
    next: c.newNames?.map((n) => n.name),
    snapshotDelay: Math.round(c.callbackAt! - c.started),
    callbackWait: Math.round(c.updateDoneAt! - c.callbackAt!),
    wait: Math.round(c.updateDoneAt! - c.started),
    readyError: c.readyError,
  }));
}
async function titleRoundTrip(
  page: Playwright.Page,
  selector = 'a[data-vt-post-link]',
  returnViaHome = false,
) {
  const link = page.locator(selector).first();
  const id = await link.getAttribute('data-vt-post-link');
  const href = await link.getAttribute('href');
  assert(href, 'post link must have a destination');
  const title = page.locator(`[data-vt-post-id="${id}"][data-vt-role="title"]`);
  await title.evaluate((e) => e.scrollIntoView({ block: 'center', behavior: 'instant' }));
  await page.waitForTimeout(200);
  await settle(page);
  const before = await page.evaluate(() => ({
    entry: (history.state as NavigationHistoryState).__yggdrasilNavigation,
    y: scrollY,
    url: location.href,
    count: __vtCalls.length,
    document: __vtDocument,
  }));
  const titleBox = await title.boundingBox();
  assert(titleBox, 'post title must have a layout box');
  await page.mouse.click(
    titleBox.x + Math.min(titleBox.width / 2, 50),
    titleBox.y + titleBox.height / 2,
  );
  await ready(page, href);
  await page.waitForSelector(`[data-vt-detail="${id}"]`);
  assert.equal(
    await page.evaluate(() => __vtDocument),
    before.document,
    'navigation must retain document',
  );
  const forward = await page.evaluate((i) => __vtCalls[i], before.count);
  assert(
    forward?.oldNames.some((n) => n.name === 'vt-post-title'),
    'source title must be named',
  );
  assert(
    forward?.newNames?.some((n) => n.name === 'vt-post-title'),
    'destination title must be named',
  );
  // 首次进入文章页时分包冷加载会超出 300ms 预算：协调器跳过空过渡，由 chunk 场景覆盖。
  const late =
    forward.readyError === 'AbortError' && forward.updateDoneAt! - forward.started >= 250;
  assert(late || !forward.readyError, `forward transition rejected: ${forward.readyError}`);
  if (returnViaHome) await page.getByRole('link', { name: '文章', exact: true }).click();
  else await page.goBack();
  await ready(page, new URL(before.url).pathname);
  await page.waitForSelector(selector);
  const after = await page.evaluate(() => ({
    entry: (history.state as NavigationHistoryState).__yggdrasilNavigation,
    y: scrollY,
    call: __vtCalls.at(-1),
  }));
  assert(after.call, 'return navigation must start a transition');
  if (returnViaHome) {
    assert.notEqual(after.entry, before.entry, 'Home creates a new history entry');
    assert.equal(await page.evaluate(() => __vtDocument), before.document, 'Home retains document');
    assert(
      after.call.oldNames.some((n) => n.name === 'vt-post-title'),
      'Home source title must be named',
    );
  } else assert.equal(after.entry, before.entry, 'back restores original history entry');
  assert(
    Math.abs(after.y - before.y) < 5,
    `scroll restored: expected ${before.y}, actual ${after.y}`,
  );
  assert(
    after.call.newNames?.some((n) => n.name === 'vt-post-title'),
    'reverse target title must be named',
  );
  assert(!after.call.readyError, `reverse transition rejected: ${after.call.readyError}`);
  return { id, href, scroll: before.y };
}
async function main() {
  const state = await launch();
  const { page, browser } = state;
  try {
    await fresh(state);
    console.log('PASS home-detail-back', JSON.stringify(await titleRoundTrip(page)));
    console.log(
      'PASS home-detail-breadcrumb',
      JSON.stringify(await titleRoundTrip(page, undefined, true)),
    );
    await push(page, '/search');
    await page.locator('#article-search').fill(SEARCH_QUERY);
    await page.locator('#article-search').press('Enter');
    await page.waitForSelector('a.search-result');
    const beforeTitles = await page.locator('.search-result-title').allTextContents();
    console.log(
      'PASS search-detail-back',
      JSON.stringify(await titleRoundTrip(page, 'a.search-result')),
    );
    assert.equal(await page.locator('#article-search').inputValue(), SEARCH_QUERY);
    assert.deepEqual(await page.locator('.search-result-title').allTextContents(), beforeTitles);
    console.log('PASS search-query-results-restored', beforeTitles.length);
    await push(page, TAG_PATH);
    assert.equal(await page.locator('.archive-tags .chip[aria-current="page"]').count(), 1);
    console.log(
      'PASS tag-detail-back',
      JSON.stringify(await titleRoundTrip(page, '.archive-entry')),
    );
    await push(page, '/archives');
    // 标签多于 16 个时才有展开按钮；展开状态随历史记录恢复。
    const toggle = page.locator('.archive-tags-toggle');
    const canExpand = (await toggle.count()) > 0;
    if (canExpand) {
      await toggle.click();
      assert.equal(await toggle.getAttribute('aria-expanded'), 'true');
    }
    console.log(
      'PASS archive-detail-back',
      JSON.stringify(await titleRoundTrip(page, '.archive-entry')),
    );
    if (canExpand)
      assert.equal(
        await page.locator('.archive-tags-toggle').getAttribute('aria-expanded'),
        'true',
      );
    console.log('PASS archive-open-restored');
    if (TOC_POST_PATH) await push(page, TOC_POST_PATH);
    else {
      await page.locator('.archive-entry').first().click();
      await page.waitForSelector('[data-vt-detail]');
      await settle(page);
    }
    const anchorBefore = await page.evaluate(() => ({
      entry: (history.state as NavigationHistoryState).__yggdrasilNavigation,
      count: __vtCalls.length,
    }));
    const hash = await page.locator('.toc-sidebar a[href^="#"]').first().getAttribute('href');
    await page
      .locator('.toc-sidebar a[href^="#"]')
      .first()
      .evaluate((a) => (a as HTMLAnchorElement).click());
    await page.waitForFunction((hash) => location.hash === hash, hash);
    assert.equal(
      await page.evaluate(() => (history.state as NavigationHistoryState).__yggdrasilNavigation),
      anchorBefore.entry,
    );
    assert.equal(await page.evaluate(() => __vtCalls.length), anchorBefore.count);
    assert.equal(
      await page.evaluate(() => window.__routeTransitions.currentRoute()),
      await page.evaluate(() => location.pathname + location.hash),
    );
    console.log('PASS hash-preserves-history-and-skips-animation', hash);
    await page.evaluate(() => {
      __routeTransitions.push('/about');
      __routeTransitions.push('/archives');
      __routeTransitions.push('/search');
    });
    await ready(page, '/search');
    assert.equal(await page.locator('#article-search').count(), 1);
    console.log('PASS rapid-programmatic-navigation');
    await page.evaluate(() => {
      __routeTransitions.push('/about');
      // 点与当前明暗相反的一段，确保触发圆形扩散。
      const target = document.documentElement.classList.contains('dark') ? 'light' : 'dark';
      document
        .querySelector<HTMLElement>(
          `[data-vt-shell="frontend-header"] .theme-seg [aria-label="${target === 'dark' ? '深色' : '浅色'}"]`,
        )!
        .click();
    });
    await ready(page, '/about');
    console.log('PASS navigation-theme-race');
    assert.equal(
      await page.locator('[style*="view-transition-name: vt-"]').count(),
      0,
      'temporary shared names cleaned',
    );
    assert.deepEqual(state.errors, [], 'no JS errors');
    if (process.env.VT_DEBUG)
      console.log('TRACES', JSON.stringify(traceSummary(await page.evaluate(() => __vtCalls))));
  } catch (error) {
    console.error('FAIL', error instanceof Error ? error.message : error);
    console.log(
      'FAILURE',
      JSON.stringify({
        snapshot: await snapshot(page).catch(() => null),
        traces: traceSummary(await page.evaluate(() => __vtCalls).catch(() => [])),
        pageErrors: state.errors,
        httpErrors: state.httpErrors,
      }),
    );
    throw error;
  } finally {
    await browser.close();
  }
}
async function fallback({
  name,
  ...options
}: LaunchOptions & InstrumentOptions & { name: string }) {
  const state = await launch(options);
  try {
    await fresh(state, options);
    const initial = await state.page.evaluate(() => __vtCalls.length);
    await state.page.locator('a[data-vt-post-link]').first().click();
    await state.page.waitForSelector('[data-vt-detail]');
    await settle(state.page);
    assert.equal(await state.page.evaluate(() => __vtCalls.length), initial, name);
    await state.page.goBack();
    await state.page.waitForSelector('a[data-vt-post-link]');
    await settle(state.page);
    await state.page.locator('a[data-vt-post-link]').first().click();
    await state.page.waitForSelector('[data-vt-detail]');
    await state.page.getByRole('link', { name: '文章', exact: true }).click();
    await ready(state.page, '/');
    assert.equal(await state.page.evaluate(() => __vtCalls.length), initial, `${name} Home return`);
    assert.deepEqual(state.errors, []);
    console.log(`PASS ${name}`);
  } finally {
    await state.browser.close();
  }
}
async function mobile() {
  const state = await launch({ viewport: { width: 390, height: 844 } });
  try {
    await fresh(state);
    console.log('PASS mobile-title-roundtrip', JSON.stringify(await titleRoundTrip(state.page)));
    console.log(
      'PASS mobile-breadcrumb-roundtrip',
      JSON.stringify(await titleRoundTrip(state.page, undefined, true)),
    );
    assert.deepEqual(state.errors, []);
  } finally {
    await state.browser.close();
  }
}

const SHARE_META = [
  'name="description"',
  'property="og:description"',
  'property="og:type"',
  'property="og:url"',
  'property="og:image"',
];
async function headMeta(page: Playwright.Page) {
  return page.evaluate(
    (selectors) =>
      Object.fromEntries(
        selectors.map((sel) => [
          sel,
          Array.from(document.head.querySelectorAll(`meta[${sel}]`), (n) =>
            n.getAttribute('content'),
          ),
        ]),
      ),
    SHARE_META,
  );
}
async function shareMeta() {
  const state = await launch();
  const { page, browser } = state;
  try {
    await fresh(state);
    const home = await headMeta(page);
    for (const selector of SHARE_META)
      assert.equal(home[selector].length, 1, `home must have exactly one ${selector}`);
    assert(home['property="og:image"'][0], 'site og:image must be non-empty');
    const documentId = await page.evaluate(() => __vtDocument);
    const link = page.locator('a[data-vt-post-link]').first();
    const href = await link.getAttribute('href');
    assert(href, 'post link must have a destination');
    await link.click();
    await ready(page, href);
    await page.waitForSelector('[data-vt-detail]');
    const postSpa = await headMeta(page);
    await page.getByRole('link', { name: '文章', exact: true }).click();
    await ready(page, '/');
    assert.equal(
      await page.evaluate(() => __vtDocument),
      documentId,
      'share metadata roundtrip must retain document',
    );
    assert.deepEqual(
      await headMeta(page),
      home,
      'home head after SPA return must match direct load',
    );
    await page.goto(`${BASE}${href}`, { waitUntil: 'domcontentloaded' });
    await page.waitForSelector('[data-vt-detail]');
    for (const selector of SHARE_META)
      assert.equal(postSpa[selector].length, 1, `post must have exactly one ${selector}`);
    assert.deepEqual(
      postSpa,
      await headMeta(page),
      'post head after SPA navigation must match direct load',
    );
    assert.deepEqual(state.errors, []);
    console.log(
      'PASS share-meta-spa',
      JSON.stringify({ href, ogUrl: postSpa['property="og:url"'] }),
    );
  } finally {
    await browser.close();
  }
}

async function slowResponse() {
  const state = await launch();
  const { page, browser } = state;
  try {
    await page.route('**/api/get_public_post_by_slug*', async (route) => {
      await new Promise((resolve) => setTimeout(resolve, 800));
      await route.continue();
    });
    await fresh(state);
    await page.locator('a[data-vt-post-link]').first().click();
    await page.waitForFunction(() => __vtCalls.at(-1)?.updateDoneAt);
    const early = await page.evaluate(() => ({
      trace: __vtCalls.at(-1),
      detail: Boolean(document.querySelector('[data-vt-detail]')),
    }));
    assert(!early.detail, 'slow response must show the loading view at deadline');
    assert(early.trace, 'slow response must start a transition');
    assert(
      early.trace.updateDoneAt! - early.trace.started < 450,
      'slow response exceeds wait budget',
    );
    await page.waitForSelector('[data-vt-detail]');
    await settle(page);
    assert.equal(await page.evaluate(() => __vtCalls.length), 1, 'late content must not replay');
    assert.deepEqual(state.errors, []);
    console.log('PASS 300ms-response-budget-no-replay');
  } finally {
    await browser.close();
  }
}

// 悬停即预取目标路由分包与页面数据：点击时命中缓存，赶上 300ms 等待窗口且不重复请求。
async function prefetch() {
  const state = await launch();
  const { page, browser } = state;
  try {
    await fresh(state);
    const apiRequests: string[] = [];
    page.on('request', (r) => {
      if (r.url().includes('/api/get_public_post_by_slug')) apiRequests.push(r.url());
    });
    await page.route('**/api/get_public_post_by_slug*', async (route) => {
      await new Promise((resolve) => setTimeout(resolve, 800));
      await route.continue();
    });
    const link = page.locator('a[data-vt-post-link]').first();
    await link.evaluate((e) => e.scrollIntoView({ block: 'center', behavior: 'instant' }));
    const chunk = page.waitForResponse((r) => /split___post_route_view_.*\.wasm/.test(r.url()));
    const data = page.waitForResponse((r) => r.url().includes('/api/get_public_post_by_slug'));
    await link.hover();
    await Promise.all([chunk, data]); // 悬停即拿到分包与数据
    const before = await page.evaluate(() => __vtCalls.length);
    await link.click();
    await page.waitForFunction((i) => __vtCalls[i]?.updateDoneAt, before);
    const early = await page.evaluate(
      (i) => ({
        trace: __vtCalls[i],
        detail: Boolean(document.querySelector('[data-vt-detail]')),
      }),
      before,
    );
    assert(early.detail, '预取命中后应在等待窗口内渲染详情');
    assert(
      early.trace.newNames?.some((n) => n.name === 'vt-post-title'),
      '共享标题过渡应保留',
    );
    assert.equal(apiRequests.length, 1, '点击后不应再次请求文章数据');
    await page.waitForSelector('[data-vt-detail]');
    assert.deepEqual(state.errors, []);
    console.log('PASS hover-prefetch-chunk-and-data');
  } finally {
    await browser.close();
  }
}

// 目标页分包超出 300ms 预算：协调器跳过空过渡，旧正文淡出，新页进场且不重播。
async function delayWasm(page: Playwright.Page, ms: number) {
  await page.route('**/pkg/**', async (route) => {
    if (!route.request().url().split('?')[0].endsWith('.wasm')) return route.continue();
    await new Promise((resolve) => setTimeout(resolve, ms));
    await route.continue().catch(() => {});
  });
}
async function chunkLate() {
  const state = await launch();
  const { page, browser } = state;
  try {
    await fresh(state);
    await delayWasm(page, 1200);
    await page.evaluate(() => {
      const frames: TransitionFrame[] = [];
      window.__vtFrames = frames;
      const inView = (el: Element) => {
        if (el.getClientRects().length === 0) return false;
        const rect = el.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0 && rect.bottom > 0 && rect.top < innerHeight;
      };
      const sample = () => {
        const main = document.querySelector('main');
        // 路由挂起时旧正文是 main 中非 .route-pending 的直接子元素，骨架槽位是 .route-pending。
        const old = main
          ? Array.from(main.children).filter((el) => !el.classList.contains('route-pending'))
          : [];
        const slotEl = main?.querySelector(':scope > .route-pending');
        const slotStyle = slotEl && getComputedStyle(slotEl);
        frames.push({
          pending: document.documentElement.classList.contains('is-route-pending'),
          old: old.length
            ? Math.max(...old.map((el) => Number(getComputedStyle(el).opacity)))
            : null,
          events: old.length ? getComputedStyle(old[0]).pointerEvents : null,
          slot: slotStyle
            ? { display: slotStyle.display !== 'none', opacity: Number(slotStyle.opacity) }
            : null,
          skeletonInView: Array.from(document.querySelectorAll('main .skeleton')).some(inView),
          detail: Boolean(document.querySelector('[data-vt-detail]')),
        });
        requestAnimationFrame(sample);
      };
      requestAnimationFrame(sample);
    });
    const calls = await page.evaluate(() => __vtCalls.length);
    await page.locator('a[data-vt-post-link]').first().click();
    await page.waitForSelector('[data-vt-detail]', { timeout: 30000 });
    await settle(page);
    const result = await page.evaluate(
      (count) => ({
        calls: __vtCalls.slice(count),
        total: __vtCalls.length,
        frames: __vtFrames,
        pending: document.documentElement.classList.contains('is-route-pending'),
        opacity: Number(getComputedStyle(document.querySelector('main')!).opacity),
        slots: document.querySelectorAll('main > .route-pending').length,
        bodyOpacity: Math.min(
          ...Array.from(document.querySelectorAll('main > :not(.route-pending)'), (el) =>
            Number(getComputedStyle(el).opacity),
          ),
        ),
        handled: Boolean(document.querySelector('[data-vt-enter-handled]')),
      }),
      calls,
    );
    const [call] = result.calls;
    assert.equal(result.calls.length, 1, 'late chunk must start exactly one transition');
    assert(call.updateDoneAt! - call.started < 450, 'wait budget exceeded');
    assert.equal(call.readyError, 'AbortError', 'empty transition must be skipped');
    const detail = result.frames.findIndex((frame) => frame.detail);
    assert(detail > 0, 'detail frame not sampled');
    const shown = result.frames.findIndex(
      (frame) =>
        frame.pending && frame.slot?.display && frame.slot.opacity === 1 && frame.skeletonInView,
    );
    assert(
      shown >= 0 && shown < detail,
      'route skeleton must appear in the viewport before the new page',
    );
    assert(
      result.frames
        .slice(shown, detail)
        .filter((frame) => frame.pending && frame.slot?.display)
        .every((frame) => frame.old !== null && frame.old <= 0.05),
      'old body must be gone while the route skeleton is shown',
    );
    assert(
      result.frames.some((frame) => frame.pending && frame.events === 'none'),
      'old body must stop taking pointer events while fading',
    );
    // 从骨架出现到新页面正文出现之间不能有空白帧：覆盖分包未到与分包已到、数据未到两段。
    const blank = result.frames.slice(shown, detail).findIndex((frame) => !frame.skeletonInView);
    assert.equal(
      blank,
      -1,
      `blank frame ${blank} between the route skeleton and the page: ${JSON.stringify(result.frames.slice(shown + blank - 1, shown + blank + 2))}`,
    );
    assert.equal(result.pending, false, 'pending class must be cleared');
    assert.equal(result.slots, 0, 'route skeleton slot must be removed after commit');
    assert.equal(result.opacity, 1, 'new page must be visible');
    assert.equal(result.bodyOpacity, 1, 'new page body must be visible');
    assert.equal(result.total, calls + 1, 'entry must not replay');
    assert.equal(result.handled, false, 'entry animation must stay enabled');
    assert.deepEqual(state.errors, []);
    console.log('PASS chunk-late-fade-out');
  } finally {
    await browser.close();
  }

  await chunkLateScrolled();
  await chunkInterrupted('back');
  await chunkInterrupted('push');
}

// 旧页面已滚到页底时，路由骨架仍须在视口内（sticky 槽位贴在页头下方）。
async function chunkLateScrolled() {
  const state = await launch();
  const { page, browser } = state;
  try {
    await fresh(state);
    await delayWasm(page, 1200);
    await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
    await page.evaluate(() => window.__routeTransitions.push('/changelog'));
    await page.waitForFunction(() =>
      document.documentElement.classList.contains('is-route-pending'),
    );
    await page.waitForTimeout(200);
    const slot = await page.evaluate(() => {
      const el = document.querySelector('main > .route-pending');
      const inView = (node: Element) => {
        if (node.getClientRects().length === 0) return false;
        const rect = node.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0 && rect.bottom > 0 && rect.top < innerHeight;
      };
      return {
        top: el?.getBoundingClientRect().top,
        innerHeight,
        display: el && getComputedStyle(el).display,
        skeletonInView: Array.from(document.querySelectorAll('main .skeleton')).some(inView),
      };
    });
    assert.notEqual(slot.display, 'none', 'route skeleton slot must be displayed');
    assert(
      slot.top !== undefined && slot.top >= 0 && slot.top <= slot.innerHeight / 2,
      `route skeleton slot must stay near the viewport top: ${slot.top}`,
    );
    assert(
      slot.skeletonInView,
      'route skeleton must be visible after the old page was scrolled down',
    );
    await page.waitForFunction(
      () => document.querySelector('main h1')?.textContent === '更新日志',
      null,
      { timeout: 30000 },
    );
    await settle(page);
    await page.waitForFunction(
      () => !document.documentElement.classList.contains('is-route-pending'),
    );
    assert.deepEqual(state.errors, []);
    console.log('PASS chunk-late-skeleton-in-view');
  } finally {
    await browser.close();
  }
}

// 用请求屏障保证导航发生在目标视图分包尚未加载时，不能靠固定延迟碰运气。
async function chunkInterrupted(kind: 'back' | 'push') {
  const state = await launch();
  const { page, browser } = state;
  const { promise: blocked, resolve: release } = Promise.withResolvers<void>();
  const chunk = /\/pkg\/split___changelog_route_view_.*\.wasm(?:\?|$)/;
  try {
    await fresh(state);
    await page.waitForSelector('html[data-hydrated]');
    const original = await page.evaluate(() => ({
      document: __vtDocument,
      heading: document.querySelector('main h1')?.textContent,
    }));
    await page.route(chunk, async (route) => {
      await blocked;
      await route.continue();
    });
    const requested = page.waitForRequest(chunk);
    await page.evaluate(() => window.__routeTransitions.push('/changelog'));
    await requested;
    await page.waitForFunction(() =>
      document.documentElement.classList.contains('is-route-pending'),
    );
    assert.equal(
      await page.locator('[data-vt-route]').getAttribute('data-vt-route'),
      '/',
      'blocked destination must not replace the home DOM',
    );

    if (kind === 'back') await page.goBack();
    else await page.locator('.site-nav a[href="/archives"]').click();
    const recoveredRoute = kind === 'back' ? '/' : '/archives';
    await ready(page, recoveredRoute);
    await page.waitForFunction(
      () => !document.documentElement.classList.contains('is-route-pending'),
    );
    assert.equal(
      await page.locator('main > .route-pending').count(),
      0,
      `${kind}: route skeleton slot must be removed after recovery`,
    );

    // 中断后才释放旧请求，迟到的分包不能覆盖当前页或破坏下一次导航。
    const response = page.waitForResponse(chunk);
    release();
    const loaded = await response;
    assert(loaded.ok(), 'blocked chunk must eventually load');
    assert.equal(await loaded.finished(), null, 'blocked chunk response must complete');
    await page.waitForLoadState('networkidle');
    assert.deepEqual(state.errors, [], `${kind}: interrupted navigation must not panic`);
    assert.deepEqual(
      state.consoleErrors,
      [],
      `${kind}: interrupted navigation must not log errors`,
    );
    assert.equal(
      await page.locator('[data-vt-route]').getAttribute('data-vt-route'),
      recoveredRoute,
      'late chunk must not commit the cancelled route',
    );
    assert.equal(
      await page.locator('main h1').textContent(),
      kind === 'back' ? original.heading : '归档',
    );
    assert.equal(
      await page.locator('main').evaluate((main) => Number(getComputedStyle(main).opacity)),
      1,
      'recovered page must be visible',
    );

    // 真实点击并检查正文与文档身份，防止 URL 改变或整页刷新掩盖 WASM 已失效。
    if (kind === 'push') {
      await page.locator('.site-nav a[href="/"]').click();
      await ready(page, '/');
      assert.equal(await page.locator('main h1').textContent(), original.heading);
    }
    await page.locator('.site-nav a[href="/archives"]').click();
    await ready(page, '/archives');
    await page.waitForFunction(
      () => !document.documentElement.classList.contains('is-route-pending'),
    );
    assert.equal(await page.locator('main h1').textContent(), '归档');
    const end = await page.evaluate(() => ({
      path: location.pathname,
      route: window.__routeTransitions.currentRoute(),
      opacity: Number(getComputedStyle(document.querySelector('main')!).opacity),
      document: __vtDocument,
    }));
    assert.deepEqual(end, {
      path: '/archives',
      route: '/archives',
      opacity: 1,
      document: original.document,
    });
    assert.deepEqual(state.errors, [], `${kind}: subsequent navigation must not panic`);
    assert.deepEqual(state.consoleErrors, [], `${kind}: subsequent navigation must not log errors`);
    console.log(`PASS chunk-late-${kind}-keeps-spa-alive`);
  } finally {
    release();
    try {
      await page.unrouteAll({ behavior: 'wait' });
    } finally {
      await browser.close();
    }
  }
}

async function ensureLoggedOut(
  context: Playwright.BrowserContext,
  logoutEndpoint: string | undefined,
) {
  // Use the context cookie jar even when login succeeded but routing or rendering
  // failed. A DOM click cannot reliably clean up that case.
  const session = (await context.cookies(BASE)).find(
    (cookie) => cookie.name === 'session' && cookie.value,
  );
  if (!session) return;
  assert(logoutEndpoint, 'missing logout endpoint for test-session cleanup');
  const response = await context.request.post(`${BASE}${logoutEndpoint}`, {
    headers: { Origin: new URL(BASE).origin, 'Content-Type': 'application/json' },
    data: {},
  });
  assert(response.ok(), `test-session cleanup failed: HTTP ${response.status()}`);
  assert.equal(
    ((await response.json()) as { success: boolean }).success,
    true,
    'test-session cleanup was rejected',
  );
  assert(
    !(await context.cookies(BASE)).some((cookie) => cookie.name === 'session' && cookie.value),
    'test-session cleanup did not expire the session cookie',
  );
}

// 后台子页面分包迟到：顶栏下方出现后台骨架，分包到达后撤销。
async function adminChunkLate(page: Playwright.Page) {
  // 登录后 URL 先于后台外壳提交：等外壳与仪表盘挂载，否则推送发生在登录页上。
  await page.waitForSelector('.admin-shell main .admin-dash');
  await settle(page);
  const chunk = /\/pkg\/split___logs_route_view_.*\.wasm(?:\?|$)/;
  await page.route(chunk, async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 1200));
    await route.continue().catch(() => {});
  });
  try {
    await page.evaluate(() => window.__routeTransitions.push('/admin/logs'));
    await page.waitForFunction(() =>
      document.documentElement.classList.contains('is-route-pending'),
    );
    await page.waitForTimeout(200);
    const slot = await page.evaluate(() => {
      const el = document.querySelector('main > .admin-route-pending');
      const inView = (node: Element) => {
        if (node.getClientRects().length === 0) return false;
        const rect = node.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0 && rect.bottom > 0 && rect.top < innerHeight;
      };
      return {
        display: el && getComputedStyle(el).display,
        skeletonInView: Array.from(
          document.querySelectorAll('main .admin-route-pending .skeleton'),
        ).some(inView),
      };
    });
    assert(slot.display && slot.display !== 'none', 'admin route skeleton slot must be displayed');
    assert(slot.skeletonInView, 'admin route skeleton must be visible in the viewport');
    await ready(page, '/admin/logs');
    await page.waitForFunction(
      () => !document.documentElement.classList.contains('is-route-pending'),
    );
    assert.equal(
      await page.locator('main > .route-pending').count(),
      0,
      'admin route skeleton slot must be removed after commit',
    );
    console.log('PASS admin-chunk-late-skeleton');
  } finally {
    await page.unroute(chunk);
  }
}

async function admin() {
  if (!process.env.VT_ADMIN_USERNAME || !process.env.VT_ADMIN_PASSWORD) {
    console.log(
      'SKIP admin (set VT_ADMIN_USERNAME and VT_ADMIN_PASSWORD to include session checks)',
    );
    return;
  }
  const state = await launch();
  const { page, browser, context } = state;
  let logoutEndpoint: string | undefined;
  let seed: AdminPost[] = [];
  let paginationFixture = false;
  try {
    // Browser-only table rows exercise pagination and nested scrolling without
    // creating or editing posts. The first row retains a real preview identity.
    await page.route('**/api/list_posts*', async (route) => {
      const response = await route.fetch();
      const data = (await response.json()) as { posts?: AdminPost[]; total?: number };
      if (data.posts?.length) seed = data.posts;
      if (!paginationFixture) return route.fulfill({ response });
      assert(seed.length, 'admin fixture requires at least one existing post');
      const item = seed.find((post) => post.status === 'Draft') || seed[0];
      data.posts = Array.from({ length: 20 }, (_, index) => ({
        ...item,
        id: index === 0 ? item.id : 900000 + index,
        status: 'Draft',
        title: index === 0 ? item.title : `Browser-only row ${index}`,
      }));
      data.total = 60;
      await route.fulfill({ response, json: data });
    });
    await page.route('**/api/get_post_preview*', async (route) => {
      const response = await route.fetch();
      const data = (await response.json()) as {
        post?: { content_html: string; toc_html: string | null };
      };
      if (data.post) {
        data.post.content_html =
          '<p style="min-height:1100px">Browser-only lightweight preview.</p>';
        data.post.toc_html = null;
      }
      await route.fulfill({ response, json: data });
    });
    // server fn 地址不再带哈希后缀，退出登录端点固定。
    logoutEndpoint = '/api/logout';
    await fresh(state);
    await push(page, '/login');
    await page.locator('#login-username').fill(process.env.VT_ADMIN_USERNAME);
    await page.locator('#login-password').fill(process.env.VT_ADMIN_PASSWORD);
    await page.getByRole('button', { name: '登录', exact: true }).click();
    await page.waitForURL(/\/admin\/?$/);
    await settle(page);
    await adminChunkLate(page);
    await push(page, '/admin/posts');
    await page.waitForSelector('a[data-vt-post-link]');
    const published = seed.find((post) => post.status === 'Published');
    assert(published, 'published-to-preview regression requires one published post');
    const publishedLink = page.locator(`a[data-vt-post-link="${published.id}"]`);
    const preview = await publishedLink.getAttribute('href');
    assert(preview, 'published post must have a preview destination');
    assert(
      preview.startsWith('/admin/preview/'),
      'published admin titles must keep the stable preview layout',
    );
    await publishedLink.click();
    await ready(page, preview);
    await page.waitForSelector(`[data-vt-detail="${published.id}"]`);
    // 预览改版后没有「返回列表」链接：回列表走后退，恢复由历史条目完成。
    await page.goBack();
    await ready(page, '/admin/posts');
    console.log('PASS published-admin-article-preview-return');
    paginationFixture = true;
    await push(page, '/admin/');
    await push(page, '/admin/posts');
    await page.waitForSelector('a[data-vt-post-link]');
    await page.getByRole('tab', { name: '草稿', exact: true }).click();
    const input = page.getByPlaceholder('按标题搜索');
    await input.fill('Browser');
    await input.press('Enter');
    await page.waitForTimeout(250);
    await page.getByRole('button', { name: /下一页/ }).click();
    await page.waitForFunction(
      () =>
        (
          JSON.parse(
            __routeTransitions.readState('admin-posts') || '{}',
          ) as Partial<AdminPostsState>
        ).page === 2,
    );
    await page.waitForTimeout(250);
    await input.fill('Browser unsubmitted');
    await page.evaluate(() => (document.querySelector('[data-vt-scroll]')!.scrollTop = 120));
    const source = await page.evaluate(() => ({
      entry: (history.state as NavigationHistoryState).__yggdrasilNavigation,
      scroll: document.querySelector('[data-vt-scroll]')!.scrollTop,
    }));
    const link = page.locator('a[data-vt-post-link]').first();
    const href = await link.getAttribute('href');
    assert(href, 'preview link must have a destination');
    await link.click();
    await ready(page, href);
    await page.waitForSelector('[data-vt-detail]');
    // 同上：改版后的预览页没有回列表链接，用后退触发列表状态恢复。
    await page.goBack();
    await ready(page, '/admin/posts');
    await page.waitForSelector('a[data-vt-post-link]');
    const restored = await page.evaluate(
      () => JSON.parse(__routeTransitions.readState('admin-posts')!) as AdminPostsState,
    );
    assert.equal(restored.page, 2);
    assert.equal(restored.status, 'draft');
    assert.equal(restored.search_query, 'Browser');
    assert.equal(restored.search_input, 'Browser unsubmitted');
    assert(
      Math.abs(
        (await page.evaluate(() => document.querySelector('[data-vt-scroll]')!.scrollTop)) -
          source.scroll,
      ) < 5,
    );
    console.log('PASS admin-preview-return-pagination-filters-scroll');
    await page.getByRole('button', { name: '退出登录', exact: true }).click();
    await ready(page, '/login');
    const previousValues = await page.evaluate(
      (id) => __routeTransitions.entries.get(id)?.values.size,
      source.entry,
    );
    assert(previousValues === undefined || previousValues === 0);
    console.log('PASS login-logout-clears-admin-history');
    await page.getByRole('link', { name: '去注册', exact: true }).click();
    await ready(page, '/register');
    console.log('PASS login-register-transition');
    assert.deepEqual(state.errors, []);
  } finally {
    try {
      await ensureLoggedOut(context, logoutEndpoint);
    } finally {
      await browser.close();
    }
  }
}

export { ensureLoggedOut, fresh, launch };

if (import.meta.main)
  (async () => {
    const mode = process.argv[2] || 'all';
    assert(
      [
        'all',
        'public',
        'reduced',
        'unsupported',
        'mobile',
        'share',
        'slow',
        'chunk',
        'admin',
        'prefetch',
      ].includes(mode),
      `未知场景：${mode}`,
    );
    if (mode === 'all' || mode === 'public') await main();
    if (mode === 'all' || mode === 'reduced')
      await fallback({ name: 'reduced-motion', reducedMotion: 'reduce' });
    if (mode === 'all' || mode === 'unsupported')
      await fallback({ name: 'unsupported-browser', unsupported: true });
    if (mode === 'all' || mode === 'mobile') await mobile();
    if (mode === 'all' || mode === 'share') await shareMeta();
    if (mode === 'all' || mode === 'slow') await slowResponse();
    if (mode === 'all' || mode === 'chunk') await chunkLate();
    if (mode === 'all' || mode === 'admin') await admin();
    if (mode === 'all' || mode === 'prefetch') await prefetch();
  })().catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
