#!/usr/bin/env bun
/**
 * P3 双产物发布布局验收（plan §6.2 门 4/门 5、§6.8）。
 *
 * 前置：完整 release 构建（make build），产物在
 *   target/dx/yggdrasil/release/web/{server,public}
 * 其中 public 含 full 的 index.html、flavors/reader/index.html 与
 * release-manifest.json（make build 最后一步会自动校验一次静态闭包；
 * 本脚本在其上验证运行时行为）。
 *
 * 覆盖：
 *   1. 按路径固定选 flavor（plan §6.5）：reader/full/404 各路径的 HTML 引用
 *      正确客户端闭包，绝不串味（匿名页面不加载 full wasm）。
 *   2. HTML 缓存命中不改变 flavor（plan §6.7）：二次 GET 命中 x-ssr-cache，
 *      ETag/304 往返；带 hash 资源 immutable、可变 URL（模板/清单）不受一年
 *      immutable 影响。
 *   3. 浏览器：reader 页 hydration、SPA 导航、跨入口整页导航进入图鉴（full），
 *      同一文档不并发启动两个 wasm 应用（plan §6.6）；输出「首次阅读 → 进入
 *      后台」累计下载量报告（plan §6.8）。
 *   4. 旧发布资源保留窗口（plan §6.7）：YGG_PREVIOUS_PUBLIC_DIR 指向上一发布
 *      目录时，当前发布缺失的带 hash 资源按 404 回退（.br 协商、immutable），
 *      不遮蔽当前发布真实缺失，也不遮蔽当前发布已有资源。
 *   5. 完整应用回退模式（plan §6.7）：移除 reader 产物后服务端整体回退 full
 *      模板，所有路径（含独立笔记入口）hydration 正常，后台守卫不变。
 *
 * 用法：bun scripts/test-dual-flavor.cjs
 *   RELEASE_BUILD=<dir>  指定发布布局（默认 target/dx/yggdrasil/release/web）
 *   WASM_PRELOAD_AB=1    在同一隔离服务上追加 P5 preload 冷/热缓存测量
 * 需要本机 docker 与 psql/pg_isready/createdb（自管一次性容器，结束后清理）。
 */
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const net = require('node:net');
const { bootstrapRefs, verifyWasmPreload } = require('./release-bootstrap.cjs');
const ROOT = path.resolve(__dirname, '..');
const BUILD = path.resolve(process.env.RELEASE_BUILD || path.join(ROOT, 'target/dx/yggdrasil/release/web'));
const artifacts = path.resolve(process.env.REGRESSION_ARTIFACT_DIR || path.join(ROOT, 'target/regressions', `${Date.now()}-dual-flavor`));
fs.mkdirSync(artifacts, { recursive: true });
const runtime = fs.mkdtempSync(path.join(os.tmpdir(), 'ygg-dual-flavor-'));
const container = `ygg-dual-flavor-${process.pid}-${Date.now()}`;
const PLAYWRIGHT = process.env.PLAYWRIGHT_MODULE || path.join(ROOT, 'libs/node_modules/playwright');
let server;
let proc;
let cleaned = false;
let failures = 0;
const notes = [];

function check(name, ok, detail = '') {
  if (ok) console.log(`  ✓ ${name}`);
  else { failures += 1; console.error(`  ✗ ${name}${detail ? ` —— ${detail}` : ''}`); }
}
async function run(args, options = {}) {
  proc = Bun.spawn(args, { cwd: ROOT, env: process.env, stdout: 'inherit', stderr: 'inherit', ...options });
  const code = await proc.exited;
  proc = null;
  if (code !== 0) throw new Error(`${args.join(' ')} exited ${code}`);
}
async function capture(args) {
  const p = Bun.spawn(args, { stdout: 'pipe', stderr: 'pipe' });
  const out = await new Response(p.stdout).text();
  if (await p.exited !== 0) throw new Error(await new Response(p.stderr).text());
  return out.trim();
}
async function cleanup() {
  if (cleaned) return;
  cleaned = true;
  if (proc) { proc.kill(); await proc.exited; }
  if (server) { server.kill(); await server.exited; }
  await capture(['docker', 'rm', '-f', container]).catch(() => {});
  fs.rmSync(runtime, { recursive: true, force: true });
}
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, async () => { await cleanup(); process.exit(1); });
async function freePort() {
  const listener = net.createServer();
  await new Promise((resolve) => listener.listen(0, '127.0.0.1', resolve));
  const port = listener.address().port;
  await new Promise((resolve) => listener.close(resolve));
  return port;
}
async function waitFor(check, description, attempts = 120) {
  for (let i = 0; i < attempts; i += 1) {
    if (await check()) return;
    await Bun.sleep(500);
  }
  throw new Error(`Timed out waiting for ${description}; see ${artifacts}`);
}
const bootstrapSet = (html) => new Set(bootstrapRefs(html));

