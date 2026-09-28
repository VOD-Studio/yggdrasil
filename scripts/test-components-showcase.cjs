#!/usr/bin/env node
/**
 * Browser acceptance for the public component atlas. Connects to an existing site only.
 *
 * SHOWCASE_BASE=http://127.0.0.1:8080 PLAYWRIGHT_MODULE=/path/to/playwright \
 *   CHROMIUM_PATH=/path/to/chromium node scripts/test-components-showcase.cjs
 * Optional SHOWCASE_ARTIFACT_DIR stores screenshots/report; default is a temporary directory.
 * SHOWCASE_ONLY=post-footer runs focused footer layout, theme, and sample-link checks.
 * SHOWCASE_ONLY=post-header checks full, unscaled headers and draft/summary variants.
 * SHOWCASE_ONLY=post-nav-links checks responsive navigation and missing-neighbor states.
 * Sensitive showcase requests are recorded, aborted, and fail the run.
 */
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');

const BASE = (process.env.SHOWCASE_BASE || 'http://127.0.0.1:8080').replace(/\/$/, '');
const ORIGIN = new URL(BASE).origin;
const ARTIFACTS = process.env.SHOWCASE_ARTIFACT_DIR ||
  fs.mkdtempSync(path.join(os.tmpdir(), 'yggdrasil-showcase-'));
fs.mkdirSync(ARTIFACTS, { recursive: true });

// These selectors describe actual shared UI, rather than the catalog metadata or title.
const TARGETS = [
  ['admin-layout', 'AdminLayout', '[data-showcase-preview="admin-layout"]'],
  ['asset-picker-modal', 'AssetPickerModal', '.showcase-picker-panel img'],
  ['asset-upload-modal', 'AssetUploadModal', '.showcase-upload-demo :text("封面.webp")'],
  ['code-runner', 'CodeRunner', '.showcase-runner-demo .code-runner-editor .cm-editor'],
  ['comment-form', 'CommentForm', '.comment-editor-mount .ProseMirror'],
  ['comment-item', 'CommentItem', '.comment-list'],
  ['comment-list', 'CommentList', '.comment-list'],
  ['pending-comment-item', 'PendingCommentItem', ':text("这条样例评论正在等待审核")'],
  ['comment-section', 'CommentSection', '.comment-editor-mount .ProseMirror'],
  ['footer', 'Footer', '[data-showcase-preview="footer"] footer'],
  ['frontend-layout', 'FrontendLayout', '[data-showcase-preview="frontend-layout"] main'],
  ['header', 'Header', '[data-showcase-preview="header"] nav'],
  ['post-content', 'PostContent', '[data-showcase-preview="post-content"] .post-content'],
  ['post-footer', 'PostFooter', '[data-showcase-preview="post-footer"] .post-footer'],
  ['post-header', 'PostHeader', '[data-showcase-preview="post-header"] .post-header'],
  ['post-nav-links', 'PostNavLinks', '[data-showcase-preview="post-nav-links"] .paginav'],
  ['post-toc', 'PostToc', '[data-showcase-preview="post-toc"] .toc-sidebar'],
  ['tiptap-editor', 'TiptapEditor', '.showcase-browser-host--tiptap .ProseMirror'],
  ['code-mirror-editor', 'CodeMirrorEditor', '.showcase-browser-host--code .cm-editor'],
  ['xterm-terminal', 'XtermTerminal', '.showcase-browser-host--xterm .xterm'],
  ['lightbox', 'Lightbox', '.showcase-browser-lightbox-grid img'],
];
const CATALOG = JSON.parse(fs.readFileSync(
  path.join(__dirname, '../src/pages/components_showcase_data.json'), 'utf8'));
const MODES = new Map(CATALOG.components.map(component => [component.slug, component.preview_mode]));
const HEAVY = ['tiptap-editor', 'code-mirror-editor', 'xterm-terminal'];
const BUNDLES = ['/tiptap/editor.js', '/codemirror/editor.js', '/xterm/terminal.js'];
const SENSITIVE = new Set([
  'getcurrentuser', 'logout', 'listassets', 'getuploadsettings', 'getcomments', 'createcomment',
  'checkpendingstatus', 'startExec'.toLowerCase(), 'startexecstream', 'getexecresult',
  'executesql', 'getdbschema',
]);
const NORMAL_FRONTEND_READS = new Set(['getsitesettings']);
const report = { base: BASE, artifactDir: ARTIFACTS, passed: [], apiRequests: [],
  blockedBusinessRequests: [], pageErrors: [], screenshots: [], limitations: [] };
let stage = 'baseline';
let baselineReads = new Map();
let visitId = 0;

