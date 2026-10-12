#!/usr/bin/env bun
/**
 * Real admin UI acceptance against an already running disposable development site.
 * Writes one existing article. ADMIN_POST_ID and ADMIN_SESSION must point at test fixtures.
 *
 * ADMIN_BASE=http://127.0.0.1:8080 ADMIN_POST_ID=1 \
 * ADMIN_SESSION=post-write-regression PLAYWRIGHT_MODULE=/path/to/playwright \
 * CHROMIUM_PATH=/path/to/chromium bun scripts/test-admin-editor.cjs
 * Optional ADMIN_ARTIFACT_DIR stores screenshots and a JSON report.
 * Does not install dependencies or start/stop the server. Load/save error scenarios
 * inject HTTP 503; successful loading, saving, asset selection, config generation,
 * clipboard operations and log streaming use the application.
 */
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');

const BASE = (process.env.ADMIN_BASE || 'http://127.0.0.1:8080').replace(/\/$/, '');
const POST_ID = Number(process.env.ADMIN_POST_ID);
const SESSION = process.env.ADMIN_SESSION || 'post-write-regression';
assert(Number.isInteger(POST_ID) && POST_ID > 0, 'ADMIN_POST_ID must name an existing disposable test article');
const ARTIFACTS = process.env.ADMIN_ARTIFACT_DIR || fs.mkdtempSync(path.join(os.tmpdir(), 'yggdrasil-admin-'));
fs.mkdirSync(ARTIFACTS, { recursive: true });
const EDIT_PATH = `/admin/write/${POST_ID}`;
const TITLE = '后台浏览器回归文章';
const BODY = '后台编辑器回归正文，保存失败后仍保留这段内容。';
const FAKE_TOKEN = 'ygg_browser_acceptance_fixture';
const report = { base: BASE, postId: POST_ID, artifactDir: ARTIFACTS, passed: [], screenshots: [],
  requests: [], pageErrors: [], layout: [] };
let stage = 'startup';
let currentPage;
let saveFailures = 0;
let loadFailures = 0;
let saveAttempts = 0;

