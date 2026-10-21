#!/usr/bin/env bun
/**
 * P5a release WASM preload A/B. Existing LOCAL release server only; no build/DB lifecycle.
 *
 * BASE_URL=http://127.0.0.1:8080 POST_PATH=/post/dual-flavor-post SSR_CACHE_SECS=60 \
 *   bun scripts/test-wasm-preload.cjs
 * RELEASE_BUILD defaults to target/dx/yggdrasil/release/web; alternatively RELEASE_MANIFEST.
 * PLAYWRIGHT_MODULE / CHROMIUM_PATH follow other browser scripts. SAMPLES >= 5 (default 5).
 * WASM_PRELOAD_ARTIFACT_DIR overrides target/regressions/<timestamp>-wasm-preload.
 * LATENCY_MS=40 DOWNLOAD_KBPS=4000 UPLOAD_KBPS=10000 CPU_RATE=4 TIMEOUT_MS=90000
 * OBSERVE_MS=5000 (>=5000, after load, to catch unused-preload console warnings).
 *
 * Three fixed routes: HOME_PATH=/, POST_PATH (published article), FULL_PATH=/about/components.
 * Each round alternates A/B order. Each arm/purpose gets a fresh context, cold document,
 * then a NEW document in the SAME context (warm HTTP cache, not reload revalidation).
 * Separate passive-vitals and active-interaction pairs: 5 rounds = 120 documents.
 * Interaction is a trusted click opening the shared Dioxus mobile menu, NOT __dx_mainWasm.
 * This is an upper-bound proxy for menu readiness, not standards-based TTI or editor readiness.
 *
 * NO Playwright routes: routing disables HTTP cache, even for unhandled requests.
 * Two loopback HTTP proxies buffer/decompress HTML equally; only baseline removes head WASM
 * preload. Both strip document encoding/validators and use no-store. Assets preserve RAW br
 * bytes/cache headers via node:http pipes. No CDP cache disabling, SWs, or persistent profiles.
 * Proxy buffering changes document TTFB/streaming: compare these arms, not absolute origin TTI.
 * Writes raw samples + medians/paired deltas/retention gates to report.json, plus one
 * post-interaction screenshot per preload page. --self-test runs pure helpers only.
 * Exit 1 = correctness/measurement failure; exit 2 = valid data but retain gates not met.
 * Exit 0 = retain candidate under this fixed local profile, NOT a production performance claim.
 */
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const https = require('node:https');
const zlib = require('node:zlib');
const { createHash } = require('node:crypto');

const ROOT = path.resolve(__dirname, '..');
const BASE = new URL(process.env.BASE_URL || 'http://127.0.0.1:8080');
assert(['localhost', '127.0.0.1', '[::1]'].includes(BASE.hostname), 'BASE_URL must be loopback, never production');
assert(['http:', 'https:'].includes(BASE.protocol) && BASE.pathname === '/' && !BASE.search && !BASE.hash && !BASE.username && !BASE.password,
  'BASE_URL must be a credential-free local origin');
const BUILD = path.resolve(process.env.RELEASE_BUILD || path.join(ROOT, 'target/dx/yggdrasil/release/web'));
const MANIFEST = path.resolve(process.env.RELEASE_MANIFEST || path.join(BUILD, 'public/release-manifest.json'));
const ARTIFACTS = path.resolve(process.env.WASM_PRELOAD_ARTIFACT_DIR ||
  path.join(process.env.REGRESSION_ARTIFACT_DIR || path.join(ROOT, 'target/regressions'), `${Date.now()}-wasm-preload`));
