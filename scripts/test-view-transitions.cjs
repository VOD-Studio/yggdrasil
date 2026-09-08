#!/usr/bin/env node
/**
 * Native Chromium regression checks against a running, populated development site.
 * No content is written. Cover and slow-response cases use isolated browser fixtures.
 *
 * PLAYWRIGHT_MODULE=/path/to/playwright CHROMIUM_PATH=/usr/bin/chromium \
 *   node scripts/test-view-transitions.cjs
 * Optional: VT_BASE, VT_SEARCH_QUERY, VT_TAG_PATH, VT_DEBUG=1.
 * Supply VT_ADMIN_USERNAME and VT_ADMIN_PASSWORD to include login/preview/logout.
 */
const assert = require('node:assert/strict');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const BASE = (process.env.VT_BASE || 'http://127.0.0.1:8080').replace(/\/$/, '');
const SEARCH_QUERY = process.env.VT_SEARCH_QUERY || 'Rust';
const TAG_PATH = process.env.VT_TAG_PATH || '/tags/Rust';
const FIXTURE_IMAGE = '/images/xiantiaoxiaogou_02.webp';
const FIXTURE_FILE = path.join(__dirname, '../public', FIXTURE_IMAGE);

async function launch(options = {}) {
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
  await page.route('https://fonts.googleapis.com/**', route => route.fulfill({ status: 200, contentType: 'text/css', body: '' }));
  const errors = [];
  const consoleErrors = [];
  const failedRequests = [];
  const httpErrors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => {
    if (message.type() === 'error') consoleErrors.push(message.text());
  });
  page.on('response', async response => {
    if (response.status() >= 400) httpErrors.push({ url: response.url(), status: response.status(), body: (await response.text().catch(() => '')).slice(0, 600) });
  });
  page.on('requestfailed', request => failedRequests.push({
    url: request.url(), error: request.failure()?.errorText,
  }));
  return { browser, context, page, errors, consoleErrors, failedRequests, httpErrors };
}

async function snapshot(page) {
  return page.evaluate(() => ({
    url: location.href,
    title: document.title,
    heading: document.querySelector('main h1')?.textContent,
    startViewTransition: typeof document.startViewTransition,
    core: typeof window.__routeTransitions,
    historyState: history.state,
    readyState: document.readyState,
    shells: Array.from(document.querySelectorAll('[data-vt-shell]'), e => e.dataset.vtShell),
    details: Array.from(document.querySelectorAll('[data-vt-detail]'), e => e.dataset.vtDetail),
    listReady: Boolean(document.querySelector('[data-vt-list="true"]')),
    listPending: Boolean(document.querySelector('[data-vt-list-pending="true"]')),
    posts: Array.from(document.querySelectorAll('a[data-vt-post-link]')).slice(0, 12).map(link => {
      const id = link.dataset.vtPostLink;
      const title = document.querySelector(`[data-vt-post-id="${id}"][data-vt-role="title"]`);
      const cover = document.querySelector(`[data-vt-post-id="${id}"][data-vt-role="cover"] img.blur-img-full`);
      return { id, href: link.getAttribute('href'), title: title?.textContent?.trim(),
        cover: cover?.getAttribute('src'), coverReady: Boolean(cover?.complete && cover?.naturalWidth > 0) };
    }),
    tags: Array.from(document.querySelectorAll('a[href^="/tags/"]')).slice(0, 12).map(e => e.getAttribute('href')),
    wasm: performance.getEntriesByType('resource').filter(r => r.name.includes('.wasm')).map(r => ({
      url: r.name, duration: r.duration, size: r.transferSize,
    })),
  }));
}

