#!/usr/bin/env bun
// Real database saves, with delays/failures injected only where named in the report.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE);
const base = process.env.ADMIN_BASE;
const artifacts = process.env.REGRESSION_ARTIFACT_DIR;
const report = { passed: [], errors: [], requests: [] };
const endpoint = url => new URL(url).pathname.split('/').pop().replace(/[^a-z]/gi, '').toLowerCase();
const sql = statement => execFileSync('psql', ['-d', 'ygg_browser_test', '-At', '-v', 'ON_ERROR_STOP=1', '-c', statement], { encoding: 'utf8' }).trim();
const note = () => JSON.parse(sql("SELECT row_to_json(n) FROM notes n ORDER BY id DESC LIMIT 1"));
const title = id => sql(`SELECT r.title FROM note_revisions r JOIN notes n ON n.id=r.note_id AND n.version=r.version WHERE n.id=${id}`);
const pass = name => { report.passed.push(name); console.log('PASS', name); };
// 页头保存状态：已保存且没有新的修改。
async function saved(page) {
  await page.locator('.write-status[data-state="saved"]').waitFor();
}
async function main() {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  await context.addCookies([{ name: 'session', value: process.env.ADMIN_SESSION, url: base }]);
  const page = await context.newPage();
  page.setDefaultTimeout(30000);
  page.on('pageerror', e => report.errors.push(e.message));
  page.on('dialog', dialog => dialog.accept());
  let delaySave = false, failSave = false, delayHistory = false;
  await page.route('https://fonts.googleapis.com/**', r => r.fulfill({ body: '' }));
  await page.route('**/api/**', async route => {
    const name = endpoint(route.request().url());
    report.requests.push({ name, body: route.request().postData() });
    if (name.startsWith('savenote')) {
      if (failSave) { failSave = false; await route.fulfill({ status: 503, body: 'Regression save failure' }); return; }
      if (delaySave) { delaySave = false; await new Promise(resolve => setTimeout(resolve, 700)); }
    }
    if (name.startsWith('getownednote') && delayHistory) {
      delayHistory = false; await new Promise(resolve => setTimeout(resolve, 700));
    }
    await route.continue();
  });
  try {
    await page.goto(base + '/admin/notes/new');
    await page.locator('#note-tiptap .ProseMirror').waitFor();
    await page.getByLabel('笔记标题').fill('第一版');
    await page.locator('#note-tiptap .ProseMirror').fill('正文保持私密');
    await saved(page);
    const id = note().id;
    const initialVersion = note().version;
    assert.equal(note().published_version, null);
    assert.equal(note().knowledge_version, null);
    pass('autosave creates a private working draft');
    await page.goto(`${base}/admin/notes/edit/${id}`);
    await page.locator('#note-tiptap .ProseMirror').waitFor();

    delaySave = true;
    const started = page.waitForRequest(r => endpoint(r.url()).startsWith('savenote'));
    await page.getByLabel('笔记标题').fill('提交中的版本');
    await started;
    await page.getByLabel('笔记标题').fill('保存期间继续输入');
    await saved(page);
    assert.equal(title(id), '保存期间继续输入');
    assert.equal(await page.getByLabel('笔记标题').inputValue(), '保存期间继续输入');
    pass('delayed save preserves new typing and subsequently saves it');

    failSave = true;
    await page.getByLabel('笔记标题').fill('失败后仍保留');
    await page.getByRole('alert').filter({ hasText: /Regression|503|error|Error/ }).waitFor();
    assert.equal(await page.getByLabel('笔记标题').inputValue(), '失败后仍保留');
    const retry = page.waitForResponse(r => endpoint(r.url()).startsWith('savenote') && r.ok());
    await page.getByRole('button', { name: '保存草稿', exact: true }).click();
    await retry; await saved(page);
    assert.equal(title(id), '失败后仍保留');
    pass('failed save retains content and explicit retry persists it');

    await page.getByRole('button', { name: '发布到前台', exact: true }).click();
    await page.getByRole('button', { name: '更新公开版', exact: true }).waitFor();
    const published = note().published_version;
    await page.getByRole('button', { name: '收录当前版本', exact: true }).click();
    await page.getByRole('button', { name: '移出', exact: true }).waitFor();
    const knowledge = note().knowledge_version;
    await page.getByLabel('笔记标题').fill('只修改工作稿');
    const modified = page.waitForResponse(r => endpoint(r.url()).startsWith('savenote') && r.ok());
    await modified; await saved(page);
    assert.equal(note().published_version, published);
    assert.equal(note().knowledge_version, knowledge);
    pass('autosave leaves public and knowledge snapshots unchanged');

    const restore = page.getByRole('button', { name: `恢复 v${initialVersion}`, exact: true });
    await restore.waitFor();
    delayHistory = true;
    const fetching = page.waitForRequest(r => endpoint(r.url()).startsWith('getownednote'));
    await restore.click(); await fetching;
    await page.getByLabel('笔记标题').fill('历史加载时继续编辑');
    await page.getByRole('alert').filter({ hasText: '恢复期间有新的编辑' }).waitFor();
    assert.equal(await page.getByLabel('笔记标题').inputValue(), '历史加载时继续编辑');
    pass('delayed history fetch cannot overwrite newer local edits');
    await page.getByRole('button', { name: '保存草稿', exact: true }).click(); await saved(page);
    const beforeRestore = note().version;
    await restore.click();
    await page.getByText('历史版本已恢复为新草稿', { exact: true }).waitFor();
    assert.equal(title(id), '第一版');
    assert(note().version > beforeRestore);
    assert.equal(note().published_version, published);
    assert.equal(note().knowledge_version, knowledge);
    pass('history restoration writes a new draft version without publishing');

    // Force a real optimistic-lock conflict by advancing the server revision.
    sql(`INSERT INTO note_revisions(note_id,version,kind,title,content_md,content_html,toc_html,summary,tags)
      SELECT note_id,version+1,kind,'外部修改',content_md,content_html,toc_html,summary,tags FROM note_revisions
      WHERE note_id=${id} AND version=(SELECT version FROM notes WHERE id=${id});
      UPDATE notes SET version=version+1 WHERE id=${id};`);
    await page.getByLabel('笔记标题').fill('本地冲突内容');
    await page.getByRole('alert').filter({ hasText: '笔记已在别处修改' }).waitFor();
    assert.equal(await page.getByLabel('笔记标题').inputValue(), '本地冲突内容');
    assert.equal(title(id), '外部修改');
    pass('real version conflict retains local text and server revision');
    await page.screenshot({ path: path.join(artifacts, 'notes-conflict.png'), fullPage: true });

    await page.reload();
    await page.getByRole('button', { name: '恢复未保存内容', exact: true }).waitFor();
    await page.getByRole('button', { name: '恢复未保存内容', exact: true }).click();
    await saved(page);
    assert.equal(await page.getByLabel('笔记标题').inputValue(), '本地冲突内容');
    pass('reload offers and restores the unsaved session draft');

    await page.evaluate(route => window.__routeTransitions.push(route), `/admin/notes/edit/${id}`);
    await page.locator('#note-tiptap .ProseMirror').waitFor();
    for (const variant of [
      { name: 'desktop-dark', width: 1440, height: 1000, scheme: 'dark', motion: 'no-preference' },
      { name: 'mobile-light', width: 390, height: 844, scheme: 'light', motion: 'reduce' },
    ]) {
      await page.setViewportSize({ width: variant.width, height: variant.height });
      await page.emulateMedia({ colorScheme: variant.scheme, reducedMotion: variant.motion });
      await page.screenshot({ path: path.join(artifacts, `notes-${variant.name}.png`), fullPage: true });
    }
    delaySave = true;
    const leavingSave = page.waitForRequest(r => endpoint(r.url()).startsWith('savenote'));
    await page.getByLabel('笔记标题').fill('离开旧笔记时的请求');
    await leavingSave;
    await page.evaluate(() => window.__routeTransitions.push('/admin/notes/new'));
    await page.waitForURL('**/admin/notes/new');
    await page.locator('#note-tiptap .ProseMirror').waitFor();
    await page.waitForTimeout(1000);
    assert.equal(await page.getByLabel('笔记标题').inputValue(), '');
    assert.equal(await page.locator('#note-tiptap .ProseMirror').textContent(), '');
    pass('leaving during save cannot populate the next editor with an old response');
    assert.deepEqual(report.errors, []);
  } finally {
    fs.writeFileSync(path.join(artifacts, 'notes.json'), JSON.stringify(report, null, 2));
    await browser.close();
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
