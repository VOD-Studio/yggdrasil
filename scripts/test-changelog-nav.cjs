#!/usr/bin/env bun
// Run against make dev; no content or account changes are made.
const assert = require('node:assert/strict');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = process.env.CHANGELOG_BASE || 'http://localhost:8080';
const output = process.env.CHANGELOG_SCREENSHOTS || '/tmp';

async function main() {
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
  });
  try {
    for (const scenario of [
      { name: 'desktop-light', width: 1440, height: 1000, dark: false },
      { name: 'desktop-dark', width: 1440, height: 1000, dark: true },
      { name: 'desktop-short', width: 1280, height: 680, dark: true },
      { name: 'mobile-light', width: 390, height: 844, dark: false },
      { name: 'mobile-dark', width: 390, height: 844, dark: true },
      { name: 'mobile-reduced', width: 390, height: 844, dark: true, reduced: true },
    ]) {
      const context = await browser.newContext({
        viewport: { width: scenario.width, height: scenario.height },
        reducedMotion: scenario.reduced ? 'reduce' : 'no-preference',
        colorScheme: scenario.dark ? 'dark' : 'light',
      });
      const page = await context.newPage();
      page.setDefaultTimeout(15000);
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.addInitScript(dark => {
        localStorage.setItem('yggdrasil-theme', dark ? 'dark' : 'light');
        window.__changelogDocument = Math.random();
      }, scenario.dark);
      await page.route('https://fonts.googleapis.com/**', route => route.fulfill({ status: 200, contentType: 'text/css', body: '' }));
      await page.goto(`${base}/changelog`);
      await page.waitForSelector('.changelog-nav[data-ready]');
      await page.waitForTimeout(500);
      const marker = await page.evaluate(() => window.__changelogDocument);
      const nav = page.locator('.changelog-nav');
      const scroller = page.locator('.changelog-nav-scroll');
      const links = page.locator('.changelog-nav-link');
      const count = await links.count();
      assert(count > 15, 'requires the real long version history');
      const bounds = await nav.boundingBox();
      const mobile = scenario.width < 1024;
      assert(bounds.y + bounds.height <= scenario.height - 8, 'directory must fit the viewport before scrolling');
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no page horizontal overflow');
      await page.screenshot({ path: `${output}/changelog-${scenario.name}.png` });

      // The moving pill must interpolate between rows, rather than just changing color.
      if (!scenario.reduced) {
        const motion = await page.evaluate(async () => {
          const nav = document.querySelector('.changelog-nav');
          const links = [...nav.querySelectorAll('.changelog-nav-link')];
          const indicator = nav.querySelector('.changelog-nav-indicator');
          const start = getComputedStyle(indicator).transform;
          const card = document.getElementById(links[1].hash.slice(1));
          window.scrollTo({ top: card.getBoundingClientRect().top + scrollY - 80, behavior: 'instant' });
          await new Promise(resolve => setTimeout(resolve, 100));
          const during = getComputedStyle(indicator).transform;
          await new Promise(resolve => setTimeout(resolve, 400));
          const end = getComputedStyle(indicator).transform;
          return { start, during, end };
        });
        assert.notEqual(motion.start, motion.during, 'indicator starts moving');
        assert.notEqual(motion.during, motion.end, 'indicator animates between positions');
        await page.evaluate(() => window.scrollTo({ top: 0, behavior: 'instant' }));
        await page.waitForTimeout(500);
      }

      const pageTop = await page.evaluate(() => scrollY);
      await scroller.hover();
      await page.mouse.wheel(mobile ? 20000 : 0, mobile ? 0 : 20000);
      await page.waitForTimeout(400);
      const directoryEnd = await scroller.evaluate(el => ({
        atEnd: el.scrollTop + el.clientHeight >= el.scrollHeight - 2,
        atRight: el.scrollLeft + el.clientWidth >= el.scrollWidth - 2,
        hasStartFade: el.hasAttribute('data-overflow-start'),
        hasEndFade: el.hasAttribute('data-overflow-end'),
      }));
      assert(mobile ? directoryEnd.atRight : directoryEnd.atEnd, 'oldest versions must be reachable inside directory');
      assert(directoryEnd.hasStartFade && !directoryEnd.hasEndFade, 'overflow fades follow the directory edges');
      assert.equal(await page.evaluate(() => scrollY), pageTop, 'directory scrolling does not move the page');
      await page.mouse.wheel(mobile ? 1000 : 0, mobile ? 0 : 1000);
      await page.waitForTimeout(150);
      assert.equal(await page.evaluate(() => scrollY), pageTop, 'wheel at directory edge does not escape to the page');

      const target = links.nth(count - 3);
      const hash = await target.getAttribute('href');
      await target.click();
      await page.mouse.move(scenario.width - 20, 200);
      await page.waitForFunction(hash => document.querySelector('.changelog-nav-link[aria-current]')?.getAttribute('href') === hash, hash);
      await page.waitForTimeout(scenario.reduced ? 100 : 1500);
      assert.equal(await page.evaluate(() => window.__changelogDocument), marker, 'anchor clicks do not reload the document');
      assert.equal(await page.evaluate(() => location.hash), hash);
      const landing = await page.evaluate(hash => {
        const card = document.getElementById(hash.slice(1)).getBoundingClientRect();
        const directory = document.querySelector('.changelog-nav').getBoundingClientRect();
        const header = document.querySelector('header.sticky').getBoundingClientRect();
        const scroller = document.querySelector('.changelog-nav-scroll').getBoundingClientRect();
        const link = document.querySelector('.changelog-nav-link[aria-current]').getBoundingClientRect();
        return { top: card.top, below: innerWidth < 1024 ? directory.bottom : header.bottom,
          visible: innerWidth < 1024 ? link.left >= scroller.left && link.right <= scroller.right : link.top >= scroller.top && link.bottom <= scroller.bottom,
          transition: getComputedStyle(document.querySelector('.changelog-nav-indicator')).transitionDuration };
      }, hash);
      assert(landing.top >= landing.below + 8, `version heading is below sticky controls: ${JSON.stringify(landing)}`);
      assert(landing.visible, 'active version remains visible in the directory');
      assert.equal(landing.transition, scenario.reduced ? '0s' : '0.32s, 0.32s');
      await page.screenshot({ path: `${output}/changelog-${scenario.name}-old-version.png` });

      await page.goto(`${base}/changelog${hash}`);
      await page.waitForSelector('.changelog-nav[data-ready]');
      await page.waitForFunction(hash => document.querySelector('.changelog-nav-link[aria-current]')?.getAttribute('href') === hash, hash);
      await page.waitForTimeout(2200);
      await page.evaluate(() => window.scrollTo({ top: document.documentElement.scrollHeight, behavior: 'instant' }));
      await page.waitForFunction(() => {
        const links = [...document.querySelectorAll('.changelog-nav-link')];
        return links[links.length - 1].hasAttribute('aria-current');
      });
      assert.equal(await page.locator('[data-nav-position]').textContent(), `${count} / ${count}`);

      const spaMarker = await page.evaluate(() => window.__changelogDocument);
      if (mobile) await page.getByRole('button', { name: '打开导航菜单' }).click();
      await page.locator('header a[href="/about"]:visible').first().click();
      await page.waitForURL(`${base}/about`);
      await page.locator('main a[href="/changelog"]').click();
      await page.waitForSelector('.changelog-nav[data-ready]');
      assert.equal(await page.evaluate(() => window.__changelogDocument), spaMarker, 'SPA remount keeps the same document');
      assert.equal(await page.locator('.changelog-nav-link[aria-current]').count(), 1);
      assert.deepEqual(errors, []);
      console.log(`PASS ${scenario.name}: bounded directory, independent scrolling, anchor landing, active follow, direct hash, SPA remount`);
      await context.close();
    }
  } finally {
    await browser.close();
  }
}

main().catch(error => { console.error(error); process.exitCode = 1; });