function apiName(pathname) {
  return pathname.split('/').pop().replace(/[^a-z]/gi, '').toLowerCase();
}
function isSensitive(pathname) {
  return pathname.startsWith('/api/') &&
    (SENSITIVE.has(apiName(pathname)) ||
      /^\/api\/(?:upload|comments\/upload|notes\/upload|exec\/)/i.test(pathname));
}
function pass(name, extra) {
  report.passed.push(name);
  console.log('PASS', name, extra || '');
}
async function attachMonitoring(page) {
  page.on('pageerror', error => report.pageErrors.push({ stage, message: error.message }));
  page.on('request', request => {
    const url = new URL(request.url());
    if (url.origin === ORIGIN && url.pathname.startsWith('/api/')) {
      report.apiRequests.push({ stage, visitId, method: request.method(), path: url.pathname });
    }
  });
  await page.route('**/api/**', async route => {
    const request = route.request();
    const url = new URL(request.url());
    const endpoint = `${request.method()} ${url.pathname}`;
    if (stage !== 'baseline' && url.origin === ORIGIN && isSensitive(url.pathname) &&
      !baselineReads.has(endpoint)) {
      report.blockedBusinessRequests.push({ stage, method: request.method(), path: url.pathname });
      await route.abort();
      return;
    }
    await route.continue();
  });
  // The dev toast may request Google Fonts; it is unrelated to component behavior.
  await page.route('https://fonts.googleapis.com/**', route =>
    route.fulfill({ status: 200, contentType: 'text/css', body: '' }));
}
async function goto(page, route) {
  if (stage !== 'baseline') visitId++;
  await page.goto(`${BASE}${route}`, { waitUntil: 'domcontentloaded', timeout: 90000 });
  const ready = route === '/about/components' ? '.showcase-gallery' :
    route.startsWith('/about/components/') ? '.showcase-detail' : 'main';
  await page.locator(ready).first().waitFor({ timeout: 90000 });
  await page.waitForFunction(() => window.__routeTransitions?.entryId(), null, { timeout: 90000 });
  await page.waitForTimeout(80);
}
async function push(page, route) {
  visitId++;
  await page.evaluate(next => window.__routeTransitions.push(next), route);
  await page.waitForURL(`${BASE}${route}`);
}
function assertNetworkDelta() {
  const after = report.apiRequests.filter(request => request.stage !== 'baseline');
  const unexpected = after.filter(request => {
    const endpoint = `${request.method} ${request.path}`;
    return !baselineReads.has(endpoint) && !NORMAL_FRONTEND_READS.has(apiName(request.path));
  });
  assert.deepEqual(unexpected, [], 'showcase introduced API endpoints absent from /about baseline');
  for (const [endpoint, baselineCount] of baselineReads) {
    if (NORMAL_FRONTEND_READS.has(apiName(endpoint.split(' ')[1]))) continue;
    const byVisit = new Map();
    for (const request of after.filter(request => `${request.method} ${request.path}` === endpoint)) {
      byVisit.set(request.visitId, (byVisit.get(request.visitId) || 0) + 1);
    }
    for (const [visit, observed] of byVisit) {
      assert(observed <= baselineCount,
        `${endpoint} occurred ${observed} times during showcase visit ${visit}; /about baseline was ${baselineCount}`);
    }
  }
  report.baselineReads = [...baselineReads].map(([endpoint, count]) => ({ endpoint, count }));
}
async function realPreview(page, slug, selector, card = false) {
  const root = card ? page.locator(`#showcase-${slug} .showcase-card-preview`) :
    page.locator('.showcase-detail .showcase-preview-instance');
  await root.locator(selector).first().waitFor({ state: 'visible', timeout: 45000 });
  assert.equal(await root.locator('.showcase-reference-art').count(), 0,
    `${slug} fell back to the generic reference card`);
  if (HEAVY.includes(slug)) {
    await root.locator('.showcase-browser-placeholder').first()
      .waitFor({ state: 'hidden', timeout: 10000 });
  }
  if (slug === 'post-header') {
    const layout = await root.evaluate(element => {
      const preview = element.querySelector('[data-showcase-preview="post-header"]');
      const outer = element.getBoundingClientRect();
      const box = preview.getBoundingClientRect();
      const title = preview.querySelector('[data-showcase-sample="published"] .post-title');
      return {
        centered: Math.abs(box.left + box.width / 2 - outer.left - outer.width / 2) < 1,
        titleSize: Number.parseFloat(getComputedStyle(title).fontSize),
        samples: [...preview.querySelectorAll('[data-showcase-sample]')].map(sample => {
          const sheet = sample.getBoundingClientRect();
          const header = sample.querySelector('.post-header');
          return {
            unscaled: Math.abs(header.getBoundingClientRect().width -
              Number.parseFloat(getComputedStyle(header).width)) < 1,
            noOverflow: sample.scrollWidth <= sample.clientWidth + 1 &&
              sample.scrollHeight <= sample.clientHeight + 1,
            visibleContent: [...header.children].every(child => {
              const rect = child.getBoundingClientRect();
              return rect.top >= sheet.top && rect.bottom <= Math.min(sheet.bottom, outer.bottom) + 1 &&
                rect.left >= sheet.left && rect.right <= sheet.right + 1;
            }),
          };
        }),
      };
    });
    assert(layout.centered, 'article header preview is centered in its workspace');
    assert(layout.titleSize >= (card ? 24 : 28), 'article title stays readable');
    assert(layout.samples.every(sample => sample.unscaled && sample.noOverflow && sample.visibleContent),
      'all header samples render at natural size with complete content and no inner scrollbars');
  }
  if (slug === 'post-footer') {
    const layout = await root.evaluate(element => {
      const preview = element.querySelector('[data-showcase-preview="post-footer"]');
      const box = preview.getBoundingClientRect();
      const outer = element.getBoundingClientRect();
      return {
        scrollHeight: preview.scrollHeight,
        clientHeight: preview.clientHeight,
        visibleTitles: [...preview.querySelectorAll('.post-title-nav')].every(title => {
          const rect = title.getBoundingClientRect();
          return rect.top >= box.top && rect.bottom <= Math.min(box.bottom, outer.bottom) + 1 &&
            rect.left >= box.left && rect.right <= box.right + 1;
        }),
      };
    });
    assert(layout.scrollHeight <= layout.clientHeight + 1, 'footer preview has no inner scrollbar');
    assert(layout.visibleTitles, 'footer preview shows adjacent article titles in full');
  }
  if (slug === 'post-nav-links') {
    const layout = await root.evaluate(element => {
      const preview = element.querySelector('[data-showcase-preview="post-nav-links"]');
      const outer = element.getBoundingClientRect();
      const box = preview.getBoundingClientRect();
      return {
        centered: Math.abs(box.left + box.width / 2 - outer.left - outer.width / 2) < 1,
        noOverflow: preview.scrollWidth <= preview.clientWidth + 1 &&
          preview.scrollHeight <= preview.clientHeight + 1,
        titleSize: Math.min(...[...preview.querySelectorAll('.post-title-nav')]
          .map(title => Number.parseFloat(getComputedStyle(title).fontSize))),
        naturalSize: [...preview.querySelectorAll('.paginav')].every(nav =>
          Math.abs(nav.getBoundingClientRect().width - Number.parseFloat(getComputedStyle(nav).width)) < 1),
        visibleTitles: [...preview.querySelectorAll('.post-title-nav')].every(title => {
          const rect = title.getBoundingClientRect();
          return rect.left >= box.left && rect.right <= box.right + 1 &&
            rect.top >= box.top && rect.bottom <= Math.min(box.bottom, outer.bottom) + 1;
        }),
      };
    });
    assert(layout.centered && layout.naturalSize, 'navigation preview stays centered and unscaled');
    assert(layout.noOverflow && layout.visibleTitles, 'navigation titles are complete without inner scrolling');
    assert(layout.titleSize >= (card ? 14 : 16), 'article titles remain readable');
  }
}
async function screenshot(page, name) {
  const file = path.join(ARTIFACTS, `${name}.png`);
  await page.screenshot({ path: file, animations: 'disabled' });
  report.screenshots.push(file);
}
async function elementScreenshot(locator, name) {
  const file = path.join(ARTIFACTS, `${name}.png`);
  await locator.screenshot({ path: file, animations: 'disabled' });
  report.screenshots.push(file);
}
async function assertNoDocumentOverflow(page, slug) {
  const width = await page.evaluate(() => ({ view: innerWidth,
    document: document.documentElement.scrollWidth }));
  assert(width.document <= width.view + 2,
    `${slug} overflows the ${width.view}px viewport by ${width.document - width.view}px`);
}
async function commentStorage(page) {
  return page.evaluate(() => Object.fromEntries(
    Object.entries(localStorage).filter(([key]) => /comment|pending|author/i.test(key))));
}
async function catalog(page) {
  stage = 'catalog';
  await goto(page, '/about/components');
  await page.locator('.showcase-card').first().waitFor();
  await page.waitForTimeout(700);
  const early = await page.evaluate(() => performance.getEntriesByType('resource')
    .map(entry => new URL(entry.name).pathname));
  for (const bundle of BUNDLES) {
    assert(!early.includes(bundle), `${bundle} loaded before its card was visible`);
  }
  pass('cold catalog keeps browser libraries deferred');

  assert.equal(await page.locator('.showcase-card').count(), CATALOG.components.length);
  for (const component of CATALOG.components) {
    assert(['interactive', 'static'].includes(component.preview_mode),
      `${component.slug} has invalid preview mode`);
    const preview = page.locator(`#showcase-${component.slug} .showcase-card-preview`);
    assert.equal(await preview.locator('.showcase-reference-art').count(), 0,
      `${component.slug} fell back to reference art`);
    assert(await preview.evaluate(element => [...element.children].some(child =>
      !child.classList.contains('showcase-card-index'))),
    `${component.slug} has an empty preview subtree`);
  }
  pass(`${CATALOG.components.length} catalog cards have preview structure and valid modes`);

  await page.waitForFunction(() => document.querySelector('.showcase-grid')?.classList.contains('is-masonry'));
  const layout = await page.evaluate(() => {
    const box = slug => document.querySelector(`#showcase-${slug}`).getBoundingClientRect();
    const preview = document.querySelector('#showcase-asset-picker-modal .showcase-card-preview');
    const panel = document.querySelector('#showcase-asset-picker-modal .showcase-picker-panel');
    return {
      admin: { top: box('admin-layout').top, bottom: box('admin-layout').bottom },
      picker: { top: box('asset-picker-modal').top, bottom: box('asset-picker-modal').bottom },
      upload: { top: box('asset-upload-modal').top, left: box('asset-upload-modal').left },
      runner: { top: box('code-runner').top, left: box('code-runner').left },
      panelBottom: panel.getBoundingClientRect().bottom,
      previewBottom: preview.getBoundingClientRect().bottom,
    };
  });
  assert.equal(layout.admin.top, layout.picker.top, 'adjacent cards keep catalog order');
  assert(layout.picker.bottom > layout.admin.bottom, 'cards keep different natural heights');
  assert(layout.upload.top < layout.runner.top, 'the next card fills the shorter column');
  assert(layout.upload.left < layout.runner.left, 'card columns keep source order');
  assert(layout.panelBottom <= layout.previewBottom, 'asset picker actions are fully visible');
  pass('catalog uses ordered masonry and shows the full asset picker preview');

  await page.setViewportSize({ width: 901, height: 900 });
  await page.waitForFunction(() => {
    const preview = document.querySelector('#showcase-asset-picker-modal .showcase-card-preview');
    const panel = preview?.querySelector('.showcase-picker-panel');
    return panel && panel.getBoundingClientRect().bottom <= preview.getBoundingClientRect().bottom;
  });
  await assertNoDocumentOverflow(page, 'narrow masonry catalog');

  for (const width of [768, 390]) {
    await page.setViewportSize({ width, height: 900 });
    await page.waitForFunction(() => !document.querySelector('.showcase-grid')?.classList.contains('is-masonry'));
    assert.equal(await page.locator('.showcase-card').first().evaluate(element => element.style.gridRowEnd), '');
    await assertNoDocumentOverflow(page, 'catalog');
  }
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.waitForFunction(() => document.querySelector('.showcase-grid')?.classList.contains('is-masonry'));
  pass('catalog switches to one column on narrow viewports');

  for (const [index, [group]] of CATALOG.groups.entries()) {
    const button = page.locator('.showcase-categories button').nth(index);
    await button.click();
    await page.waitForFunction(position =>
      document.querySelectorAll('.showcase-categories button')[position]?.getAttribute('aria-pressed') === 'true',
    index);
    const expected = group === 'all' ? CATALOG.components.length :
      CATALOG.components.filter(component => component.group === group).length;
    assert.equal(await page.locator('.showcase-card').count(), expected,
      `${group} catalog category count`);
  }
  await page.locator('.showcase-categories button').first().click();
  pass('all catalog category filters match the catalog data');

  for (const [slug, name, selector] of TARGETS) {
    await page.locator('.showcase-search input').fill(name);
    const card = page.locator(`#showcase-${slug}`);
    await card.waitFor();
    await card.scrollIntoViewIfNeeded();
    await realPreview(page, slug, selector, true);
    if (['asset-picker-modal', 'asset-upload-modal', 'admin-layout', 'code-runner',
      'tiptap-editor', 'lightbox'].includes(slug)) await screenshot(page, `card-${slug}`);
    if (['asset-picker-modal', 'asset-upload-modal', 'admin-layout', 'code-runner'].includes(slug)) {
      await elementScreenshot(card, `card-element-${slug}`);
    }
  }
  pass(`${TARGETS.length} catalog cards render their actual UI`);

  await page.locator('.showcase-search input').fill('Checkbox');
  const checkboxCard = page.locator('#showcase-checkbox');
  assert.equal(await checkboxCard.evaluate(element => getComputedStyle(element).cursor), 'pointer');
  await checkboxCard.locator('label').first().click();
  assert.equal(new URL(page.url()).pathname, '/about/components',
    'using a card preview should not open its detail page');
  pass('cards show a pointer without hijacking preview controls');

  await page.locator('.showcase-search input').fill('TiptapEditor');
  const card = page.locator('#showcase-tiptap-editor');
  await card.scrollIntoViewIfNeeded();
  const y = await page.evaluate(() => scrollY);
  visitId++;
  await card.locator('.showcase-card-caption p').click();
  await page.waitForURL(`${BASE}/about/components/tiptap-editor`);
  pass('clicking a card caption opens its detail page');
  await realPreview(page, 'tiptap-editor', TARGETS.find(item => item[0] === 'tiptap-editor')[2]);
  visitId++;
  await page.locator('.showcase-back').click();
  await page.waitForURL(`${BASE}/about/components`);
  await page.waitForTimeout(350);
  assert.equal(await page.locator('.showcase-search input').inputValue(), 'TiptapEditor');
  const returnedY = await page.evaluate(() => scrollY);
  report.catalogReturnScroll = { before: y, after: returnedY };
  assert(Math.abs(returnedY - y) < 200,
    `return to catalog should restore the prior card position (${y} → ${returnedY})`);
  visitId++;
  await page.goBack();
  await page.waitForURL(`${BASE}/about/components/tiptap-editor`);
  visitId++;
  await page.goForward();
  await page.waitForURL(`${BASE}/about/components`);
  pass('catalog search, scroll, back and forward state');
}
async function details(page, viewport, colorScheme, reducedMotion, label) {
  stage = `details-${label}`;
  await page.setViewportSize(viewport);
  await page.emulateMedia({ colorScheme, reducedMotion });
  for (const [slug, , selector] of TARGETS) {
    await goto(page, `/about/components/${slug}`);
    await realPreview(page, slug, slug === 'post-toc' && viewport.width < 1200 ?
      '[data-showcase-preview="post-toc"] details.toc summary' : selector);
    const expectedReset = MODES.get(slug) === 'interactive' ? 1 : 0;
    assert.equal(await page.locator('.showcase-live-toolbar button').count(), expectedReset,
      `${slug} reset control disagrees with its preview mode`);
    await assertNoDocumentOverflow(page, slug);
    await screenshot(page, `${slug}-${label}`);
  }
  pass(`${TARGETS.length} direct details at ${viewport.width}px ${colorScheme} ${reducedMotion}`);
}

