#!/usr/bin/env bun
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { loadPlaywright } from './libs-modules.ts';

const { chromium } = loadPlaywright();
const base = process.env.ADMIN_BASE!;
const artifacts = process.env.REGRESSION_ARTIFACT_DIR!;
const runtime = process.env.REGRESSION_RUNTIME!;
const endpoint = (url: string) =>
  new URL(url).pathname
    .split('/')
    .pop()!
    .replace(/[^a-z]/gi, '')
    .toLowerCase();
const sql = (statement: string) =>
  execFileSync(
    'psql',
    ['-d', 'ygg_browser_test', '-At', '-v', 'ON_ERROR_STOP=1', '-c', statement],
    { encoding: 'utf8' },
  ).trim();
const report: { passed: string[]; errors: string[]; requests: string[] } = {
  passed: [],
  errors: [],
  requests: [],
};
const pass = (name: string) => {
  report.passed.push(name);
  console.log('PASS', name);
};
async function main() {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  await context.addCookies([{ name: 'session', value: process.env.ADMIN_SESSION!, url: base }]);
  const page = await context.newPage();
  page.setDefaultTimeout(30000);
  let failCreate = false,
    failPoll = false,
    polls = 0,
    restores = 0;
  page.on('pageerror', (e) => report.errors.push(e.message));
  await page.route('https://fonts.googleapis.com/**', (r) => r.fulfill({ body: '' }));
  await page.route('**/api/**', async (route) => {
    const name = endpoint(route.request().url());
    report.requests.push(name);
    if (name.startsWith('restorebackup')) restores++;
    if (name.startsWith('createbackup') && failCreate) {
      failCreate = false;
      await route.fulfill({ status: 503, body: 'Regression create failure' });
      return;
    }
    if (name.startsWith('gettaskprogress')) {
      polls++;
      if (failPoll) {
        failPoll = false;
        await route.fulfill({ status: 503, body: 'Regression progress failure' });
        return;
      }
    }
    await route.continue();
  });
  const create = page.getByRole('button', { name: '创建备份', exact: true });
  const finished = () =>
    page.waitForFunction(() =>
      [...document.querySelectorAll('button')].some(
        (b) => b.textContent!.trim() === '创建备份' && !b.disabled,
      ),
    );
  const expectBackupFailure = async (reason: RegExp) => {
    const failed = page.waitForResponse(
      async (r) =>
        endpoint(r.url()) === 'gettaskprogress' && r.ok() && (await r.json()).status === 'Failed',
    );
    await create.click();
    const progress = await (await failed).json();
    assert.equal(progress.result_filename, null);
    assert.match(progress.error, reason);
    const panel = page.getByRole('status').filter({ hasText: '数据库备份失败' });
    await panel.getByText(reason).waitFor();
    await finished();
    assert.equal(await page.locator('tbody tr').count(), 0);
    const files = fs
      .readdirSync(path.join(runtime, 'backups'), { withFileTypes: true, recursive: true })
      .filter((entry) => !entry.isDirectory())
      .map((entry) => entry.name);
    assert.deepEqual(files, [], '失败的任务不得留下 SQL、素材包或暂存文件');
  };
  try {
    await page.goto(base + '/admin/system');
    await page.getByRole('tab', { name: '备份', exact: true }).click();
    await create.waitFor();
    failCreate = true;
    await create.click();
    await page.getByRole('alert').filter({ hasText: '备份失败' }).waitFor();
    await finished();
    pass('creation failure is visible and releases the operation lock');

    await expectBackupFailure(/数据库备份需要 pg_dump，请安装 PostgreSQL 客户端/);
    pass('missing pg_dump fails the real task without publishing SQL or uploads');

    const pgDump = Bun.which('pg_dump');
    assert(pgDump, 'backup regression requires a real pg_dump on the host PATH');
    const appPgDump = path.join(runtime, 'pg-bin', 'pg_dump');
    fs.writeFileSync(appPgDump, '#!/bin/sh\nprintf "%s" "partial dump"\nexit 7\n', {
      mode: 0o755,
    });
    await expectBackupFailure(/pg_dump 导出失败:.*\b7\b/);
    pass('nonzero pg_dump exit discards partial stdout and reports its status with empty stderr');

    fs.unlinkSync(appPgDump);
    fs.symlinkSync(pgDump, appPgDump);

    await create.click();
    await page.locator('tbody tr').first().waitFor();
    await finished();
    const row = page.locator('tbody tr').first();
    const filename = (await row.locator('td').first().textContent())!.trim();
    assert.match(filename, /\.sql$/);
    assert.equal(await row.locator('td').nth(2).textContent(), 'pg_dump');
    await row.getByRole('link', { name: '素材', exact: true }).waitFor();
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
    await page.getByText('100%', { exact: true }).waitFor();
    await finished();
    // The old completed progress is cleared synchronously when restoring begins.
    assert.equal(sql('SELECT title FROM posts WHERE id=1'), '回归测试文章');
    assert.equal(restores, 1);
    pass('confirmed restore makes one request and restores the actual database snapshot');

    await page.locator('input[type=file]').setInputFiles({
      name: 'manual_regression_import.sql',
      mimeType: 'application/sql',
      buffer: backup,
    });
    const imported = page.locator('tbody tr').filter({ hasText: 'manual_regression_import.sql' });
    await imported.waitFor();
    pass('authenticated import registers a real signed backup and refreshes the list');
    await imported.getByRole('button', { name: '删除', exact: true }).click();
    await page.getByRole('button', { name: '确认删除', exact: true }).click();
    await imported.waitFor({ state: 'detached' });
    pass('confirmed deletion removes the imported backup');

    const salvageFilename = 'renamed_legacy_export.sql';
    const salvage = Buffer.from(
      '-- YGGDRASIL BACKUP v1\n-- created_at: 2026-08-10\n-- mode: sql-fallback\n\n-- table: posts\n1,旧数据\n',
    );
    const importing = page.waitForResponse(
      (r) => endpoint(r.url()) === 'import' && r.request().method() === 'POST',
    );
    await page.locator('input[type=file]').setInputFiles({
      name: salvageFilename,
      mimeType: 'application/sql',
      buffer: salvage,
    });
    const importResponse = await importing;
    assert(importResponse.ok(), await importResponse.text());
    const salvageRow = page.locator('tbody tr').filter({ hasText: salvageFilename });
    await salvageRow.getByText('仅供数据抢救', { exact: true }).waitFor();
    await finished();
    assert(await salvageRow.getByRole('button', { name: '恢复', exact: true }).isDisabled());
    const save = page.waitForEvent('download');
    await salvageRow.getByRole('link', { name: '下载', exact: true }).click();
    const saved = await (await save).path();
    assert(saved);
    assert.deepEqual(fs.readFileSync(saved), salvage);
    await page.screenshot({ path: path.join(artifacts, 'backup-salvage.png'), fullPage: true });

    const refused = await page.request.post(`${base}/api/restore_backup`, {
      headers: { Origin: base },
      data: { filename: salvageFilename, confirm: true },
    });
    assert(!refused.ok(), '恢复接口必须拒绝抢救文件，不得返回任务 ID');
    assert.match(await refused.text(), /这是仅供数据抢救的旧导出，不能直接恢复数据库/);
    assert.equal(sql('SELECT title FROM posts WHERE id=1'), '回归测试文章');
    pass('renamed legacy export is downloadable byte for byte but cannot be restored');
    await salvageRow.getByRole('button', { name: '删除', exact: true }).click();
    await page.getByRole('button', { name: '确认删除', exact: true }).click();
    await salvageRow.waitFor({ state: 'detached' });
    pass('confirmed deletion still removes a salvage-only export');

    failPoll = true;
    await create.click();
    await page.getByRole('alert').filter({ hasText: '获取备份进度失败' }).waitFor();
    await finished();
    pass('poll failure is visible and permits subsequent operations');
    const refreshed = page.waitForResponse(
      (r) => endpoint(r.url()).startsWith('listbackups') && r.ok(),
    );
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
    await page.getByText('100%', { exact: true }).waitFor();
    await finished();
    pass('unmount stops polling and re-entry can start a new backup');
    for (const variant of [
      { name: 'light', width: 1440, height: 1000, scheme: 'light' },
      { name: 'mobile-dark', width: 390, height: 844, scheme: 'dark' },
    ] as const) {
      await page.setViewportSize({ width: variant.width, height: variant.height });
      await page.emulateMedia({ colorScheme: variant.scheme, reducedMotion: 'reduce' });
      await page.screenshot({
        path: path.join(artifacts, `backup-${variant.name}.png`),
        fullPage: true,
      });
    }
    assert.deepEqual(report.errors, []);
  } finally {
    fs.writeFileSync(path.join(artifacts, 'backup.json'), JSON.stringify(report, null, 2));
    await browser.close();
  }
}
main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
