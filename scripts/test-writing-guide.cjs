#!/usr/bin/env node
// Read-only browser regression. Uses the same Playwright setup as test-view-transitions.cjs.
// VT_BASE=http://localhost:8080 PLAYWRIGHT_MODULE=/path/to/playwright node scripts/test-writing-guide.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = (process.env.VT_BASE || 'http://localhost:8080').replace(/\/$/, '');
const output = process.env.WRITING_QA_DIR || '/tmp/yggdrasil-writing-qa';

async function main() {
  await fs.mkdir(output, { recursive: true });
  const source = await fs.readFile(path.join(__dirname, '../public/skills/yggdrasil-writing/SKILL.md'), 'utf8');
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
  });
  try {
    for (const variant of [
      { name: 'desktop-light', width: 1440, colorScheme: 'light', reducedMotion: 'no-preference' },
      { name: 'desktop-dark', width: 1440, colorScheme: 'dark', reducedMotion: 'no-preference' },
      { name: 'mobile-light', width: 390, colorScheme: 'light', reducedMotion: 'reduce' },
      { name: 'mobile-dark', width: 390, colorScheme: 'dark', reducedMotion: 'reduce' },
    ]) {
      const context = await browser.newContext({
        viewport: { width: variant.width, height: 1000 },
        colorScheme: variant.colorScheme,
        reducedMotion: variant.reducedMotion,
        permissions: ['clipboard-read', 'clipboard-write'],
      });
      const page = await context.newPage();
      const errors = [];
      let documents = 0;
      page.on('pageerror', error => errors.push(error.message));
      page.on('request', request => { if (request.isNavigationRequest() && request.frame() === page.mainFrame() && new URL(request.url()).pathname.startsWith('/about')) documents++; });
      await page.route('https://fonts.googleapis.com/**', route => route.fulfill({ status: 200, contentType: 'text/css', body: '' }));
      await page.goto(`${base}/about/writing`);
      await page.locator('.writing-content h2').first().waitFor();
      await page.waitForFunction(() => window.__routeTransitions?.entryId(), null, { timeout: 90000 });
      // A successful interactive config switch establishes that WASM hydration finished.
      await page.getByRole('button', { name: 'OMP', exact: true }).click();
      await page.waitForFunction(() => document.querySelector('.writing-config-meta > code')?.textContent.includes('.omp'));
      assert.equal(await page.locator('main h1').count(), 1);
      assert.equal(await page.locator('.writing-content table').count(), 1);
      assert.equal(await page.locator('.writing-content [data-runnable]').count(), 0);
      assert.equal(await page.locator('.writing-content').getByText('description:', { exact: false }).count(), 0);
      assert(!/claude/i.test(await page.locator('.writing-page').innerText()));
      const rawResponse = await context.request.get(`${base}/skills/yggdrasil-writing/SKILL.md`);
      assert.equal(rawResponse.status(), 200);
      assert.equal(await rawResponse.text(), source);

      const installPrompt = await page.locator('.writing-install-prompt').innerText();
      assert.match(installPrompt, /https?:\/\/\S+\/skills\/yggdrasil-writing\/SKILL\.md/);
      assert(installPrompt.includes('用户全局 skill'));
      assert(installPrompt.includes('~/.agents/skills/yggdrasil-writing/SKILL.md'));
      assert(!installPrompt.includes('\n'));
      await page.getByRole('button', { name: /复制安装指令/ }).click();
      await page.waitForFunction(() => document.querySelector('.writing-install [role="status"]')?.textContent === '已复制');
      assert.equal(await page.evaluate(() => navigator.clipboard.readText()), installPrompt);
      await page.locator('.writing-install').evaluate(element => scrollTo({ top: scrollY + element.getBoundingClientRect().top - 100, behavior: 'instant' }));
      await page.locator('.writing-install').screenshot({ path: path.join(output, `${variant.name}-install.png`) });

      const copySkill = page.getByRole('button', { name: /复制 Skill 源码/ }).first();
      await copySkill.click();
      await page.waitForFunction(() => document.querySelector('.writing-actions [role="status"]')?.textContent === '已复制');
      assert.equal(await page.evaluate(() => navigator.clipboard.readText()), source);
      for (const client of ['Codex', 'OMP', 'OpenCode']) {
        await page.getByRole('button', { name: client, exact: true }).click();
        const code = page.locator('.writing-config-code');
        await page.waitForFunction(name => document.querySelector('.writing-config-code')?.getAttribute('aria-label') === `${name} MCP 配置`, client);
        const config = await code.innerText();
        assert(config.includes('YOUR_YGGDRASIL_TOKEN'));
        if (client === 'OMP') assert.equal(JSON.parse(config).mcpServers.yggdrasil.type, 'http');
        if (client === 'OpenCode') assert.equal(JSON.parse(config).mcp.yggdrasil.type, 'remote');
        if (client === 'Codex') assert(config.includes('[mcp_servers.yggdrasil]'));
        await page.getByRole('button', { name: /复制配置/ }).click();
        await page.waitForFunction(() => document.querySelector('.writing-config-meta [role="status"]')?.textContent === '已复制');
        assert.equal(await page.evaluate(() => navigator.clipboard.readText()), config);
      }
      const downloadEvent = page.waitForEvent('download');
      await page.getByRole('link', { name: '下载 SKILL.md ↓' }).click();
      const download = await downloadEvent;
      assert.equal(download.suggestedFilename(), 'SKILL.md');
      assert.equal(await fs.readFile(await download.path(), 'utf8'), source);

      await page.locator('.writing-toc summary').click();
      const headingLink = page.getByRole('navigation', { name: '技能目录' }).getByRole('link', { name: '可运行代码块', exact: true });
      const hash = await headingLink.getAttribute('href');
      await headingLink.click();
      await page.waitForFunction(expected => location.hash === expected, hash);
      assert(await page.locator(hash).isVisible());
      await page.locator('.writing-back').click();
      await page.locator('.about-writing-card').waitFor();
      await page.locator('.about-writing-card').focus();
      await page.keyboard.press('Enter');
      await page.locator('.writing-content').waitFor();
      await page.goBack();
      await page.locator('.about-writing-card').waitFor();
      await page.waitForFunction(() => !document.documentElement.classList.contains('is-route-transitioning'));
      await page.locator('.about-writing-card').screenshot({ path: path.join(output, `${variant.name}-entry.png`), animations: 'disabled' });
      await page.goForward();
      await page.locator('.writing-content').waitFor();
      await page.waitForFunction(() => !document.documentElement.classList.contains('is-route-transitioning'));
      assert.equal(await page.evaluate(() => document.documentElement.classList.contains('dark')), variant.colorScheme === 'dark');
      assert.equal(documents, 1, 'About/guide navigation should stay within the SPA');
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'page must not overflow horizontally');
      await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
      await page.screenshot({ path: path.join(output, `${variant.name}.png`), fullPage: false });
      await page.locator('.writing-connect').screenshot({ path: path.join(output, `${variant.name}-config.png`) });
      // Both unavailable Clipboard API and permission rejection must give useful feedback.
      for (const unavailable of [true, false]) {
        await page.evaluate(missing => Object.defineProperty(navigator, 'clipboard', {
          configurable: true,
          value: missing ? undefined : { writeText: () => Promise.reject(new Error('Denied')) },
        }), unavailable);
        await copySkill.click();
        await page.waitForFunction(() => document.querySelector('.writing-actions [role="status"]')?.textContent.includes('复制失败'));
      }
      assert.deepEqual(errors, []);
      console.log(`PASS ${variant.name}: render, copy, configs, download, anchors, SPA, clipboard failures`);
      await context.close();
    }
  } finally {
    await browser.close();
  }
}

main().catch(error => { console.error(error); process.exitCode = 1; });
