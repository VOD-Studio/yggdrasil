#!/usr/bin/env bun
/** Homepage anchor regression against a running development site; no content is written. */
const assert = require('node:assert/strict');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const base = (process.env.VT_BASE || 'http://127.0.0.1:8080').replace(/\/$/, '');
const output = process.env.HOME_READING_SCREENSHOTS || '/tmp';

async function ready(page) {
  await page.waitForFunction(() => window.__routeTransitions?.entryId()
    && document.querySelector('[data-vt-list="true"]')
    && document.querySelector('a[data-vt-post-link]')
    && !document.documentElement.classList.contains('is-route-transitioning'));
  // Let images and the initial reveal finish before measuring the scroll animation.
  await page.waitForTimeout(2200);
}

async function startReading(page, reduced, keyboard = false) {
  await page.evaluate(() => window.scrollTo({ top: 0, behavior: 'instant' }));
  const link = page.locator('.home-reading-link');
  if (keyboard) await link.focus();
  const before = await page.evaluate(() => ({
    entry: window.__routeTransitions.entryId(),
    navigation: window.__routeTransitions.navigationId(),
    history: history.length,
    document: window.__homeReadingDocument,
  }));
  await page.evaluate(() => {
    window.__homeReadingScrollCalls = [];
    window.__homeReadingSamples = [];
    const started = performance.now();
    function sample() {
      window.__homeReadingSamples.push(scrollY);
      if (performance.now() - started < 1000) requestAnimationFrame(sample);
    }
    requestAnimationFrame(sample);
  });
  if (keyboard) await link.press('Enter');
  else await link.click();
  await page.waitForTimeout(1100);
  const after = await page.evaluate(() => {
    const header = document.querySelector('header.sticky').getBoundingClientRect();
    const posts = document.getElementById('home-posts').getBoundingClientRect();
    return {
      entry: window.__routeTransitions.entryId(),
      navigation: window.__routeTransitions.navigationId(),
      history: history.length,
      document: window.__homeReadingDocument,
      hash: location.hash,
      route: window.__routeTransitions.currentRoute(),
      scroll: scrollY,
      gap: posts.top - header.bottom,
      calls: window.__homeReadingScrollCalls,
      samples: window.__homeReadingSamples,
    };
  });
  assert.equal(after.document, before.document, 'reading stays in the current document');
  assert.equal(after.entry, before.entry, 'reading retains the current history entry');
  assert.equal(after.navigation, before.navigation, 'same-page anchors bypass route navigation');
  assert.equal(after.history, before.history, 'repeated anchors do not add history entries');
  assert.equal(after.hash, '#home-posts');
  assert.equal(after.route, '/#home-posts', 'the route coordinator follows the replaced hash');
  assert(after.scroll > 100, 'the page scrolls to the article list');
  assert(Math.abs(after.gap - 16) <= 2,
    `the list clears the sticky header: ${JSON.stringify({ scroll: after.scroll, gap: after.gap, calls: after.calls })}`);
  assert.equal(after.calls.length, 1, 'no restoration interrupts the requested scroll');
  assert.equal(after.calls[0].behavior, reduced ? 'instant' : 'smooth');
  const intermediate = new Set(after.samples.filter(y => y > 2 && y < after.scroll - 2));
  if (!reduced) assert(intermediate.size > 3, 'scrolling visibly interpolates across frames');
  return { scroll: after.scroll, animatedFrames: intermediate.size };
}

(async () => {
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
  });
  try {
    for (const scenario of [
      { name: 'desktop-light', width: 1440, height: 1000, dark: false },
      { name: 'desktop-dark', width: 1440, height: 1000, dark: true },
      { name: 'mobile-light', width: 390, height: 844, dark: false },
      { name: 'mobile-dark', width: 390, height: 844, dark: true },
      { name: 'desktop-reduced', width: 1440, height: 1000, dark: true, reduced: true },
      { name: 'mobile-reduced', width: 390, height: 844, dark: true, reduced: true },
    ]) {
      const context = await browser.newContext({
        viewport: { width: scenario.width, height: scenario.height },
        reducedMotion: scenario.reduced ? 'reduce' : 'no-preference',
      });
      try {
        const page = await context.newPage();
        page.setDefaultTimeout(90000);
        const errors = [];
        page.on('pageerror', error => errors.push(error.message));
        await page.addInitScript(dark => {
          localStorage.setItem('yggdrasil-theme', dark ? 'dark' : 'light');
          window.__homeReadingDocument = Math.random();
          window.__homeReadingScrollCalls = [];
          const scroll = window.scrollTo.bind(window);
          window.scrollTo = (...args) => {
            window.__homeReadingScrollCalls.push(args[0]);
            scroll(...args);
          };
        }, scenario.dark);
        await page.route('https://fonts.googleapis.com/**', route => route.fulfill({
          status: 200, contentType: 'text/css', body: '',
        }));
        await page.goto(`${base}/`);
        await ready(page);
        await page.screenshot({ path: `${output}/home-reading-${scenario.name}-before.png` });
        const first = await startReading(page, scenario.reduced);
        await page.screenshot({ path: `${output}/home-reading-${scenario.name}-after.png` });
        const repeat = await startReading(page, scenario.reduced, true);

        // Return through the actual SPA links, without visiting an article to install anchors.
        await page.evaluate(() => window.scrollTo({ top: 0, behavior: 'instant' }));
        await page.locator('.home-text-link[href="/about"]').click();
        await page.waitForFunction(() => document.querySelector('[data-vt-route="/about"]')
          && !document.documentElement.classList.contains('is-route-transitioning'));
        await page.locator('header a[href="/"]').first().click();
        await ready(page);
        const spa = await startReading(page, scenario.reduced);

        await page.goto(`${base}/#home-posts`);
        await ready(page);
        const direct = await page.evaluate(() => ({
          scroll: scrollY,
          top: document.getElementById('home-posts').getBoundingClientRect().top,
          header: document.querySelector('header.sticky').getBoundingClientRect().bottom,
          overflow: document.documentElement.scrollWidth > innerWidth,
        }));
        assert(direct.scroll > 100 && direct.top >= direct.header - 2,
          'direct fragment loading leaves the list visible below the header');
        assert(!direct.overflow, 'no horizontal overflow');
        assert.deepEqual(errors, [], 'no browser runtime errors');
        console.log(JSON.stringify({ scenario: scenario.name, first, repeat, spa, direct }));
      } finally {
        await context.close();
      }
    }
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