function apiName(url) {
  return new URL(url).pathname.split('/').pop().replace(/[^a-z]/gi, '').toLowerCase();
}
function isApi(response, name) {
  return new URL(response.url()).pathname.startsWith('/api/') && apiName(response.url()).startsWith(name);
}
function pass(name) {
  report.passed.push(name);
  console.log('PASS', name);
}
async function screenshot(page, name) {
  const filename = path.join(ARTIFACTS, `${name}.png`);
  await page.screenshot({ path: filename, fullPage: false, animations: 'disabled' });
  report.screenshots.push(filename);
}
async function settle(page) {
  await page.waitForFunction(() => !document.documentElement.classList.contains('is-route-transitioning'));
}
async function editorReady(page) {
  await page.locator('#tiptap-editor .ProseMirror').waitFor({ timeout: 90000 });
  await page.getByRole('button', { name: '更新文章', exact: true }).waitFor();
  await page.waitForFunction(() => {
    const button = [...document.querySelectorAll('button')].find(el => el.textContent.trim() === '更新文章');
    return button && !button.disabled;
  });
  await settle(page);
}
async function spa(page, route) {
  await page.waitForFunction(() => window.__routeTransitions?.push);
  await page.evaluate(route => window.__routeTransitions.push(route), route);
  await page.waitForURL(`${BASE}${route}`);
  await settle(page);
}
async function editorCounts(page) {
  return page.evaluate(() => ({ creates: window.__adminRegression.editorCreates,
    destroys: window.__adminRegression.editorDestroys,
    active: window.TiptapEditor?._instances?.size || 0 }));
}
async function createContext(browser, variant) {
  const context = await browser.newContext({ viewport: variant.viewport,
    colorScheme: variant.scheme, reducedMotion: variant.motion || 'no-preference',
    permissions: ['clipboard-read', 'clipboard-write'] });
  await context.addCookies([{ name: 'session', value: SESSION, url: BASE, httpOnly: true, sameSite: 'Lax' }]);
  await context.addInitScript(() => {
    const stats = { editorCreates: 0, editorDestroys: 0, streams: [], document: Math.random().toString(36) };
    window.__adminRegression = stats;
    // Wrap the IIFE's global assignment before the lazy library loader resolves.
    let editorModule;
    Object.defineProperty(window, 'TiptapEditor', {
      configurable: true,
      get: () => editorModule,
      set(value) {
        editorModule = value;
        if (!value?.create || value.__adminInstrumented) return;
        value.__adminInstrumented = true;
        const create = value.create;
        value.create = function (...args) {
          const instance = create.apply(this, args);
          if (instance) {
            stats.editorCreates++;
            const destroy = instance.destroy;
            let destroyed = false;
            instance.destroy = function (...args) {
              if (!destroyed) {
                destroyed = true;
                stats.editorDestroys++;
              }
              return destroy.apply(this, args);
            };
          }
          return instance;
        };
      },
    });
    const OriginalEventSource = window.EventSource;
    window.EventSource = class extends OriginalEventSource {
      constructor(...args) {
        super(...args);
        this.__adminStream = { url: String(args[0]), closed: false, opened: false };
        this.addEventListener('open', () => { this.__adminStream.opened = true; });
        stats.streams.push(this.__adminStream);
      }
      close() {
        this.__adminStream.closed = true;
        return super.close();
      }
    };
  });
  const page = await context.newPage();
  currentPage = page;
  page.setDefaultTimeout(20000);
  page.on('pageerror', error => report.pageErrors.push({ stage, message: error.message }));
  page.on('request', request => {
    const url = new URL(request.url());
    if (url.origin === new URL(BASE).origin && url.pathname.startsWith('/api/')) {
      report.requests.push({ stage, method: request.method(), path: url.pathname });
    }
  });
  await page.route('https://fonts.googleapis.com/**', route =>
    route.fulfill({ status: 200, contentType: 'text/css', body: '' }));
  await page.route('**/api/**', async route => {
    const request = route.request();
    const name = apiName(request.url());
    if (name.startsWith('updatepost')) {
      saveAttempts++;
      if (saveFailures > 0) {
        saveFailures--;
        await new Promise(resolve => setTimeout(resolve, 350));
        await route.fulfill({ status: 503, contentType: 'text/plain', body: 'Acceptance save failure' });
        return;
      }
    }
    if (name.startsWith('getpostbyid') && loadFailures > 0) {
      loadFailures--;
      await route.fulfill({ status: 503, contentType: 'text/plain', body: 'Acceptance load failure' });
      return;
    }
    await route.continue();
  });
  return { context, page };
}

