#!/usr/bin/env bun
// Standalone browser regression: rebuild with make dev-assets before running.
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

interface CloseFlightFrame {
  time: number;
  transform: string;
  layoutW: number;
  layoutH: number;
  opacity: number;
  originHidden: boolean;
  origin: { x: number; y: number; w: number; h: number };
  points: { x: number; y: number }[];
  angle: number;
  scrollY: number;
}

const root = path.resolve(__dirname, '..');
const continuityArtifactDir = process.env.LIGHTBOX_ARTIFACT_DIR;
const script = fs.readFileSync(path.join(root, 'public/lightbox/lightbox.js'), 'utf8');
const css = fs.readFileSync(path.join(root, 'public/lightbox/lightbox.css'), 'utf8');
const articleCss = fs.readFileSync(path.join(root, 'public/style.css'), 'utf8');
const diagramSvg = (dark: boolean) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="10 20 480 160" width="480" height="160" style="color:${dark ? '#e5e7eb' : '#172554'}" overflow="visible"><title>${dark ? '深色' : '浅色'}流程图</title><style>p{margin:0}foreignObject{font:16px sans-serif}</style><defs><marker id="fixture-arrow-${dark}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0 0L10 5L0 10z" fill="currentColor"/></marker></defs><rect x="20" y="30" width="450" height="130" rx="8" fill="${dark ? '#1e293b' : '#dbeafe'}"/><foreignObject x="35" y="50" width="420" height="90"><div xmlns="http://www.w3.org/1999/xhtml" style="color:inherit"><p>开始 → 完成</p><p>LongEnglishLabelWithoutUnexpectedWrapping</p></div></foreignObject><path data-edge="" d="M35 145H440" stroke="currentColor" stroke-width="2" marker-end="url(#fixture-arrow-${dark})"/></svg>`;
const diagramFixture = `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>${articleCss}\n${css}\nbody{min-height:2200px}.post-content{padding:80px 24px}.mermaid-diagram{padding:24px;border:2px solid gray}.mermaid-diagram svg{max-width:100%;height:auto}</style></head><body><main class="post-content md-content" data-showcase-lightbox-owner="diagram-browser"><figure class="mermaid-diagram"><div class="mermaid-light">${diagramSvg(false)}</div><div class="mermaid-dark">${diagramSvg(true)}</div></figure></main><script>${script}</script><script>window.__initLightbox('.post-content');</script></body></html>`;
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

// 用布局角点而非包围盒辨别方向：180° 的错误姿态也可能具有完全相同的包围盒。
async function assertCloseContinuity(
  page: Playwright.Page,
  openMedia: (page: Playwright.Page) => Promise<void>,
  originSelector: string,
  deg: number,
  mode: 'escape' | 'scroll' | 'mid-rotation',
  artifactName: string,
): Promise<void> {
  await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
  await openMedia(page);
  const reduced = await page.evaluate(() => matchMedia('(prefers-reduced-motion: reduce)').matches);
  if (mode !== 'mid-rotation') {
    for (let step = 0; step < deg / 90; step += 1) {
      await page.getByRole('button', { name: '向右旋转', exact: true }).click();
      // 等实际 CSS 旋转结束，而不是只等目标 style 已经写入。
      await page.waitForFunction(() => {
        const media = document.querySelector<HTMLElement>('.lightbox-img')!;
        return !media.getAnimations().some((animation) => animation.playState === 'running');
      });
    }
  }
  const result = await page.evaluate(
    async ({ originSelector, mode, reduced }) => {
      const media = document.querySelector<HTMLElement>('.lightbox-img')!;
      const origin = document.querySelector<HTMLElement | SVGSVGElement>(originSelector)!;
      const nextFrame = () => {
        const { promise, resolve } = Promise.withResolvers<number>();
        requestAnimationFrame(resolve);
        return promise;
      };
      const pause = (milliseconds: number) => {
        const { promise, resolve } = Promise.withResolvers<void>();
        setTimeout(resolve, milliseconds);
        return promise;
      };
      const paintChannel = new MessageChannel();
      const paintedFrame = async () => {
        await nextFrame();
        // 在整轮 rAF 写入并绘制后采样；MessageChannel 避免嵌套 timer 钳制造成漏帧。
        const { promise, resolve } = Promise.withResolvers<void>();
        paintChannel.port1.onmessage = () => resolve();
        paintChannel.port2.postMessage(null);
        await promise;
      };
      const capture = (): CloseFlightFrame => {
        const style = getComputedStyle(media);
        const matrix = new DOMMatrixReadOnly(
          style.transform === 'none' ? undefined : style.transform,
        );
        const layoutW = Number.parseFloat(media.style.width);
        const layoutH = Number.parseFloat(media.style.height);
        const points = [
          [0, 0],
          [layoutW, 0],
          [0, layoutH],
        ].map(([x, y]) => {
          const p = matrix.transformPoint(new DOMPoint(x, y));
          return { x: p.x, y: p.y };
        });
        const rect = origin.getBoundingClientRect();
        let originHidden = false;
        for (let node: Element | null = origin; node; node = node.parentElement) {
          const computed = getComputedStyle(node);
          originHidden ||= computed.visibility === 'hidden' || Number(computed.opacity) === 0;
        }
        return {
          time: performance.now(),
          transform: style.transform,
          layoutW,
          layoutH,
          opacity: Number(style.opacity),
          originHidden,
          origin: { x: rect.x, y: rect.y, w: rect.width, h: rect.height },
          points,
          angle: (Math.atan2(matrix.b, matrix.a) * 180) / Math.PI,
          scrollY,
        };
      };
      const frames: CloseFlightFrame[] = [];
      const recovery: { phase: string; sample: CloseFlightFrame }[] = [];
      if (mode === 'scroll' && !reduced) {
        // 同一不可变快照应支持反向归零；短滚动停手回弹后还要再次收回。
        const startY = scrollY;
        recovery.push({ phase: 'start', sample: capture() });
        scrollTo({ top: startY + 24, behavior: 'instant' });
        await paintedFrame();
        await paintedFrame();
        recovery.push({ phase: 'outward', sample: capture() });
        scrollTo({ top: startY, behavior: 'instant' });
        await paintedFrame();
        await paintedFrame();
        recovery.push({ phase: 'reversed', sample: capture() });
        await pause(400);
        scrollTo({ top: startY + 24, behavior: 'instant' });
        await paintedFrame();
        await paintedFrame();
        recovery.push({ phase: 'short-scroll', sample: capture() });
        await pause(450);
        recovery.push({ phase: 'rebounded', sample: capture() });
      }
      if (mode === 'mid-rotation' && !reduced) {
        // 此模式只能从首次 0→90° 开始，不能用动画目标值冒充眼前姿态。
        document.querySelector<HTMLElement>('[aria-label="向右旋转"]')!.click();
        const deadline = performance.now() + 1000;
        while (true) {
          await nextFrame();
          const angle = capture().angle;
          if (angle >= 15 && angle <= 75) break;
          if (performance.now() > deadline) throw new Error('未捕获到首次旋转的 15°–75° 中间帧');
        }
      }
      const overlay = document.querySelector('.lightbox-overlay')!;
      const removeChild = Node.prototype.removeChild;
      let terminal: CloseFlightFrame | null = null;
      // 精确终点和删除同任务执行：只观察当前 overlay 的移除前同步状态，不读取脱离 DOM 的样式。
      Node.prototype.removeChild = function <T extends Node>(this: Node, child: T): T {
        if ((child as Node) === overlay) terminal = capture();
        return removeChild.call(this, child) as T;
      };
      try {
        const before = capture();
        if (mode === 'scroll') {
          scrollBy({ top: 6, behavior: 'instant' });
          // scroll 事件在浏览器下一渲染步骤派发，不把它误称为同步关闭首帧。
        } else
          document.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
          );
        const immediate = document.querySelector('.lightbox-overlay') ? capture() : null;
        const started = performance.now();
        let ticks = 0;
        while (document.querySelector('.lightbox-overlay')) {
          // 普通过渡在 rAF 读取的计算姿态就是本帧绘制值；不能延后到 timer 清理之后漏掉末帧。
          if (mode === 'scroll') await paintedFrame();
          else await nextFrame();
          if (document.querySelector('.lightbox-overlay')) frames.push(capture());
          if (performance.now() - started > 2500) throw new Error('连续收回超过 2500ms 未清理');
          if (mode === 'scroll' && document.querySelector('.lightbox-overlay') && ticks++ < 50) {
            scrollBy({ top: 6, behavior: 'instant' });
          }
        }
        return {
          before,
          immediate,
          frames,
          recovery,
          terminal: terminal as CloseFlightFrame | null,
          originRestored:
            !origin.closest('.lightbox-origin-hidden') &&
            getComputedStyle(origin).visibility !== 'hidden',
          overlays: document.querySelectorAll('.lightbox-overlay').length,
          copies: document.querySelectorAll('.lightbox-img').length,
        };
      } finally {
        Node.prototype.removeChild = removeChild;
        paintChannel.port1.close();
        paintChannel.port2.close();
      }
    },
    { originSelector, mode, reduced },
  );
  const visibleSamples = [
    result.before,
    ...(result.immediate ? [result.immediate] : []),
    ...result.frames,
  ];
  const samples = [...visibleSamples, ...(result.terminal ? [result.terminal] : [])];
  const angularDelta = (a: number, b: number) => ((b - a + 540) % 360) - 180;
  let angle = samples[0].angle;
  let totalAngle = 0;
  const unwrappedAngles = [angle];
  for (let i = 1; i < samples.length; i += 1) {
    const delta = angularDelta(samples[i - 1].angle, samples[i].angle);
    angle += delta;
    totalAngle += Math.abs(delta);
    unwrappedAngles.push(angle);
  }
  if (continuityArtifactDir) {
    fs.mkdirSync(continuityArtifactDir, { recursive: true });
    fs.writeFileSync(
      path.join(continuityArtifactDir, `${artifactName}.json`),
      JSON.stringify(
        { artifactName, deg, mode, reduced, ...result, unwrappedAngles, totalAngle },
        null,
        2,
      ),
    );
  }
  assert.equal(result.overlays, 0, `${artifactName}: 灯箱应已清理`);
  assert.equal(result.copies, 0, `${artifactName}: 关闭后只保留正文原图`);
  assert.ok(result.originRestored, `${artifactName}: 正文原图应恢复`);
  if (reduced) {
    if (mode !== 'scroll') assert.equal(result.immediate, null, '减少动态效果应同任务立即关闭');
    assert.equal(result.frames.length, 0, '减少动态效果不得播放收回动画');
    await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
    return;
  }
  const pointError = (a: typeof result.before.points, b: typeof result.before.points) =>
    Math.max(...a.map((point, index) => Math.hypot(point.x - b[index].x, point.y - b[index].y)));
  assert.ok(result.immediate, `${artifactName}: 应保留收回副本`);
  if (mode !== 'scroll') {
    assert.ok(
      pointError(result.before.points, result.immediate.points) < 1,
      `${artifactName}: 关闭同任务首帧三个角点不可跳动`,
    );
  }
  if (mode === 'mid-rotation') {
    assert.ok(result.before.angle >= 15 && result.before.angle <= 75, '应从实际旋转中间帧关闭');
  }
  const start = result.recovery.find((entry) => entry.phase === 'start')?.sample;
  if (start) {
    for (const phase of ['reversed', 'rebounded']) {
      const sample = result.recovery.find((entry) => entry.phase === phase)!.sample;
      assert.ok(
        pointError(start.points, sample.points) < 1,
        `${artifactName}: ${phase} 应恢复起始姿态`,
      );
      assert.ok(Math.abs(angularDelta(start.angle, sample.angle)) < 0.1, '回弹不可永久改写旋转角');
    }
    const outward = result.recovery.find((entry) => entry.phase === 'outward')!.sample;
    assert.ok(Math.abs(angularDelta(start.angle, outward.angle)) > 1, '短滚动应真实开始转正');
  }
  assert.ok(totalAngle <= 182, `${artifactName}: 总角程须 ≤180°+2°，实际 ${totalAngle}`);
  assert.ok(result.frames.length > 1, `${artifactName}: 应记录实际动画帧`);
  for (const sample of [...visibleSamples, ...result.recovery.map((entry) => entry.sample)]) {
    assert.ok(sample.layoutW > 0 && sample.layoutH > 0, '布局尺寸必须真实有效');
    assert.equal(sample.opacity, 1, `${artifactName}: 同宽高比副本不可用淡出掩盖旋转`);
    assert.ok(sample.originHidden, `${artifactName}: 飞行结束前正文原图应隐藏`);
  }
  const last = result.frames.at(-1)!;
  const target = [
    { x: last.origin.x, y: last.origin.y },
    { x: last.origin.x + last.origin.w, y: last.origin.y },
    { x: last.origin.x, y: last.origin.y + last.origin.h },
  ];
  const error = pointError(last.points, target);
  assert.ok(error < 2, `${artifactName}: 最后可见帧三个角点须贴合实时原图，误差 ${error}px`);
  assert.ok(Math.abs(angularDelta(last.angle, 0)) < 1, `${artifactName}: 最后可见帧方向须转正`);
  assert.ok(result.terminal, `${artifactName}: 必须捕获删除前的同步终点`);
  const terminalTarget = [
    { x: result.terminal.origin.x, y: result.terminal.origin.y },
    { x: result.terminal.origin.x + result.terminal.origin.w, y: result.terminal.origin.y },
    { x: result.terminal.origin.x, y: result.terminal.origin.y + result.terminal.origin.h },
  ];
  assert.ok(pointError(result.terminal.points, terminalTarget) < 0.1, '删除前必须提交精确角点终点');
  assert.ok(Math.abs(angularDelta(result.terminal.angle, 0)) < 0.01, '删除前必须精确转正');
  if (mode === 'scroll') {
    assert.ok(
      Math.abs(last.origin.y - result.before.origin.y) > 30,
      '滚动收尾必须跟随移动中的原图',
    );
  }
  await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
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
  const diagramOrigin = dark ? '.mermaid-dark svg' : '.mermaid-light svg';
  const angles = name === 'desktop-light' ? [90, 270, 450, 900] : [90];
  for (const angle of angles) {
    await assertCloseContinuity(
      page,
      openDiagram,
      diagramOrigin,
      angle,
      'escape',
      `${name}-svg-${angle}-escape`,
    );
  }
  if (name === 'desktop-light') {
    await assertCloseContinuity(
      page,
      openDiagram,
      diagramOrigin,
      90,
      'mid-rotation',
      `${name}-svg-90-mid-rotation`,
    );
    await assertCloseContinuity(
      page,
      openDiagram,
      diagramOrigin,
      90,
      'scroll',
      `${name}-svg-90-scroll`,
    );
  }
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
      const context = await browser.newContext({
        ...options,
        ...(continuityArtifactDir
          ? {
              recordVideo: {
                dir: path.join(continuityArtifactDir, name),
                size: options.viewport ?? undefined,
              },
            }
          : {}),
      });
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
      if (name === 'desktop-light' || reduced) {
        for (const angle of reduced ? [90] : [90, 270, 450, 900]) {
          await assertCloseContinuity(
            page,
            open,
            '.post-content img:first-child',
            angle,
            'escape',
            `${name}-image-${angle}-escape`,
          );
        }
        if (!reduced) {
          for (const mode of ['mid-rotation', 'scroll'] as const) {
            await assertCloseContinuity(
              page,
              open,
              '.post-content img:first-child',
              90,
              mode,
              `${name}-image-90-${mode}`,
            );
          }
        }
      }
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
      if (continuityArtifactDir) {
        await page.video()!.saveAs(path.join(continuityArtifactDir, `${name}.webm`));
      }
    }
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
