#!/usr/bin/env bun
// Owns only the temporary container, process and working directory created here.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const net = require('node:net');
const { createHash } = require('node:crypto');
const ROOT = path.resolve(__dirname, '..');
const mode = process.argv[2] || 'database';
const suites = {
  slash: 'test-slash-menu.cjs', smoke: 'test-maintenance.cjs', admin: 'test-admin-editor.cjs',
  lightbox: 'test-lightbox.cjs', showcase: 'test-components-showcase.cjs',
  notes: 'test-notes-editor.cjs', backup: 'test-backup-ui.cjs', runner: 'test-runner-ui.cjs',
};
if (mode !== 'database' && !suites[mode]) throw new Error(`Unknown suite: ${mode}`);
const artifacts = path.resolve(process.env.REGRESSION_ARTIFACT_DIR ||
  path.join(ROOT, 'target/regressions', `${Date.now()}-${mode}`));
fs.mkdirSync(artifacts, { recursive: true });
const runtime = fs.mkdtempSync(path.join(os.tmpdir(), 'ygg-regression-'));
const container = `ygg-regression-${process.pid}-${Date.now()}`;
let server;
let child;
let cleaned = false;
async function run(args, options = {}) {
  child = Bun.spawn(args, { cwd: ROOT, env: process.env, stdout: 'inherit', stderr: 'inherit', ...options });
  const code = await child.exited;
  child = null;
  if (code !== 0) throw new Error(`${args[0]} exited ${code}`);
}
async function capture(args) {
  const proc = Bun.spawn(args, { stdout: 'pipe', stderr: 'pipe' });
  const output = await new Response(proc.stdout).text();
  if (await proc.exited !== 0) throw new Error(await new Response(proc.stderr).text());
  return output.trim();
}
async function cleanup() {
  if (cleaned) return;
  cleaned = true;
  if (child) { child.kill(); await child.exited; }
  if (server) { server.kill(); await server.exited; }
  await capture(['docker', 'rm', '-f', container]).catch(() => {});
  fs.rmSync(runtime, { recursive: true, force: true });
}
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, async () => { await cleanup(); process.exit(1); });
async function freePort() {
  const listener = net.createServer();
  await new Promise(resolve => listener.listen(0, '127.0.0.1', resolve));
  const port = listener.address().port;
  await new Promise(resolve => listener.close(resolve));
  return port;
}
async function waitFor(check, description, attempts = 120) {
  for (let i = 0; i < attempts; i++) {
    if (await check()) return;
    await Bun.sleep(500);
  }
  throw new Error(`Timed out waiting for ${description}; see ${artifacts}`);
}
async function main() {
  await capture(['docker', 'run', '-d', '--name', container, '-e', 'POSTGRES_PASSWORD=regression',
    '-p', '127.0.0.1::5432', 'postgres:18-alpine']);
  const port = (await capture(['docker', 'port', container, '5432/tcp'])).split(':').at(-1);
  const pg = { ...process.env, PGHOST: '127.0.0.1', PGPORT: port, PGUSER: 'postgres', PGPASSWORD: 'regression' };
  await waitFor(async () => {
    const p = Bun.spawn(['pg_isready'], { env: pg, stdout: 'ignore', stderr: 'ignore' });
    return await p.exited === 0;
  }, 'PostgreSQL');
  if (mode === 'database') {
    await run(['bash', 'scripts/test-databases.sh'], { env: pg });
    return;
  }
  const build = path.join(ROOT, 'target/dx/yggdrasil/debug/web');
  if (!fs.existsSync(path.join(build, 'server'))) throw new Error('Run make regression-build first');
  await run(['createdb', 'ygg_browser_test'], { env: pg });
  const appPort = await freePort();
  const base = `http://127.0.0.1:${appPort}`;
  fs.symlinkSync(path.join(build, 'public'), path.join(runtime, 'public'), 'dir');
  fs.mkdirSync(path.join(runtime, 'uploads'));
  fs.copyFileSync(path.join(ROOT, 'public/images/samples/clay-circle.webp'), path.join(runtime, 'uploads/fixture.webp'));
  const env = { ...pg, DATABASE_URL: `postgresql://postgres:regression@127.0.0.1:${port}/ygg_browser_test`,
    IP: '127.0.0.1', PORT: String(appPort), APP_BASE_URL: base, COOKIE_SECURE: 'false', SSR_CACHE_SECS: '0',
    DIOXUS_ASSET_DIR: path.join(build, 'public'), MCP_TOKEN_ENC_KEY: '42'.repeat(32),
    ADMIN_USERNAME: '', ADMIN_PASSWORD: '', ADMIN_EMAIL: '', CODE_RUNNER_LANGUAGES: 'python' };
  server = Bun.spawn([path.join(build, 'server')], { cwd: runtime, env,
    stdout: Bun.file(path.join(artifacts, 'server.log')), stderr: Bun.file(path.join(artifacts, 'server-error.log')) });
  await waitFor(async () => {
    if (server.exitCode !== null) throw new Error(`Server exited; see ${artifacts}`);
    return fetch(`${base}/readyz`).then(r => r.ok).catch(() => false);
  }, 'test site');
  const hash = createHash('sha256').update('regression-admin').digest('hex');
  const sql = `INSERT INTO users(id,username,email,password_hash,role) VALUES(1,'regression','regression@test.invalid','unused','admin');
    INSERT INTO sessions(user_id,token_hash,expires_at) VALUES(1,'${hash}',NOW()+INTERVAL '1 hour');
    INSERT INTO posts(id,author_id,title,slug,summary,content_md,content_html,status,cover_image)
      VALUES(1,1,'回归测试文章','regression-post','摘要','回归正文','<p>回归正文</p>','draft','/uploads/fixture.webp');
    SELECT setval('posts_id_seq',1); SELECT setval('users_id_seq',1);
    INSERT INTO assets(id,path,filename,mime,size_bytes,width,height)
      VALUES('00000000-0000-4000-8000-000000000001','fixture.webp','fixture.webp','image/webp',1,100,100);
    INSERT INTO asset_refs(asset_id,post_id) VALUES('00000000-0000-4000-8000-000000000001',1);`;
  const fixture = path.join(runtime, 'fixture.sql');
  fs.writeFileSync(fixture, sql);
  await run(['psql', '-v', 'ON_ERROR_STOP=1', '-d', 'ygg_browser_test', '-f', fixture], { env: pg });
  await run(['bun', path.join(ROOT, 'scripts', suites[mode])], { env: { ...env,
    PLAYWRIGHT_MODULE: path.join(ROOT, 'libs/node_modules/playwright'), ADMIN_BASE: base, VT_BASE: base,
    SHOWCASE_BASE: base, SHOWCASE_ARTIFACT_DIR: artifacts,
    ADMIN_POST_ID: '1', ADMIN_SESSION: 'regression-admin', ADMIN_ARTIFACT_DIR: artifacts,
    REGRESSION_ARTIFACT_DIR: artifacts, REGRESSION_RUNTIME: runtime } });
}
main().catch(error => { console.error(error); process.exitCode = 1; }).finally(async () => {
  await cleanup(); console.log(`Regression artifacts: ${artifacts}`);
});
