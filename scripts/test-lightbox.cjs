#!/usr/bin/env bun
// Standalone browser regression: rebuild with make build-libs before running.
// PLAYWRIGHT_MODULE=/path/to/playwright CHROMIUM_PATH=/path/to/chromium bun scripts/test-lightbox.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');

const root = path.resolve(__dirname, '..');
const script = fs.readFileSync(path.join(root, 'public/lightbox/lightbox.js'), 'utf8');
const css = fs.readFileSync(path.join(root, 'public/lightbox/lightbox.css'), 'utf8');
const fixture = `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>
  body { margin: 0; min-height: 2000px; background: #fafafa; color: #111; }
  html.dark body { background: #111; color: #fafafa; }
  .post-content { padding: 80px 24px; }
  .post-content img { width: 240px; height: 135px; object-fit: cover; cursor: zoom-in; }
  ${css}
</style></head><body>
<main class="post-content" data-showcase-lightbox-owner="browser-fixture">
<img src="/uploads/first.svg?w=240" alt="横图"><img src="/uploads/portrait.svg?w=240" alt="竖图">
</main><script>${script}</script><script>window.__initLightbox('.post-content');</script>
</body></html>`;
const svg = (width, height) => `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">
<rect width="100%" height="100%" fill="#60a5fa"/><circle cx="50%" cy="50%" r="100" fill="#f59e0b"/>
</svg>`;

async function waitForImage(page) {
  await page.waitForFunction(() => {
    const img = document.querySelector('.lightbox-img');
    return img?.complete && img.naturalWidth > 0 && Number(getComputedStyle(img).opacity) > 0.99;
  });
}

async function open(page, index = 0) {
  await page.locator('.post-content img').nth(index).click();
  await waitForImage(page);
  await page.waitForTimeout(320);
}

async function close(page) {
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector('.lightbox-overlay'));
}

async function assertCenteredRotation(page, reduced) {
  await page.getByRole('button', { name: '放大', exact: true }).click();
  await page.waitForTimeout(200);
  const centers = await page.evaluate(async () => {
    document.querySelector('[aria-label="顺时针旋转 90 度"]').click();
    const samples = [];
    const started = performance.now();
    while (performance.now() - started < 220) {
      await new Promise(requestAnimationFrame);
      const r = document.querySelector('.lightbox-img').getBoundingClientRect();
      samples.push({ x: r.x + r.width / 2, y: r.y + r.height / 2 });
    }
    return { samples, width: innerWidth, height: innerHeight };
  });
  for (const center of centers.samples) {
    assert.ok(Math.abs(center.x - centers.width / 2) < 1, 'rotation center drifted horizontally');
    assert.ok(Math.abs(center.y - centers.height / 2) < 1, 'rotation center drifted vertically');
  }
  const dims = await page.locator('.lightbox-img').evaluate((img) => ({
    width: img.style.width,
    height: img.style.height,
  }));
  for (let step = 0; step < 4; step += 1) {
    await page.getByRole('button', { name: '顺时针旋转 90 度', exact: true }).click();
    await page.waitForTimeout(reduced ? 10 : 200);
  }
  assert.deepEqual(
    await page.locator('.lightbox-img').evaluate((img) => ({
      width: img.style.width,
      height: img.style.height,
    })),
    dims,
    'rotation must preserve its layout box',
  );
  assert.match(await page.locator('.lightbox-img').evaluate((img) => img.style.transform), /rotate\(450deg\)/);
}

async function assertGallery(page) {
  await page.keyboard.press('ArrowRight');
  await page.waitForFunction(() => document.querySelector('.lightbox-counter')?.textContent === '2 / 2');
  await waitForImage(page);
  const image = await page.locator('.lightbox-img').evaluate((img) => ({
    src: img.getAttribute('src'),
    alt: img.alt,
    width: Number.parseFloat(img.style.width),
    height: Number.parseFloat(img.style.height),
  }));
  assert.equal(image.src, '/uploads/portrait.svg');
  assert.equal(image.alt, '竖图');
  assert.ok(Math.abs(image.width / image.height - 900 / 1600) < 0.001);
  assert.equal(await page.getByRole('link', { name: '下载原图' }).getAttribute('href'), '/uploads/portrait.svg');
  assert.equal(await page.getByRole('link', { name: '下载原图' }).getAttribute('download'), 'portrait.svg');
}