async function main() {
  const browser = await chromium.launch({ headless: true,
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
    args: ['--no-sandbox'] });
  try {
    const { context, page } = await createContext(browser, {
      viewport: { width: 1440, height: 1000 }, scheme: 'light' });
    stage = 'editor-backfill';
    const read = page.waitForResponse(response => isApi(response, 'getpostbyid'));
    await page.goto(`${BASE}${EDIT_PATH}`, { waitUntil: 'domcontentloaded', timeout: 90000 });
    const fixture = (await (await read).json()).post;
    assert(fixture, 'test article must exist');
    await editorReady(page);
    assert.equal(await page.getByPlaceholder('输入文章标题...').inputValue(), fixture.title);
    assert.equal(await page.getByPlaceholder('自定义 URL，留空自动生成...').inputValue(), fixture.slug);
    assert.equal(await page.getByPlaceholder('选填，留空将自动从正文提取...').inputValue(), fixture.summary || '');
    const markdown = await page.evaluate(() => window.TiptapEditor._instances.get('tiptap-editor').getMarkdown());
    assert.equal(markdown.trim(), fixture.content_md.trim(), 'loaded Markdown must reach the editor');
    assert.deepEqual(await editorCounts(page), { creates: 1, destroys: 0, active: 1 });
    await page.evaluate(() => { window.__originalEditor = window.TiptapEditor._instances.get('tiptap-editor'); });
    pass('edit route backfills real article metadata and Markdown once');
    await page.waitForFunction(() => {
      const img = document.querySelector('.cover-preview img');
      return img?.complete && img.naturalWidth > 0;
    });
    await screenshot(page, 'light-editor-backfill');

    stage = 'editor-input';
    await page.getByPlaceholder('输入文章标题...').fill(TITLE);
    await page.getByPlaceholder('选填，留空将自动从正文提取...').fill('后台浏览器回归摘要');
    await page.getByPlaceholder('输入标签，逗号或分号分隔...').fill('Rust，测试；表单');
    await page.locator('.cover-preview').hover();
    await page.locator('.cover-preview').getByRole('button', { name: '移除', exact: true }).click();
    await page.getByText('输入链接', { exact: true }).click();
    await page.getByPlaceholder('粘贴图片链接...').fill('/images/xiantiaoxiaogou_02.webp');
    await page.getByRole('button', { name: '确认', exact: true }).click();
    await page.waitForFunction(() => {
      const img = document.querySelector('.cover-preview img');
      return img?.complete && img.naturalWidth > 0;
    });
    await page.locator('#tiptap-editor .ProseMirror').fill(BODY);
    await page.waitForFunction(body => window.TiptapEditor._instances.get('tiptap-editor').getMarkdown().includes(body), BODY);
    assert(await page.evaluate(() => window.__originalEditor === window.TiptapEditor._instances.get('tiptap-editor')),
      'metadata/cover/body input must retain the existing editor handle');
    assert.deepEqual(await editorCounts(page), { creates: 1, destroys: 0, active: 1 });
    assert.equal(await page.locator('.cover-preview .animate-pulse').count(), 0,
      'body changes must not restart the loaded cover preview');
    pass('metadata, cover and body input retain editor and loaded cover');

    stage = 'cover-picker';
    await page.locator('.cover-preview').hover();
    await page.locator('.cover-preview').getByRole('button', { name: '素材库', exact: true }).click();
    const picker = page.getByRole('dialog', { name: '选择封面图' });
    await picker.waitFor();
    await picker.locator(`button:has(img[src^="${fixture.cover_image}"])`).click();
    await picker.waitFor({ state: 'hidden' });
    assert.equal(await page.locator('.cover-preview img').getAttribute('data-src'), fixture.cover_image);
    assert.equal((await editorCounts(page)).creates, 1);
    pass('cover asset picker loads real assets and updates the form');

    stage = 'save-failure';
    saveFailures = 1;
    const attemptsBefore = saveAttempts;
    await page.getByRole('button', { name: '更新文章', exact: true }).evaluate(button => {
      button.click(); button.click(); button.click();
    });
    await page.getByText(/更新失败:/).waitFor();
    assert.equal(saveAttempts - attemptsBefore, 1, 'rapid duplicate clicks must start one request');
    assert.equal(await page.getByPlaceholder('输入文章标题...').inputValue(), TITLE);
    assert(await page.evaluate(body => window.TiptapEditor._instances.get('tiptap-editor').getMarkdown().includes(body), BODY));
    await screenshot(page, 'light-save-failure');
    pass('failed save retains editable data and duplicate clicks issue one request');

    stage = 'save-success';
    const save = page.waitForResponse(response => isApi(response, 'updatepost'));
    await page.getByRole('button', { name: '更新文章', exact: true }).click();
    assert.equal((await (await save).json()).success, true, 'retry must save to the disposable database');
    await page.waitForURL(`${BASE}/admin/posts`);
    await page.waitForFunction(() => window.TiptapEditor._instances.size === 0);
    assert.deepEqual(await editorCounts(page), { creates: 1, destroys: 1, active: 0 });
    const reread = page.waitForResponse(response => isApi(response, 'getpostbyid'));
    await spa(page, EDIT_PATH);
    const persisted = (await (await reread).json()).post;
    await editorReady(page);
    assert.equal(persisted.title, TITLE);
    assert(persisted.content_md.includes(BODY));
    assert.deepEqual([...persisted.tags].sort(), ['Rust', '测试', '表单'].sort());
    assert.equal(persisted.summary, '后台浏览器回归摘要');
    assert.equal(persisted.cover_image, fixture.cover_image);
    assert.deepEqual(await editorCounts(page), { creates: 2, destroys: 1, active: 1 });
    await page.getByRole('link', { name: '文章列表', exact: true }).click();
    await page.waitForURL(`${BASE}/admin/posts`);
    await page.waitForFunction(() => window.TiptapEditor._instances.size === 0);
    assert.deepEqual(await editorCounts(page), { creates: 2, destroys: 2, active: 0 });
    pass('real retry persists one form snapshot; SPA remount/unmount creates and destroys handles');

    stage = 'load-failure-retry';
    loadFailures = 1;
    await spa(page, EDIT_PATH);
    await page.getByText(/加载失败:/).waitFor();
    assert.equal(await page.locator('#tiptap-editor .ProseMirror').count(), 0);
    await screenshot(page, 'light-load-failure');
    await page.getByRole('button', { name: '重试', exact: true }).click();
    await editorReady(page);
    assert.equal(await page.getByPlaceholder('输入文章标题...').inputValue(), TITLE);
    pass('failed load presents retry and then initializes the real article');

    stage = 'mcp-copy';
    await spa(page, '/admin/mcp');
    await page.getByRole('heading', { name: 'MCP 服务器', exact: true }).waitFor();
    await page.getByPlaceholder('ygg_xxxxxxxxxxxxxxxxxxxx').fill(FAKE_TOKEN);
    const copy = page.locator('.mcp-copy-button-primary');
    await page.waitForFunction(() => {
      const button = document.querySelector('.mcp-copy-button-primary');
      return button && !button.disabled && !button.textContent.includes('准备中');
    });
    await copy.click();
    await page.waitForFunction(() => document.querySelector('.mcp-copy-button-primary')?.dataset.copied === 'true');
    assert((await page.evaluate(() => navigator.clipboard.readText())).includes(FAKE_TOKEN));
    await screenshot(page, 'light-mcp-config');
    pass('MCP configuration is generated and actual clipboard copy reports success');

    stage = 'assets-smoke';
    await spa(page, '/admin/assets');
    await page.getByRole('heading', { name: '素材管理', exact: true }).waitFor();
    await page.locator('.assets-lightbox').waitFor();
    assert(await page.locator('.assets-lightbox img.blur-img-full').count() > 0);
    await page.getByRole('button', { name: /被 \d+ 处引用/ }).first().click();
    await page.getByText(/引用此素材 ·/).waitFor();
    await screenshot(page, 'light-assets-references');
    await page.keyboard.press('Escape');
    await page.getByText(/引用此素材 ·/).waitFor({ state: 'hidden' });
    await page.getByRole('button', { name: '未引用', exact: true }).click();
    await page.getByRole('heading', { name: '暂无素材', exact: true }).waitFor();
    pass('asset cards, protected references and filter tabs render and respond');

    stage = 'logs-smoke';
    await spa(page, '/admin/logs');
    await page.getByRole('heading', { name: '运行日志', exact: true }).waitFor();
    await page.waitForFunction(() => window.__adminRegression.streams.at(-1)?.opened);
    await page.getByText('已连接', { exact: true }).waitFor();
    const beforeFilters = await page.evaluate(() => window.__adminRegression.streams.length);
    await page.getByRole('button', { name: 'DEBUG', exact: true }).click();
    await page.waitForFunction(before => window.__adminRegression.streams.length > before, beforeFilters);
    const beforeSearch = await page.evaluate(() => window.__adminRegression.streams.length);
    await page.getByPlaceholder('搜索日志内容，Enter 生效').fill('后台回归');
    await page.getByPlaceholder('搜索日志内容，Enter 生效').press('Enter');
    await page.waitForFunction(before => window.__adminRegression.streams.length > before &&
      window.__adminRegression.streams.at(-1)?.opened, beforeSearch);
    assert(await page.evaluate(() => window.__adminRegression.streams.slice(0, -1).every(stream => stream.closed)),
      'filter remount must close earlier EventSource instances');
    await screenshot(page, 'light-logs-filter');
    await spa(page, '/admin/posts');
    assert(await page.evaluate(() => window.__adminRegression.streams.every(stream => stream.closed)),
      'leaving logs must close the remaining EventSource');
    pass('log level/search filtering reconnects once and closes prior streams on remount/unmount');
    await context.close();

    for (const variant of [
      { name: 'dark', viewport: { width: 1280, height: 820 }, scheme: 'dark' },
      { name: 'mobile', viewport: { width: 390, height: 844 }, scheme: 'light' },
      { name: 'reduced', viewport: { width: 1200, height: 820 }, scheme: 'light', motion: 'reduce' },
    ]) {
      stage = variant.name;
      const { context, page } = await createContext(browser, variant);
      await page.goto(`${BASE}${EDIT_PATH}`, { waitUntil: 'domcontentloaded', timeout: 90000 });
      await editorReady(page);
      assert.equal(await page.getByPlaceholder('输入文章标题...').inputValue(), TITLE);
      report.layout.push(await page.evaluate(name => ({ name, width: innerWidth,
        documentWidth: document.documentElement.scrollWidth,
        editorWidth: document.querySelector('#tiptap-editor').getBoundingClientRect().width,
        dark: document.documentElement.classList.contains('dark') }), variant.name));
      assert.equal(report.layout.at(-1).dark, variant.scheme === 'dark', 'theme must match the browser preference');
      await screenshot(page, `${variant.name}-editor`);
      if (variant.name === 'mobile') {
        assert(report.layout.at(-1).editorWidth >= 180, 'narrow screens must start with a usable editor');
        assert.equal(await page.locator('[data-vt-scroll="admin-write-settings"]').count(), 0);
        await page.getByRole('button', { name: '文章设置', exact: true }).click();
        await page.locator('[data-vt-scroll="admin-write-settings"]').waitFor();
        await page.getByPlaceholder('选填，留空将自动从正文提取...').fill('侧栏状态检查');
        await page.waitForTimeout(100);
        assert.equal(await page.locator('[data-vt-scroll="admin-write-settings"]').count(), 1,
          'form changes must retain the user-selected sidebar state');
        await page.getByPlaceholder('选填，留空将自动从正文提取...').fill('后台浏览器回归摘要');
        await screenshot(page, 'mobile-settings');
      }
      await page.getByRole('button', { name: '收起设置', exact: true }).click();
      await page.locator('[data-vt-scroll="admin-write-settings"]').waitFor({ state: 'hidden' });
      await screenshot(page, `${variant.name}-focused-editor`);
      assert(await page.locator('#tiptap-editor').evaluate(el => el.getBoundingClientRect().width >= 180),
        'focused editor must retain usable width');
      await context.close();
      pass(`${variant.name}: direct load, backfill and focus toggle`);
    }
    assert.deepEqual(report.pageErrors, [], 'browser must not report uncaught application errors');
  } catch (error) {
    report.failure = String(error.stack || error);
    if (currentPage && !currentPage.isClosed()) await screenshot(currentPage, 'failure').catch(() => {});
    throw error;
  } finally {
    await browser.close();
    fs.writeFileSync(path.join(ARTIFACTS, 'report.json'), JSON.stringify(report, null, 2));
    console.log('Artifacts:', ARTIFACTS);
  }
}

main().catch(error => { console.error(error); process.exitCode = 1; });
