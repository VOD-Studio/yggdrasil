#!/usr/bin/env bun
// Owns a disposable database and two local application processes. Never uses a production URL.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const net = require('node:net');
const { chromium } = require('../libs/node_modules/playwright');
const root = path.resolve(__dirname, '..');
const build = path.resolve(process.env.PUBLIC_TEST_BUILD || path.join(root, 'target/dx/yggdrasil/debug/web'));
const artifacts = path.join(root, 'target/regressions', `${Date.now()}-public-performance`);
const runtime = fs.mkdtempSync(path.join(os.tmpdir(), 'ygg-public-perf-'));
const container = `ygg-public-perf-${process.pid}-${Date.now()}`;
const servers = [];
let browser;
let pg;
fs.mkdirSync(artifacts, { recursive: true });
async function command(args, options = {}) {
  const child = Bun.spawn(args, { cwd: root, env: process.env, stdout: 'pipe', stderr: 'pipe', ...options });
  const [out, err] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text()]);
  if (await child.exited !== 0) throw new Error(`${args[0]} failed: ${err}`);
  return out.trim();
}
async function sql(text) {
  const child = Bun.spawn(['psql', '-X', '-v', 'ON_ERROR_STOP=1', '-A', '-t', '-d', 'ygg_mcp_test'],
    { env: pg, stdin: 'pipe', stdout: 'pipe', stderr: 'pipe' });
  child.stdin.write(text); child.stdin.end();
  const [out, err] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text()]);
  if (await child.exited !== 0) throw new Error(err);
  return out.trim();
}
async function freePort() {
  const server = net.createServer();
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const port = server.address().port; await new Promise(resolve => server.close(resolve)); return port;
}
async function until(check, message, timeout = 20000) {
  const start = Date.now();
  do { if (await check()) return; await Bun.sleep(100); } while (Date.now() - start < timeout);
  throw new Error(message);
}
async function read(base, route, options = {}) {
  const response = await fetch(base + route, { redirect: 'manual', ...options });
  const body = await response.text();
  return { status: response.status, headers: Object.fromEntries(response.headers), body };
}
async function hit(base, route) {
  let result;
  await until(async () => { result = await read(base, route); return result.headers['x-ssr-cache'] === 'HIT'; }, 'HTML never entered cache');
  return result;
}
async function main() {
  assert(fs.existsSync(path.join(build, 'server')), 'Run make regression-build first');
  await command(['docker', 'run', '-d', '--name', container, '-e', 'POSTGRES_PASSWORD=regression',
    '-e', 'POSTGRES_DB=ygg_mcp_test', '-p', '127.0.0.1::5432', 'postgres:16-alpine']);
  const port = (await command(['docker', 'port', container, '5432/tcp'])).split(':').at(-1);
  pg = { ...process.env, PGHOST: '127.0.0.1', PGPORT: port, PGUSER: 'postgres', PGPASSWORD: 'regression' };
  await until(async () => {
    try { await command(['pg_isready'], { env: pg }); return true; } catch { return false; }
  }, 'Database not ready');
  const bases = [];
  for (let i = 0; i < 2; i++) {
    const appPort = await freePort(); const base = `http://127.0.0.1:${appPort}`; bases.push(base);
    const cwd = path.join(runtime, `app-${i}`); fs.mkdirSync(cwd); fs.mkdirSync(path.join(cwd, 'uploads'));
    fs.symlinkSync(path.join(build, 'public'), path.join(cwd, 'public'), 'dir');
    const env = { ...pg, DATABASE_URL: `postgresql://postgres:regression@127.0.0.1:${port}/ygg_mcp_test`,
      IP: '127.0.0.1', PORT: String(appPort), APP_BASE_URL: base, COOKIE_SECURE: 'false', SSR_CACHE_SECS: i === 0 ? '2' : '60',
      DIOXUS_ASSET_DIR: path.join(build, 'public'), MCP_TOKEN_ENC_KEY: '42'.repeat(32),
      ADMIN_USERNAME: '', ADMIN_PASSWORD: '', ADMIN_EMAIL: '', SYSINFO_SAMPLE_SECS: '5', RUST_LOG: 'info' };
    const server = Bun.spawn([path.join(build, 'server')], { cwd, env,
      stdout: Bun.file(path.join(artifacts, `server-${i}.log`)), stderr: Bun.file(path.join(artifacts, `server-${i}-error.log`)) });
    servers.push(server);
    await until(async () => {
      if (server.exitCode !== null) throw new Error(`Server ${i} exited; see ${artifacts}`);
      return fetch(`${base}/readyz`).then(r => r.ok).catch(() => false);
    }, 'Application not ready', 60000);
  }
  await sql(`INSERT INTO users(id,username,email,password_hash,role) VALUES(1,'reader-test','reader@test.invalid','unused','admin');
    INSERT INTO posts(author_id,title,slug,summary,content_md,content_html,status,published_at,word_count,reading_time)
    SELECT 1,'归档文章 '||g,'perf-'||lpad(g::text,3,'0'),'无需发送到归档的摘要',
      repeat('NOT_FOR_PUBLIC_PAYLOAD ',2000),
      CASE WHEN g=1 THEN repeat('<p>阅读页与评论懒加载回归正文。</p>',350) ELSE '<p>文章正文</p>' END,
      'published',NOW()-g*INTERVAL '1 minute',300,2 FROM generate_series(1,151) g;`);
  const route = '/post/perf-001';
  await until(async () => (await read(bases[0], route)).body.includes('归档文章 1'), 'Seed not visible');
  const warm = await Promise.all(bases.map(base => hit(base, route)));
  for (let i = 0; i < bases.length; i++) {
    const cached = warm[i]; assert(cached.headers.etag);
    const conditional = await read(bases[i], route, { headers: { 'If-None-Match': cached.headers.etag } });
    assert.equal(conditional.status, 304); assert.equal(conditional.body, '');
    assert.equal((await read(bases[i], route, { headers: { Cookie: 'theme=light' } })).headers['x-ssr-cache'], 'BYPASS');
    assert.equal((await read(bases[i], route + '?preview=1')).headers['x-ssr-cache'], 'BYPASS');
    assert.equal((await read(bases[i], '/notes')).headers['cache-control'], 'no-store');
    assert.equal((await read(bases[i], '/admin')).status, 302);
  }
  await Bun.sleep(2200);
  assert.equal((await read(bases[0], route)).headers['x-ssr-cache'], 'MISS', 'HTML TTL expires');
  await Promise.all(bases.map(base => hit(base, route)));
  const beforeRollback = await read(bases[0], route);
  await sql("BEGIN; UPDATE posts SET title='回滚内容' WHERE slug='perf-001'; ROLLBACK;");
  const rolledBack = await read(bases[0], route);
  assert.equal(rolledBack.headers['x-ssr-generation'], beforeRollback.headers['x-ssr-generation']);
  assert(!rolledBack.body.includes('回滚内容'));
  await sql("UPDATE posts SET title='双机即时更新' WHERE slug='perf-001';");
  const committed = Date.now();
  await until(async () => (await Promise.all(bases.map(base => read(base, route)))).every(r => r.body.includes('双机即时更新')), 'Two-node invalidation failed');
  const invalidationMs = Date.now() - committed;
  assert(invalidationMs < 5000, `Notification was delayed ${invalidationMs} ms`);
  await until(async () => Number(await sql("SELECT count(*) FROM pg_stat_activity WHERE application_name='yggdrasil-cache-sync';")) === 2, 'Cache listeners not connected');
  const beforeDisconnect = await Promise.all(bases.map(base => read(base, route)));
  await sql("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE application_name='yggdrasil-cache-sync';");
  await until(async () => (await Promise.all(bases.map(base => read(base, route)))).every((r, i) =>
    r.headers['x-ssr-generation'] !== beforeDisconnect[i].headers['x-ssr-generation']), 'Disconnect did not invalidate caches');
  await Promise.all(bases.map(base => hit(base, route)));
  await sql("UPDATE posts SET title='断线后更新' WHERE slug='perf-001';");
  assert((await read(bases[1], route)).body.includes('双机即时更新'), 'Second node must still have the 60-second cached snapshot while disconnected');
  await until(async () => (await Promise.all(bases.map(base => read(base, route)))).every(r => r.body.includes('断线后更新')), 'Reconnect lost committed update');
  await until(async () => Number(await sql("SELECT count(*) FROM pg_stat_activity WHERE application_name='yggdrasil-cache-sync';")) === 2, 'Listeners did not reconnect');

  browser = await chromium.launch({ headless: true, proxy: { server: 'http://127.0.0.1:9', bypass: '127.0.0.1,localhost' } });
  const browserResults = [];
  for (const mobile of [false, true]) for (const theme of ['light', 'dark']) {
    const context = await browser.newContext({ viewport: mobile ? { width: 390, height: 844 } : { width: 1440, height: 900 }, colorScheme: theme });
    const page = await context.newPage(); const editorRequests = []; const errors = [];
    page.on('request', request => { if (request.url().includes('/tiptap/editor.js')) editorRequests.push(request.url()); });
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(bases[0] + route);
    await page.waitForFunction(() => Boolean(window.__dx_mainWasm), {}, { timeout: 60000 });
    await page.waitForTimeout(500);
    assert.equal(editorRequests.length, 0, 'Offscreen comments must not download the editor');
    await page.screenshot({ path: path.join(artifacts, `${mobile ? 'mobile' : 'desktop'}-${theme}-article.png`) });
    await page.getByRole('form', { name: '发表评论', exact: true }).scrollIntoViewIfNeeded();
    const editor = page.locator('.comment-editor-mount .ProseMirror'); await editor.waitFor({ state: 'visible', timeout: 30000 });
    await editor.fill('评论草稿懒加载成功'); assert.equal(editorRequests.length, 1);
    await page.screenshot({ path: path.join(artifacts, `${mobile ? 'mobile' : 'desktop'}-${theme}-comments.png`) });
    await page.goto(bases[0] + '/archives');
    await page.waitForFunction(() => Boolean(window.__dx_mainWasm), {}, { timeout: 60000 });
    assert.equal(await page.locator('.archive-entry').count(), 100);
    await page.getByRole('button', { name: '加载更早的文章' }).click();
    await page.waitForFunction(() => document.querySelectorAll('.archive-entry').length === 151);
    assert.equal(await page.getByRole('button', { name: '加载更早的文章' }).count(), 0);
    await page.locator('a[href="/post/perf-001"]').click();
    await page.waitForFunction(() => document.querySelector('h1.post-title')?.textContent.includes('断线后更新'));
    assert.deepEqual(errors, []);
    browserResults.push({ mobile, theme, archiveEntries: 151, editorRequests: editorRequests.length, errors });
    console.log(JSON.stringify(browserResults.at(-1)));
    await context.close();
  }
  const report = { invalidationMs, cache: 'HIT / MISS / BYPASS / 304 / TTL / rollback / reconnect passed', browserResults };
  fs.writeFileSync(path.join(artifacts, 'results.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report));
}
main().catch(error => { console.error(error); process.exitCode = 1; }).finally(async () => {
  await browser?.close();
  for (const server of servers) { server.kill(); await server.exited; }
  await command(['docker', 'rm', '-f', container]).catch(() => {});
  fs.rmSync(runtime, { recursive: true, force: true });
  console.log(`Artifacts: ${artifacts}`);
});
