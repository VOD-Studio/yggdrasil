#!/usr/bin/env node
/**
 * Browser acceptance for the public component atlas. Connects to an existing site only.
 *
 * SHOWCASE_BASE=http://127.0.0.1:8080 PLAYWRIGHT_MODULE=/path/to/playwright \
 *   CHROMIUM_PATH=/path/to/chromium node scripts/test-components-showcase.cjs
 * Optional SHOWCASE_ARTIFACT_DIR stores screenshots/report; default is a temporary directory.
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
  pass('19 catalog cards render their actual UI');

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
  pass(`19 direct details at ${viewport.width}px ${colorScheme} ${reducedMotion}`);
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
  await dialog.getByRole('button', { name: '初春的小狗.webp' }).click();
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