async function instrument(page, { unsupported = false } = {}) {
  await page.addInitScript(({ unsupported }) => {
    window.__vtCalls = [];
    window.__vtDocument = Math.random().toString(36);
    if (unsupported) {
      Object.defineProperty(document, 'startViewTransition', { configurable: true, value: undefined });
      return;
    }
    const start = document.startViewTransition?.bind(document);
    if (!start) return;
    const names = () => Array.from(document.querySelectorAll('[data-vt-role], [data-vt-shell]'))
      .filter(e => e.style.viewTransitionName)
      .map(e => ({ name: e.style.viewTransitionName, role: e.dataset.vtRole, id: e.dataset.vtPostId }));
    document.startViewTransition = update => {
      const call = { started: performance.now(), from: location.pathname, oldNames: names(), kind: document.documentElement.className };
      window.__vtCalls.push(call);
      const transition = start(async () => {
        call.callbackAt = performance.now();
        await update();
        call.updateDoneAt = performance.now();
        call.to = location.pathname;
        call.newNames = names();
      });
      transition.ready.then(() => { call.readyAt = performance.now(); }, error => { call.readyError = error.name; });
      transition.finished.then(() => { call.finishedAt = performance.now(); }, error => { call.finishError = error.name; });
      return transition;
    };
  }, { unsupported });
}
async function settle(page) {
  await page.waitForFunction(() => !document.documentElement.classList.contains('is-route-transitioning') && !document.documentElement.classList.contains('is-theme-transitioning'));
  await page.waitForTimeout(80);
}
async function ready(page, route) {
  await page.waitForURL(`${BASE}${route}`);
  await page.waitForFunction(route => document.querySelector('[data-vt-route]')?.dataset.vtRoute === route, route);
  await settle(page);
}
async function push(page, route) {
  await page.evaluate(route => window.__routeTransitions.push(route), route);
  await ready(page, route);
}
async function fresh(state, options = {}) {
  await instrument(state.page, options);
  await state.page.goto(`${BASE}/`, { waitUntil: 'domcontentloaded', timeout: 90000 });
  await state.page.waitForFunction(() => window.__routeTransitions?.entryId() && document.querySelector('a[data-vt-post-link]'), null, { timeout: 90000 });
  await settle(state.page);
}
function traceSummary(calls) {
  return calls.map(c => ({ from: c.from, to: c.to, old: c.oldNames.map(n => n.name), next: c.newNames?.map(n => n.name), snapshotDelay: Math.round(c.callbackAt - c.started), callbackWait: Math.round(c.updateDoneAt - c.callbackAt), wait: Math.round(c.updateDoneAt - c.started), readyError: c.readyError }));
}
async function titleRoundTrip(page, selector = 'a[data-vt-post-link]') {
  const link = page.locator(selector).first();
  const id = await link.getAttribute('data-vt-post-link');
  const href = await link.getAttribute('href');
  const title = page.locator(`[data-vt-post-id="${id}"][data-vt-role="title"]`);
  await title.evaluate(e => e.scrollIntoView({ block: 'center', behavior: 'instant' }));
  await page.waitForTimeout(200);
  await settle(page);
  const before = await page.evaluate(() => ({ entry: history.state.__yggdrasilNavigation, y: scrollY, url: location.href, count: __vtCalls.length, document: __vtDocument }));
  const titleBox = await title.boundingBox();
  await page.mouse.click(titleBox.x + Math.min(titleBox.width / 2, 50), titleBox.y + titleBox.height / 2);
  await ready(page, href);
  await page.waitForSelector(`[data-vt-detail="${id}"]`);
  assert.equal(await page.evaluate(() => __vtDocument), before.document, 'navigation must retain document');
  const forward = await page.evaluate(i => __vtCalls[i], before.count);
  assert(forward?.oldNames.some(n => n.name === 'vt-post-title'), 'source title must be named');
  assert(forward?.newNames.some(n => n.name === 'vt-post-title'), 'destination title must be named');
  assert(!forward.readyError, `forward transition rejected: ${forward.readyError}`);
  await page.goBack();
  await ready(page, new URL(before.url).pathname);
  await page.waitForSelector(selector);
  const after = await page.evaluate(() => ({ entry: history.state.__yggdrasilNavigation, y: scrollY, call: __vtCalls.at(-1) }));
  assert.equal(after.entry, before.entry, 'back restores original history entry');
  assert(Math.abs(after.y - before.y) < 5, `scroll restored: expected ${before.y}, actual ${after.y}`);
  assert(after.call.newNames.some(n => n.name === 'vt-post-title'), 'reverse target title must be named');
  assert(!after.call.readyError, `reverse transition rejected: ${after.call.readyError}`);
  return { id, href, scroll: before.y };
}
async function main() {
  const state = await launch();
  const { page, browser } = state;
  try {
    await fresh(state);
    console.log('PASS home-detail-back', JSON.stringify(await titleRoundTrip(page)));
    await push(page, '/search');
    await page.locator('#article-search').fill(SEARCH_QUERY);
    await page.locator('#article-search').press('Enter');
    await page.waitForSelector('.search-result-link');
    const beforeTitles = await page.locator('.search-result-link h2').allTextContents();
    console.log('PASS search-detail-back', JSON.stringify(await titleRoundTrip(page, '.search-result-link')));
    assert.equal(await page.locator('#article-search').inputValue(), SEARCH_QUERY);
    assert.deepEqual(await page.locator('.search-result-link h2').allTextContents(), beforeTitles);
    console.log('PASS search-query-results-restored', beforeTitles.length);
    await push(page, TAG_PATH);
    await page.getByRole('button', { name: '最早', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.tag-sort')?.dataset.order === 'earliest');
    console.log('PASS tag-detail-back', JSON.stringify(await titleRoundTrip(page)));
    assert.equal(await page.locator('.tag-sort').getAttribute('data-order'), 'earliest');
    console.log('PASS tag-order-restored');
    await push(page, '/archives');
    await page.locator('.archive-tags button.collapsible-trigger').click();
    assert.equal(await page.locator('.archive-tags').getAttribute('data-open'), 'true');
    await page.waitForTimeout(350);
    console.log('PASS archive-detail-back', JSON.stringify(await titleRoundTrip(page, '.archive-entry')));
    assert.equal(await page.locator('.archive-tags').getAttribute('data-open'), 'true');
    console.log('PASS archive-open-restored');
    await page.locator('.archive-entry').first().click();
    await page.waitForSelector('[data-vt-detail]');
    await settle(page);
    const anchorBefore = await page.evaluate(() => ({ entry: history.state.__yggdrasilNavigation, count: __vtCalls.length }));
    const hash = await page.locator('.toc-sidebar a[href^="#"]').first().getAttribute('href');
    await page.locator('.toc-sidebar a[href^="#"]').first().evaluate(a => a.click());
    await page.waitForFunction(hash => location.hash === hash, hash);
    assert.equal(await page.evaluate(() => history.state.__yggdrasilNavigation), anchorBefore.entry);
    assert.equal(await page.evaluate(() => __vtCalls.length), anchorBefore.count);
    assert.equal(await page.evaluate(() => window.__routeTransitions.currentRoute()), await page.evaluate(() => location.pathname + location.hash));
    console.log('PASS hash-preserves-history-and-skips-animation', hash);
    await page.evaluate(() => { __routeTransitions.push('/about'); __routeTransitions.push('/archives'); __routeTransitions.push('/search'); });
    await ready(page, '/search');
    assert.equal(await page.locator('#article-search').count(), 1);
    console.log('PASS rapid-programmatic-navigation');
    await page.evaluate(() => { __routeTransitions.push('/about'); document.querySelector('.theme-toggle').click(); });
    await ready(page, '/about');
    console.log('PASS navigation-theme-race');
    assert.equal(await page.locator('[style*="view-transition-name: vt-"]').count(), 0, 'temporary shared names cleaned');
    assert.deepEqual(state.errors, [], 'no JS errors');
    if (process.env.VT_DEBUG) console.log('TRACES', JSON.stringify(traceSummary(await page.evaluate(() => __vtCalls))));
  } catch (error) {
    console.error('FAIL', error.message);
    console.log('FAILURE', JSON.stringify({ snapshot: await snapshot(page).catch(() => null), traces: traceSummary(await page.evaluate(() => __vtCalls).catch(() => [])), pageErrors: state.errors, httpErrors: state.httpErrors }));
    throw error;
  } finally { await browser.close(); }
}
async function fallback({ name, ...options }) {
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
    assert.deepEqual(state.errors, []);
    console.log(`PASS ${name}`);
  } finally { await state.browser.close(); }
}
async function mobile() {
  const state = await launch({ viewport: { width: 390, height: 844 } });
  try {
    await fresh(state);
    console.log('PASS mobile-title-roundtrip', JSON.stringify(await titleRoundTrip(state.page)));
    assert.deepEqual(state.errors, []);
  } finally { await state.browser.close(); }
}
async function coverFixture(mode) {
  const state = await launch();
  const { page, browser } = state;
  try {
    await page.route('**/api/list_published_posts*', async route => {
      const response = await route.fetch();
      const data = await response.json();
      if (data.posts?.[0]) data.posts[0].cover_image = FIXTURE_IMAGE;
      await route.fulfill({ response, json: data });
    });
    await page.route('**/api/get_post_by_slug*', async route => {
      const response = await route.fetch();
      const data = await response.json();
      assert(data.post, 'a published article is required for cover fixtures');
      data.post.cover_image = FIXTURE_IMAGE;
      data.post.content_html = '<p style="min-height:1000px">Browser-only lightweight article fixture.</p>';
      data.post.toc_html = null;
      await route.fulfill({ response, json: data });
    });
    await page.route(`**${FIXTURE_IMAGE}?**`, async route => {
      if (new URL(route.request().url()).searchParams.get('w') === '1200') {
        if (mode === 'failure') return route.fulfill({ status: 404, body: '' });
        if (mode === 'slow') await new Promise(resolve => setTimeout(resolve, 800));
      }
      await route.fulfill({ status: 200, contentType: 'image/webp', path: FIXTURE_FILE });
    });
    await fresh(state);
    // Remount the home resource so the source cover comes from a client response,
    // avoiding edits to SSR hydration data or Dioxus-managed DOM.
    await push(page, '/search');
    await push(page, '/');
    const cover = page.locator('[data-vt-role="cover"]').first();
    await cover.scrollIntoViewIfNeeded();
    await page.waitForFunction(() => {
      const image = document.querySelector('[data-vt-role="cover"] .blur-img-full');
      return image?.complete && image.naturalWidth > 0;
    });
    const id = await cover.getAttribute('data-vt-post-id');
    const link = page.locator(`a[data-vt-post-link="${id}"]`);
    const href = await link.getAttribute('href');
    const count = await page.evaluate(() => __vtCalls.length);
    await link.click();
    await ready(page, href);
    const forward = await page.evaluate(index => __vtCalls[index], count);
    assert(forward.oldNames.some(n => n.name === 'vt-post-cover'));
    assert(forward.newNames.some(n => n.name === 'vt-post-title'));
    assert.equal(forward.newNames.some(n => n.name === 'vt-post-cover'), mode === 'success');
    assert(!forward.readyError, 'cover transition must not be rejected');
    if (mode === 'slow') {
      assert(forward.updateDoneAt - forward.started < 450, 'slow image exceeds wait budget');
      await page.waitForTimeout(900);
      assert.equal(await page.evaluate(() => __vtCalls.length), count + 1, 'late image must not replay');
    }
    if (mode === 'success') {
      await page.goBack();
      await ready(page, '/');
      const reverse = await page.evaluate(() => __vtCalls.at(-1));
      assert(reverse.newNames.some(n => n.name === 'vt-post-cover'));
      assert(!reverse.readyError);
    }
    assert.deepEqual(state.errors, []);
    console.log(`PASS cover-${mode}`);
  } finally { await browser.close(); }
}

async function slowResponse() {
  const state = await launch();
  const { page, browser } = state;
  try {
    await page.route('**/api/get_post_by_slug*', async route => {
      await new Promise(resolve => setTimeout(resolve, 800));
      await route.continue();
    });
    await fresh(state);
    await page.locator('a[data-vt-post-link]').first().click();
    await page.waitForFunction(() => __vtCalls.at(-1)?.updateDoneAt);
    const early = await page.evaluate(() => ({
      trace: __vtCalls.at(-1), detail: Boolean(document.querySelector('[data-vt-detail]')),
    }));
    assert(!early.detail, 'slow response must show the loading view at deadline');
    assert(early.trace.updateDoneAt - early.trace.started < 450, 'slow response exceeds wait budget');
    await page.waitForSelector('[data-vt-detail]');
    await settle(page);
    assert.equal(await page.evaluate(() => __vtCalls.length), 1, 'late content must not replay');
    assert.deepEqual(state.errors, []);
    console.log('PASS 300ms-response-budget-no-replay');
  } finally { await browser.close(); }
}

async function admin() {
  if (!process.env.VT_ADMIN_USERNAME || !process.env.VT_ADMIN_PASSWORD) {
    console.log('SKIP admin (set VT_ADMIN_USERNAME and VT_ADMIN_PASSWORD to include session checks)');
    return;
  }
  const state = await launch();
  const { page, browser } = state;
  let loggedIn = false;
  let seed = [];
  let paginationFixture = false;
  try {
    // Browser-only table rows exercise pagination and nested scrolling without
    // creating or editing posts. The first row retains a real preview identity.
    await page.route('**/api/list_posts*', async route => {
      const response = await route.fetch();
      const data = await response.json();
      if (data.posts?.length) seed = data.posts;
      if (!paginationFixture) return route.fulfill({ response });
      assert(seed.length, 'admin fixture requires at least one existing post');
      const item = seed.find(post => post.status === 'Draft') || seed[0];
      data.posts = Array.from({ length: 20 }, (_, index) => ({
        ...item, id: index === 0 ? item.id : 900000 + index, status: 'Draft',
        title: index === 0 ? item.title : `Browser-only row ${index}`,
      }));
      data.total = 60;
      await route.fulfill({ response, json: data });
    });
    await page.route('**/api/get_post_preview*', async route => {
      const response = await route.fetch();
      const data = await response.json();
      if (data.post) {
        data.post.content_html = '<p style="min-height:1100px">Browser-only lightweight preview.</p>';
        data.post.toc_html = null;
      }
      await route.fulfill({ response, json: data });
    });
    await fresh(state);
    await push(page, '/login');
    await page.locator('#login-username').fill(process.env.VT_ADMIN_USERNAME);
    await page.locator('#login-password').fill(process.env.VT_ADMIN_PASSWORD);
    await page.getByRole('button', { name: '登录', exact: true }).click();
    await page.waitForURL(/\/admin\/?$/);
    loggedIn = true;
    await settle(page);
    await push(page, '/admin/posts');
    await page.waitForSelector('a[data-vt-post-link]');
    const published = seed.find(post => post.status === 'Published');
    assert(published, 'published-to-preview regression requires one published post');
    const publishedLink = page.locator(`a[data-vt-post-link="${published.id}"]`);
    const preview = await publishedLink.getAttribute('href');
    assert(preview.startsWith('/admin/preview/'), 'published admin titles must keep the stable preview layout');
    await publishedLink.click();
    await ready(page, preview);
    await page.waitForSelector(`[data-vt-detail="${published.id}"]`);
    await page.locator('a[data-vt-return]').filter({ hasText: '返回列表' }).click();
    await ready(page, '/admin/posts');
    console.log('PASS published-admin-article-preview-return');
    paginationFixture = true;
    await push(page, '/admin/');
    await push(page, '/admin/posts');
    await page.waitForSelector('a[data-vt-post-link]');
    await page.getByRole('button', { name: '草稿', exact: true }).click();
    const input = page.getByPlaceholder('搜索文章标题...');
    await input.fill('Browser');
    await input.press('Enter');
    await page.waitForTimeout(250);
    await page.getByRole('button', { name: /下一页/ }).click();
    await page.waitForFunction(() => JSON.parse(__routeTransitions.readState('admin-posts') || '{}').page === 2);
    await page.waitForTimeout(250);
    await input.fill('Browser unsubmitted');
    await page.evaluate(() => document.querySelector('[data-vt-scroll]').scrollTop = 120);
    const source = await page.evaluate(() => ({
      entry: history.state.__yggdrasilNavigation,
      scroll: document.querySelector('[data-vt-scroll]').scrollTop,
    }));
    const link = page.locator('a[data-vt-post-link]').first();
    const href = await link.getAttribute('href');
    await link.click();
    await ready(page, href);
    await page.waitForSelector('[data-vt-detail]');
    await page.locator('a[data-vt-return]').filter({ hasText: '返回列表' }).click();
    await ready(page, '/admin/posts');
    await page.waitForSelector('a[data-vt-post-link]');
    const restored = await page.evaluate(() => JSON.parse(__routeTransitions.readState('admin-posts')));
    assert.equal(restored.page, 2);
    assert.equal(restored.status, 'draft');
    assert.equal(restored.search_query, 'Browser');
    assert.equal(restored.search_input, 'Browser unsubmitted');
    assert(Math.abs(await page.evaluate(() => document.querySelector('[data-vt-scroll]').scrollTop) - source.scroll) < 5);
    console.log('PASS admin-preview-return-pagination-filters-scroll');
    await page.getByRole('button', { name: '退出', exact: true }).click();
    await ready(page, '/login');
    loggedIn = false;
    const previousValues = await page.evaluate(id => __routeTransitions.entries.get(id)?.values.size, source.entry);
    assert(previousValues === undefined || previousValues === 0);
    console.log('PASS login-logout-clears-admin-history');
    await page.getByRole('link', { name: '还没有账号？去注册' }).click();
    await ready(page, '/register');
    console.log('PASS login-register-transition');
    assert.deepEqual(state.errors, []);
  } finally {
    if (loggedIn) {
      await page.getByRole('button', { name: '退出', exact: true }).click().catch(() => {});
    }
    await browser.close();
  }
}

(async () => {
  const mode = process.argv[2] || 'all';
  if (mode === 'all' || mode === 'public') await main();
  if (mode === 'all' || mode === 'reduced') await fallback({ name: 'reduced-motion', reducedMotion: 'reduce' });
  if (mode === 'all' || mode === 'unsupported') await fallback({ name: 'unsupported-browser', unsupported: true });
  if (mode === 'all' || mode === 'mobile') await mobile();
  if (mode === 'all' || mode === 'cover') {
    for (const outcome of ['success', 'failure', 'slow']) await coverFixture(outcome);
  }
  if (mode === 'all' || mode === 'slow') await slowResponse();
  if (mode === 'all' || mode === 'admin') await admin();
})().catch(error => { console.error(error.message); process.exitCode = 1; });
