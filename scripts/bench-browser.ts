import assert from 'node:assert/strict';
import type { Browser, BrowserContext, Page } from 'playwright';
import { comparePaired, summarize } from './bench-statistics';
import { NOTEBOOK_ROUTE, NOTES_ROUTE } from './bench-workloads';

export const BROWSER_ROUTES = ['/', '/post/bench-plain-prose', NOTES_ROUTE, NOTEBOOK_ROUTE];
export const BROWSER_PROFILE = {
  viewport: { width: 390, height: 844 },
  latencyMs: 150,
  downloadBytesPerSecond: 200_000,
  uploadBytesPerSecond: 200_000,
  cpuSlowdown: 4,
  paintSettleMs: 750,
};

type PageMarks = { hydrationMs: number; lcpMs: number };
type InstrumentedWindow = Window & { benchmarkMarks: PageMarks };

async function measurePage(context: BrowserContext, url: string) {
  const page = await context.newPage();
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('response', (response) => {
    if (response.status() >= 400 && new URL(response.url()).origin === new URL(url).origin) {
      errors.push(`${response.status()} ${response.url()}`);
    }
  });
  try {
    await configurePage(context, page);
    const response = await page.goto(url, { waitUntil: 'load', timeout: 60_000 });
    assert.equal(response?.status(), 200, url);
    await page.waitForFunction(
      () => {
        const marks = (window as unknown as InstrumentedWindow).benchmarkMarks;
        return marks.hydrationMs > 0 && marks.lcpMs > 0;
      },
      undefined,
      { timeout: 45_000 },
    );
    await Bun.sleep(BROWSER_PROFILE.paintSettleMs);
    const metrics = await page.evaluate(() => {
      const marks = (window as unknown as InstrumentedWindow).benchmarkMarks;
      const navigation = performance.getEntriesByType(
        'navigation',
      )[0] as PerformanceNavigationTiming;
      const resources = (
        performance.getEntriesByType('resource') as PerformanceResourceTiming[]
      ).filter((entry) => new URL(entry.name).origin === location.origin);
      const wasm = resources.filter((entry) => new URL(entry.name).pathname.endsWith('.wasm'));
      const sum = (entries: PerformanceResourceTiming[], key: 'encodedBodySize' | 'transferSize') =>
        entries.reduce((total, entry) => total + entry[key], 0);
      return {
        ttfbMs: navigation.responseStart,
        fcpMs: performance.getEntriesByName('first-contentful-paint')[0]?.startTime,
        lcpMs: marks.lcpMs,
        hydrationMs: marks.hydrationMs,
        htmlEncodedBytes: navigation.encodedBodySize,
        transferBytes: sum([navigation, ...resources], 'transferSize'),
        wasmTransferBytes: sum(wasm, 'transferSize'),
        wasmEncodedBytes: sum(wasm, 'encodedBodySize'),
        wasmRequests: wasm.length,
      };
    });
    const { fcpMs } = metrics;
    assert(fcpMs && metrics.wasmRequests, `缺少浏览器性能条目 ${url}`);
    assert.deepEqual(errors, [], url);
    return { ...metrics, fcpMs, originCache: (await response?.headerValue('x-ssr-cache')) ?? null };
  } finally {
    await page.close();
  }
}

async function configurePage(context: BrowserContext, page: Page) {
  const cdp = await context.newCDPSession(page);
  await cdp.send('Network.enable');
  await cdp.send('Network.emulateNetworkConditions', {
    offline: false,
    latency: BROWSER_PROFILE.latencyMs,
    downloadThroughput: BROWSER_PROFILE.downloadBytesPerSecond,
    uploadThroughput: BROWSER_PROFILE.uploadBytesPerSecond,
  });
  await cdp.send('Emulation.setCPUThrottlingRate', { rate: BROWSER_PROFILE.cpuSlowdown });
  await page.addInitScript(() => {
    performance.setResourceTimingBufferSize(1000);
    const marks = { hydrationMs: 0, lcpMs: 0 };
    (window as unknown as InstrumentedWindow).benchmarkMarks = marks;
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) marks.lcpMs = entry.startTime;
    }).observe({ type: 'largest-contentful-paint', buffered: true });
    const observer = new MutationObserver(() => {
      if (document.documentElement?.dataset.hydrated === 'true') {
        marks.hydrationMs = performance.now();
        observer.disconnect();
      }
    });
    observer.observe(document, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ['data-hydrated'],
    });
  });
}

export async function measureBrowser(browser: Browser, url: string, round: number) {
  // 每轮新建上下文测浏览器冷加载。在同一上下文用新页面测复用 HTTP 缓存的加载。
  const context = await browser.newContext({
    viewport: BROWSER_PROFILE.viewport,
    deviceScaleFactor: 1,
    isMobile: true,
    hasTouch: true,
    locale: 'zh-CN',
    timezoneId: 'Asia/Shanghai',
  });
  try {
    const cold = await measurePage(context, url);
    const warm = await measurePage(context, url);
    return { round, cold, warm };
  } finally {
    await context.close();
  }
}

export type BrowserRun = Awaited<ReturnType<typeof measureBrowser>>;
type CacheMode = 'cold' | 'warm';
type Sample = BrowserRun[CacheMode];
const METRICS = [
  'ttfbMs',
  'fcpMs',
  'lcpMs',
  'hydrationMs',
  'htmlEncodedBytes',
  'transferBytes',
  'wasmTransferBytes',
  'wasmEncodedBytes',
  'wasmRequests',
] as const satisfies readonly (keyof Sample)[];

export function summarizeBrowser(runs: BrowserRun[], mode: CacheMode) {
  return Object.fromEntries(
    METRICS.map((metric) => [metric, summarize(runs.map((run) => run[mode][metric]))]),
  );
}

export function compareBrowser(a: BrowserRun[], b: BrowserRun[]) {
  return (['cold', 'warm'] as const).flatMap((mode) =>
    (['lcpMs', 'hydrationMs'] as const).map((metric) => ({
      mode,
      metric,
      result: comparePaired(a, b, (run) => run[mode][metric]),
    })),
  );
}
