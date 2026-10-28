#!/usr/bin/env bun
import { createHash } from 'node:crypto';
// 真实 release 产物验收（先 make build）：分包按需加载、SPA 导航不重载主包、
// SSR 头部/状态码/缓存与水合、后台鉴权、滚动发布旧分包兜底，以及各公开路由的
// 首屏 WASM Brotli 体积门槛。基础设施同 regression.ts：临时 postgres 容器、
// 空闲端口、/readyz 等待、退出清理。
// 用法：bun scripts/test-release-split.ts（RELEASE_OUT 默认 target/release-dist）
import * as fs from 'node:fs';
import * as net from 'node:net';
import * as os from 'node:os';
import * as path from 'node:path';
import type * as Playwright from 'playwright';

const ROOT = path.resolve(__dirname, '..');
const RELEASE_OUT = path.resolve(ROOT, process.env.RELEASE_OUT || 'target/release-dist');
const SITE = path.join(RELEASE_OUT, 'public');
const SERVER_BIN = path.join(RELEASE_OUT, 'server');
const SIZE_LIMIT = 594603;
// 合并补丁版 wasm-split 下公开路由直开约 20–70 个 .wasm，原版 120–380 个；超出说明构建
// 用了未打补丁的 cargo-leptos（见 Makefile 的 CARGO_LEPTOS_INSTALL）。
const WASM_FILE_LIMIT = 150;
const SIZE_ROUTES = [
  '/',
  '/post/split-a',
  '/archives',
  '/tags/split',
  '/notes',
  '/about',
  '/friends',
  '/changelog',
  '/search',
];
const { chromium } = require(
  process.env.PLAYWRIGHT_MODULE || path.join(ROOT, 'libs/node_modules/playwright'),
) as typeof Playwright;

const artifacts = path.resolve(
  process.env.REGRESSION_ARTIFACT_DIR ||
    path.join(ROOT, 'target/regressions', `${Date.now()}-release-split`),
);
fs.mkdirSync(artifacts, { recursive: true });
const runtime = fs.mkdtempSync(path.join(os.tmpdir(), 'ygg-release-split-'));
const container = `ygg-release-split-${process.pid}-${Date.now()}`;
let server: Bun.Subprocess | undefined;
let browser: Playwright.Browser | undefined;
let child: Bun.Subprocess | null | undefined;
let cleaned = false;