async function postHeader(page) {
  const selector = '[data-showcase-preview="post-header"] .post-header';
  for (const width of [1440, 390, 320]) {
    for (const colorScheme of ['light', 'dark']) {
      stage = `post-header-${width}-${colorScheme}`;
      await page.setViewportSize({ width, height: 1000 });
      await page.emulateMedia({ colorScheme, reducedMotion: width < 640 ? 'reduce' : 'no-preference' });
      await goto(page, '/about/components');
      await page.locator('.showcase-search input').fill('PostHeader');
      const card = page.locator('#showcase-post-header');
      await card.scrollIntoViewIfNeeded();
      await realPreview(page, 'post-header', selector, true);
      await assertNoDocumentOverflow(page, 'post-header-card');
      await elementScreenshot(card, `${stage}-card`);

      visitId++;
      await card.locator('.showcase-card-caption p').click();
      await page.waitForURL(`${BASE}/about/components/post-header`);
      await realPreview(page, 'post-header', selector);
      const preview = page.locator('[data-showcase-preview="post-header"]');
      assert.equal(await preview.locator('.post-header').count(), 3);
      assert.equal(await preview.locator('[data-showcase-sample="draft"] .entry-hint').count(), 1);
      assert.equal(await preview.locator('[data-showcase-sample="published"] .entry-hint').count(), 0);
      assert.equal(await preview.locator('[data-showcase-sample="no-summary"] .post-description').count(), 0);
      const longTitle = await preview.locator('[data-showcase-sample="no-summary"] .post-title')
        .evaluate(element => element.getBoundingClientRect().height >
          Number.parseFloat(getComputedStyle(element).lineHeight) + 1);
      assert(longTitle, 'long article titles wrap onto multiple lines');
      await assertNoDocumentOverflow(page, 'post-header-detail');
      await elementScreenshot(page.locator('.showcase-live-panel'), `${stage}-detail`);

      // Stress natural wrapping without creating or modifying an article.
      await preview.locator('[data-showcase-sample="published"]').evaluate(element => {
        element.querySelector('.post-title').textContent = 'LongTitleWithoutSpaces'.repeat(16);
        element.querySelector('.post-description').textContent = '一段较长的摘要，用来观察窄屏下的自然换行。'.repeat(6);
      });
      await realPreview(page, 'post-header', selector);
      await assertNoDocumentOverflow(page, 'post-header-long-content');
      await goto(page, '/about/components/post-header');
      await realPreview(page, 'post-header', selector);
      pass(`${stage}: complete card, SPA/direct detail, draft mark, no summary and long content`);
    }
  }
}

