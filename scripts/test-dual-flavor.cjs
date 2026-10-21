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
 *      ETag/304 往返。
 *   3. 浏览器：reader 页 hydration、SPA 导航、跨入口整页导航进入图鉴（full），
 *      同一文档不并发启动两个 wasm 应用（plan §6.6）。
 *
 * 用法：bun scripts/test-dual-flavor.cjs
 *   RELEASE_BUILD=<dir>  指定发布布局（默认 target/dx/yggdrasil/release/web）
 * 需要本机 docker 与 psql/pg_isready/createdb（自管一次性容器，结束后清理）。
 */
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const net = require('node:net');
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
/** 提取 HTML 里引用的客户端入口（assets/wasm 下的 src/href），归一化 /./ 前缀。 */
function bootstrapRefs(html) {
  const refs = new Set();
  const re = /(?:src|href)\s*=\s*["'](\/(?:\.\/)?(?:assets|wasm)\/[^"']+)["']/g;
  let m;
  while ((m = re.exec(html)) !== null) refs.add('/' + m[1].replace(/^\/+/, '').replace(/^\.\//, ''));
  return refs;
}

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
  server = Bun.spawn([path.join(BUILD, 'server')], { cwd: runtime, env,
    stdout: Bun.file(path.join(artifacts, 'server.log')), stderr: Bun.file(path.join(artifacts, 'server-error.log')) });
  await waitFor(async () => {
    if (server.exitCode !== null) {
      throw new Error(`服务端启动即退出（缺失产物会明确失败，见 ${artifacts}/server-error.log）`);
    }
    return fetch(`${base}/readyz`).then((r) => r.ok).catch(() => false);
  }, 'release site');
  console.log(`发布布局已启动：${base}（flavor 模板：${Object.entries(manifest.flavors).map(([f, i]) => `${f}=${i.template}`).join(', ')}）`);

  const seed = path.join(runtime, 'fixture.sql');
  fs.writeFileSync(seed, `INSERT INTO users(id,username,email,password_hash,role) VALUES(1,'regression','regression@test.invalid','unused','admin');
    INSERT INTO posts(id,author_id,title,slug,summary,content_md,content_html,status,published_at,word_count,reading_time)
      VALUES(1,1,'双产物验收文章','dual-flavor-post','摘要','<p>正文</p>','<p>正文</p>','published',NOW(),100,1);
    SELECT setval('posts_id_seq',1); SELECT setval('users_id_seq',1);`);
  await run(['psql', '-v', 'ON_ERROR_STOP=1', '-d', 'ygg_dual_flavor_test', '-f', seed], { env: pg });

  // ── 1. 按路径选 flavor（HTTP 层） ─────────────────────────────
  console.log('\n[1] 按路径固定选择 flavor');
  const readerPaths = ['/', '/archives', '/about', '/about/writing', '/friends', '/changelog',
    '/search', '/notes', '/post/dual-flavor-post', '/tags/x', '/page/2', '/no/such/path'];
  const fullPaths = ['/about/components', '/about/components/checkbox', '/login', '/register'];
  for (const p of readerPaths) {
    const res = await fetch(base + p);
    const refs = bootstrapRefs(await res.text());
    const wrong = [...refs].filter((r) => flavorRefs.full.has(r) && !flavorRefs.reader.has(r));
    const expect404 = p === '/no/such/path';
    check(`${p} → reader 模板（${expect404 ? '404' : '200'}）`, expect404 ? res.status === 404 : res.ok, `status=${res.status}`);
    check(`${p} 不含 full 专属入口`, wrong.length === 0, wrong.join(', '));
    if (!refs.has(entryOf('reader'))) check(`${p} 引用 reader 入口`, false, [...refs].join(', '));
  }
  for (const p of fullPaths) {
    const res = await fetch(base + p);
    const refs = bootstrapRefs(await res.text());
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
    check('缓存命中返回同一 flavor 模板', bootstrapRefs(body1).size > 0 && bootstrapRefs(body2).size > 0
      && [...bootstrapRefs(body1)].every((r) => bootstrapRefs(body2).has(r)));
    const notModified = await fetch(base + '/', { headers: { 'if-none-match': etag } });
    check('ETag/304 往返', notModified.status === 304, `status=${notModified.status}`);
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
  const readerWasmFetched = wasmRequests.slice();
  check('首页只拉取 reader wasm（绝不加载 full wasm）',
    readerWasmFetched.length >= 1
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
  check('跨入口重新拉取了 full wasm（整页导航释放 reader 运行时）', newWasm.length >= 1, newWasm.join(', '));
  const showcaseText = await page.textContent('body');
  check('图鉴页内容渲染', /组件图鉴|COMPONENT/i.test(showcaseText || ''));

  check('无页面 JS 错误', pageErrors.length === 0 && pageErrorsByLoad.every((l) => l.errors.length === 0),
    [...pageErrors, ...pageErrorsByLoad.flatMap((l) => l.errors.map((e) => `[${l.path}] ${e}`))].slice(0, 3).join(' | '));
  notes.push(`wasm 请求序列：\n    ${wasmRequests.join('\n    ')}`);
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
