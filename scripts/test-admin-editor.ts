#!/usr/bin/env bun
/**
 * Real admin UI acceptance against an already running disposable development site.
 * Writes one existing article. ADMIN_POST_ID and ADMIN_SESSION must point at test fixtures.
 *
 * ADMIN_BASE=http://127.0.0.1:8080 ADMIN_POST_ID=1 \
 * ADMIN_SESSION=post-write-regression PLAYWRIGHT_MODULE=/path/to/playwright \
 * CHROMIUM_PATH=/path/to/chromium bun scripts/test-admin-editor.ts
 * Optional ADMIN_ARTIFACT_DIR stores screenshots and a JSON report.
 * Does not install dependencies or start/stop the server. Load/save error scenarios
 * inject HTTP 503; successful loading, saving, asset selection, config generation,
 * clipboard operations and log streaming use the application.
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type * as Playwright from 'playwright';

const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright') as typeof Playwright;

interface EditorInstance {
  getMarkdown(): string;
  destroy(...args: unknown[]): unknown;
}
interface EditorModule {
  _instances: Map<string, EditorInstance>;
  create(...args: unknown[]): EditorInstance | null;
  __adminInstrumented?: boolean;
}
interface StreamRecord {
  url: string;
  closed: boolean;
  opened: boolean;
}
interface RegressionStats {
  editorCreates: number;
  editorDestroys: number;
  streams: StreamRecord[];
  document: string;
}
type ScriptWindow = Window &
  typeof globalThis & {
    __routeTransitions: { push(route: string): void };
    __adminRegression: RegressionStats;
    TiptapEditor: EditorModule;
    __originalEditor: EditorInstance | undefined;
  };
type Variant = {
  name?: string;
  viewport: Playwright.ViewportSize;
  scheme: Playwright.BrowserContextOptions['colorScheme'];
  motion?: Playwright.BrowserContextOptions['reducedMotion'];
};
interface Layout {
  name: string;
  width: number;
  documentWidth: number;
  editorWidth: number;
  dark: boolean;
}
interface PostFixture {
  title: string;
  slug: string;
  summary: string | null;
  content_md: string;
  cover_image: string;
  tags: string[];
}

const BASE = (process.env.ADMIN_BASE || 'http://127.0.0.1:8080').replace(/\/$/, '');
const POST_ID = Number(process.env.ADMIN_POST_ID);
const SESSION = process.env.ADMIN_SESSION! || 'post-write-regression';
assert(
  Number.isInteger(POST_ID) && POST_ID > 0,
  'ADMIN_POST_ID must name an existing disposable test article',
);
const ARTIFACTS =
  process.env.ADMIN_ARTIFACT_DIR || fs.mkdtempSync(path.join(os.tmpdir(), 'yggdrasil-admin-'));
fs.mkdirSync(ARTIFACTS, { recursive: true });
const EDIT_PATH = `/admin/write/${POST_ID}`;
const TITLE = '后台浏览器回归文章';
const BODY = '后台编辑器回归正文，保存失败后仍保留这段内容。';
const report: {
  base: string;
  postId: number;
  artifactDir: string;
  passed: string[];
  screenshots: string[];
  requests: { stage: string; method: string; path: string }[];
  pageErrors: { stage: string; message: string }[];
  layout: Layout[];
  failure?: string;
} = {
  base: BASE,
  postId: POST_ID,
  artifactDir: ARTIFACTS,
  passed: [],
  screenshots: [],
  requests: [],
  pageErrors: [],
  layout: [],
};
let stage = 'startup';
let currentPage: Playwright.Page | undefined;
let saveFailures = 0;
let loadFailures = 0;
let saveAttempts = 0;

function apiName(url: string) {
  return new URL(url).pathname
    .split('/')
    .pop()!
    .replace(/[^a-z]/gi, '')
    .toLowerCase();
}
function isApi(response: Playwright.Response, name: string) {
  return (
    new URL(response.url()).pathname.startsWith('/api/') && apiName(response.url()).startsWith(name)
  );
}
function pass(name: string) {
  report.passed.push(name);
  console.log('PASS', name);
}
async function screenshot(page: Playwright.Page, name: string) {
  const filename = path.join(ARTIFACTS, `${name}.png`);
  await page.screenshot({ path: filename, fullPage: false, animations: 'disabled' });
  report.screenshots.push(filename);
}
async function settle(page: Playwright.Page) {
  await page.waitForFunction(
    () => !document.documentElement.classList.contains('is-route-transitioning'),
  );
}
// 编辑器挂载、文章回填完成，页头保存状态为「已保存」。
async function editorReady(page: Playwright.Page) {
  await page.locator('#tiptap-editor .ProseMirror').waitFor({ timeout: 90000 });
  await page.locator('.write-status', { hasText: '已保存' }).waitFor();
  await settle(page);
}
async function spa(page: Playwright.Page, route: string) {
  await page.waitForFunction(() => (window as ScriptWindow).__routeTransitions?.push);
  await page.evaluate((route) => (window as ScriptWindow).__routeTransitions.push(route), route);
  await page.waitForURL(`${BASE}${route}`);
  await settle(page);
}
async function editorCounts(page: Playwright.Page) {
  return page.evaluate(() => ({
    creates: (window as ScriptWindow).__adminRegression.editorCreates,
    destroys: (window as ScriptWindow).__adminRegression.editorDestroys,
    active: (window as ScriptWindow).TiptapEditor?._instances?.size || 0,
  }));
}
async function createContext(browser: Playwright.Browser, variant: Variant) {
  const context = await browser.newContext({
    viewport: variant.viewport,
    colorScheme: variant.scheme,
    reducedMotion: variant.motion || 'no-preference',
    permissions: ['clipboard-read', 'clipboard-write'],
  });
  await context.addCookies([
    { name: 'session', value: SESSION, url: BASE, httpOnly: true, sameSite: 'Lax' },
  ]);
  await context.addInitScript(() => {
    const stats: RegressionStats = {
      editorCreates: 0,
      editorDestroys: 0,
      streams: [],
      document: Math.random().toString(36),
    };
    (window as ScriptWindow).__adminRegression = stats;
    // Wrap the IIFE's global assignment before the lazy library loader resolves.
    let editorModule: EditorModule | undefined;
    Object.defineProperty(window as ScriptWindow, 'TiptapEditor', {
      configurable: true,
      get: () => editorModule,
      set(value: EditorModule | undefined) {
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
    const OriginalEventSource = (window as ScriptWindow).EventSource;
    (window as ScriptWindow).EventSource = class extends OriginalEventSource {
      __adminStream: StreamRecord;
      constructor(...args: ConstructorParameters<typeof EventSource>) {
        super(...args);
        this.__adminStream = { url: String(args[0]), closed: false, opened: false };
        this.addEventListener('open', () => {
          this.__adminStream.opened = true;
        });
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
  page.on('pageerror', (error) => report.pageErrors.push({ stage, message: error.message }));
  page.on('request', (request) => {
    const url = new URL(request.url());
    if (url.origin === new URL(BASE).origin && url.pathname.startsWith('/api/')) {
      report.requests.push({ stage, method: request.method(), path: url.pathname });
    }
  });
  await page.route('https://fonts.googleapis.com/**', (route) =>
    route.fulfill({ status: 200, contentType: 'text/css', body: '' }),
  );
  await page.route('**/api/**', async (route) => {
    const request = route.request();
    const name = apiName(request.url());
    if (name.startsWith('updatepost')) {
      saveAttempts++;
      if (saveFailures > 0) {
        saveFailures--;
        await new Promise((resolve) => setTimeout(resolve, 350));
        await route.fulfill({
          status: 503,
          contentType: 'text/plain',
          body: 'Acceptance save failure',
        });
        return;
      }
    }
    if (name.startsWith('getpostbyid') && loadFailures > 0) {
      loadFailures--;
      await route.fulfill({
        status: 503,
        contentType: 'text/plain',
        body: 'Acceptance load failure',
      });
      return;
    }
    await route.continue();
  });
  return { context, page };
}

// 会话查询失败不得当作未登录；验证成功前不挂载编辑器或发出文章请求。
async function verifyAuth(browser: Playwright.Browser) {
  for (const scenario of [
    { name: 'service-error', viewport: { width: 1440, height: 1000 }, anonymous: false },
    { name: 'network-error', viewport: { width: 390, height: 844 }, anonymous: true },
  ]) {
    stage = `auth-${scenario.name}`;
    const { context, page } = await createContext(browser, { ...scenario, scheme: 'light' });
    let attempts = 0;
    const { promise: pendingRetry, resolve: releaseRetry } = Promise.withResolvers<void>();
    const { promise: startedRetry, resolve: retryStarted } = Promise.withResolvers<void>();
    await page.route('**/api/get_current_user', async (route) => {
      attempts++;
      if (attempts === 1) {
        if (scenario.name === 'network-error') return route.abort('failed');
        return route.fulfill({
          status: 503,
          contentType: 'text/html',
          body: '<h1>503 Service Unavailable</h1>',
        });
      }
      retryStarted();
      await pendingRetry;
      if (scenario.anonymous) {
        return route.fulfill({ contentType: 'application/json', body: '{"user":null}' });
      }
      return route.continue();
    });
    try {
      await page.goto(`${BASE}${EDIT_PATH}`, { waitUntil: 'domcontentloaded', timeout: 90000 });
      const failure = page.getByRole('group', { name: '登录状态验证失败', exact: true });
      await failure.waitFor({ timeout: 90000 });
      await settle(page);
      assert.equal(page.url(), `${BASE}${EDIT_PATH}`, 'lookup errors must retain the admin URL');
      assert.equal(await page.locator('#write-title').count(), 0);
      assert(
        !report.requests.some(
          (request) =>
            request.stage === stage &&
            ['get_post_by_id', 'get_admin_nav_counts'].includes(request.path.split('/').pop()!),
        ),
        'unverified sessions must not start admin data requests',
      );
      await screenshot(page, stage);
      const retry = failure.getByRole('button', { name: '重新加载', exact: true });
      await retry.evaluate((button: HTMLButtonElement) => {
        button.click();
        button.click();
        button.click();
      });
      await startedRetry;
      assert.equal(attempts, 2, 'duplicate retry clicks must issue only one request');
      assert(await retry.isDisabled(), 'retry must be disabled while the lookup is pending');
      assert.equal(await page.locator('#write-title').count(), 0);
      releaseRetry();
      if (scenario.anonymous) {
        await page.waitForURL(`${BASE}/login`);
        await page.locator('#login-username').waitFor();
        pass('network failure stays in admin; explicit anonymous retry redirects to login');
      } else {
        await editorReady(page);
        assert.equal(page.url(), `${BASE}${EDIT_PATH}`);
        assert.equal(await failure.count(), 0);
        await spa(page, '/admin/posts');
        await page.getByRole('heading', { name: '文章', exact: true }).waitFor();
        assert.equal(attempts, 2, 'successful verification must be reused within the admin layout');
        pass(
          'service failure blocks admin data; one retry restores the real session and navigation',
        );
      }
    } finally {
      releaseRetry();
      await context.close();
    }
  }
}