const results: { name: string; ok: boolean; detail: string }[] = [];
const check = (name: string, ok: boolean, detail = '') => {
  results.push({ name, ok: !!ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`);
};

async function run(args: string[], options: { env?: NodeJS.ProcessEnv } = {}) {
  child = Bun.spawn(args, {
    cwd: ROOT,
    env: process.env,
    stdout: 'inherit',
    stderr: 'inherit',
    ...options,
  });
  const code = await child.exited;
  child = null;
  if (code !== 0) throw new Error(`${args[0]} exited ${code}`);
}
async function capture(args: string[]) {
  const proc = Bun.spawn(args, { stdout: 'pipe', stderr: 'pipe' });
  const output = await new Response(proc.stdout).text();
  if ((await proc.exited) !== 0) throw new Error(await new Response(proc.stderr).text());
  return output.trim();
}
async function cleanup() {
  if (cleaned) return;
  cleaned = true;
  if (child) {
    child.kill();
    await child.exited;
  }
  if (browser) await browser.close().catch(() => {});
  if (server) {
    server.kill();
    await server.exited;
  }
  await capture(['docker', 'rm', '-f', container]).catch(() => {});
  fs.rmSync(runtime, { recursive: true, force: true });
}
for (const signal of ['SIGINT', 'SIGTERM'])
  process.on(signal, async () => {
    await cleanup();
    process.exit(1);
  });
async function freePort() {
  const listener = net.createServer();
  const listening = Promise.withResolvers<void>();
  listener.listen(0, '127.0.0.1', listening.resolve);
  await listening.promise;
  const port = (listener.address() as net.AddressInfo).port;
  const closed = Promise.withResolvers<void>();
  listener.close(() => closed.resolve());
  await closed.promise;
  return port;
}
async function waitFor(check: () => Promise<boolean>, description: string, attempts = 120) {
  for (let i = 0; i < attempts; i++) {
    if (await check()) return;
    await Bun.sleep(500);
  }
  throw new Error(`Timed out waiting for ${description}; see ${artifacts}`);
}

// 每个页面收集脚本错误、失败响应与 .wasm 请求（响应路径，按请求顺序）。
interface PageLog {
  errors: string[];
  failed: string[];
  wasm: string[];
}
function instrument(page: Playwright.Page) {
  const log: PageLog = { errors: [], failed: [], wasm: [] };
  page.on('pageerror', (e) => log.errors.push(`pageerror: ${e.message}`));
  page.on('console', (m) => {
    if (m.type() === 'error') log.errors.push(`console: ${m.text()}`);
  });
  page.on('request', (r) => {
    const p = new URL(r.url()).pathname;
    if (p.endsWith('.wasm')) log.wasm.push(p);
  });
  page.on('response', (r) => {
    if (r.status() >= 400) log.failed.push(`${r.status()} ${new URL(r.url()).pathname}`);
  });
  return log;
}
// 资源 404 会同时产生一条 "Failed to load resource" 控制台错误；单独由 failed 列表判定。
const scriptErrors = (log: PageLog) =>
  log.errors.filter((e) => !e.includes('Failed to load resource'));
const hydrated = async (page: Playwright.Page) => {
  await page.waitForSelector('html[data-hydrated="true"]', { state: 'attached', timeout: 30000 });
  await page.waitForLoadState('networkidle');
};
const names = (paths: string[]) => paths.map((p) => p.replace('/pkg/', '')).join(', ');

async function main() {
  for (const required of [SERVER_BIN, SITE, path.join(RELEASE_OUT, 'hash.txt')]) {
    if (!fs.existsSync(required)) throw new Error(`缺少 ${required}，先运行 make build`);
  }
  const hashes = Object.fromEntries(
    fs
      .readFileSync(path.join(RELEASE_OUT, 'hash.txt'), 'utf8')
      .split('\n')
      .filter((l) => l.includes(':'))
      .map((l) => {
        const i = l.indexOf(':');
        return [l.slice(0, i).trim(), l.slice(i + 1).trim()];
      }),
  );
  const mainWasm = `/pkg/yggdrasil.${hashes.wasm}.wasm`;
  if (!fs.existsSync(path.join(SITE, mainWasm))) throw new Error(`主包缺失：${mainWasm}`);

  await capture([
    'docker',
    'run',
    '-d',
    '--name',
    container,
    '-e',
    'POSTGRES_PASSWORD=regression',
    '-p',
    '127.0.0.1::5432',
    'postgres:18-alpine',
  ]);
  const port = (await capture(['docker', 'port', container, '5432/tcp'])).split(':').at(-1);
  const pg = {
    ...process.env,
    PGHOST: '127.0.0.1',
    PGPORT: port,
    PGUSER: 'postgres',
    PGPASSWORD: 'regression',
  };
  await waitFor(async () => {
    const p = Bun.spawn(['pg_isready'], { env: pg, stdout: 'ignore', stderr: 'ignore' });
    return (await p.exited) === 0;
  }, 'PostgreSQL');
  await run(['createdb', 'ygg_release_split'], { env: pg });

  const appPort = await freePort();
  const base = `http://127.0.0.1:${appPort}`;
  const previous = path.join(runtime, 'previous-public');
  fs.mkdirSync(path.join(previous, 'pkg'), { recursive: true });
  const legacy = 'legacy_chunk.AbCdEfGhIjKlMnOpQrStUv.wasm';
  fs.writeFileSync(path.join(previous, 'pkg', legacy), Buffer.alloc(16, 7));
  fs.mkdirSync(path.join(runtime, 'uploads'));

  const env = {
    ...pg,
    DATABASE_URL: `postgresql://postgres:regression@127.0.0.1:${port}/ygg_release_split`,
    IP: '127.0.0.1',
    PORT: String(appPort),
    APP_BASE_URL: base,
    COOKIE_SECURE: 'false',
    SSR_CACHE_SECS: '60',
    LEPTOS_SITE_ROOT: SITE,
    MCP_TOKEN_ENC_KEY: '42'.repeat(32),
    ADMIN_USERNAME: '',
    ADMIN_PASSWORD: '',
    ADMIN_EMAIL: '',
    YGG_PREVIOUS_PUBLIC_DIR: previous,
  };
  const appServer = Bun.spawn([SERVER_BIN], {
    cwd: runtime,
    env,
    stdout: Bun.file(path.join(artifacts, 'server.log')),
    stderr: Bun.file(path.join(artifacts, 'server-error.log')),
  });
  server = appServer;
  await waitFor(async () => {
    if (appServer.exitCode !== null) throw new Error(`Server exited; see ${artifacts}`);
    return fetch(`${base}/readyz`)
      .then((r) => r.ok)
      .catch(() => false);
  }, 'release server');

  // 种子数据：管理员 + 会话，三篇已发布文章（标签 split，含可运行 python 代码块）。
  const sessionToken = 'release-split-admin';
  const tokenHash = createHash('sha256').update(sessionToken).digest('hex');
  const code = '```python\nprint(1)\n```';
  const html = '<p>拆分验收正文</p><pre><code class="language-python">print(1)\n</code></pre>';
  const posts = [
    ['split-a', '拆分验收 A', 3],
    ['split-b', '拆分验收 B', 2],
    ['split-c', '拆分验收 C', 1],
  ]
    .map(
      ([
        slug,
        title,
        id,
      ]) => `INSERT INTO posts(id,author_id,title,slug,summary,content_md,content_html,status,published_at)
      VALUES(${id},1,'${title}','${slug}','拆分验收摘要','拆分验收正文\n\n${code}','${html}','published',NOW() - INTERVAL '${id} hour');
      INSERT INTO post_tags(post_id,tag_id) VALUES(${id},1);`,
    )
    .join('\n');
  const sql = `INSERT INTO users(id,username,email,password_hash,role) VALUES(1,'splitadmin','split@test.invalid','unused','admin');
    INSERT INTO sessions(user_id,token_hash,expires_at) VALUES(1,'${tokenHash}',NOW()+INTERVAL '1 hour');
    INSERT INTO tags(id,name) VALUES(1,'split');
    ${posts}
    SELECT setval('users_id_seq',1); SELECT setval('posts_id_seq',3); SELECT setval('tags_id_seq',1);`;
  const fixture = path.join(runtime, 'fixture.sql');
  fs.writeFileSync(fixture, sql);
  await run(['psql', '-v', 'ON_ERROR_STOP=1', '-d', 'ygg_release_split', '-f', fixture], {
    env: pg,
  });

  browser = await chromium.launch();

  // 1. 直开 /：主包 + 首页分包，不含文章/后台/登录/图鉴分包。
  {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    const log = instrument(page);
    await page.goto(`${base}/`);
    await hydrated(page);
    check(
      '1 首页直开：加载主包与首页分包',
      log.wasm.includes(mainWasm) && log.wasm.some((p) => p.includes('home_route')),
      names(log.wasm),
    );
    check(
      '1 首页不加载文章/后台/登录/图鉴分包',
      !log.wasm.some((p) =>
        ['post_route', 'admin', 'login', 'showcase'].some((s) => p.includes(s)),
      ),
      names(log.wasm),
    );
    check('1 首页无脚本错误', scriptErrors(log).length === 0, scriptErrors(log).join(' | '));
    check('1 首页无 ≥400 响应', log.failed.length === 0, log.failed.join(', '));

    // 2. SPA 导航：页面不重载、主包不再请求，只增量拉文章分包。
    await page.evaluate(() => {
      (window as Window & { __marker?: number }).__marker = 1;
    });
    log.wasm.length = 0;
    await page.locator('a[href="/post/split-a"]').first().click();
    await page.waitForURL(`${base}/post/split-a`);
    await page.waitForSelector('.post-content', { timeout: 15000 });
    await page.waitForLoadState('networkidle');
    check(
      '2 SPA 导航保留页面状态',
      (await page.evaluate(() => (window as Window & { __marker?: number }).__marker)) === 1,
    );
    check(
      '2 导航按需加载文章分包且不再请求主包',
      log.wasm.some((p) => p.includes('post_route')) && !log.wasm.includes(mainWasm),
      names(log.wasm),
    );
    check('2 SPA 导航无脚本错误', scriptErrors(log).length === 0, scriptErrors(log).join(' | '));
    await ctx.close();
  }

  // 3. SSR 头部、404、缓存命中后的水合。
  {
    const first = await fetch(`${base}/post/split-a`);
    const body = await first.text();
    const head = body.slice(0, body.indexOf('</head>'));
    check(
      '3 文章 SSR：200 且 <head> 含文章标题',
      first.status === 200 && /<title>[^<]*拆分验收 A/.test(head),
    );
    // 分包 preload 会和 CSS 抢带宽（docs/首屏性能实测-2026-10-28.md）：只保留主包本身的
    // preload 与加载器的 modulepreload。首页 SSR 期间调用了懒加载取数函数，以前会展开数百条。
    const homeBody = await (await fetch(`${base}/`)).text();
    for (const [route, routeHead] of [
      ['/', homeBody.slice(0, homeBody.indexOf('</head>'))],
      ['/post/split-a', head],
    ]) {
      const wasmPreloads = routeHead.match(/<link[^>]*type="application\/wasm"[^>]*>/g) ?? [];
      check(
        `3 ${route} 头部只 preload 主包 wasm，不展开分包`,
        wasmPreloads.length === 1 && wasmPreloads[0].includes(mainWasm),
        `${wasmPreloads.length} 条`,
      );
      check(
        `3 ${route} 头部 modulepreload 分包加载器`,
        routeHead.includes(`<link rel="modulepreload" href="/pkg/__wasm_split.${hashes.split}.js"`),
      );
    }
    for (const entry of [mainWasm, `/pkg/yggdrasil.${hashes.js}.js`]) {
      const r = await fetch(`${base}${entry}`);
      await r.arrayBuffer();
      check(
        `3 内容寻址入口 ${entry} 为 immutable`,
        r.status === 200 && (r.headers.get('cache-control') || '').includes('immutable'),
        `status=${r.status} cache-control=${r.headers.get('cache-control')}`,
      );
    }
    const missing = await fetch(`${base}/post/no-such`);
    await missing.text();
    check('3 未知文章返回 404', missing.status === 404, `status=${missing.status}`);
    const badPage = await fetch(`${base}/page/abc`);
    const badPageBody = await badPage.text();
    check(
      '3 非法页码返回 404 且不带文章数据',
      badPage.status === 404 && !badPageBody.includes('\\"posts\\":['),
      `status=${badPage.status}`,
    );
    const badPageAgain = await fetch(`${base}/page/abc`);
    await badPageAgain.text();
    check(
      '3 非法页码不进 SSR 缓存',
      badPageAgain.status === 404 && badPageAgain.headers.get('x-ssr-cache') !== 'HIT',
      `status=${badPageAgain.status} x-ssr-cache=${badPageAgain.headers.get('x-ssr-cache')}`,
    );
    const second = await fetch(`${base}/post/split-a`);
    await second.text();
    check(
      '3 第二次请求命中 SSR 缓存',
      second.headers.get('x-ssr-cache') === 'HIT',
      `x-ssr-cache=${second.headers.get('x-ssr-cache')}`,
    );
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    const log = instrument(page);
    const resp = await page.goto(`${base}/post/split-a`);
    await hydrated(page);
    check(
      '3 缓存命中页正常水合',
      (await resp!.allHeaders())['x-ssr-cache'] === 'HIT' && scriptErrors(log).length === 0,
      scriptErrors(log).join(' | '),
    );
    await ctx.close();
  }

  // 4. 后台：登录态加载后台分包，未登录 302 到 /login。
  {
    const ctx = await browser.newContext();
    await ctx.addCookies([
      { name: 'session', value: sessionToken, url: base, httpOnly: true, sameSite: 'Lax' },
    ]);
    const page = await ctx.newPage();
    const log = instrument(page);
    await page.goto(`${base}/admin/posts`);
    await hydrated(page);
    check(
      '4 后台直开加载后台分包',
      log.wasm.some((p) => p.includes('admin')),
      names(log.wasm),
    );
    check('4 后台无脚本错误', scriptErrors(log).length === 0, scriptErrors(log).join(' | '));
    await ctx.close();
    const anonymous = await fetch(`${base}/admin`, { redirect: 'manual' });
    check(
      '4 未登录 GET /admin → 302 /login',
      anonymous.status === 302 && anonymous.headers.get('location') === '/login',
      `status=${anonymous.status} location=${anonymous.headers.get('location')}`,
    );
  }

  // 5. 滚动发布：已删除的旧分包从上一发布目录兜底。
  {
    const r = await fetch(`${base}/pkg/${legacy}`);
    await r.arrayBuffer();
    check(
      '5 旧分包由 previous-public 兜底',
      r.status === 200 &&
        r.headers.get('x-previous-release') === '1' &&
        (r.headers.get('cache-control') || '').includes('immutable'),
      `status=${r.status} x-previous-release=${r.headers.get('x-previous-release')} cache-control=${r.headers.get('cache-control')}`,
    );
  }

  // 6. 体积门槛：每条路由直开所需 .wasm 的 Brotli 字节数之和。
  {
    const sizes: Record<string, { files: string[]; brotli_bytes: number }> = {};
    for (const route of SIZE_ROUTES) {
      const ctx = await browser.newContext();
      const page = await ctx.newPage();
      const log = instrument(page);
      await page.goto(`${base}${route}`);
      await hydrated(page);
      const files = [...new Set(log.wasm)].sort();
      let brotli = 0;
      for (const file of files) {
        const br = path.join(SITE, `${file}.br`);
        if (!fs.existsSync(br)) throw new Error(`缺 .br 旁车：${file}`);
        brotli += fs.statSync(br).size;
      }
      sizes[route] = { files: files.map((f) => f.replace('/pkg/', '')), brotli_bytes: brotli };
      await ctx.close();
    }
    fs.writeFileSync(path.join(artifacts, 'sizes.json'), JSON.stringify(sizes, null, 2) + '\n');
    for (const [route, info] of Object.entries(sizes)) {
      check(
        `6 体积 ${route} ≤ ${SIZE_LIMIT}`,
        info.brotli_bytes <= SIZE_LIMIT,
        `${info.brotli_bytes} B；${info.files.join(', ')}`,
      );
      check(
        `6 分包数 ${route} ≤ ${WASM_FILE_LIMIT}`,
        info.files.length <= WASM_FILE_LIMIT,
        `${info.files.length} 个 .wasm`,
      );
    }
  }
}

main()
  .catch((error) => {
    console.error(error);
    results.push({ name: 'runtime', ok: false, detail: String(error) });
  })
  .finally(async () => {
    await cleanup();
    const failed = results.filter((r) => !r.ok);
    console.log(
      `\n${results.length - failed.length}/${results.length} 通过；产物目录 ${artifacts}`,
    );
    if (failed.length) {
      console.log('失败项：');
      for (const f of failed) console.log(`  - ${f.name}${f.detail ? `：${f.detail}` : ''}`);
      process.exitCode = 1;
    }
  });
