#!/usr/bin/env bun
// Standalone browser regression: rebuild with make build-libs before running.
// PLAYWRIGHT_MODULE=/path/to/playwright CHROMIUM_PATH=/path/to/chromium bun scripts/test-lightbox.ts
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import type * as Playwright from 'playwright';
import { loadPlaywright } from './libs-modules.ts';

const { chromium } = loadPlaywright();
type ScriptWindow = Window &
  typeof globalThis & {
    __initLightbox(selectors: string | string[]): void;
    __closeLightboxFor(owner: string): void;
  };

const root = path.resolve(__dirname, '..');
const script = fs.readFileSync(path.join(root, 'public/lightbox/lightbox.js'), 'utf8');
const css = fs.readFileSync(path.join(root, 'public/lightbox/lightbox.css'), 'utf8');
const articleCss = fs.readFileSync(path.join(root, 'public/style.css'), 'utf8');
const coreCss = fs.readFileSync(
  path.join(root, 'public/yggdrasil-core/yggdrasil-core.css'),
  'utf8',
);
const diagramSvg = (dark: boolean) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="10 20 480 160" width="480" height="160" style="color:${dark ? '#e5e7eb' : '#172554'}" overflow="visible"><title>${dark ? '深色' : '浅色'}流程图</title><style>p{margin:0}foreignObject{font:16px sans-serif}</style><defs><marker id="fixture-arrow-${dark}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0 0L10 5L0 10z" fill="currentColor"/></marker></defs><rect x="20" y="30" width="450" height="130" rx="8" fill="${dark ? '#1e293b' : '#dbeafe'}"/><foreignObject x="35" y="50" width="420" height="90"><div xmlns="http://www.w3.org/1999/xhtml" style="color:inherit"><p>开始 → 完成</p><p>LongEnglishLabelWithoutUnexpectedWrapping</p></div></foreignObject><path data-edge="" d="M35 145H440" stroke="currentColor" stroke-width="2" marker-end="url(#fixture-arrow-${dark})"/></svg>`;
const diagramFixture = `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>${articleCss}\n${coreCss}\n${css}\nbody{min-height:2200px}.post-content{padding:80px 24px}.mermaid-diagram{padding:24px;border:2px solid gray}.mermaid-diagram svg{max-width:100%;height:auto}</style></head><body><main class="post-content md-content" data-showcase-lightbox-owner="diagram-browser"><figure class="mermaid-diagram"><div class="mermaid-light">${diagramSvg(false)}</div><div class="mermaid-dark">${diagramSvg(true)}</div></figure></main><script>${script}</script><script>window.__initLightbox('.post-content');</script></body></html>`;
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
const svg = (
  width: number,
  height: number,
) => `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">
<rect width="100%" height="100%" fill="#60a5fa"/><circle cx="50%" cy="50%" r="100" fill="#f59e0b"/>
</svg>`;

async function waitForImage(page: Playwright.Page) {
  await page.waitForFunction(() => {
    const img = document.querySelector<HTMLImageElement>('.lightbox-img');
    return img?.complete && img.naturalWidth > 0 && Number(getComputedStyle(img).opacity) > 0.99;
  });
}

async function originVisibility(page: Playwright.Page) {
  return page
    .locator('.post-content img')
    .evaluateAll((els) => els.map((el) => getComputedStyle(el).visibility));
}

async function open(page: Playwright.Page, index = 0) {
  await page.locator('.post-content img').nth(index).click();
  await waitForImage(page);
  await page.waitForTimeout(320);
}

async function close(page: Playwright.Page) {
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector<HTMLElement>('.lightbox-overlay')!);
}

// Fit state keeps the picture inside the stage between the top bar, the bottom
// caption/thumbnail strip and (on wide screens) the navigation arrows.
async function assertFitClearsChrome(page: Playwright.Page) {
  const boxes = await page.evaluate(() => {
    const rect = (sel: string): DOMRect | null =>
      document.querySelector<HTMLElement>(sel)?.getBoundingClientRect().toJSON() ?? null;
    return {
      img: rect('.lightbox-img'),
      top: rect('.lightbox-topbar'),
      bottom: rect('.lightbox-bottom'),
      prev: rect('.lightbox-prev'),
      width: innerWidth,
    };
  });
  assert.ok(boxes.img!.top >= boxes.top!.bottom - 0.5, 'image overlaps the top bar');
  assert.ok(
    boxes.img!.bottom <= boxes.bottom!.top + 0.5,
    'image overlaps the caption and thumbnails',
  );
  if (boxes.prev && boxes.width >= 640) {
    assert.ok(boxes.img!.left >= boxes.prev.right - 0.5, 'image overlaps the navigation arrow');
  }
}

async function assertCenteredRotation(page: Playwright.Page, reduced: boolean) {
  await assertFitClearsChrome(page);
  // Narrow screens hide the ± buttons (pinch replaces them); the + key steps the same levels.
  const zoomIn = page.getByRole('button', { name: '放大', exact: true });
  if (await zoomIn.isVisible()) await zoomIn.click();
  else await page.keyboard.press('+');
  assert.equal(await page.locator('.lightbox-zoom-level').textContent(), '150%');
  await page.waitForTimeout(200);
  const centers = await page.evaluate(async () => {
    document.querySelector<HTMLElement>('[aria-label="向右旋转"]')!.click();
    const samples = [];
    const started = performance.now();
    while (performance.now() - started < 220) {
      await new Promise(requestAnimationFrame);
      const r = document.querySelector<HTMLElement>('.lightbox-img')!.getBoundingClientRect();
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
    await page.getByRole('button', { name: '向右旋转', exact: true }).click();
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
  assert.match(
    await page.locator('.lightbox-img').evaluate((img) => img.style.transform),
    /rotate\(450deg\)/,
  );
}

async function assertGallery(page: Playwright.Page) {
  await page.keyboard.press('ArrowRight');
  await page.waitForFunction(
    () => document.querySelector<HTMLElement>('.lightbox-counter')?.textContent === '2 / 2',
  );
  await waitForImage(page);
  const image = await page.locator('.lightbox-img').evaluate((img: HTMLImageElement) => ({
    src: img.getAttribute('src'),
    alt: img.alt,
    width: Number.parseFloat(img.style.width),
    height: Number.parseFloat(img.style.height),
  }));
  assert.equal(image.src, '/uploads/portrait.svg');
  assert.equal(image.alt, '竖图');
  await assertFitClearsChrome(page);
  assert.equal(
    await page.locator('.lightbox-thumb[aria-current="true"]').getAttribute('aria-label'),
    '第 2 张',
  );
  assert.ok(Math.abs(image.width / image.height - 900 / 1600) < 0.001);
  assert.equal(
    await page.getByRole('link', { name: '下载原图' }).getAttribute('href'),
    '/uploads/portrait.svg',
  );
  assert.equal(
    await page.getByRole('link', { name: '下载原图' }).getAttribute('download'),
    'portrait.svg',
  );
  assert.deepEqual(
    await originVisibility(page),
    ['visible', 'visible'],
    'switching reveals the previous origin; mismatched thumbnail stays visible',
  );
}

async function assertSpaRefresh(page: Playwright.Page) {
  await page.evaluate(() => {
    const root = document.querySelector<HTMLElement>('.post-content')!;
    root.prepend(root.lastElementChild!);
    const third = document.createElement('img');
    third.src = '/uploads/third.svg';
    root.append(third);
    (window as ScriptWindow).__initLightbox(['.post-content', 'main']);
    (root.children[1] as HTMLElement).click();
  });
  await waitForImage(page);
  assert.equal(await page.locator('.lightbox-overlay').count(), 1);
  assert.equal(await page.locator('.lightbox-counter').textContent(), '2 / 3');
  await page.keyboard.press('ArrowRight');
  await page.waitForFunction(
    () =>
      document.querySelector<HTMLElement>('.lightbox-img')?.getAttribute('src') ===
      '/uploads/third.svg',
  );
  await waitForImage(page);
  await close(page);
}

async function assertPinchAndDrag(page: Playwright.Page, openMedia = open) {
  await openMedia(page);
  const session = await page.context().newCDPSession(page);
  const { width, height } = page.viewportSize()!;
  const touch = (x: number, y: number, id: number) => ({ x, y, id });
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
  await page.locator('.lightbox-zoom-level').click();
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
  await page.waitForFunction(() => !document.querySelector<HTMLElement>('.lightbox-overlay')!);
  await session.detach();
}

// Mouse drags pan at every scale, including axes where the image is still smaller than the
// viewport (fit, or 150% of a landscape image vertically); the image stays where it was dropped.
async function assertMousePan(page: Playwright.Page, openMedia = open) {
  await openMedia(page);
  const imgBox = async () => (await page.locator('.lightbox-img').boundingBox())!;
  const drag = async (dx: number, dy: number) => {
    const b = await imgBox();
    const x = b.x + b.width / 2;
    const y = b.y + b.height / 2;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + dx, y + dy, { steps: 8 });
    await page.mouse.up();
    await page.waitForTimeout(260);
  };
  const fit = await imgBox();
  await drag(-60, 40);
  const panned = await imgBox();
  assert.ok(
    Math.abs(panned.x - (fit.x - 60)) < 1 && Math.abs(panned.y - (fit.y + 40)) < 1,
    'fit drag should pan',
  );
  assert.equal(await page.locator('.lightbox-overlay').count(), 1, 'mouse drag must not close');
  await page.keyboard.press('+');
  await page.waitForTimeout(260);
  const zoomed = await imgBox();
  await drag(0, -12);
  const zoomedPanned = await imgBox();
  assert.ok(
    Math.abs(zoomedPanned.y - (zoomed.y - 12)) < 1,
    'zoomed drag should pan an axis smaller than the viewport',
  );
  await close(page);
}

// Scrolling at 6px/frame must keep the flight glued to the live thumbnail: on the last
// painted frame before removal the image rect equals the thumbnail rect.
async function assertScrollCloseTracksOrigin(page: Playwright.Page) {
  await open(page);
  const last = await page.evaluate(
    () =>
      new Promise<{ dx: number; dy: number; dw: number }>((resolve) => {
        let frame = 0;
        let last = { dx: NaN, dy: NaN, dw: NaN };
        const tick = (): void => {
          requestAnimationFrame(() => {
            // After paint, before the next frame: reads the state just painted.
            setTimeout(() => {
              const img = document.querySelector('.lightbox-img')?.getBoundingClientRect();
              const origin = document.querySelector('.post-content img')?.getBoundingClientRect();
              if (img && origin) {
                last = {
                  dx: img.left - origin.left,
                  dy: img.top - origin.top,
                  dw: img.width - origin.width,
                };
              }
              if (!document.querySelector('.lightbox-overlay')) {
                resolve(last);
                return;
              }
              if (frame++ < 50) scrollBy(0, 6);
              tick();
            }, 0);
          });
        };
        tick();
      }),
  );
  assert.ok(
    Math.abs(last.dx) < 2 && Math.abs(last.dy) < 2 && Math.abs(last.dw) < 2,
    `scroll-close must land on the live thumbnail: ${JSON.stringify(last)}`,
  );
  await page.evaluate(() => scrollTo(0, 0));
}

async function openDiagram(page: Playwright.Page) {
  await page.locator('figure.mermaid-diagram').click();
  await page.waitForSelector('.lightbox-overlay svg.lightbox-diagram');
  await page.waitForTimeout(340);
}

async function assertDiagram(
  page: Playwright.Page,
  name: string,
  options: Playwright.BrowserContextOptions,
) {
  await page.goto('https://fixture.test/diagrams');
  const dark = options.colorScheme === 'dark';
  await page.evaluate((dark) => document.documentElement.classList.toggle('dark', dark), dark);
  const source = await page.locator('figure').innerHTML();
  if (options.reducedMotion !== 'reduce') {
    const firstFrame = await page.evaluate(() => {
      const figure = document.querySelector<HTMLElement>('figure')!;
      const source = [...figure.querySelectorAll('svg')].find(
        (svg) => svg.getBoundingClientRect().width > 0,
      )!;
      const origin = source.getBoundingClientRect();
      figure.click();
      const media = document.querySelector<SVGSVGElement>('.lightbox-diagram')!;
      const r = media.getBoundingClientRect();
      return {
        dx: r.x - origin.x,
        dy: r.y - origin.y,
        dw: r.width - origin.width,
        opacity: getComputedStyle(media).opacity,
      };
    });
    assert.ok(
      Math.abs(firstFrame.dx) < 1 && Math.abs(firstFrame.dy) < 1 && Math.abs(firstFrame.dw) < 1,
      '流程图首帧应覆盖内部 SVG',
    );
    assert.equal(firstFrame.opacity, '1');
    await page.waitForTimeout(340);
    await close(page);
  }
  await openDiagram(page);
  assert.equal(
    await page
      .locator('.lightbox-counter:visible, .lightbox-prev, .lightbox-next, .lightbox-thumb')
      .count(),
    0,
  );
  assert.match(
    (await page.locator('.lightbox-diagram').textContent()) ?? '',
    dark ? /深色/ : /浅色/,
  );
  // 同 ID 的原图 defs 仍在文档里：必须验证标记实际绘制，而不只检查引用属性。
  const rendered = await page.locator('.lightbox-diagram').screenshot();
  const edge = page.locator('.lightbox-diagram [data-edge]');
  await edge.evaluate((path) => (path.style.markerEnd = 'none'));
  const withoutMarker = await page.locator('.lightbox-diagram').screenshot();
  await edge.evaluate((path) => path.style.removeProperty('marker-end'));
  assert.notDeepEqual(rendered, withoutMarker, '隐藏原图后，SVG 箭头仍须实际绘制');
  const selectedTitle = await page.locator('.lightbox-diagram title').textContent();
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowLeft');
  assert.equal(await page.locator('.lightbox-diagram title').textContent(), selectedTitle);
  await assertCenteredRotation(page, options.reducedMotion === 'reduce');
  const artifactDir =
    process.env.LIGHTBOX_ARTIFACT_DIR ?? path.join(root, 'target/regressions/mermaid-lightbox');
  fs.mkdirSync(artifactDir, { recursive: true });
  await page.screenshot({ path: path.join(artifactDir, `${name}-diagram.png`) });
  const downloadEvent = page.waitForEvent('download');
  await page.getByRole('link', { name: '下载 SVG', exact: true }).click();
  const download = await downloadEvent;
  assert.equal(download.suggestedFilename(), 'diagram.svg');
  const downloadPath = path.join(artifactDir, `${name}-diagram.svg`);
  await download.saveAs(downloadPath);
  // 用独立 SVG 文档打开实际下载文件，避免正文 CSS 替导出文件补齐排版。
  const exported = await page.context().newPage();
  await exported.route('https://fixture.test/download.svg', (route) =>
    route.fulfill({ contentType: 'image/svg+xml', body: fs.readFileSync(downloadPath) }),
  );
  await exported.goto('https://fixture.test/download.svg');
  const content = await exported.evaluate(() => {
    const svg = document.documentElement as unknown as SVGSVGElement;
    const foreign = document.querySelector('foreignObject')!;
    const box = foreign.getBoundingClientRect();
    const paragraphs = [...document.querySelectorAll('foreignObject p')];
    return {
      ns: svg.namespaceURI,
      viewBox: svg.getAttribute('viewBox'),
      width: svg.getAttribute('width'),
      height: svg.getAttribute('height'),
      transform: svg.style.transform,
      color: getComputedStyle(svg).color,
      labels: paragraphs.map((p) => {
        const r = p.getBoundingClientRect();
        const range = document.createRange();
        range.selectNodeContents(p);
        const text = range.getBoundingClientRect();
        return {
          text: p.textContent,
          visible: r.width > 0 && r.height > 0,
          fits:
            text.left >= box.left - 1 &&
            text.right <= box.right + 1 &&
            text.bottom <= box.bottom + 1,
        };
      }),
    };
  });
  assert.equal(content.ns, 'http://www.w3.org/2000/svg');
  assert.equal(content.viewBox, '10 20 480 160');
  assert.equal(content.width, '480');
  assert.equal(content.height, '160');
  assert.equal(content.transform, '');
  assert.equal(content.color, dark ? 'rgb(229, 231, 235)' : 'rgb(23, 37, 84)');
  assert.ok(content.labels.some((label) => label.text?.includes('开始')));
  assert.ok(content.labels.some((label) => label.text?.includes('LongEnglish')));
  assert.ok(
    content.labels.every((label) => label.visible && label.fits),
    '下载标签必须可见且不溢出',
  );
  await exported.screenshot({ path: path.join(artifactDir, `${name}-download.png`) });
  await exported.close();
  await close(page);
  assert.equal(await page.locator('figure').innerHTML(), source);
  if (options.hasTouch) await assertPinchAndDrag(page, openDiagram);
  else await assertMousePan(page, openDiagram);
  await openDiagram(page);
  await page.evaluate(() => scrollTo(0, 90));
  await page.waitForFunction(() => !document.querySelector('.lightbox-overlay'));
  await page.evaluate(() => scrollTo(0, 0));
  await openDiagram(page);
  await page.evaluate(() => (window as ScriptWindow).__closeLightboxFor('diagram-browser'));
  assert.equal(await page.locator('.lightbox-overlay').count(), 0);
  assert.notEqual(
    await page.locator('figure').evaluate((el) => getComputedStyle(el).visibility),
    'hidden',
  );
  await page.screenshot({ path: path.join(artifactDir, `${name}-diagram-owner-cleanup.png`) });
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
      [
        'mobile-dark',
        {
          viewport: { width: 390, height: 844 },
          colorScheme: 'dark',
          hasTouch: true,
          isMobile: true,
        },
      ],
      ['reduced-motion', { viewport: { width: 1000, height: 800 }, reducedMotion: 'reduce' }],
    ] as [string, Playwright.BrowserContextOptions][]) {
      const context = await browser.newContext(options);
      const page = await context.newPage();
      const errors: string[] = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await page.route('https://fixture.test/**', async (route) => {
        const url = new URL(route.request().url());
        if (url.pathname === '/') return route.fulfill({ contentType: 'text/html', body: fixture });
        if (url.pathname === '/diagrams')
          return route.fulfill({ contentType: 'text/html', body: diagramFixture });
        const portrait = url.pathname.includes('portrait');
        return route.fulfill({
          contentType: 'image/svg+xml',
          body: svg(portrait ? 900 : 1600, portrait ? 1600 : 900),
        });
      });
      await page.goto('https://fixture.test/');
      if (options.colorScheme === 'dark')
        await page.evaluate(() => document.documentElement.classList.add('dark'));
      const reduced = options.reducedMotion === 'reduce';
      await open(page);
      assert.deepEqual(
        await originVisibility(page),
        reduced ? ['visible', 'visible'] : ['hidden', 'visible'],
        'matching thumbnail hides its origin while open',
      );
      await assertCenteredRotation(page, reduced);
      if (process.env.LIGHTBOX_ARTIFACT_DIR) {
        fs.mkdirSync(process.env.LIGHTBOX_ARTIFACT_DIR, { recursive: true });
        await page.screenshot({
          path: path.join(process.env.LIGHTBOX_ARTIFACT_DIR, `${name}.png`),
        });
      }
      await assertGallery(page);
      // A short scroll that stops springs back; scrolling past the threshold and stopping closes.
      if (reduced) {
        await page.evaluate(() => scrollTo(0, 60));
        await page.waitForFunction(
          () => !document.querySelector<HTMLElement>('.lightbox-overlay')!,
        );
      } else {
        const fit = await page.locator('.lightbox-img').boundingBox();
        assert.ok(fit, 'lightbox image should have a box');
        await page.evaluate(() => scrollTo(0, 30));
        await page.waitForTimeout(50);
        const opacity = await page
          .locator('.lightbox-backdrop')
          .evaluate((el) => Number(getComputedStyle(el).opacity));
        assert.ok(opacity > 0 && opacity < 1, 'scroll should interpolate backdrop opacity');
        await page.waitForTimeout(450);
        const settled = await page.locator('.lightbox-img').boundingBox();
        const settledOpacity = await page
          .locator('.lightbox-backdrop')
          .evaluate((el) => Number(getComputedStyle(el).opacity));
        assert.ok(
          (await page.locator('.lightbox-overlay').count()) === 1 &&
            settledOpacity === 1 &&
            settled &&
            Math.abs(settled.x - fit.x) < 1 &&
            Math.abs(settled.y - fit.y) < 1 &&
            Math.abs(settled.width - fit.width) < 1 &&
            Math.abs(settled.height - fit.height) < 1,
          'a short scroll that stops should spring back open',
        );
        await page.evaluate(() => scrollTo(0, 90));
        await page.waitForFunction(() => !document.querySelector('.lightbox-overlay'), null, {
          timeout: 1000,
        });
      }
      assert.deepEqual(await originVisibility(page), ['visible', 'visible']);
      await page.evaluate(() => scrollTo(0, 0));
      if (!reduced) await assertScrollCloseTracksOrigin(page);
      await assertSpaRefresh(page);
      if (options.hasTouch) await assertPinchAndDrag(page);
      else await assertMousePan(page);
      // A fresh SPA opening immediately replaces a closing overlay.
      await open(page);
      await page.keyboard.press('Escape');
      await page.evaluate(() => document.querySelector<HTMLElement>('.post-content img')!.click());
      await waitForImage(page);
      await page.waitForTimeout(320);
      assert.equal(await page.locator('.lightbox-overlay').count(), 1);
      await page.evaluate(() => (window as ScriptWindow).__closeLightboxFor('browser-fixture'));
      await assertDiagram(page, name, options);
      assert.equal(await page.locator('.lightbox-overlay').count(), 0);
      assert.deepEqual(errors, []);
      console.log(
        `${name}: rotation, gallery, scroll settle, scroll tracking, SPA lifecycle${options.hasTouch ? ', pinch and drag' : ', mouse pan'} passed`,
      );
      await context.close();
    }
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