async function postFooter(page) {
  const selector = '[data-showcase-preview="post-footer"] .post-footer';
  for (const width of [1440, 390]) {
    for (const colorScheme of ['light', 'dark']) {
      stage = `post-footer-${width}-${colorScheme}`;
      await page.setViewportSize({ width, height: 1000 });
      await page.emulateMedia({ colorScheme, reducedMotion: width === 390 ? 'reduce' : 'no-preference' });
      await goto(page, '/about/components');
      await page.locator('.showcase-search input').fill('PostFooter');
      const card = page.locator('#showcase-post-footer');
      await card.scrollIntoViewIfNeeded();
      await realPreview(page, 'post-footer', selector, true);
      await assertNoDocumentOverflow(page, 'post-footer-card');
      await elementScreenshot(card, `${stage}-card`);

      visitId++;
      await card.locator('.showcase-card-caption p').click();
      await page.waitForURL(`${BASE}/about/components/post-footer`);
      await realPreview(page, 'post-footer', selector);
      const footer = page.locator(selector);
      assert.equal(await footer.locator('a').count(), 5);
      await elementScreenshot(page.locator('.showcase-live-panel'), `${stage}-detail`);
      for (const link of await footer.locator('a').all()) {
        await link.click();
        assert.equal(new URL(page.url()).pathname + new URL(page.url()).hash,
          '/about/components/post-footer', 'sample links stay on the footer preview');
      }
      await page.keyboard.press('Tab');
      await footer.locator('a.prev').focus();
      const focus = await footer.locator('a.prev').evaluate(element => getComputedStyle(element).outlineWidth);
      assert.equal(focus, '2px', 'adjacent article links have a visible keyboard focus');

      // Temporary content stresses wrapping without writing articles or changing fixture data.
      await footer.evaluate(element => {
        element.querySelector('.post-tags a').textContent = 'A-long-unbroken-tag-'.repeat(12);
        for (const title of element.querySelectorAll('.post-title-nav')) {
          title.textContent = '一篇很长的文章标题，保留完整的文字与阅读方向。'.repeat(5);
        }
      });
      await realPreview(page, 'post-footer', selector);
      await assertNoDocumentOverflow(page, 'post-footer-long-content');
      await footer.locator('a.prev').evaluate(element => element.remove());
      const position = await footer.locator('a.next').evaluate(element => {
        const parent = element.parentElement.getBoundingClientRect();
        const box = element.getBoundingClientRect();
        return { fullWidth: Math.abs(box.width - parent.width) < 1, right: box.right, parentRight: parent.right };
      });
      assert.equal(position.fullWidth, width === 390, 'a lone next article fills the mobile row');
      assert(Math.abs(position.right - position.parentRight) < 1, 'a lone next article keeps its alignment');
      pass(`${stage}: full card/detail, sample links, focus, long content and lone next article`);

      await goto(page, '/about/components/post-footer');
      await realPreview(page, 'post-footer', selector);
    }
  }
}