async function main() {
  const publicDir = path.join(BUILD, 'public');
  const manifestPath = path.join(publicDir, 'release-manifest.json');
  if (!fs.existsSync(path.join(BUILD, 'server'))) throw new Error(`缺少服务端：${BUILD}/server（先 make build）`);
  if (!fs.existsSync(manifestPath)) throw new Error(`缺少发布清单：${manifestPath}（先 make build）`);
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  const flavorRefs = Object.fromEntries(Object.entries(manifest.flavors).map(([f, info]) => [f, new Set(info.bootstrap_assets)]));
  const wasmOf = (flavor) => manifest.flavors[flavor].assets.filter((a) => a.path.endsWith('.wasm')).map((a) => a.path);
  const readerWasmUrls = wasmOf('reader');
  const fullWasmUrls = wasmOf('full');
  const entryOf = (flavor) => (manifest.flavors[flavor].bootstrap_assets).find((r) => r.endsWith('.js'));

  await capture(['docker', 'run', '-d', '--name', container, '-e', 'POSTGRES_PASSWORD=regression',
    '-p', '127.0.0.1::5432', 'postgres:18-alpine']);
  const pgPort = (await capture(['docker', 'port', container, '5432/tcp'])).split(':').at(-1);
  const pg = { ...process.env, PGHOST: '127.0.0.1', PGPORT: pgPort, PGUSER: 'postgres', PGPASSWORD: 'regression' };
  await waitFor(async () => {
    const p = Bun.spawn(['pg_isready'], { env: pg, stdout: 'ignore', stderr: 'ignore' });
    return await p.exited === 0;
  }, 'PostgreSQL');
  await run(['createdb', 'ygg_dual_flavor_test'], { env: pg });

  const appPort = await freePort();
  const base = `http://127.0.0.1:${appPort}`;
  fs.mkdirSync(path.join(runtime, 'uploads'), { recursive: true });
  const env = { ...pg, DATABASE_URL: `postgresql://postgres:regression@127.0.0.1:${pgPort}/ygg_dual_flavor_test`,
    IP: '127.0.0.1', PORT: String(appPort), APP_BASE_URL: base, COOKIE_SECURE: 'false',
    SSR_CACHE_SECS: '60', DIOXUS_ASSET_DIR: publicDir, MCP_TOKEN_ENC_KEY: '42'.repeat(32),
    ADMIN_USERNAME: '', ADMIN_PASSWORD: '', ADMIN_EMAIL: '' };
  // 服务端（重）启动：kill 旧进程 → 以叠加 env 拉起 → readyz 就绪。
  const startServer = async (label, extraEnv = {}) => {
    if (server) { server.kill(); await server.exited; }
    server = Bun.spawn([path.join(BUILD, 'server')], { cwd: runtime,
      env: { ...env, ...extraEnv },
      stdout: Bun.file(path.join(artifacts, `server-${label}.log`)),
      stderr: Bun.file(path.join(artifacts, `server-${label}-error.log`)) });
    await waitFor(async () => {
      if (server.exitCode !== null) {
        throw new Error(`服务端启动即退出（${label}；缺失产物会明确失败，见 ${artifacts}/server-${label}-error.log）`);
      }
      return fetch(`${base}/readyz`).then((r) => r.ok).catch(() => false);
    }, `release site (${label})`);
  };
  const fetchBody = async (p, headers) => {
    const res = await fetch(base + p, { headers });
    return { res, body: await res.text() };
  };
  await startServer('boot');
  console.log(`发布布局已启动：${base}（flavor 模板：${Object.entries(manifest.flavors).map(([f, i]) => `${f}=${i.template}`).join(', ')}）`);

  const seed = path.join(runtime, 'fixture.sql');
  const fixtureHtml = process.env.WASM_PRELOAD_AB === '1'
    ? '<p>固定的公开长文章样例，用于比较相同内容下的首屏显示、资源请求与菜单交互。正文不含运行块或外部图片，不会发起执行、上传或鉴权请求。</p>'.repeat(200)
    : '<p>正文</p>';
  fs.writeFileSync(seed, `INSERT INTO users(id,username,email,password_hash,role) VALUES(1,'regression','regression@test.invalid','unused','admin');
    INSERT INTO posts(id,author_id,title,slug,summary,content_md,content_html,status,published_at,word_count,reading_time)
      VALUES(1,1,'双产物验收文章','dual-flavor-post','摘要','正文','${fixtureHtml}','published',NOW(),100,1);
    SELECT setval('posts_id_seq',1); SELECT setval('users_id_seq',1);`);
  await run(['psql', '-v', 'ON_ERROR_STOP=1', '-d', 'ygg_dual_flavor_test', '-f', seed], { env: pg });

  // ── 1. 按路径选 flavor（HTTP 层） ─────────────────────────────
  console.log('\n[1] 按路径固定选择 flavor');
  const readerPaths = ['/', '/archives', '/about', '/about/writing', '/friends', '/changelog',
    '/search', '/notes', '/post/dual-flavor-post', '/tags/x', '/page/2', '/no/such/path'];
  const fullPaths = ['/about/components', '/about/components/checkbox', '/login', '/register'];
  for (const p of readerPaths) {
    const res = await fetch(base + p);
    const body = await res.text();
    verifyWasmPreload(body, readerWasmUrls[0]);
    const refs = bootstrapSet(body);
    const wrong = [...refs].filter((r) => flavorRefs.full.has(r) && !flavorRefs.reader.has(r));
    const expect404 = p === '/no/such/path';
    check(`${p} → reader 模板（${expect404 ? '404' : '200'}）`, expect404 ? res.status === 404 : res.ok, `status=${res.status}`);
    check(`${p} 不含 full 专属入口`, wrong.length === 0, wrong.join(', '));
    if (!refs.has(entryOf('reader'))) check(`${p} 引用 reader 入口`, false, [...refs].join(', '));
  }
  for (const p of fullPaths) {
    const res = await fetch(base + p);
    const body = await res.text();
    verifyWasmPreload(body, fullWasmUrls[0]);
    const refs = bootstrapSet(body);
    const wrong = [...refs].filter((r) => flavorRefs.reader.has(r) && !flavorRefs.full.has(r));
    check(`${p} → full 模板（${res.status}）`, res.ok, `status=${res.status}`);
    check(`${p} 不含 reader 专属入口`, wrong.length === 0, wrong.join(', '));
  }
  {
    const res = await fetch(base + '/admin', { redirect: 'manual' });
    check('/admin 匿名 302 → /login（服务端守卫，跨入口整页）', res.status === 302 && new URL(res.headers.get('location'), base).pathname === '/login',
      `status=${res.status} location=${res.headers.get('location')}`);
  }

  // ── 2. 缓存命中与 flavor 一致性 ──────────────────────────────
  console.log('\n[2] HTML 缓存与 ETag（同一缓存实例不混 flavor）');
  {
    const first = await fetch(base + '/');
    const body1 = await first.text();
    const etag = first.headers.get('etag');
    check('首次请求可缓存（x-ssr-cache: MISS/BYPASS）', ['MISS', 'HIT'].includes(first.headers.get('x-ssr-cache') || ''), String(first.headers.get('x-ssr-cache')));
    const second = await fetch(base + '/');
    const body2 = await second.text();
    check('二次请求命中缓存（x-ssr-cache: HIT）', second.headers.get('x-ssr-cache') === 'HIT', String(second.headers.get('x-ssr-cache')));
    check('缓存命中返回同一 flavor 模板', bootstrapSet(body1).size > 0 && bootstrapSet(body2).size > 0
      && [...bootstrapSet(body1)].every((r) => bootstrapSet(body2).has(r)));
    const notModified = await fetch(base + '/', { headers: { 'if-none-match': etag } });
    check('ETag/304 往返', notModified.status === 304, `status=${notModified.status}`);

    // 缓存头边界（plan §6.7）：带 hash 的 JS/WASM immutable；可变 URL（模板、
    // 发布清单）不得使用一年 immutable。
    const assetCacheControl = (await fetch(base + readerWasmUrls[0])).headers.get('cache-control') || '';
    check('带 hash 资源 immutable 或长 max-age',
      /immutable/i.test(assetCacheControl) || /max-age=31536000/.test(assetCacheControl),
      assetCacheControl);
    for (const mutable of ['/', '/release-manifest.json']) {
      const cc = (await fetch(base + mutable)).headers.get('cache-control') || '';
      check(`可变 URL 不受一年 immutable 影响：${mutable}`, !/immutable/i.test(cc), cc);
    }
  }

  // ── 3. 浏览器：hydration / SPA / 跨入口文档导航 ───────────────
  console.log('\n[3] 浏览器 hydration 与跨产物导航');
  const { chromium } = require(PLAYWRIGHT);
  const browser = await chromium.launch();
  const page = await browser.newPage();
  const wasmRequests = [];
  page.on('request', (req) => { if (req.url().endsWith('.wasm')) wasmRequests.push(req.url()); });
  const pageErrors = [];
  page.on('pageerror', (err) => pageErrors.push(err.stack || String(err)));
  const pageErrorsByLoad = [];

  await page.goto(base + '/', { waitUntil: 'domcontentloaded' });
  pageErrorsByLoad.push({ path: '/', errors: pageErrors.splice(0) });
  await page.waitForFunction(() => Boolean(window.__dx_mainWasm), {}, { timeout: 60000 });
  check('首页 hydration 完成（reader wasm 初始化）', true);
  // 「首次阅读 → 进入后台」累计下载（plan §6.8）：必须在首次冷加载的首页文档上
  // 采样——后续 goto 会换成新文档且 wasm 命中 HTTP 缓存（transferSize=0），
  // 就不再是首次阅读的真实传输量。preload 与实例请求由浏览器去重计一次。
  const downloadOf = () => page.evaluate(() => {
    const rows = [...performance.getEntriesByType('navigation'), ...performance.getEntriesByType('resource')];
    return {
      bytes: rows.reduce((n, e) => n + (e.transferSize || 0), 0),
      entries: rows.map((e) => ({ name: e.name, type: e.entryType, transferSize: e.transferSize, encodedBodySize: e.encodedBodySize })),
    };
  });
  const readerDownload = await downloadOf();
  const readerWasmFetched = wasmRequests.slice();
  check('首页仅请求一次 reader wasm（preload 复用，绝不加载 full wasm）',
    readerWasmFetched.length === 1
    && readerWasmFetched.every((u) => {
      const p = new URL(u).pathname;
      return readerWasmUrls.includes(p) && !fullWasmUrls.includes(p);
    }),
    readerWasmFetched.join(', '));

  // reader 内 SPA 导航：点击「归档」。
  await Promise.all([page.waitForURL('**/archives'), page.click('a[href="/archives"]')]);
  await page.waitForFunction(() => Boolean(window.__dx_mainWasm), {}, { timeout: 60000 });
  check('reader 内 SPA 导航到 /archives（无整页重载）', page.url().endsWith('/archives'));
  check('SPA 导航未重新下载 wasm（单页会话内复用运行时）', wasmRequests.length === readerWasmFetched.length,
    `before=${readerWasmFetched.length} after=${wasmRequests.length}`);

  // 跨入口：关于页 → 组件图鉴（reader 里是普通 <a>，整页文档导航进入 full）。
  await page.goto(base + '/about', { waitUntil: 'domcontentloaded' });
  pageErrorsByLoad.push({ path: '/about', errors: pageErrors.splice(0) });
  const showcaseHref = await page.getAttribute('a.showcase-entry', 'href');
  check('reader 的图鉴入口是普通 <a href>（文档导航）', showcaseHref === '/about/components', String(showcaseHref));
  const beforeNav = wasmRequests.length;
  await Promise.all([page.waitForURL('**/about/components'), page.click('a.showcase-entry')]);
  await page.waitForLoadState('domcontentloaded');
  pageErrorsByLoad.push({ path: '/about→/about/components', errors: pageErrors.splice(0) });
  await page.waitForFunction(() => Boolean(window.__dx_mainWasm), {}, { timeout: 60000 });
  check('跨入口进入图鉴（full 模板）', page.url().endsWith('/about/components'));
  const newWasm = wasmRequests.slice(beforeNav);
  check('跨入口仅请求一次 full wasm（preload 复用，整页释放 reader 运行时）',
    newWasm.length === 1 && fullWasmUrls.includes(new URL(newWasm[0]).pathname), newWasm.join(', '));
  const showcaseText = await page.textContent('body');
  check('图鉴页内容渲染', /组件图鉴|COMPONENT/i.test(showcaseText || ''));

  const fullDownload = await downloadOf();
  const kib = (n) => `${(n / 1024).toFixed(1)} KiB`;
  notes.push(`首次阅读（reader 文档）传输 ${kib(readerDownload.bytes)}；图鉴（full 文档）传输 ${kib(fullDownload.bytes)}；首次阅读→进入后台累计 ${kib(readerDownload.bytes + fullDownload.bytes)}`);
  fs.writeFileSync(path.join(artifacts, 'first-read-then-admin-download.json'), JSON.stringify({
    collected_at: new Date().toISOString(),
    base,
    reader_bytes: readerDownload.bytes,
    showcase_full_bytes: fullDownload.bytes,
    cumulative_bytes: readerDownload.bytes + fullDownload.bytes,
    reader_entries: readerDownload.entries,
    showcase_full_entries: fullDownload.entries,
  }, null, 2) + '\n');

  check('无页面 JS 错误', pageErrors.length === 0 && pageErrorsByLoad.every((l) => l.errors.length === 0),
    [...pageErrors, ...pageErrorsByLoad.flatMap((l) => l.errors.map((e) => `[${l.path}] ${e}`))].slice(0, 3).join(' | '));
  notes.push(`wasm 请求序列：\n    ${wasmRequests.join('\n    ')}`);

  // P5 测量复用本次隔离数据库与真实 release 服务，不另起开发构建。
  if (process.env.WASM_PRELOAD_AB === '1') {
    await run([process.execPath, path.join(ROOT, 'scripts/test-wasm-preload.cjs')], {
      env: { ...process.env, BASE_URL: base, RELEASE_BUILD: BUILD, SSR_CACHE_SECS: env.SSR_CACHE_SECS,
        REGRESSION_ARTIFACT_DIR: path.join(artifacts, 'wasm-preload') },
    });
  }

  // ── 4. 旧发布资源保留窗口（plan §6.7 滚动发布） ───────────────
  console.log('\n[4] 旧发布资源保留窗口（YGG_PREVIOUS_PUBLIC_DIR）');
  {
    const previousPublic = path.join(runtime, 'previous-public');
    fs.rmSync(previousPublic, { recursive: true, force: true });
    fs.cpSync(publicDir, previousPublic, { recursive: true });
    // 先以窗口模式启动（模板闭包校验在启动期完成），再模拟「新发布不再收录
    // 旧 reader wasm」——对应已打开标签页仍持有旧 HTML 引用的场景。
    await startServer('prev-window', { YGG_PREVIOUS_PUBLIC_DIR: previousPublic });
    const oldWasm = readerWasmUrls[0];
    const oldRel = oldWasm.replace(/^\//, '');
    fs.rmSync(path.join(publicDir, oldRel));
    fs.rmSync(path.join(publicDir, oldRel + '.br'), { force: true });

    const res = await fetch(base + oldWasm);
    const bytes = new Uint8Array(await res.arrayBuffer());
    const original = new Uint8Array(fs.readFileSync(path.join(previousPublic, oldRel)));
    check('新发布缺失的旧 hash 资源 → 窗口回退 200（字节与上一发布一致）',
      res.status === 200 && bytes.length === original.length && bytes.every((b, i) => b === original[i]),
      `status=${res.status} bytes=${bytes.length}/${original.length}`);
    check('回退响应带 x-previous-release 标记与 immutable',
      res.headers.get('x-previous-release') === '1' && /immutable/i.test(res.headers.get('cache-control') || ''),
      `x-previous-release=${res.headers.get('x-previous-release')} cc=${res.headers.get('cache-control')}`);
    check('回退响应 MIME 正确（application/wasm）',
      (res.headers.get('content-type') || '').includes('application/wasm'), String(res.headers.get('content-type')));

    // .br 协商：客户端解压后与原文件一致，或显式回传 br 编码字节（MIME 不变）。
    const brRes = await fetch(base + oldWasm, { headers: { 'accept-encoding': 'br' } });
    const brBytes = new Uint8Array(await brRes.arrayBuffer());
    const brRaw = new Uint8Array(fs.readFileSync(path.join(previousPublic, oldRel + '.br')));
    const identity = brBytes.length === original.length && brBytes.every((b, i) => b === original[i]);
    const encoded = brRes.headers.get('content-encoding') === 'br'
      && brBytes.length === brRaw.length && brBytes.every((b, i) => b === brRaw[i]);
    check('回退 .br 协商（解压后字节一致或显式 br 编码）', brRes.status === 200 && (identity || encoded),
      `status=${brRes.status} ce=${brRes.headers.get('content-encoding')} bytes=${brBytes.length}`);

    const fullRes = await fetch(base + fullWasmUrls[0]);
    check('当前发布已有资源不经过窗口（无 x-previous-release 标记）',
      fullRes.status === 200 && !fullRes.headers.get('x-previous-release'), `status=${fullRes.status}`);
    const missing = await fetch(base + `/assets/truly-missing-${Date.now()}.js`);
    check('两代发布都缺失时保留 404', missing.status === 404, `status=${missing.status}`);

    // 还原 reader wasm（回退模式阶段仍需双产物布局）。
    fs.copyFileSync(path.join(previousPublic, oldRel), path.join(publicDir, oldRel));
    fs.copyFileSync(path.join(previousPublic, oldRel + '.br'), path.join(publicDir, oldRel + '.br'));
  }

  // ── 5. 完整应用回退模式（plan §6.7） ─────────────────────────
  console.log('\n[5] 完整应用回退模式');
  {
    const flavorsDir = path.join(publicDir, 'flavors');
    const hidden = path.join(runtime, 'flavors.hidden');
    fs.renameSync(flavorsDir, hidden);
    await startServer('full-fallback');
    const log = fs.readFileSync(path.join(artifacts, 'server-full-fallback.log'), 'utf8');
    check('启动日志声明完整应用模式', log.includes('完整应用模式'));
    for (const p of ['/', '/post/dual-flavor-post', '/about/components', '/notes', '/search']) {
      const { res, body } = await fetchBody(p);
      const refs = bootstrapSet(body);
      const readerOnly = [...refs].filter((r) => flavorRefs.reader.has(r) && !flavorRefs.full.has(r));
      let preloadOk = true;
      let preloadDetail = '';
      try { verifyWasmPreload(body, fullWasmUrls[0]); } catch (error) { preloadOk = false; preloadDetail = error.message; }
      check(`${p} → full 模板且 preload full wasm（${res.status}）`,
        res.ok && refs.has(entryOf('full')) && readerOnly.length === 0 && preloadOk,
        `status=${res.status} readerOnly=${readerOnly.join(', ')} preload=${preloadDetail}`);
    }
    {
      const res = await fetch(base + '/admin', { redirect: 'manual' });
      check('/admin 匿名 302 → /login（回退模式守卫不变）',
        res.status === 302 && new URL(res.headers.get('location'), base).pathname === '/login',
        `status=${res.status}`);
    }
    const fallbackPage = await browser.newPage();
    const fallbackWasm = [];
    fallbackPage.on('request', (req) => { if (req.url().endsWith('.wasm')) fallbackWasm.push(req.url()); });
    const fallbackErrors = [];
    fallbackPage.on('pageerror', (err) => fallbackErrors.push(err.stack || String(err)));
    await fallbackPage.goto(base + '/', { waitUntil: 'domcontentloaded' });
    await fallbackPage.waitForFunction(() => Boolean(window.__dx_mainWasm), {}, { timeout: 60000 });
    check('回退模式首页 hydration（加载 full wasm）',
      fallbackWasm.length >= 1 && fallbackWasm.every((u) => fullWasmUrls.includes(new URL(u).pathname)),
      fallbackWasm.join(', '));
    check('回退模式无页面 JS 错误', fallbackErrors.length === 0, fallbackErrors.slice(0, 3).join(' | '));
    await fallbackPage.close();
    fs.renameSync(hidden, flavorsDir);
  }
  await browser.close();

  console.log(`\n${notes.join('\n')}`);
  if (failures) {
    console.error(`\n双产物验收失败：${failures} 项（详见上方 ✗）；artifacts: ${artifacts}`);
    process.exitCode = 1;
  } else {
    console.log(`\n双产物验收全部通过；artifacts: ${artifacts}`);
  }
}

main().catch((error) => { console.error(error); process.exitCode = 1; }).finally(cleanup);
