#!/usr/bin/env bun
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import type * as Playwright from 'playwright';
import type { BrowserContextOptions } from 'playwright';

const { chromium } = require(process.env.PLAYWRIGHT_MODULE!) as typeof Playwright;
const base = process.env.ADMIN_BASE!;
const artifacts = process.env.REGRESSION_ARTIFACT_DIR!;
async function main() {
  const browser = await chromium.launch({ headless: true });
  const report: { pages: { name: string; route: string }[]; errors: string[]; requests: string[] } =
    { pages: [], errors: [], requests: [] };
  try {
    for (const variant of [
      { name: 'desktop-light', viewport: { width: 1440, height: 1000 }, colorScheme: 'light' },
      {
        name: 'mobile-dark',
        viewport: { width: 390, height: 844 },
        colorScheme: 'dark',
        reducedMotion: 'reduce',
      },
    ] satisfies (BrowserContextOptions & { name: string })[]) {
      const { name, ...options } = variant;
      const context = await browser.newContext(options);
      await context.addCookies([{ name: 'session', value: process.env.ADMIN_SESSION!, url: base }]);
      const page = await context.newPage();
      page.on('pageerror', (e) => report.errors.push(e.message));
      page.on('request', (r) => {
        if (r.url().startsWith(base)) report.requests.push(r.url().slice(base.length));
      });
      await page.route('https://fonts.googleapis.com/**', (r) => r.fulfill({ body: '' }));
      for (const route of [
        '/',
        '/archives',
        '/friends',
        '/about',
        '/search',
        '/about/writing',
        '/notes',
        '/admin/notes',
        '/admin/notebooks',
        '/admin/system',
        '/admin/runner',
      ]) {
        const response = await page.goto(base + route);
        assert.equal(response?.status(), 200, `${route} 应返回 200`);
        assert.equal(new URL(page.url()).pathname, route, `${route} 不应重定向`);
        await page
          .locator('html[data-hydrated="true"]')
          .waitFor({ state: 'attached', timeout: 60000 });
        await page
          .locator(`[data-vt-route="${route}"]`)
          .waitFor({ state: 'attached', timeout: 60000 });
        await page.getByRole('main').first().waitFor({ timeout: 60000 });
        if (route === '/about/writing')
          await page.getByRole('heading', { name: '写作指南', exact: true }).waitFor();
        if (route === '/admin/notes') await page.getByLabel('快速记录').waitFor();
        if (route === '/admin/runner') await page.locator('.cm-editor').waitFor({ timeout: 60000 });
        await page.waitForFunction(
          () => !document.documentElement.classList.contains('is-route-transitioning'),
        );
        assert(!page.url().includes('/login'), `Unauthorized fixture at ${route}`);
        await page.screenshot({
          path: path.join(artifacts, `${name}-${route.replaceAll('/', '-')}.png`),
          fullPage: true,
        });
        report.pages.push({ name, route });
      }
      await context.close();
    }
    assert.deepEqual(report.errors, []);
  } finally {
    fs.writeFileSync(path.join(artifacts, 'smoke.json'), JSON.stringify(report, null, 2));
    await browser.close();
  }
}
main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