async function postNavLinks(page) {
  const selector = '[data-showcase-preview="post-nav-links"] .paginav';
  for (const width of [1440, 760, 390, 320]) {
    for (const colorScheme of ['light', 'dark']) {
      stage = `post-nav-links-${width}-${colorScheme}`;
      await page.setViewportSize({ width, height: 1000 });
      await page.emulateMedia({ colorScheme, reducedMotion: width < 640 ? 'reduce' : 'no-preference' });
      await goto(page, '/about/components');
      await page.waitForFunction(dark => document.documentElement.classList.contains('dark') === dark,
        colorScheme === 'dark');
      await page.locator('.showcase-search input').fill('PostNavLinks');
      const card = page.locator('#showcase-post-nav-links');
      await card.scrollIntoViewIfNeeded();
      await realPreview(page, 'post-nav-links', selector, true);
      await assertNoDocumentOverflow(page, 'post-nav-links-card');
      await card.locator('.paginav a').first().click();
      assert.equal(new URL(page.url()).pathname + new URL(page.url()).hash, '/about/components');
      await elementScreenshot(card, `${stage}-card`);

      visitId++;
      await card.locator('.showcase-card-caption p').click();
      await page.waitForURL(`${BASE}/about/components/post-nav-links`);
      await realPreview(page, 'post-nav-links', selector);
      const preview = page.locator('[data-showcase-preview="post-nav-links"]');
      const both = preview.locator('[data-showcase-sample="both"]');
      assert.equal(await both.locator('.paginav a').count(), 2);
      assert.equal(await preview.locator('[data-showcase-sample="next-only"] a.prev').count(), 0);
      assert.equal(await preview.locator('[data-showcase-sample="next-only"] a.next').count(), 1);
      assert.equal(await preview.locator('[data-showcase-sample="prev-only"] a.prev').count(), 1);
      assert.equal(await preview.locator('[data-showcase-sample="prev-only"] a.next').count(), 0);
      assert.equal(await preview.locator('[data-showcase-sample="empty"] nav').count(), 0,
        'no neighbors means no empty navigation landmark');

      const grids = await preview.locator('.paginav').evaluateAll(navs => navs.map(nav => {
        const box = nav.getBoundingClientRect();
        const links = [...nav.querySelectorAll('a')].map(link => link.getBoundingClientRect());
        return { narrow: box.width <= 480 || innerWidth <= 600,
          fullWidth: links.every(link => Math.abs(link.width - box.width) < 1),
          stacked: links.length < 2 || links[1].top >= links[0].bottom,
          alignedRight: Math.abs(links.at(-1).right - box.right) < 1 };
      }));
      for (const [index, grid] of grids.entries()) {
        assert.equal(grid.fullWidth, grid.narrow, 'card layout follows its container width');
        assert.equal(grid.stacked, grid.narrow || index > 0);
        if (grid.narrow || index === 0) assert(grid.alignedRight);
      }
      for (const link of await preview.locator('.paginav a').all()) {
        await link.click();
        assert.equal(new URL(page.url()).pathname + new URL(page.url()).hash,
          '/about/components/post-nav-links', 'sample links stay on the preview');
      }
      await page.keyboard.press('Tab');
      const first = both.locator('a.prev');
      await first.focus();
      assert.equal(await first.evaluate(element => getComputedStyle(element).outlineWidth), '2px');
      await page.keyboard.press('Enter');
      assert.equal(new URL(page.url()).pathname + new URL(page.url()).hash,
        '/about/components/post-nav-links', 'keyboard activation keeps samples local');
      if (width < 640) {
        assert.equal(await first.evaluate(element => getComputedStyle(element).transitionDuration), '0s');
      }
      await assertNoDocumentOverflow(page, 'post-nav-links-detail');
      await elementScreenshot(page.locator('.showcase-live-panel'), `${stage}-detail`);

      await both.locator('.post-title-nav').evaluateAll(titles => {
        for (const title of titles) title.textContent = 'LongTitleWithoutSpaces'.repeat(15);
      });
      await realPreview(page, 'post-nav-links', selector);
      await assertNoDocumentOverflow(page, 'post-nav-links-long-titles');
      await goto(page, '/about/components/post-nav-links');
      await realPreview(page, 'post-nav-links', selector);
      pass(`${stage}: natural card/detail, single/empty states, focus and long titles`);
    }
  }
}

