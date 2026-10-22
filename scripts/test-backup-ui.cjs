#!/usr/bin/env bun
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE);
const base = process.env.ADMIN_BASE;
const artifacts = process.env.REGRESSION_ARTIFACT_DIR;
const endpoint = url => new URL(url).pathname.split('/').pop().replace(/[^a-z]/gi, '').toLowerCase();
const sql = statement => execFileSync('psql', ['-d', 'ygg_browser_test', '-At', '-v', 'ON_ERROR_STOP=1', '-c', statement], { encoding: 'utf8' }).trim();
const report = { passed: [], errors: [], requests: [] };
const pass = name => { report.passed.push(name); console.log('PASS', name); };
async function main() {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  await context.addCookies([{ name: 'session', value: process.env.ADMIN_SESSION, url: base }]);
  const page = await context.newPage();
  page.setDefaultTimeout(30000);
  let failCreate = false, failPoll = false, polls = 0, restores = 0;
  page.on('pageerror', e => report.errors.push(e.message));
  await page.route('https://fonts.googleapis.com/**', r => r.fulfill({ body: '' }));
  await page.route('**/api/**', async route => {
    const name = endpoint(route.request().url()); report.requests.push(name);
    if (name.startsWith('restorebackup')) restores++;
    if (name.startsWith('createbackup') && failCreate) {
      failCreate = false; await route.fulfill({ status: 503, body: 'Regression create failure' }); return;
    }
    if (name.startsWith('gettaskprogress')) {
      polls++;
      if (failPoll) { failPoll = false; await route.fulfill({ status: 503, body: 'Regression progress failure' }); return; }
    }
    await route.continue();
  });
  const create = page.getByRole('button', { name: '创建备份', exact: true });
  const finished = () => page.waitForFunction(() => [...document.querySelectorAll('button')].some(b => b.textContent.trim() === '创建备份' && !b.disabled));
  try {
    await page.goto(base + '/admin/system');
    await page.getByRole('tab', { name: '备份', exact: true }).click();
    await create.waitFor();
    failCreate = true;
    await create.click();
    await page.getByRole('alert').filter({ hasText: '备份失败' }).waitFor();
    await finished();
    pass('creation failure is visible and releases the operation lock');

    await create.click();
    await page.locator('tbody tr').first().waitFor(); await finished();
    const row = page.locator('tbody tr').first();
    const filename = (await row.locator('td').first().textContent()).trim();
    assert.match(filename, /\.sql$/);
    assert.equal(await row.locator('td').nth(2).textContent(), 'pg_dump');
    assert(polls > 0);
    pass('real pg_dump backup completes through progress polling and appears in the list');
    const download = await context.request.get(`${base}/api/database/backups/${filename}`);
    assert(download.ok());
    const backup = await download.body();
    assert(backup.toString().includes('-- YGGDRASIL BACKUP v1'));

    sql("UPDATE posts SET title='after backup' WHERE id=1");
    await row.getByRole('button', { name: '恢复', exact: true }).click();
    const confirmation = page.getByRole('button', { name: '确认恢复', exact: true });
    await confirmation.click();
    await page.getByText('100%', { exact: true }).waitFor(); await finished();
    // The old completed progress is cleared synchronously when restoring begins.
    assert.equal(sql('SELECT title FROM posts WHERE id=1'), '回归测试文章');
    assert.equal(restores, 1);
    pass('confirmed restore makes one request and restores the actual database snapshot');

    await page.locator('input[type=file]').setInputFiles({ name: 'manual_regression_import.sql', mimeType: 'application/sql', buffer: backup });
    const imported = page.locator('tbody tr').filter({ hasText: 'manual_regression_import.sql' });
    await imported.waitFor();
    pass('authenticated import registers a real signed backup and refreshes the list');
    await imported.getByRole('button', { name: '删除', exact: true }).click();
    await page.getByRole('button', { name: '确认删除', exact: true }).click();
    await imported.waitFor({ state: 'detached' });
    pass('confirmed deletion removes the imported backup');

    failPoll = true;
    await create.click();
    await page.getByRole('alert').filter({ hasText: '获取备份进度失败' }).waitFor();
    await finished();
    pass('poll failure is visible and permits subsequent operations');
    const refreshed = page.waitForResponse(r => endpoint(r.url()).startsWith('listbackups') && r.ok());
    await page.getByRole('button', { name: '刷新列表', exact: true }).click();
    await refreshed;
    await page.getByRole('alert').waitFor({ state: 'detached' });
    pass('successful explicit refresh clears the previous error');

    await create.click();
    const started = polls;
    await page.getByRole('tab', { name: '数据库', exact: true }).click();
    await page.waitForTimeout(2000);
    assert.equal(polls, started);
    await page.getByRole('tab', { name: '备份', exact: true }).click();
    await create.waitFor();
    await create.click();
    await page.getByText('100%', { exact: true }).waitFor(); await finished();
    pass('unmount stops polling and re-entry can start a new backup');
    for (const variant of [
      { name: 'light', width: 1440, height: 1000, scheme: 'light' },
      { name: 'mobile-dark', width: 390, height: 844, scheme: 'dark' },
    ]) {
      await page.setViewportSize({ width: variant.width, height: variant.height });
      await page.emulateMedia({ colorScheme: variant.scheme, reducedMotion: 'reduce' });
      await page.screenshot({ path: path.join(artifacts, `backup-${variant.name}.png`), fullPage: true });
    }
    assert.deepEqual(report.errors, []);
  } finally {
    fs.writeFileSync(path.join(artifacts, 'backup.json'), JSON.stringify(report, null, 2));
    await browser.close();
  }
}
main().catch(e => { console.error(e); process.exitCode = 1; });