async function main() {
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
    args: ['--no-sandbox'],
  });
  try {
    await verifyAuth(browser);
    const { context, page } = await createContext(browser, {
      viewport: { width: 1440, height: 1000 },
      scheme: 'light',
    });
    stage = 'editor-backfill';
    const read = page.waitForResponse((response) => isApi(response, 'getpostbyid'));
    await page.goto(`${BASE}${EDIT_PATH}`, { waitUntil: 'domcontentloaded', timeout: 90000 });
    const fixture = ((await (await read).json()) as { post: PostFixture | null }).post;
    assert(fixture, 'test article must exist');
    await editorReady(page);
    assert.equal(await page.locator('#write-title').inputValue(), fixture.title);
    assert.equal(await page.locator('#write-slug').inputValue(), fixture.slug);
    assert.equal(await page.locator('#write-summary').inputValue(), fixture.summary || '');
    const markdown = await page.evaluate(() =>
      (window as ScriptWindow).TiptapEditor._instances.get('tiptap-editor')!.getMarkdown(),
    );
    assert.equal(
      markdown.trim(),
      fixture.content_md.trim(),
      'loaded Markdown must reach the editor',
    );
    assert.deepEqual(await editorCounts(page), { creates: 1, destroys: 0, active: 1 });
    await page.evaluate(() => {
      (window as ScriptWindow).__originalEditor = (
        window as ScriptWindow
      ).TiptapEditor._instances.get('tiptap-editor')!;
    });
    pass('edit route backfills real article metadata and Markdown once');
    await page.waitForFunction(() => {
      const img = document.querySelector<HTMLImageElement>('.write-cover img');
      return img?.complete && img.naturalWidth > 0;
    });
    await screenshot(page, 'light-editor-backfill');

    stage = 'editor-input';
    const actions = page.locator('.write-cover-actions');
    await page.locator('#write-title').fill(TITLE);
    await page.locator('#write-summary').fill('后台浏览器回归摘要');
    await page.locator('#write-tag-input').fill('Rust，测试；表单');
    await page.locator('#write-tag-input').press('Enter');
    await actions.getByRole('button', { name: '移除', exact: true }).click();
    await actions.getByRole('button', { name: '输入链接', exact: true }).click();
    await page.getByLabel('封面图片链接').fill('/images/samples/clay-circle.webp');
    await page.getByRole('button', { name: '确认', exact: true }).click();
    await page.waitForFunction(() => {
      const img = document.querySelector<HTMLImageElement>('.write-cover img');
      return img?.complete && img.naturalWidth > 0;
    });
    await page.locator('#tiptap-editor .ProseMirror').fill(BODY);
    await page.waitForFunction(
      (body) =>
        (window as ScriptWindow).TiptapEditor._instances
          .get('tiptap-editor')!
          .getMarkdown()
          .includes(body),
      BODY,
    );
    assert(
      await page.evaluate(
        () =>
          (window as ScriptWindow).__originalEditor ===
          (window as ScriptWindow).TiptapEditor._instances.get('tiptap-editor')!,
      ),
      'metadata/cover/body input must retain the existing editor handle',
    );
    assert.deepEqual(await editorCounts(page), { creates: 1, destroys: 0, active: 1 });
    assert.equal(
      await page.locator('.write-cover-loading').count(),
      0,
      'body changes must not restart the loaded cover preview',
    );
    await page.locator('.write-status', { hasText: '有未保存的修改' }).waitFor();
    pass('metadata, cover and body input retain editor and loaded cover');

    stage = 'cover-picker';
    await actions.getByRole('button', { name: '素材库', exact: true }).click();
    const picker = page.getByRole('dialog', { name: '选择封面图' });
    await picker.waitFor();
    await picker.locator(`button:has(img[src^="${fixture.cover_image}"])`).click();
    await picker.waitFor({ state: 'hidden' });
    assert.equal(
      await page.locator('.write-cover img').getAttribute('data-src'),
      fixture.cover_image,
    );
    assert.equal((await editorCounts(page)).creates, 1);
    pass('cover asset picker loads real assets and updates the form');

    stage = 'save-failure';
    saveFailures = 1;
    const attemptsBefore = saveAttempts;
    await page
      .getByRole('button', { name: '存草稿', exact: true })
      .evaluate((button: HTMLButtonElement) => {
        button.click();
        button.click();
        button.click();
      });
    await page.locator('.write-status[data-tone="danger"]').waitFor();
    assert.equal(saveAttempts - attemptsBefore, 1, 'rapid duplicate clicks must start one request');
    assert.equal(await page.locator('#write-title').inputValue(), TITLE);
    assert(
      await page.evaluate(
        (body) =>
          (window as ScriptWindow).TiptapEditor._instances
            .get('tiptap-editor')!
            .getMarkdown()
            .includes(body),
        BODY,
      ),
    );
    await screenshot(page, 'light-save-failure');
    pass('failed save retains editable data and duplicate clicks issue one request');

    stage = 'save-success';
    const save = page.waitForResponse((response) => isApi(response, 'updatepost'));
    // 失败状态可点重试，按失败时的状态再保存一次。
    await page.locator('button.write-status').click();
    assert.equal(
      (await (await save).json()).success,
      true,
      'retry must save to the disposable database',
    );
    await page.locator('.write-status', { hasText: '已保存' }).waitFor();
    assert.equal(new URL(page.url()).pathname, EDIT_PATH, 'saving keeps the editor open');
    await spa(page, '/admin/posts');
    await page.waitForFunction(() => (window as ScriptWindow).TiptapEditor._instances.size === 0);
    assert.deepEqual(await editorCounts(page), { creates: 1, destroys: 1, active: 0 });
    const reread = page.waitForResponse((response) => isApi(response, 'getpostbyid'));
    await spa(page, EDIT_PATH);
    const persisted = ((await (await reread).json()) as { post: PostFixture }).post;
    await editorReady(page);
    assert.equal(persisted.title, TITLE);
    assert(persisted.content_md.includes(BODY));
    assert.deepEqual([...persisted.tags].sort(), ['Rust', '测试', '表单'].sort());
    assert.equal(persisted.summary, '后台浏览器回归摘要');
    assert.equal(persisted.cover_image, fixture.cover_image);
    assert.deepEqual(await editorCounts(page), { creates: 2, destroys: 1, active: 1 });
    await page.getByRole('link', { name: '返回文章列表', exact: true }).click();
    await page.waitForURL(`${BASE}/admin/posts`);
    await page.waitForFunction(() => (window as ScriptWindow).TiptapEditor._instances.size === 0);
    assert.deepEqual(await editorCounts(page), { creates: 2, destroys: 2, active: 0 });
    pass('real retry persists one form snapshot; SPA remount/unmount creates and destroys handles');

    stage = 'load-failure-retry';
    loadFailures = 1;
    await spa(page, EDIT_PATH);
    await page.getByRole('heading', { name: '文章加载失败', exact: true }).waitFor();
    assert.equal(await page.locator('#tiptap-editor .ProseMirror').count(), 0);
    await screenshot(page, 'light-load-failure');
    await page.getByRole('button', { name: '重新加载', exact: true }).click();
    await editorReady(page);
    assert.equal(await page.locator('#write-title').inputValue(), TITLE);
    pass('failed load presents retry and then initializes the real article');

    stage = 'mcp-copy';
    await spa(page, '/admin/mcp');
    await page.getByRole('heading', { name: 'MCP 接入', exact: true }).waitFor();
    await page.getByRole('button', { name: '新建令牌', exact: true }).click();
    await page.getByLabel('名称').fill('browser-regression');
    await page.getByRole('button', { name: '创建令牌', exact: true }).click();
    await page.getByRole('heading', { name: '令牌已创建', exact: true }).waitFor();
    await page.getByRole('button', { name: '显示令牌', exact: true }).click();
    const plaintext = await page.locator('#mcp-plaintext').inputValue();
    assert.match(plaintext, /^ygg_/);
    await page.locator('.mcp-snippet').waitFor();
    await page.getByRole('button', { name: '复制一句话接入指令', exact: true }).click();
    await page.getByText(/已复制接入指令/).waitFor();
    assert((await page.evaluate(() => navigator.clipboard.readText())).includes(plaintext));
    await screenshot(page, 'light-mcp-config');
    pass('MCP token creation shows plaintext, client configs and copies a real agent prompt');

    stage = 'assets-smoke';
    await spa(page, '/admin/assets');
    await page.getByRole('heading', { name: '素材', exact: true }).waitFor();
    await page.locator('.asset-tile').first().waitFor();
    await page.locator('.asset-tile-open').first().click();
    await page.locator('.asset-detail').getByText('引用', { exact: true }).waitFor();
    assert((await page.locator('.asset-detail .asset-ref').count()) > 0);
    assert(await page.locator('.asset-detail').getByRole('button', { name: '删除' }).isDisabled());
    await screenshot(page, 'light-assets-references');
    await page.getByRole('tab', { name: /未引用/ }).click();
    await page.getByRole('heading', { name: '没有未引用的素材', exact: true }).waitFor();
    pass('asset grid, protected references and filter tabs render and respond');

    stage = 'logs-smoke';
    await spa(page, '/admin/logs');
    await page.getByRole('heading', { name: '运行日志', exact: true }).waitFor();
    await page.waitForFunction(
      () => (window as ScriptWindow).__adminRegression.streams.at(-1)?.opened,
    );
    await page.getByText('已连接', { exact: true }).waitFor();
    const beforeFilters = await page.evaluate(
      () => (window as ScriptWindow).__adminRegression.streams.length,
    );
    await page.getByRole('button', { name: 'DEBUG', exact: true }).click();
    await page.waitForFunction(
      (before) => (window as ScriptWindow).__adminRegression.streams.length > before,
      beforeFilters,
    );
    const beforeSearch = await page.evaluate(
      () => (window as ScriptWindow).__adminRegression.streams.length,
    );
    await page.getByPlaceholder('搜索日志内容，Enter 生效').fill('后台回归');
    await page.getByPlaceholder('搜索日志内容，Enter 生效').press('Enter');
    await page.waitForFunction(
      (before) =>
        (window as ScriptWindow).__adminRegression.streams.length > before &&
        (window as ScriptWindow).__adminRegression.streams.at(-1)?.opened,
      beforeSearch,
    );
    assert(
      await page.evaluate(() =>
        (window as ScriptWindow).__adminRegression.streams
          .slice(0, -1)
          .every((stream) => stream.closed),
      ),
      'filter remount must close earlier EventSource instances',
    );
    await screenshot(page, 'light-logs-filter');
    await spa(page, '/admin/posts');
    assert(
      await page.evaluate(() =>
        (window as ScriptWindow).__adminRegression.streams.every((stream) => stream.closed),
      ),
      'leaving logs must close the remaining EventSource',
    );
    pass('log level/search filtering reconnects once and closes prior streams on remount/unmount');
    await context.close();

    for (const variant of [
      { name: 'dark', viewport: { width: 1280, height: 820 }, scheme: 'dark' },
      { name: 'mobile', viewport: { width: 390, height: 844 }, scheme: 'light' },
      {
        name: 'reduced',
        viewport: { width: 1200, height: 820 },
        scheme: 'light',
        motion: 'reduce',
      },
    ] as const) {
      stage = variant.name;
      const { context, page } = await createContext(browser, variant);
      await page.goto(`${BASE}${EDIT_PATH}`, { waitUntil: 'domcontentloaded', timeout: 90000 });
      await editorReady(page);
      assert.equal(await page.locator('#write-title').inputValue(), TITLE);
      report.layout.push(
        await page.evaluate(
          (name) => ({
            name,
            width: innerWidth,
            documentWidth: document.documentElement.scrollWidth,
            editorWidth: document
              .querySelector<HTMLElement>('#tiptap-editor')!
              .getBoundingClientRect().width,
            dark: document.documentElement.classList.contains('dark'),
          }),
          variant.name,
        ),
      );
      assert.equal(
        report.layout.at(-1)!.dark,
        variant.scheme === 'dark',
        'theme must match the browser preference',
      );
      assert(
        report.layout.at(-1)!.documentWidth <= report.layout.at(-1)!.width,
        'no horizontal page scroll',
      );
      await screenshot(page, `${variant.name}-editor`);
      const drawer = page.locator('[data-vt-scroll="admin-write-settings"]');
      if (variant.name === 'mobile') {
        assert(
          report.layout.at(-1)!.editorWidth >= 180,
          'narrow screens must start with a usable editor',
        );
        assert.equal(await drawer.count(), 0, 'narrow screens start with the drawer closed');
        await page.getByRole('button', { name: '属性', exact: true }).click();
        await drawer.waitFor();
        await page.locator('#write-summary').fill('侧栏状态检查');
        await page.waitForTimeout(100);
        assert.equal(
          await drawer.count(),
          1,
          'form changes must retain the user-selected drawer state',
        );
        await page.locator('#write-summary').fill('后台浏览器回归摘要');
        await screenshot(page, 'mobile-settings');
      }
      await page.getByRole('button', { name: '收起属性', exact: true }).click();
      await drawer.waitFor({ state: 'hidden' });
      await screenshot(page, `${variant.name}-focused-editor`);
      assert(
        await page
          .locator('#tiptap-editor')
          .evaluate((el) => el.getBoundingClientRect().width >= 180),
        'focused editor must retain usable width',
      );
      await context.close();
      pass(`${variant.name}: direct load, backfill and focus toggle`);
    }
    assert.deepEqual(report.pageErrors, [], 'browser must not report uncaught application errors');
  } catch (error) {
    report.failure = String(error instanceof Error ? error.stack || error : error);
    if (currentPage && !currentPage.isClosed())
      await screenshot(currentPage, 'failure').catch(() => {});
    throw error;
  } finally {
    await browser.close();
    fs.writeFileSync(path.join(ARTIFACTS, 'report.json'), JSON.stringify(report, null, 2));
    console.log('Artifacts:', ARTIFACTS);
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