function number(name, fallback, min) {
  const value = Number(process.env[name] ?? fallback);
  assert(Number.isFinite(value) && value >= min, `${name} must be >= ${min}`);
  return value;
}
const CONFIG = {
  samples: number('SAMPLES', 5, 5), latencyMs: number('LATENCY_MS', 40, 0),
  downloadKbps: number('DOWNLOAD_KBPS', 4000, 1), uploadKbps: number('UPLOAD_KBPS', 10000, 1),
  cpuRate: number('CPU_RATE', 4, 1), timeoutMs: number('TIMEOUT_MS', 90000, 1000),
  observeMs: number('OBSERVE_MS', 5000, 5000), viewport: { width: 390, height: 844 },
  declaredSsrCacheSecs: process.env.SSR_CACHE_SECS ?? null,
};
assert(Number.isInteger(CONFIG.samples), 'SAMPLES must be an integer');
const POST = process.env.POST_PATH || '/post/dual-flavor-post';
assert(/^\/post\/[^/?#]+$/.test(POST), 'POST_PATH must be /post/<published-slug>');
const HOME = process.env.HOME_PATH || '/';
const FULL = process.env.FULL_PATH || '/about/components';
assert(HOME === '/' || /^\/page\/[1-9]\d*$/.test(HOME), 'HOME_PATH must be / or /page/<n>');
assert(FULL === '/about/components' || /^\/about\/components\/[a-z0-9-]+$/.test(FULL), 'FULL_PATH must be a public showcase route');
const CASES = [
  { name: 'reader-home', path: HOME, flavor: 'reader', selector: 'main' },
  { name: 'reader-article', path: POST, flavor: 'reader', selector: 'h1.post-title' },
  { name: 'full-showcase', path: FULL, flavor: 'full', selector: FULL === '/about/components' ? '.showcase-gallery' : '.showcase-detail' },
];
const MENU = 'header[data-vt-shell="frontend-header"] button[aria-controls="mobile-nav-menu"]';
const READ_APIS = new Set(['getsitesettings', 'getshowcasedata', 'getcurrentuser',
  'listpublishedposts', 'getpublicpostbyslug', 'getcomments', 'checkpendingstatus']);
const report = {
  schemaVersion: 1, startedAt: new Date().toISOString(), base: BASE.origin, manifestPath: MANIFEST,
  config: CONFIG, cases: CASES, samples: [], preflight: [], proxyDocuments: [], blockedRequests: [], errors: [],
  methodology: {
    cache: 'Fresh nonpersistent context = cold HTTP cache; second new page in same context = warm. Browser/OS/server/compiled-code caches are NOT cleared.',
    routing: 'No Playwright route; Network.setCacheDisabled(false); serviceWorkers=block. Assets transparently piped, HTML buffered/decompressed/no-store on BOTH arms.',
    interaction: 'Trusted mobile-menu clicks, observed aria-expanded and .is-open. Navigation-relative successful response time is an upper bound, with polling/actionability overhead; NOT formal TTI.',
    vitals: 'Independent no-input companion documents; LCP and session-window CLS observed until load + OBSERVE_MS. WASM timing/transfer medians also use these passive documents; interaction samples retain their own raw resource timings.',
    safety: 'Only local origin. Fresh anonymous contexts; cookies/Authorization stripped upstream. API deny-by-default except named read-only calls (including anonymous current-user status); never login/register/logout/execute/upload/submit.',
  },
  limitations: [
    'Both proxies buffer full SSR HTML before delivery and normalize document headers; relative A/B only, not raw-origin streaming/TTFB. Asset Brotli bytes/cache headers are untouched.',
    'Two proxy ports isolate cache namespaces; A/B use otherwise identical fresh contexts and alternate order. Warm is a repeat direct document visit, not reload, bfcache, SPA, or persisted browser startup.',
    'Menu readiness does not establish article comment/editor/runner readiness or full-app hydration/compilation completion. No compile/instantiate timings are inferred from __dx_mainWasm.',
    'LCP/CLS are finite-window local mobile lab observations, not field metrics. Long-lived shifts, desktop/themes/reduced-motion, authentication and editing are not covered.',
    'External networking and direct origin access are blocked by a dead browser proxy; only the two guarded measurement proxy ports bypass it. Remote-resource pages are not representative; failed requests are recorded.',
    'SSR_CACHE_SECS is caller-declared, not introspected. Server cache status/generation and HTML hashes are recorded; use immutable DB fixtures and a quiet release server.',
  ],
};
let browser;
const proxies = [];
const capturedDocuments = new Map();
const agent = BASE.protocol === 'https:' ? new https.Agent({ keepAlive: true }) : new http.Agent({ keepAlive: true });
const transport = BASE.protocol === 'https:' ? https : http;
const sha = value => createHash('sha256').update(value).digest('hex');
const median = values => {
  const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
  return sorted.length ? (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2 : null;
};
const mad = values => { const m = median(values); return median(values.map(v => Math.abs(v - m))); };
function decode(buffer, encoding) {
  if (!encoding || encoding === 'identity') return buffer;
  if (encoding === 'br') return zlib.brotliDecompressSync(buffer);
  if (encoding === 'gzip') return zlib.gunzipSync(buffer);
  if (encoding === 'deflate') return zlib.inflateSync(buffer);
  throw new Error(`Unsupported content-encoding ${encoding}`);
}
function attr(tag, name) {
  const match = tag.match(new RegExp(`\\s${name}\\s*=\\s*(?:"([^"]*)"|'([^']*)'|([^\\s>]+))`, 'i'));
  return (match?.[1] ?? match?.[2] ?? match?.[3] ?? '').replace(/&amp;/g, '&');
}
function wasmLinks(html) {
  const head = html.match(/<head\b[^>]*>[\s\S]*?<\/head\s*>/i)?.[0];
  assert(head, 'SSR response has no head');
  return [...head.matchAll(/<link\b[^>]*>/gi)].map(match => match[0]).filter(tag =>
    attr(tag, 'rel').toLowerCase().split(/\s+/).includes('preload') &&
    (attr(tag, 'type').toLowerCase() === 'application/wasm' || /\.wasm$/.test(new URL(attr(tag, 'href'), BASE).pathname)));
}
function baselineHtml(html) {
  const links = new Set(wasmLinks(html));
  return html.replace(/<head\b[^>]*>[\s\S]*?<\/head\s*>/i, head =>
    head.replace(/<link\b[^>]*>/gi, tag => links.has(tag) ? '' : tag));
}
function requestBuffer(route, headers = {}) {
  return new Promise((resolve, reject) => {
    const req = transport.get(new URL(route, BASE), { agent, headers: { 'accept-encoding': 'br, gzip', ...headers } }, res => {
      const chunks = [];
      res.on('data', chunk => chunks.push(chunk));
      res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, raw: Buffer.concat(chunks) }));
      res.on('error', reject);
    });
    req.setTimeout(CONFIG.timeoutMs, () => req.destroy(new Error('Origin request timeout')));
    req.on('error', reject);
  });
}
function serveHtml(variant, route, outgoing, source) {
  try {
    const { html, headers, status } = source;
    assert(!/\.wasm/i.test(String(headers.link || '')), 'HTTP Link WASM hints would contaminate baseline');
    const body = variant === 'baseline' ? baselineHtml(html) : html;
    report.proxyDocuments.push({ variant, path: route, status,
      sourceSha256: sha(html), transformedSha256: sha(body), sourcePreloads: wasmLinks(html).length,
      servedPreloads: wasmLinks(body).length, ssrCache: headers['x-ssr-cache'] ?? null,
      ssrGeneration: headers['x-ssr-generation'] ?? null, captured: Boolean(source.captured) });
    const outHeaders = { ...headers };
    for (const name of ['content-encoding', 'content-length', 'etag', 'last-modified', 'content-md5',
      'digest', 'content-digest', 'repr-digest', 'accept-ranges', 'age', 'expires', 'transfer-encoding',
      'set-cookie', 'connection']) delete outHeaders[name];
    outHeaders['cache-control'] = 'no-store';
    outgoing.writeHead(status, outHeaders); outgoing.end(body);
  } catch (error) {
    report.errors.push(`Proxy ${variant} ${route}: ${error.message}`);
    outgoing.writeHead(502); outgoing.end('HTML transform failed');
  }
}
async function startProxy(variant) {
  const server = http.createServer((incoming, outgoing) => {
    const url = new URL(incoming.url, BASE);
    const api = url.pathname.startsWith('/api/');
    const apiName = url.pathname.split('/').pop().replace(/[^a-z]/gi, '').toLowerCase();
    const safe = url.origin === BASE.origin && (api ? READ_APIS.has(apiName) && ['GET', 'HEAD', 'POST'].includes(incoming.method) : ['GET', 'HEAD'].includes(incoming.method));
    if (!safe) {
      report.blockedRequests.push({ variant, method: incoming.method, path: url.pathname });
      incoming.resume(); outgoing.writeHead(403); outgoing.end('Blocked by preload measurement safety policy'); return;
    }
    if (incoming.method === 'GET' && !url.search && capturedDocuments.has(url.pathname)) {
      incoming.resume();
      serveHtml(variant, url.pathname, outgoing, capturedDocuments.get(url.pathname));
      return;
    }
    const headers = { ...incoming.headers, host: BASE.host };
    for (const key of ['authorization', 'cookie', 'connection', 'proxy-connection']) delete headers[key];
    // Origin translation only for allowlisted read APIs; no state-changing APIs are forwarded.
    if (headers.origin) headers.origin = BASE.origin;
    if (headers.referer) headers.referer = new URL(new URL(headers.referer).pathname, BASE).href;
    const upstream = transport.request(url, { method: incoming.method, headers, agent }, res => {
      // 页面结束可能取消尚未完成的图片等请求；所有响应都要接住流错误。
      res.on('error', error => {
        if (outgoing.destroyed) return;
        report.errors.push(`Proxy response: ${error.message}`);
        outgoing.destroy();
      });
      const outHeaders = { ...res.headers };
      delete outHeaders['set-cookie'];
      delete outHeaders.connection;
      if (!String(res.headers['content-type']).includes('text/html')) {
        outgoing.writeHead(res.statusCode, outHeaders); res.pipe(outgoing); return;
      }
      const chunks = [];
      res.on('data', chunk => chunks.push(chunk));
      res.on('end', () => {
        try {
          const html = decode(Buffer.concat(chunks), res.headers['content-encoding']).toString('utf8');
          serveHtml(variant, url.pathname, outgoing, { html, status: res.statusCode, headers: res.headers });
        } catch (error) {
          report.errors.push(`Proxy ${variant} ${url.pathname}: ${error.message}`);
          outgoing.writeHead(502); outgoing.end('HTML transform failed');
        }
      });
    });
    upstream.setTimeout(CONFIG.timeoutMs, () => upstream.destroy(new Error('Proxy upstream timeout')));
    upstream.on('error', error => {
      if (outgoing.destroyed) return;
      report.errors.push(`Proxy upstream: ${error.message}`);
      if (!outgoing.headersSent) outgoing.writeHead(502);
      outgoing.end();
    });
    outgoing.on('close', () => { if (!outgoing.writableFinished) upstream.destroy(); });
    incoming.pipe(upstream);
  });
  server.on('clientError', (_error, socket) => socket.destroy());
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const proxy = { variant, server, base: `http://127.0.0.1:${server.address().port}` };
  proxies.push(proxy);
  return proxy;
}
async function preflight() {
  const manifestBytes = fs.readFileSync(MANIFEST);
  const manifest = JSON.parse(manifestBytes);
  report.releaseManifest = manifest;
  report.manifestSha256 = sha(manifestBytes);
  const remote = await requestBuffer('/release-manifest.json');
  assert.equal(remote.status, 200, 'Origin must serve release-manifest.json (not a dev server)');
  assert.equal(sha(decode(remote.raw, remote.headers['content-encoding'])), sha(manifestBytes), 'Local and served manifests differ');
  const wasm = {};
  for (const flavor of ['reader', 'full']) {
    const assets = manifest.flavors[flavor].assets.filter(asset => asset.path.endsWith('.wasm'));
    assert.equal(assets.length, 1, `Expected exactly one ${flavor} WASM in release manifest`);
    wasm[flavor] = assets[0].path;
    const asset = await requestBuffer(assets[0].path);
    const body = decode(asset.raw, asset.headers['content-encoding']);
    assert.equal(asset.status, 200);
    assert.match(String(asset.headers['content-type']), /^application\/wasm(?:;|$)/i);
    assert.equal(asset.headers['content-encoding'], 'br', `${flavor} origin must serve actual Brotli`);
    assert.equal(sha(body), assets[0].sha256, `${flavor} served WASM disagrees with manifest`);
    assert.match(String(asset.headers['cache-control']), /(?:max-age=[1-9]\d*|immutable)/i, 'WASM needs reusable HTTP cache headers');
    report.preflight.push({ flavor, wasm: assets[0].path, rawBrotliBytes: asset.raw.length,
      decodedBytes: body.length, sha256: sha(body), headers: asset.headers });
  }
  assert.notEqual(wasm.reader, wasm.full, 'Two independent flavors required');
  for (const entry of CASES) {
    // Prime the origin SSR cache before either browser arm. No fixture writes or API calls.
    const responses = [];
    for (let i = 0; i < 3; i++) {
      const res = await requestBuffer(entry.path);
      assert.equal(res.status, 200, `${entry.path} must exist; set POST_PATH to a fixed published article`);
      const html = decode(res.raw, res.headers['content-encoding']).toString('utf8');
      const links = wasmLinks(html);
      assert.equal(links.length, 1, `${entry.path}: expected one head WASM preload; build P5a release first`);
      assert.equal(new URL(attr(links[0], 'href'), BASE).pathname, wasm[entry.flavor]);
      assert.equal(attr(links[0], 'as'), 'fetch');
      assert.equal(attr(links[0], 'type'), 'application/wasm');
      assert(/\scrossorigin(?:\s|=|\/?>)/i.test(links[0]), 'WASM preload must specify crossorigin');
      assert(!/\.wasm/i.test(String(res.headers.link || '')), 'HTTP Link hints unsupported by baseline transform');
      responses.push({ status: res.status, ssrCache: res.headers['x-ssr-cache'] ?? null,
        ssrGeneration: res.headers['x-ssr-generation'] ?? null, htmlSha256: sha(html), preload: links[0] });
      fs.writeFileSync(path.join(ARTIFACTS, `${entry.name}-origin-${i}.html`), html);
      if (i === 2) capturedDocuments.set(entry.path, {
        html, status: res.status, headers: res.headers, captured: true,
      });
    }
    report.preflight.push({ case: entry.name, responses });
  }
  return wasm;
}
function installObservers() {
  performance.setResourceTimingBufferSize(5000);
  const state = { lcp: null, cls: 0, shifts: [], longTasks: [], supported: PerformanceObserver.supportedEntryTypes };
  window.__preloadMeasurement = state;
  if (state.supported.includes('largest-contentful-paint')) new PerformanceObserver(list => {
    for (const entry of list.getEntries()) state.lcp = { startTime: entry.startTime, size: entry.size,
      url: entry.url, element: entry.element?.tagName ?? null };
  }).observe({ type: 'largest-contentful-paint', buffered: true });
  let start = 0; let end = 0; let score = 0;
  if (state.supported.includes('layout-shift')) new PerformanceObserver(list => {
    for (const entry of list.getEntries()) {
      state.shifts.push({ startTime: entry.startTime, value: entry.value, hadRecentInput: entry.hadRecentInput });
      if (entry.hadRecentInput) continue;
      if (entry.startTime - end < 1000 && entry.startTime - start < 5000) score += entry.value;
      else { start = entry.startTime; score = entry.value; }
      end = entry.startTime; state.cls = Math.max(state.cls, score);
    }
  }).observe({ type: 'layout-shift', buffered: true });
  if (state.supported.includes('longtask')) new PerformanceObserver(list => {
    state.longTasks.push(...list.getEntries().map(entry => ({ startTime: entry.startTime, duration: entry.duration })));
  }).observe({ type: 'longtask', buffered: true });
}
async function menuReady(page) {
  const button = page.locator(MENU);
  await button.waitFor({ state: 'visible' });
  let attempts = 0;
  const deadline = Date.now() + CONFIG.timeoutMs;
  while (Date.now() < deadline) {
    attempts++;
    // Check before another click: a late previous commit must not toggle the menu closed.
    const ready = await page.evaluate(selector => {
      const node = document.querySelector(selector);
      return node?.getAttribute('aria-expanded') === 'true' && document.querySelector('#mobile-nav-menu.is-open') ? performance.now() : null;
    }, MENU);
    if (ready !== null) return { readyMs: ready, attempts, probe: 'trusted click -> Dioxus aria-expanded + menu.is-open' };
    await button.click({ timeout: CONFIG.timeoutMs });
    try {
      await page.waitForFunction(selector => document.querySelector(selector)?.getAttribute('aria-expanded') === 'true' &&
        document.querySelector('#mobile-nav-menu.is-open'), MENU, { timeout: 200, polling: 'raf' });
    } catch (error) { if (error.name !== 'TimeoutError') throw error; }
  }
  throw new Error('No observable Dioxus menu interaction; WASM initialization alone is insufficient');
}
async function sample(context, proxy, entry, purpose, cache, round, wasm) {
  const row = { case: entry.name, path: entry.path, flavor: entry.flavor, variant: proxy.variant, purpose, cache, round,
    wasmRequests: [], apiRequests: [], console: [], pageErrors: [], failedRequests: [], violations: [] };
  report.samples.push(row);
  const page = await context.newPage();
  page.setDefaultTimeout(CONFIG.timeoutMs);
  page.setDefaultNavigationTimeout(CONFIG.timeoutMs);
  const cdp = await context.newCDPSession(page);
  const byId = new Map();
  const cachedIds = new Set();
  cdp.on('Network.requestWillBeSent', event => {
    if (!new URL(event.request.url).pathname.endsWith('.wasm')) return;
    const request = { id: event.requestId, url: event.request.url, timestamp: event.timestamp,
      initiator: event.initiator, priority: event.request.initialPriority, servedFromCache: cachedIds.has(event.requestId) };
    row.wasmRequests.push(request); byId.set(event.requestId, request);
  });
  cdp.on('Network.requestServedFromCache', event => {
    cachedIds.add(event.requestId);
    if (byId.has(event.requestId)) byId.get(event.requestId).servedFromCache = true;
  });
  cdp.on('Network.responseReceived', event => {
    const request = byId.get(event.requestId);
    if (request) Object.assign(request, { responseTimestamp: event.timestamp, status: event.response.status,
      mimeType: event.response.mimeType, headers: event.response.headers, timing: event.response.timing,
      fromDiskCache: Boolean(event.response.fromDiskCache), fromServiceWorker: Boolean(event.response.fromServiceWorker) });
  });
  cdp.on('Network.loadingFinished', event => {
    const request = byId.get(event.requestId);
    if (request) Object.assign(request, { endTimestamp: event.timestamp, networkEncodedDataLength: event.encodedDataLength });
  });
  page.on('request', request => {
    const url = new URL(request.url());
    if (url.pathname.startsWith('/api/')) row.apiRequests.push({ method: request.method(), url: request.url() });
  });
  page.on('console', message => { if (['warning', 'error'].includes(message.type())) row.console.push({ type: message.type(), text: message.text() }); });
  page.on('pageerror', error => row.pageErrors.push(error.message));
  page.on('requestfailed', request => row.failedRequests.push({ url: request.url(), error: request.failure()?.errorText }));
  try {
    const source = capturedDocuments.get(entry.path);
    row.originCapture = { sha256: sha(source.html), ssrCache: source.headers['x-ssr-cache'] ?? null,
      ssrGeneration: source.headers['x-ssr-generation'] ?? null };
    await cdp.send('Network.enable');
    await cdp.send('Network.setCacheDisabled', { cacheDisabled: false });
    await cdp.send('Network.emulateNetworkConditions', { offline: false, latency: CONFIG.latencyMs,
      downloadThroughput: CONFIG.downloadKbps * 1000 / 8, uploadThroughput: CONFIG.uploadKbps * 1000 / 8 });
    await cdp.send('Emulation.setCPUThrottlingRate', { rate: CONFIG.cpuRate });
    await page.addInitScript(installObservers);
    const response = await page.goto(proxy.base + entry.path, { waitUntil: 'commit' });
    row.document = { status: response.status(), headers: await response.allHeaders() };
    // Start interaction immediately after commit; passive companion never receives any input.
    if (purpose === 'interaction') row.interaction = await menuReady(page);
    await page.waitForLoadState('load');
    await page.locator(entry.selector).first().waitFor({ state: 'visible' });
    await page.waitForTimeout(CONFIG.observeMs);
    row.performance = await page.evaluate(() => ({
      ...window.__preloadMeasurement, observedUntilMs: performance.now(),
      navigation: performance.getEntriesByType('navigation')[0]?.toJSON(),
      resources: performance.getEntriesByType('resource').filter(resource => /\.(wasm|js|css)(?:[?#]|$)/.test(resource.name)).map(resource => resource.toJSON()),
      preloads: [...document.head.querySelectorAll('link[rel="preload"]')].filter(link => link.type === 'application/wasm' || new URL(link.href).pathname.endsWith('.wasm'))
        .map(link => ({ href: link.href, as: link.as, type: link.type, crossOrigin: link.crossOrigin })),
    }));
    const timing = row.performance.resources.filter(resource => new URL(resource.name).pathname.endsWith('.wasm'));
    const expected = new URL(wasm[entry.flavor], proxy.base).href;
    const check = (ok, message) => { if (!ok) row.violations.push(message); };
    check(row.document.status === 200 && page.url() === proxy.base + entry.path, 'Unexpected document status/redirect');
    check(row.wasmRequests.length === 1 && timing.length === 1, 'Must have exactly ONE WASM request/resource per document (including warm cache)');
    check(row.wasmRequests.every(request => request.url === expected) && timing.every(resource => resource.name === expected), 'Wrong-flavor or unexpected WASM URL');
    const otherFlavor = entry.flavor === 'reader' ? 'full' : 'reader';
    const ownAssets = new Set(report.releaseManifest.flavors[entry.flavor].assets.map(asset => asset.path));
    const wrongAssets = new Set(report.releaseManifest.flavors[otherFlavor].assets.map(asset => asset.path).filter(asset => !ownAssets.has(asset)));
    check(!row.performance.resources.some(resource => wrongAssets.has(new URL(resource.name).pathname)), 'Wrong-flavor asset closure loaded');
    check(row.performance.preloads.length === (proxy.variant === 'preload' ? 1 : 0), 'Unexpected head WASM preload count');
    if (row.performance.preloads[0]) check(row.performance.preloads[0].href === expected, 'Wrong-flavor preload');
    for (const request of row.wasmRequests) {
      const headers = Object.fromEntries(Object.entries(request.headers || {}).map(([name, value]) => [name.toLowerCase(), value]));
      check(request.mimeType === 'application/wasm', 'WASM MIME must be application/wasm');
      check(headers['content-encoding'] === 'br', 'WASM response must preserve Brotli');
      check(Boolean(request.endTimestamp) && request.status === 200 && !request.fromServiceWorker, 'Incomplete/non-200/SW WASM response');
    }
    row.wasmTiming = timing[0] ? { startMs: timing[0].startTime, requestStartMs: timing[0].requestStart,
      responseStartMs: timing[0].responseStart, endMs: timing[0].responseEnd, transferSize: timing[0].transferSize,
      encodedBodySize: timing[0].encodedBodySize, decodedBodySize: timing[0].decodedBodySize,
      initiatorType: timing[0].initiatorType, deliveryType: timing[0].deliveryType } : null;
    row.cacheEvidence = row.wasmRequests.some(request => request.servedFromCache || request.fromDiskCache) ||
      Boolean(timing[0] && timing[0].transferSize === 0 && timing[0].encodedBodySize > 0);
    check(cache === 'warm' ? row.cacheEvidence : !row.cacheEvidence, `${cache} cache evidence disagrees with label`);
    row.preloadWarnings = row.console.filter(message => /preload/i.test(message.text));
    check(row.preloadWarnings.length === 0, 'Preload warning/error detected (including unused or credentials mismatch)');
    check(row.pageErrors.length === 0, 'Page JavaScript errors');
    check(!row.failedRequests.some(request => new URL(request.url).origin === proxy.base), 'Local resource/API request failed');
    if (purpose === 'vitals') check(row.performance.lcp !== null && row.performance.supported.includes('layout-shift'), 'Missing LCP/CLS observer data');
    if (proxy.variant === 'preload' && purpose === 'interaction' && cache === 'warm' && round === 0) {
      row.screenshot = path.join(ARTIFACTS, `${entry.name}-preload-menu-open.png`);
      await page.screenshot({ path: row.screenshot });
    }
  } catch (error) { row.violations.push(error.stack || error.message); }
  finally { await page.close(); }
  console.log(`${entry.name} ${round + 1}/${CONFIG.samples} ${proxy.variant} ${purpose} ${cache}: ${row.violations.length ? 'FAIL' : 'OK'}`);
  fs.writeFileSync(path.join(ARTIFACTS, 'report.json'), JSON.stringify(report, null, 2) + '\n');
}
function summarize() {
  report.summaries = [];
  const failures = report.samples.filter(sample => sample.violations.length);
  const gates = [];
  let readerBenefit = false;
  let noisyBenefit = false;
  const metrics = { wasmStartMs: sample => sample.purpose === 'vitals' ? sample.wasmTiming?.startMs : null,
    wasmEndMs: sample => sample.purpose === 'vitals' ? sample.wasmTiming?.endMs : null,
    transferBytes: sample => sample.purpose === 'vitals' ? sample.wasmTiming?.transferSize : null,
    interactionReadyMs: sample => sample.purpose === 'interaction' ? sample.interaction?.readyMs : null,
    lcpMs: sample => sample.purpose === 'vitals' ? sample.performance?.lcp?.startTime : null,
    cls: sample => sample.purpose === 'vitals' ? sample.performance?.cls : null };
  for (const entry of CASES) for (const cache of ['cold', 'warm']) {
    const cell = { case: entry.name, cache, arms: {}, paired: {} };
    for (const variant of ['baseline', 'preload']) {
      const rows = report.samples.filter(sample => sample.case === entry.name && sample.cache === cache && sample.variant === variant);
      cell.arms[variant] = { documents: rows.length, failures: rows.filter(row => row.violations.length).length };
      for (const [name, get] of Object.entries(metrics)) {
        const values = rows.map(get).filter(Number.isFinite);
        cell.arms[variant][name] = { n: values.length, median: median(values), mad: mad(values) };
      }
    }
    // Pair by round AND purpose; no mixing passive and interacted navigation timings.
    for (const [name, get] of Object.entries(metrics)) {
      const deltas = [];
      for (const a of report.samples.filter(sample => sample.case === entry.name && sample.cache === cache && sample.variant === 'baseline')) {
        const b = report.samples.find(sample => sample.case === a.case && sample.cache === a.cache && sample.variant === 'preload' && sample.round === a.round && sample.purpose === a.purpose);
        if (Number.isFinite(get(a)) && b && Number.isFinite(get(b))) deltas.push(get(a) - get(b));
      }
      cell.paired[name] = { n: deltas.length, rawBaselineMinusPreload: deltas, median: median(deltas), mad: mad(deltas) };
    }
    const a = cell.arms.baseline; const b = cell.arms.preload;
    const regression = (name, floor, ratio) => a[name].median !== null && b[name].median !== null &&
      b[name].median - a[name].median <= Math.max(floor, a[name].median * ratio);
    gates.push({ case: entry.name, cache, noInteractionRegression: regression('interactionReadyMs', 50, 0.05),
      noLcpRegression: regression('lcpMs', 100, 0.05), noClsRegression: regression('cls', 0.02, 0) });
    if (cache === 'cold' && entry.flavor === 'reader') {
      const advance = cell.paired.wasmStartMs;
      const ready = cell.paired.interactionReadyMs;
      const meaningful = advance.median >= 20 && ready.median >= Math.max(50, a.interactionReadyMs.median * 0.05);
      if (meaningful && ready.median > 2 * ready.mad) readerBenefit = true;
      else if (meaningful) noisyBenefit = true;
    }
    report.summaries.push(cell);
  }
  const complete = report.samples.length === CASES.length * CONFIG.samples * 2 * 2 * 2 && !failures.length &&
    !report.errors.length && !report.blockedRequests.length;
  const stableHtml = CASES.every(entry => {
    const documents = report.proxyDocuments.filter(document => document.path === entry.path);
    return new Set(documents.map(document => document.sourceSha256)).size === 1 &&
      new Set(documents.map(document => document.ssrGeneration)).size === 1 &&
      new Set(documents.map(document => document.ssrCache)).size === 1;
  });
  report.retention = {
    criteria: 'All correctness/cache/MIME/br/safety checks pass; identical upstream HTML/generation/SSR-cache status across arms; at least one cold reader case has paired median WASM start >=20ms earlier AND menu readiness >=max(50ms,5%) earlier, exceeding 2x paired-delta MAD; every cold/warm case menu regression <=max(50ms,5%), passive LCP regression <=max(100ms,5%), CLS increase <=0.02. Requires declared SSR cache configuration.',
    complete, stableHtml, declaredSsrConfig: CONFIG.declaredSsrCacheSecs !== null,
    readerBenefit, noisyBenefit, gates,
    decision: !complete || !stableHtml || CONFIG.declaredSsrCacheSecs === null ? 'inconclusive' :
      gates.some(gate => !gate.noInteractionRegression || !gate.noLcpRegression || !gate.noClsRegression) ? 'revert-or-adjust' :
        readerBenefit ? 'retain-candidate' : noisyBenefit ? 'inconclusive-noisy' : 'no-demonstrated-benefit',
  };
  if (!complete) process.exitCode = 1;
  else if (report.retention.decision !== 'retain-candidate') process.exitCode = 2;
}
async function main() {
  fs.mkdirSync(ARTIFACTS, { recursive: true });
  const wasm = await preflight();
  const arms = { baseline: await startProxy('baseline'), preload: await startProxy('preload') };
  report.proxies = Object.fromEntries(Object.entries(arms).map(([name, proxy]) => [name, proxy.base]));
  const { chromium } = require(process.env.PLAYWRIGHT_MODULE || path.join(ROOT, 'libs/node_modules/playwright'));
  report.browserProxyBypass = `<-loopback>,${Object.values(arms).map(proxy => new URL(proxy.base).host).join(',')}`;
  browser = await chromium.launch({ headless: true,
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
    // Remove Chromium's implicit loopback bypass, then allow ONLY the two guarded proxies.
    proxy: { server: 'http://127.0.0.1:9', bypass: report.browserProxyBypass } });
  report.browserVersion = browser.version();
  report.bunVersion = Bun.version;
  for (const entry of CASES) for (let round = 0; round < CONFIG.samples; round++) {
    for (const variant of round % 2 ? ['preload', 'baseline'] : ['baseline', 'preload']) {
      for (const purpose of round % 2 ? ['interaction', 'vitals'] : ['vitals', 'interaction']) {
        const context = await browser.newContext({ viewport: CONFIG.viewport, colorScheme: 'light', reducedMotion: 'no-preference',
          serviceWorkers: 'block', locale: 'zh-CN', timezoneId: 'Asia/Shanghai' });
        try {
          await sample(context, arms[variant], entry, purpose, 'cold', round, wasm);
          await sample(context, arms[variant], entry, purpose, 'warm', round, wasm);
        } finally { await context.close(); }
      }
    }
  }
  summarize();
  console.log(JSON.stringify({ retention: report.retention, medians: report.summaries }, null, 2));
}
function selfTest() {
  const link = '<link rel="preload" as="fetch" type="application/wasm" href="/assets/reader_bg-123.wasm" crossorigin="anonymous">';
  const css = '<link rel="preload" as="style" href="/style.css">';
  const module = '<link rel="modulepreload" href="/assets/entry.js">';
  const html = `<html><head>${link}${css}${module}</head><body>${link}<script>window.keep = true;</script></body></html>`;
  assert.deepEqual(wasmLinks(html), [link]);
  assert.equal(baselineHtml(html), `<html><head>${css}${module}</head><body>${link}<script>window.keep = true;</script></body></html>`);
  assert.equal(baselineHtml(baselineHtml(html)), baselineHtml(html), 'Transform is idempotent');
  const variants = [
    "<LINK HREF='/assets/a.wasm?x=1&amp;y=2' REL='prefetch PRELOAD' AS=fetch crossorigin>",
    '<link href=/assets/a.wasm rel=preload as=fetch crossorigin>',
    '<link rel="preload" type="application/wasm" href="/opaque">',
  ];
  for (const tag of variants) {
    assert.deepEqual(wasmLinks(`<head>${tag}</head>`), [tag]);
    assert.equal(baselineHtml(`<head>${tag}</head>`), '<head></head>');
  }
  assert.equal(attr(variants[0], 'href'), '/assets/a.wasm?x=1&y=2');
  const bytes = Buffer.from('Raw Brotli bytes must not be served as decoded bytes with stale headers.');
  assert.deepEqual(decode(zlib.brotliCompressSync(bytes), 'br'), bytes);
  assert.deepEqual(decode(zlib.gzipSync(bytes), 'gzip'), bytes);
  assert.deepEqual(decode(zlib.deflateSync(bytes), 'deflate'), bytes);
  assert.throws(() => decode(bytes, 'unknown'));
  assert.equal(median([5, 1, 3, null]), 3);
  assert.equal(median([4, 2, 1, 3]), 2.5);
  assert.equal(median([]), null);
  assert.equal(mad([1, 2, 3, 4, 5]), 1);
  console.log('PASS: head-only preload removal, attribute variants, retained CSS/module/body, decoding, median/MAD. No network/browser/build run.');
}
if (process.argv.includes('--self-test')) selfTest();
else main().catch(error => {
  report.errors.push(error.stack || error.message); console.error(error); process.exitCode = 1;
}).finally(async () => {
  await browser?.close();
  for (const proxy of proxies) {
    const closed = new Promise(resolve => proxy.server.close(resolve));
    proxy.server.closeAllConnections(); await closed;
  }
  agent.destroy();
  report.finishedAt = new Date().toISOString();
  fs.mkdirSync(ARTIFACTS, { recursive: true });
  fs.writeFileSync(path.join(ARTIFACTS, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(`Artifacts: ${ARTIFACTS}`);
});