async function interactions(page, onlyModalResets = false) {
  stage = 'interactions';
  await goto(page, '/about/components/admin-layout');
  const admin = page.locator('[data-showcase-preview="admin-layout"]');
  await admin.getByRole('button', { name: '退出' }).click();
  await admin.getByText('不会退出当前账号').waitFor();
  assert.equal(new URL(page.url()).pathname, '/about/components/admin-layout');
  pass('AdminLayout logout remains a local demo action');

  await goto(page, '/about/components/asset-picker-modal');
  const pickerTrigger = page.getByRole('button', { name: '打开素材选择弹窗' });
  await pickerTrigger.click();
  const dialog = page.getByRole('dialog');
  await dialog.waitFor();
  await page.waitForFunction(() => document.activeElement?.matches('[data-ygg-modal-panel][data-ygg-modal-open="true"]'));
  await dialog.getByRole('button', { name: '相机里的小狗.webp' }).click();
  await dialog.getByRole('button', { name: '下一页' }).click();
  await dialog.getByRole('button', { name: '手绘小狗.webp' }).click();
  await dialog.getByRole('button', { name: '插入 2 张图片' }).click();
  await dialog.waitFor({ state: 'hidden' });
  await page.locator('.showcase-demo-result').getByText('已确认').waitFor();
  await page.locator('.showcase-live-toolbar button').click();
  await page.locator('.showcase-demo-result').waitFor({ state: 'hidden' });
  await pickerTrigger.click();
  await page.getByRole('dialog').waitFor();
  await page.waitForFunction(() => document.activeElement?.matches('[data-ygg-modal-panel][data-ygg-modal-open="true"]'));
  await dialog.getByPlaceholder('搜索文件名 / alt').fill('院子');
  await dialog.getByRole('button', { name: '单选' }).click();
  await dialog.getByRole('button', { name: '↻ 恢复默认' }).click();
  await dialog.waitFor({ state: 'hidden' });
  assert.equal(await page.locator('.showcase-picker-panel input[type="search"]').inputValue(), '');
  assert.equal(await page.getByRole('button', { name: '多选' }).getAttribute('aria-pressed'), 'true');
  await page.getByText('已选 2 张').waitFor();
  assert.equal(await page.locator('.showcase-demo-result').count(), 0);
  await pickerTrigger.click();
  await dialog.waitFor();
  await dialog.getByText('未选择图片').waitFor();
  await page.keyboard.press('Escape');
  await page.getByRole('dialog').waitFor({ state: 'hidden' });
  await page.waitForFunction(() => document.activeElement?.textContent?.includes('打开素材选择弹窗'));
  pass('asset picker ordered selection, in-dialog reset, and Escape');

  await goto(page, '/about/components/asset-upload-modal');
  await page.getByRole('button', { name: '空状态' }).click();
  await page.getByText('加入示例文件').waitFor();
  await page.getByRole('button', { name: '代表状态' }).click();
  await page.getByText('封面.webp').waitFor();
  await page.getByRole('button', { name: '重试' }).click();
  await page.getByRole('button', { name: '重试' }).waitFor({ state: 'hidden' });
  await page.getByRole('button', { name: '代表状态' }).click();
  await page.getByRole('button', { name: '移除' }).click();
  assert.equal(await page.getByText('超大原图.png').count(), 0);
  await page.locator('.showcase-live-toolbar button').click();
  await page.getByText('超大原图.png').waitFor();
  const uploadTrigger = page.getByRole('button', { name: '打开上传弹窗' });
  await uploadTrigger.click();
  const uploadDialog = page.getByRole('dialog');
  await uploadDialog.waitFor();
  await page.waitForFunction(() => document.activeElement?.matches('[data-ygg-modal-panel][data-ygg-modal-open="true"]'));
  await uploadDialog.getByRole('button', { name: /加入示例文件/ }).click();
  await uploadDialog.getByText('示例文件-5.webp').waitFor();
  await uploadDialog.getByRole('button', { name: '↻ 恢复默认' }).click();
  await uploadDialog.waitFor({ state: 'hidden' });
  assert.equal(await page.getByText('示例文件-5.webp').count(), 0);
  await page.getByText('超大原图.png').waitFor();
  await uploadTrigger.click();
  await uploadDialog.waitFor();
  await uploadDialog.getByRole('button', { name: '重试' }).click();
  await page.waitForTimeout(200);
  await uploadDialog.getByRole('button', { name: '↻ 恢复默认' }).click();
  await uploadDialog.waitFor({ state: 'hidden' });
  await page.waitForTimeout(550);
  const failedRow = page.getByText('超大原图.png').locator('xpath=../..');
  await failedRow.getByText('大小超过 5MB 限制').waitFor();
  await failedRow.getByRole('button', { name: '重试' }).waitFor();
  await uploadTrigger.click();
  await uploadDialog.waitFor();
  await page.keyboard.press('Escape');
  await uploadDialog.waitFor({ state: 'hidden' });
  await page.waitForFunction(() => document.activeElement?.textContent?.includes('打开上传弹窗'));
  pass('upload states, in-dialog reset, stale retry cancellation, and Escape');
  if (onlyModalResets) return;

  await goto(page, '/about/components/code-runner');
  await page.getByRole('button', { name: '播放输出示例' }).click();
  await page.getByText('输出示例', { exact: true }).waitFor();
  await page.getByRole('button', { name: '错误输出' }).click();
  await page.getByRole('button', { name: '播放输出示例' }).click();
  await page.getByRole('button', { name: '↻ 恢复默认' }).click();
  pass('runner sample output and reset');

  await goto(page, '/about/components/comment-list');
  await page.getByRole('button', { name: '空列表' }).click();
  await page.getByText('暂无评论').waitFor();
  await page.getByRole('button', { name: '评论与待审核' }).click();
  await page.getByText('这条样例评论正在等待审核').waitFor();
  pass('comment list states');

  await goto(page, '/about/components/comment-form');
  const form = page.getByRole('form', { name: '发表评论' });
  await form.getByPlaceholder('昵称 *').fill('图鉴访客');
  await form.getByPlaceholder('邮箱 * (保密)').fill('showcase@example.test');
  await form.locator('.ProseMirror').fill('这是一条本地演示评论。');
  await form.getByRole('button', { name: '发表评论' }).click();
  await page.getByText('已加入本地演示列表').waitFor();
  await page.getByText('这是一条本地演示评论').waitFor();
  await page.getByRole('button', { name: '↻ 恢复默认' }).click();
  assert.equal(await page.getByText('这是一条本地演示评论').count(), 0);
  pass('comment form local submit and reset');

  await goto(page, '/about/components/comment-section');
  await page.getByRole('button', { name: '回复 青禾 的评论' }).first().click();
  const reply = page.getByRole('form', { name: '回复评论' });
  await reply.locator('.ProseMirror').fill('本地回复草稿');
  await reply.getByRole('button', { name: '取消' }).click();
  await page.getByRole('button', { name: '回复 青禾 的评论' }).first().click();
  assert((await reply.locator('.ProseMirror').innerText()).includes('本地回复草稿'));
  await reply.getByPlaceholder('昵称 *').fill('图鉴访客');
  await reply.getByPlaceholder('邮箱 * (保密)').fill('showcase@example.test');
  await reply.getByRole('button', { name: '回复', exact: true }).click();
  await page.getByText('本地回复草稿').waitFor();
  pass('comment reply draft survives cancel and submits locally');

  await goto(page, '/about/components/post-content');
  const original = await page.locator('[data-showcase-preview="post-content"] .post-content').innerText();
  await page.getByRole('button', { name: '切换文章样例' }).click();
  const next = await page.locator('[data-showcase-preview="post-content"] .post-content').innerText();
  assert.notEqual(next, original);
  pass('article content switches inside preview');

  await goto(page, '/about/components/post-toc');
  const tocScroll = page.locator('#showcase-post-toc-scroll-detail');
  await tocScroll.evaluate(element => { element.style.height = '180px'; });
  await page.locator('[data-showcase-preview="post-toc"] .toc-sidebar a').last().click();
  assert.equal(await page.evaluate(() => location.hash), '', 'preview TOC changed the page hash');
  await page.waitForFunction(() => document.querySelector('#showcase-post-toc-scroll-detail')?.scrollTop > 0,
    null, { timeout: 5000 });
  pass('post TOC navigates inside its own scroll area');

  await page.setViewportSize({ width: 390, height: 844 });
  await goto(page, '/about/components/header');
  const demoHeader = page.locator('[data-showcase-preview="header"]');
  const menuButton = demoHeader.getByRole('button', { name: '打开导航菜单' });
  const controls = await menuButton.getAttribute('aria-controls');
  assert(controls && controls !== 'mobile-nav-menu');
  await menuButton.click();
  assert.equal(await demoHeader.getByRole('button', { name: '关闭导航菜单' })
    .getAttribute('aria-expanded'), 'true');
  assert.equal(await page.locator(`#${controls}`).count(), 1);
  await page.setViewportSize({ width: 1440, height: 1000 });
  pass('sample Header mobile menu has its own ID and opens');

  await goto(page, '/about/components/lightbox');
  await page.locator('.showcase-browser-lightbox-grid img').first().click();
  await page.locator('.lightbox-overlay').waitFor();
  await page.getByRole('button', { name: '下一张' }).click();
  await page.getByText('2 / 3').waitFor();
  await page.getByRole('button', { name: '上一张' }).click();
  await page.getByText('1 / 3').waitFor();
  await page.getByRole('button', { name: '放大' }).click();
  await page.getByRole('button', { name: '顺时针旋转 90 度' }).click();
  await page.keyboard.press('Escape');
  await page.locator('.lightbox-overlay').waitFor({ state: 'hidden' });
  pass('lightbox open, zoom, rotate and Escape');

  await goto(page, '/about/components/tiptap-editor');
  await page.locator('.showcase-browser-host--tiptap .ProseMirror').waitFor();
  await page.locator('.tiptap-toggle-btn').click();
  const source = page.locator('.tiptap-source-textarea');
  await source.waitFor({ state: 'visible' });
  await source.fill(`${await source.inputValue()}\n\n图鉴源码切换成功。`);
  await page.locator('.tiptap-toggle-btn').click();
  await page.getByText('图鉴源码切换成功').waitFor();
  pass('Tiptap edits round-trip through Markdown source mode');

  await goto(page, '/about/components/code-mirror-editor');
  await page.locator('.showcase-browser-host--code .cm-editor').waitFor();
  await page.locator('.showcase-browser-toolbar select').selectOption('rust');
  await page.locator('.showcase-browser-host--code .cm-content').click();
  await page.keyboard.type('// local edit');
  await page.keyboard.press(process.platform === 'darwin' ? 'Meta+Enter' : 'Control+Enter');
  await page.getByText('不会执行代码').waitFor();
  pass('CodeMirror language, edit and local shortcut');

  await goto(page, '/about/components/xterm-terminal');
  await page.getByRole('button', { name: '播放 / 重放' }).click();
  await page.getByRole('button', { name: '暂停' }).click();
  await page.getByText('已暂停').waitFor();
  await page.getByRole('button', { name: '清屏' }).click();
  await page.getByText('已清屏').waitFor();
  pass('xterm replay, pause and clear');
}
async function lifecycle(page) {
  stage = 'lifecycle';
  for (const [slug, global] of [
    ['tiptap-editor', 'TiptapEditor'],
    ['code-mirror-editor', 'CodeMirrorEditor'],
    ['xterm-terminal', 'XtermTerminal'],
  ]) {
    for (let cycle = 0; cycle < 5; cycle++) {
      await push(page, `/about/components/${slug}`);
      await realPreview(page, slug, TARGETS.find(item => item[0] === slug)[2]);
      assert.equal(await page.evaluate(name => window[name]._instances.size, global), 1,
        `${slug} should own one instance`);
      await push(page, '/about');
      await page.waitForFunction(name => window[name]?._instances.size === 0, global);
    }
  }
  pass('browser instances return to zero after five SPA round trips each');
  await push(page, '/about/components/lightbox');
  await page.locator('.showcase-browser-lightbox-grid img').first().click();
  await page.locator('.lightbox-overlay').waitFor();
  await push(page, '/about');
  await page.locator('.lightbox-overlay').waitFor({ state: 'hidden' });
  pass('leaving lightbox preview closes only its overlay');
}
async function failureAndLateLoad(browser) {
  stage = 'library-retry';
  const context = await browser.newContext();
  const page = await context.newPage();
  page.setDefaultTimeout(30000);
  await attachMonitoring(page);
  let first = true;
  await page.route('**/codemirror/editor.js*', async route => {
    if (first) { first = false; await route.abort(); }
    else await route.continue();
  });
  try {
    await goto(page, '/about/components/code-mirror-editor');
    await page.getByRole('button', { name: '重新加载' }).click();
    await realPreview(page, 'code-mirror-editor', TARGETS.find(item => item[0] === 'code-mirror-editor')[2]);
    pass('browser library failure can retry');
  } finally { await context.close(); }

  stage = 'late-library';
  const lateContext = await browser.newContext();
  const latePage = await lateContext.newPage();
  latePage.setDefaultTimeout(30000);
  await attachMonitoring(latePage);
  let requested;
  const started = new Promise(resolve => { requested = resolve; });
  await latePage.route('**/xterm/terminal.js*', async route => {
    requested();
    await new Promise(resolve => setTimeout(resolve, 1200));
    await route.continue().catch(() => {});
  });
  try {
    await goto(latePage, '/about/components/xterm-terminal');
    await started;
    await push(latePage, '/about');
    await latePage.waitForTimeout(1500);
    assert.equal(await latePage.evaluate(() => window.XtermTerminal?._instances.size || 0), 0);
    pass('late library result cannot revive an unmounted preview');
  } finally { await lateContext.close(); }
}
async function main() {
  const browser = await chromium.launch({ headless: true,
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
    args: ['--no-sandbox'] });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 },
    colorScheme: 'light', reducedMotion: 'no-preference' });
  const page = await context.newPage();
  page.setDefaultTimeout(30000);
  await attachMonitoring(page);
  try {
    await goto(page, '/about');
    await page.waitForTimeout(400);
    baselineReads = new Map();
    for (const request of report.apiRequests.filter(request => request.stage === 'baseline')) {
      const endpoint = `${request.method} ${request.path}`;
      baselineReads.set(endpoint, (baselineReads.get(endpoint) || 0) + 1);
    }
    const storageBefore = await commentStorage(page);
    if (process.env.SHOWCASE_ONLY === 'post-nav-links') {
      await postNavLinks(page);
      assertNetworkDelta();
      assert.deepEqual(await commentStorage(page), storageBefore);
      assert.deepEqual(report.blockedBusinessRequests, []);
      assert.deepEqual(report.pageErrors, []);
      pass('focused navigation checks have no business requests, storage changes or browser errors');
      return;
    }
    if (process.env.SHOWCASE_ONLY === 'post-header') {
      await postHeader(page);
      assertNetworkDelta();
      assert.deepEqual(await commentStorage(page), storageBefore);
      assert.deepEqual(report.blockedBusinessRequests, []);
      assert.deepEqual(report.pageErrors, []);
      pass('focused header checks have no business requests, storage changes or browser errors');
      return;
    }
    if (process.env.SHOWCASE_ONLY === 'post-footer') {
      await postFooter(page);
      assertNetworkDelta();
      assert.deepEqual(await commentStorage(page), storageBefore);
      assert.deepEqual(report.blockedBusinessRequests, []);
      assert.deepEqual(report.pageErrors, []);
      pass('focused footer checks have no business requests, storage changes or browser errors');
      return;
    }
    if (process.env.SHOWCASE_ONLY === 'modal-reset') {
      await interactions(page, true);
      assertNetworkDelta();
      assert.deepEqual(await commentStorage(page), storageBefore);
      assert.deepEqual(report.blockedBusinessRequests, []);
      assert.deepEqual(report.pageErrors, []);
      pass('focused modal resets have no business requests or storage changes');
      return;
    }
    await catalog(page);
    await details(page, { width: 1440, height: 1000 }, 'light', 'no-preference', 'desktop-light');
    await details(page, { width: 390, height: 844 }, 'dark', 'reduce', 'mobile-dark');
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.emulateMedia({ colorScheme: 'light', reducedMotion: 'no-preference' });
    await interactions(page);
    await lifecycle(page);
    await failureAndLateLoad(browser);
    const storageAfter = await commentStorage(page);
    assert.deepEqual(storageAfter, storageBefore, 'showcase changed comment localStorage');
    assertNetworkDelta();
    assert.deepEqual(report.blockedBusinessRequests, [], 'showcase attempted a business request');
    assert.deepEqual(report.pageErrors, [], 'browser reported uncaught JS errors');
    pass('no new business requests, comment storage changes, or page errors');
  } catch (error) {
    report.failure = { stage, message: error.message };
    console.error('FAIL', stage, error.message);
    throw error;
  } finally {
    fs.writeFileSync(path.join(ARTIFACTS, 'report.json'), JSON.stringify(report, null, 2));
    console.log('ARTIFACTS', ARTIFACTS);
    await browser.close();
  }
}
main().catch(error => { console.error('FAIL', error); process.exitCode = 1; });
