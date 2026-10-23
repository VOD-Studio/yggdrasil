#!/usr/bin/env bun
// 原型验收：SSR + lazy 水合、SPA 分包导航、server fn 读写、后台鉴权、SSR HTML 缓存、
// 滚动发布（旧标签页懒加载旧分包）。需要先 `scripts/release.sh a` 与 `scripts/release.sh b`。
// 用法：SPIKE_DATABASE_URL=… bun scripts/verify.cjs
const { createHash, randomBytes } = require('node:crypto');
const { join } = require('node:path');
const { chromium } = require(
  process.env.PLAYWRIGHT_MODULE || join(__dirname, '../../../libs/node_modules/playwright'),
);

const SPIKE = join(__dirname, '..');
const BASE = 'http://127.0.0.1:3100';
const PG = process.env.SPIKE_DATABASE_URL;
if (!PG) throw new Error('需要 SPIKE_DATABASE_URL（一次性数据库副本）');

const results = [];
const check = (name, ok, detail = '') => {
  results.push({ name, ok: !!ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`);
};

const psql = (sql) => {
  const r = Bun.spawnSync(['psql', PG, '-Atc', sql]);
  if (r.exitCode !== 0) throw new Error(r.stderr.toString());
  return r.stdout.toString().trim();
};

let server = null;
async function startServer(id, prev) {
  const args = [join(SPIKE, 'scripts/serve.sh'), id];
  if (prev) args.push(prev);
  server = Bun.spawn(args, {
    cwd: SPIKE,
    env: { ...process.env, APP_BASE_URL: BASE },
    stdout: 'ignore',
    stderr: 'inherit',
  });
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`${BASE}/`)).ok) return;
    } catch {}
    await Bun.sleep(100);
  }
  throw new Error(`release ${id} 未能启动`);
}
async function stopServer() {
  server?.kill();
  await server?.exited;
  server = null;
}

// 每个页面收集脚本错误、控制台错误、失败请求与 WASM 请求。
function instrument(page) {
  const log = { errors: [], failed: [], wasm: [] };
  page.on('pageerror', (e) => log.errors.push(`pageerror: ${e.message}`));
  page.on('console', (m) => {
    if (m.type() === 'error') log.errors.push(`console: ${m.text()}`);
  });
  page.on('response', async (r) => {
    const url = new URL(r.url());
    if (url.pathname.endsWith('.wasm')) {
      log.wasm.push({
        path: url.pathname,
        status: r.status(),
        source: (await r.allHeaders())['x-spike-asset-source'] || 'current',
      });
    }
    if (r.status() >= 400) log.failed.push(`${r.status()} ${url.pathname}`);
  });
  return log;
}
// 资源 404 会同时产生一条 "Failed to load resource" 控制台错误；单独由 failed 列表判定。
const scriptErrors = (log) => log.errors.filter((e) => !e.includes('Failed to load resource'));
const hydrated = async (page) => {
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(300);
};

// 点击主题分段控件的“深色”：只有水合后的事件处理器会写入 dark class（ThemePreload 只管首帧）。
const toggleDark = async (page) => {
  await page.getByRole('radio', { name: '深色' }).click();
  return page
    .waitForFunction(() => document.documentElement.classList.contains('dark'), null, { timeout: 3000 })
    .then(() => true, () => false);
};
async function functional(browser) {
  // F1 首页直开：SSR 条目数 = 水合后条目数（无重复渲染），水合后交互生效。
  {
    const ssr = await (await fetch(`${BASE}/`)).text();
    const ssrRows = (ssr.match(/class="home-row"/g) || []).length;
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    const log = instrument(page);
    await page.goto(`${BASE}/`);
    await hydrated(page);
    const domRows = await page.locator('.home-row').count();
    const mains = await page.locator('main').count();
    check('F1 首页直开：SSR 与水合后列表条目一致、无重复', ssrRows > 0 && domRows === ssrRows && mains === 1, `ssr=${ssrRows} dom=${domRows} main=${mains}`);
    const dark = await toggleDark(page);
    check('F1 首页水合后主题切换可交互', dark);
    const homeChunks = log.wasm.map((w) => w.path);
    check(
      'F1 首页只加载主包 + 首页分包（不含文章/后台分包）',
      homeChunks.some((p) => p.includes('home_route_view')) &&
        !homeChunks.some((p) => p.includes('post_detail_route_view') || p.includes('admin_comments')),
      homeChunks.map((p) => p.replace('/pkg/', '')).join(', '),
    );
    check('F1 首页无脚本错误', scriptErrors(log).length === 0, scriptErrors(log).join(' | '));
    check('F1 首页无失败请求', log.failed.length === 0, log.failed.join(', '));

    // F2 SPA 导航：首页 → 文章（按需拉文章分包）→ 下一篇（同路由换 slug 重新取数）。
    log.wasm.length = 0;
    const firstHref = await page.locator('.home-row').first().getAttribute('href');
    await page.locator('.home-row').first().click();
    await page.waitForURL(`${BASE}${firstHref}`);
    await page.waitForSelector('.post-content');
    const navChunks = log.wasm.map((w) => w.path);
    check(
      'F2 SPA 进入文章页按需加载文章分包',
      navChunks.some((p) => p.includes('post_detail_route_view')) && !navChunks.some((p) => p.includes('ygg_spike.')),
      navChunks.map((p) => p.replace('/pkg/', '')).join(', '),
    );
    const title1 = await page.locator('h1').first().textContent();
    const navLink = page.locator('.post-nav a').first();
    if ((await navLink.count()) > 0) {
      const href = await navLink.getAttribute('href');
      await navLink.click();
      await page.waitForURL(`${BASE}${href}`);
      await page.waitForFunction(
        (t) => {
          const text = document.querySelector('h1')?.textContent;
          return text && text !== t;
        },
        title1,
        { timeout: 10000 },
      );
      const title2 = await page.locator('h1').first().textContent();
      check('F2 上/下一篇导航刷新内容', title2 !== title1, `${title1} → ${title2}`);
      check('F2 导航后正文唯一', (await page.locator('.post-content').count()) === 1);
    } else {
      check('F2 上/下一篇导航刷新内容', false, '首篇没有上下篇链接');
    }
    check('F2 SPA 导航无脚本错误', scriptErrors(log).length === 0, scriptErrors(log).join(' | '));
    await ctx.close();
  }

  // F3 文章直开 + 发表评论（tiptap 编辑器经 window 全局懒加载，server fn 写库）。
  const slug = psql(
    "SELECT slug FROM posts WHERE status='published' AND deleted_at IS NULL ORDER BY published_at DESC LIMIT 1",
  );
  const postId = psql(`SELECT id FROM posts WHERE slug='${slug}'`);
  const marker = `spike-${randomBytes(4).toString('hex')}`;
  {
    const ssr = await fetch(`${BASE}/post/${slug}`);
    const html = await ssr.text();
    check('F3 文章 SSR：状态 200，<title> 与 og:title 已在头部', ssr.status === 200 && /<title>[^<]+ · Yggdrasil/.test(html) && html.includes('property="og:title"'));
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    const log = instrument(page);
    await page.goto(`${BASE}/post/${slug}`);
    await hydrated(page);
    check('F3 文章直开：正文与评论区唯一', (await page.locator('.post-content').count()) === 1 && (await page.locator('[role="form"]').count()) === 1);
    await page.waitForSelector('.ProseMirror', { timeout: 15000 });
    await page.fill('input[placeholder="昵称"]', 'Spike 测试');
    await page.fill('input[placeholder="邮箱（不公开）"]', 'spike@example.com');
    await page.locator('.ProseMirror').click();
    await page.keyboard.type(`来自 Leptos 原型的评论 ${marker}`);
    await page.getByRole('button', { name: '发表' }).click();
    await page.waitForSelector('.comment-badge.is-pending', { timeout: 10000 });
    const row = psql(`SELECT status FROM comments WHERE post_id=${postId} AND content_md LIKE '%${marker}%'`);
    check('F3 发表评论：server fn 写入 pending，前端显示审核中', row === 'pending', `db=${row || '无'}`);
    check('F3 文章页无脚本错误', scriptErrors(log).length === 0, scriptErrors(log).join(' | '));
    check('F3 文章页无失败请求', log.failed.length === 0, log.failed.join(', '));
    await ctx.close();
  }

  // F4 状态码：未知文章 404、后台未登录 401。
  {
    const r404 = await fetch(`${BASE}/post/__spike_missing__`);
    const r401 = await fetch(`${BASE}/admin/comments`);
    check('F4 未知文章返回 404', r404.status === 404);
    check('F4 后台未登录返回 401', r401.status === 401);
  }

  // F5 后台审核：复用主应用 sessions 表的会话，通过后公开页（缓存已失效）显示评论。
  {
    const token = randomBytes(24).toString('hex');
    const hash = createHash('sha256').update(token).digest('hex');
    psql(
      `INSERT INTO sessions (user_id, token_hash, expires_at, generation)
       SELECT id, '${hash}', NOW() + INTERVAL '1 hour', session_generation FROM users
       WHERE role='admin' ORDER BY id LIMIT 1`,
    );
    // 先让公开页进入 SSR 缓存（必须读完响应体：缓存只在流完整结束后回填）。
    await (await fetch(`${BASE}/post/${slug}`)).text();
    const cachedBefore = await fetch(`${BASE}/post/${slug}`);
    await cachedBefore.text();
    check('F5 审核前文章页命中 SSR 缓存', cachedBefore.headers.get('x-ssr-cache') === 'HIT');
    const ctx = await browser.newContext();
    await ctx.addCookies([{ name: 'session', value: token, domain: '127.0.0.1', path: '/' }]);
    const page = await ctx.newPage();
    const log = instrument(page);
    const resp = await page.goto(`${BASE}/admin/comments`);
    await hydrated(page);
    const item = page.locator('.admin-comment', { hasText: marker });
    check('F5 后台直开：200 且列出待审评论', resp.status() === 200 && (await item.count()) === 1, `status=${resp.status()}`);
    const chunks = log.wasm.map((w) => w.path);
    check('F5 后台只加载后台分包（不含首页/文章分包）', chunks.some((p) => p.includes('admin_comments')) && !chunks.some((p) => p.includes('home_route_view') || p.includes('post_detail_route_view')), chunks.map((p) => p.replace('/pkg/', '')).join(', '));
    await item.getByRole('button', { name: '通过' }).click();
    let status = '';
    for (let i = 0; i < 50 && status !== 'approved'; i++) {
      await Bun.sleep(100);
      status = psql(`SELECT status FROM comments WHERE content_md LIKE '%${marker}%'`);
    }
    check('F5 后台“通过”写库', status === 'approved', `db=${status}`);
    const after = await fetch(`${BASE}/post/${slug}`);
    await after.text();
    check('F5 审核后公开页 SSR 缓存已失效', after.headers.get('x-ssr-cache') === 'MISS', `x-ssr-cache=${after.headers.get('x-ssr-cache')}`);
    // 评论区与原版一样在客户端加载：直接调用公开 server fn 确认评论已可见。
    const list = await fetch(`${BASE}/api/comments/list_by_post`, {
      method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' },
      body: `post_id=${postId}`,
    });
    check('F5 审核后公开评论接口返回该评论', (await list.text()).includes(marker));
    check('F5 后台无脚本错误', scriptErrors(log).length === 0, scriptErrors(log).join(' | '));
    await ctx.close();
    psql(`DELETE FROM sessions WHERE token_hash='${hash}'`);
  }

  // F6 SSR 缓存命中页的水合：回放的 HTML 与新渲染等价。
  {
    await (await fetch(`${BASE}/`)).text();
    const hit = await fetch(`${BASE}/`);
    await hit.text();
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    const log = instrument(page);
    const resp = await page.goto(`${BASE}/`);
    await hydrated(page);
    const dark = await toggleDark(page);
    check('F6 缓存命中页正常水合', hit.headers.get('x-ssr-cache') === 'HIT' && (await resp.allHeaders())['x-ssr-cache'] === 'HIT' && dark && scriptErrors(log).length === 0, scriptErrors(log).join(' | '));
    await ctx.close();
  }
  return slug;
}

// R：滚动发布。release a 打开的旧标签页，在服务端换成 release b 后继续 SPA 导航。
async function rolling(browser, slug) {
  await startServer('a');
  const openOld = async () => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    const log = instrument(page);
    await page.goto(`${BASE}/`);
    await hydrated(page);
    log.wasm.length = 0;
    return { ctx, page, log };
  };
  const withPrev = await openOld();
  const withoutPrev = await openOld();
  await stopServer();

  await startServer('b', 'a');
  const html = await (await fetch(`${BASE}/`)).text();
  check('R0 服务端已切到 release b', html.includes('name="spike-build" content="b"'));
  {
    const { ctx, page, log } = withPrev;
    await page.locator(`a[href="/post/${slug}"]`).first().click();
    await page.waitForSelector('.post-content', { timeout: 10000 });
    const build = await page.locator('[data-spike-build]').first().getAttribute('data-spike-build');
    const chunk = log.wasm.find((w) => w.path.includes('post_detail_route_view'));
    check(
      'R1 旧标签页（a）导航：从上一发布目录取回 a 的文章分包并渲染',
      chunk && chunk.status === 200 && chunk.source === 'previous' && build === 'a',
      `chunk=${chunk ? `${chunk.status}/${chunk.source}` : '无'} build=${build}`,
    );
    check('R1 旧标签页调用新服务端的 server fn 正常', (await page.locator('.post-content').count()) === 1 && scriptErrors(log).length === 0, scriptErrors(log).join(' | '));
    await ctx.close();
  }
  await stopServer();

  // 对照：不保留上一发布目录时，旧主包请求的分包 404。
  await startServer('b');
  {
    const { ctx, page, log } = withoutPrev;
    await page.locator(`a[href="/post/${slug}"]`).first().click();
    await page.waitForTimeout(3000);
    const chunk = log.wasm.find((w) => w.path.includes('post_detail_route_view'));
    const rendered = await page.locator('.post-content').count();
    check(
      'R2 对照：无保留目录时旧分包 404，页面无法渲染文章',
      chunk && chunk.status === 404 && rendered === 0,
      `chunk=${chunk ? chunk.status : '无'} rendered=${rendered} url=${page.url()} errors=${scriptErrors(log).slice(0, 2).join(' | ')}`,
    );
    await ctx.close();
  }
  await stopServer();
}

const browser = await chromium.launch();
try {
  await startServer('a');
  const slug = await functional(browser);
  await stopServer();
  await rolling(browser, slug);
} finally {
  await stopServer();
  await browser.close();
}
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} 通过`);
process.exit(failed.length ? 1 : 0);