async function assertSpaRefresh(page) {
  await page.evaluate(() => {
    const root = document.querySelector('.post-content');
    root.prepend(root.lastElementChild);
    const third = document.createElement('img');
    third.src = '/uploads/third.svg';
    root.append(third);
    window.__initLightbox(['.post-content', 'main']);
    root.children[1].click();
  });
  await waitForImage(page);
  assert.equal(await page.locator('.lightbox-overlay').count(), 1);
  assert.equal(await page.locator('.lightbox-counter').textContent(), '2 / 3');
  await page.keyboard.press('ArrowRight');
  await page.waitForFunction(() => document.querySelector('.lightbox-img')?.getAttribute('src') === '/uploads/third.svg');
  await waitForImage(page);
  await close(page);
}

async function assertPinchAndDrag(page) {
  await open(page);
  const session = await page.context().newCDPSession(page);
  const { width, height } = page.viewportSize();
  const touch = (x, y, id) => ({ x, y, id });
  const y = height / 2;
  const x = width / 2;
  await session.send('Input.dispatchTouchEvent', {
    type: 'touchStart',
    touchPoints: [touch(x - 30, y, 1), touch(x + 30, y, 2)],
  });
  await session.send('Input.dispatchTouchEvent', {
    type: 'touchMove',
    touchPoints: [touch(x - 70, y, 1), touch(x + 70, y, 2)],
  });
  await session.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  assert.equal(await page.locator('.lightbox-img.is-zoomed').count(), 1);
  await page.getByRole('button', { name: '重置视图', exact: true }).click();
  await page.waitForTimeout(200);
  await session.send('Input.dispatchTouchEvent', {
    type: 'touchStart',
    touchPoints: [touch(x, y, 1)],
  });
  await session.send('Input.dispatchTouchEvent', {
    type: 'touchMove',
    touchPoints: [touch(x, y + 150, 1)],
  });
  await session.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await page.waitForFunction(() => !document.querySelector('.lightbox-overlay'));
  await session.detach();
}

(async () => {
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
  });
  try {
    for (const [name, options] of [
      ['desktop-light', { viewport: { width: 1000, height: 800 }, colorScheme: 'light' }],
      ['desktop-dark', { viewport: { width: 1000, height: 800 }, colorScheme: 'dark' }],
      ['mobile-dark', { viewport: { width: 390, height: 844 }, colorScheme: 'dark', hasTouch: true, isMobile: true }],
      ['reduced-motion', { viewport: { width: 1000, height: 800 }, reducedMotion: 'reduce' }],
    ]) {
      const context = await browser.newContext(options);
      const page = await context.newPage();
      const errors = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await page.route('https://fixture.test/**', async (route) => {
        const url = new URL(route.request().url());
        if (url.pathname === '/') return route.fulfill({ contentType: 'text/html', body: fixture });
        const portrait = url.pathname.includes('portrait');
        return route.fulfill({ contentType: 'image/svg+xml', body: svg(portrait ? 900 : 1600, portrait ? 1600 : 900) });
      });
      await page.goto('https://fixture.test/');
      if (options.colorScheme === 'dark') await page.evaluate(() => document.documentElement.classList.add('dark'));
      const reduced = options.reducedMotion === 'reduce';
      await open(page);
      await assertCenteredRotation(page, reduced);
      if (process.env.LIGHTBOX_ARTIFACT_DIR) {
        fs.mkdirSync(process.env.LIGHTBOX_ARTIFACT_DIR, { recursive: true });
        await page.screenshot({ path: path.join(process.env.LIGHTBOX_ARTIFACT_DIR, `${name}.png`) });
      }
      await assertGallery(page);
      // Scrolling halfway shrinks towards the live thumbnail; another step closes it.
      await page.evaluate(() => scrollTo(0, 60));
      if (!reduced) {
        await page.waitForTimeout(120);
        const opacity = await page.locator('.lightbox-overlay').evaluate((el) => Number(getComputedStyle(el).opacity));
        assert.ok(opacity > 0 && opacity < 1, 'scroll should interpolate overlay opacity');
        await page.evaluate(() => scrollTo(0, 200));
      }
      await page.waitForFunction(() => !document.querySelector('.lightbox-overlay'));
      await page.evaluate(() => scrollTo(0, 0));
      await assertSpaRefresh(page);
      if (options.hasTouch) await assertPinchAndDrag(page);
      // A fresh SPA opening immediately replaces a closing overlay.
      await open(page);
      await page.keyboard.press('Escape');
      await page.evaluate(() => document.querySelector('.post-content img').click());
      await waitForImage(page);
      await page.waitForTimeout(320);
      assert.equal(await page.locator('.lightbox-overlay').count(), 1);
      await page.evaluate(() => window.__closeLightboxFor('browser-fixture'));
      assert.equal(await page.locator('.lightbox-overlay').count(), 0);
      assert.deepEqual(errors, []);
      console.log(`${name}: rotation, gallery, scroll, SPA lifecycle${options.hasTouch ? ', pinch and drag' : ''} passed`);
      await context.close();
    }
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
