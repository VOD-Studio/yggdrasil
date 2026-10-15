#!/usr/bin/env bun
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE);
const base = process.env.ADMIN_BASE;
const artifacts = process.env.REGRESSION_ARTIFACT_DIR;
async function main() {
  const browser = await chromium.launch({ headless: true });
  const report = { pages: [], errors: [], requests: [] };
  try {
    for (const variant of [
      { name: 'desktop-light', viewport: { width: 1440, height: 1000 }, colorScheme: 'light' },
      { name: 'mobile-dark', viewport: { width: 390, height: 844 }, colorScheme: 'dark', reducedMotion: 'reduce' },
    ]) {
      const { name, ...options } = variant;
      const context = await browser.newContext(options);
      await context.addCookies([{ name: 'session', value: process.env.ADMIN_SESSION, url: base }]);
      const page = await context.newPage();
      page.on('pageerror', e => report.errors.push(e.message));
      page.on('request', r => { if (r.url().startsWith(base)) report.requests.push(r.url().slice(base.length)); });
      await page.route('https://fonts.googleapis.com/**', r => r.fulfill({ body: '' }));
      for (const route of ['/', '/archives', '/friends', '/about', '/search', '/writing-guide', '/notes', '/admin/notes', '/admin/notebooks', '/admin/system', '/admin/runner']) {
        await page.goto(base + route);
        await page.waitForFunction(() => window.__routeTransitions?.push, null, { timeout: 60000 });
        await page.locator('h1').first().waitFor({ timeout: 60000 });
        if (route === '/admin/notes') await page.getByLabel('快速记录').waitFor();
        if (route === '/admin/runner') await page.locator('.cm-editor').waitFor({ timeout: 60000 });
        await page.waitForFunction(() => !document.documentElement.classList.contains('is-route-transitioning'));
        assert(!page.url().includes('/login'), `Unauthorized fixture at ${route}`);
        await page.screenshot({ path: path.join(artifacts, `${name}-${route.replaceAll('/', '-')}.png`), fullPage: true });
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
main().catch(e => { console.error(e); process.exitCode = 1; });
