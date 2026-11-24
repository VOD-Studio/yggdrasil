#!/usr/bin/env bun
/**
 * 运行：make regression-build && make test-browser BROWSER_SUITE=notes
 * 由 scripts/regression.ts 启动隔离 PostgreSQL、应用和管理员 session，准备 Bun、
 * Playwright Chromium 与 psql；不连接生产数据库。直接运行时必须提供同样的
 * ADMIN_BASE / ADMIN_SESSION / PLAYWRIGHT_MODULE / REGRESSION_ARTIFACT_DIR，
 * 以及指向一次性 ygg_browser_test 的 PG* 环境，应用 APP_BASE_URL 必须与 ADMIN_BASE 同源。
 * 批量回归只选择本次创建的两条笔记，并在 finally 中按准确 ID 清理。
 * Real database saves, with delays/failures injected only where named in the report.
 */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import type * as Playwright from 'playwright';
import { loadPlaywright } from './libs-modules.ts';

const { chromium } = loadPlaywright();
type ScriptWindow = Window &
  typeof globalThis & { __routeTransitions: { push(route: string): void } };
interface Note {
  id: number;
  version: number;
  published_version: number | null;
  knowledge_version: number | null;
}
const base = process.env.ADMIN_BASE!;
const artifacts = process.env.REGRESSION_ARTIFACT_DIR!;
const report: {
  passed: string[];
  errors: string[];
  requests: { name: string; body: string | null }[];
} = { passed: [], errors: [], requests: [] };
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
const note = (): Note =>
  JSON.parse(sql('SELECT row_to_json(n) FROM notes n ORDER BY id DESC LIMIT 1'));
const title = (id: number) =>
  sql(
    `SELECT r.title FROM note_revisions r JOIN notes n ON n.id=r.note_id AND n.version=r.version WHERE n.id=${id}`,
  );
const pass = (name: string) => {
  report.passed.push(name);
  console.log('PASS', name);
};
// 页头保存状态：已保存且没有新的修改。
async function saved(page: Playwright.Page) {
  await page.locator('.write-status[data-state="saved"]').waitFor();
}

async function batchNotes(page: Playwright.Page) {
  const fixtures: Note[] = [];
  const prefix = `批量回归-${crypto.randomUUID()}`;
  const titles = [`${prefix}-甲`, `${prefix}-乙`];
  const api = async (name: string, body: unknown) => {
    const response = await page.request.post(`${base}/api/${name}`, {
      headers: { Origin: base },
      data: body,
    });
    assert(response.ok(), `${name}: ${response.status()} ${await response.text()}`);
    return response.json();
  };
  const read = async (id: number): Promise<Note> => api('get_owned_note', { id, version: null });
  const selection = (index: number) =>
    page.getByRole('checkbox', { name: `选择笔记「${titles[index]}」`, exact: true });
  const toolbar = page.getByRole('toolbar', { name: '笔记批量操作' });
  const search = page.getByPlaceholder('搜索标题、正文或标签');
  const selectAll = page.getByRole('checkbox', { name: '全选本页笔记', exact: true });
  const selectFixtures = async () => {
    await selection(0).waitFor();
    await selection(1).waitFor();
    await selectAll.check();
    assert(await selection(0).isChecked());
    assert(await selection(1).isChecked());
  };
  const act = async (label: string, confirm?: string) => {
    const response = page.waitForResponse((r) => endpoint(r.url()) === 'batchactonnotes');
    const refreshed = page.waitForResponse((r) => endpoint(r.url()) === 'listownednotes' && r.ok());
    await toolbar.getByRole('button', { name: label, exact: true }).click();
    if (confirm) {
      const dialog = page.getByRole('alertdialog');
      await dialog.waitFor();
      assert.match(await dialog.innerText(), /当前工作稿/);
      // 打开 ConfirmDialog 不得提前发送批量写入。
      assert.equal(
        report.requests.filter((r) => r.name === 'batchactonnotes').length,
        writesBefore,
      );
      await dialog.getByRole('button', { name: confirm, exact: true }).click();
    }
    assert((await response).ok(), `批量操作失败：${label}`);
    await refreshed;
    await toolbar.waitFor({ state: 'hidden' });
    assert.equal(await selection(0).isChecked(), false);
    assert.equal(await selection(1).isChecked(), false);
  };
  let writesBefore = 0;
  try {
    for (const fixtureTitle of titles) {
      fixtures.push(
        await api('save_note', {
          draft: {
            id: null,
            expected_version: null,
            kind: 'topic',
            title: fixtureTitle,
            content_md: `隔离笔记正文 ${prefix}`,
            tags: [],
            notebook_ids: [],
          },
        }),
      );
    }
    await page.goto(`${base}/admin/notes`);
    const filtered = page.waitForResponse(
      (r) =>
        endpoint(r.url()) === 'listownednotes' &&
        r.ok() &&
        r.request().postDataJSON().filter.query === prefix,
    );
    await search.fill(prefix);
    await filtered;
    await selection(0).waitFor();
    await selection(1).waitFor();
    // 精确限定本次 fixture，避免全选影响其他笔记。
    assert.equal(await page.getByRole('checkbox', { name: /^选择笔记「/ }).count(), 2);
    await selection(0).check();
    await toolbar.getByRole('button', { name: '取消选择', exact: true }).click();
    assert.equal(await selection(0).isChecked(), false);
    await toolbar.waitFor({ state: 'hidden' });
    pass('batch cancel selection clears checked notes');

    await selection(0).check();
    await search.fill(titles[1]!);
    await selection(0).waitFor({ state: 'hidden' });
    assert.equal(await selection(1).isChecked(), false);
    await toolbar.waitFor({ state: 'hidden' });
    await search.fill(prefix);
    await selectFixtures();
    writesBefore = report.requests.filter((r) => r.name === 'batchactonnotes').length;
    await act('设为公开', '确认公开');
    for (const fixture of fixtures) {
      const current = await read(fixture.id);
      assert.equal(current.published_version, current.version);
      assert.equal(current.knowledge_version, null);
    }
    pass('batch publish confirms current drafts and resets selection');

    await selectFixtures();
    writesBefore = report.requests.filter((r) => r.name === 'batchactonnotes').length;
    await act('收录到 AI 知识库', '确认收录');
    for (const fixture of fixtures) {
      const current = await read(fixture.id);
      assert.equal(current.knowledge_version, current.version);
      assert.equal(current.published_version, current.version);
    }
    await selectFixtures();
    await act('设为私密');
    for (const fixture of fixtures) {
      const current = await read(fixture.id);
      assert.equal(current.published_version, null);
      assert.equal(current.knowledge_version, current.version);
    }
    pass('batch private leaves knowledge snapshots intact');

    // 再公开后移出知识库，以非空公开版本证明两个状态互不影响。
    await selectFixtures();
    writesBefore = report.requests.filter((r) => r.name === 'batchactonnotes').length;
    await act('设为公开', '确认公开');
    await selectFixtures();
    await act('移出 AI 知识库');
    for (const fixture of fixtures) {
      const current = await read(fixture.id);
      assert.equal(current.knowledge_version, null);
      assert.equal(current.published_version, current.version);
    }
    pass('batch exclusion leaves public snapshots intact');
    pass('search filter clears page-local batch selection');
  } finally {
    if (fixtures.length) {
      sql(`DELETE FROM notes WHERE id IN (${fixtures.map((fixture) => fixture.id).join(',')})`);
    }
  }
}

async function batchNotebooks(page: Playwright.Page) {
  const prefix = `目录回归-${crypto.randomUUID()}`;
  const bookTitle = `${prefix} 笔记本`;
  const title = (index: number) => `${prefix} ${String(index).padStart(2, '0')}`;
  const open = page.getByRole('button', { name: '添加笔记', exact: true });
  const dialog = page.getByRole('dialog', { name: '添加笔记' });
  const search = dialog.getByRole('searchbox', { name: '搜索标题、正文或标签' });
  const noteRow = (index: number) => dialog.getByText(title(index), { exact: true });
  const pick = (index: number) =>
    dialog.getByRole('checkbox', { name: `选择笔记「${title(index)}」` });
  const add = dialog.getByRole('button', { name: '加入', exact: true });
  const response = () => page.waitForResponse((r) => endpoint(r.url()) === 'addnotebooknotes');
  const api = async (name: string, body: unknown) => {
    const res = await page.request.post(`${base}/api/${name}`, {
      data: body,
      headers: { Origin: base },
    });
    assert(res.ok(), `${name}: ${res.status()} ${await res.text()}`);
    return res.json();
  };

  try {
    sql(`WITH fixture AS (
      INSERT INTO notes(owner_id,slug,version)
      SELECT 1, '${prefix}-' || lpad(i::text,2,'0'), 1 FROM generate_series(1,24) AS g(i)
      ORDER BY i RETURNING id,slug
    )
    INSERT INTO note_revisions(note_id,version,kind,title,content_md,content_html)
    SELECT id,1,CASE WHEN right(slug,2)='01' THEN 'moment' ELSE 'topic' END,
      '${prefix} ' || right(slug,2), '正文', '<p>正文</p>' FROM fixture`);
    const ids: number[] = JSON.parse(
      sql(`SELECT json_agg(id ORDER BY slug) FROM notes WHERE slug LIKE '${prefix}-%'`),
    );
    const idsFor = (...indices: number[]) => indices.map((index) => ids[index - 1]!);
    await page.emulateMedia({ colorScheme: 'light', reducedMotion: 'no-preference' });
    await page.goto(`${base}/admin/notebooks`);
    await page.getByRole('heading', { name: '还没有笔记本' }).waitFor();
    await page.locator('.admin-page-actions').getByRole('button', { name: '新建笔记本' }).click();
    await page.getByLabel('名称', { exact: true }).fill(bookTitle);
    await page.getByRole('button', { name: '创建笔记本' }).click();
    await page.getByText('这个笔记本还没有笔记。').waitFor();
    await page.getByRole('switch', { name: '公开展示这个笔记本' }).click();
    await page.getByRole('button', { name: '保存', exact: true }).click();
    await page.getByRole('button', { name: '已保存', exact: true }).waitFor();
    const bookId = Number(sql(`SELECT id FROM notebooks WHERE title='${bookTitle}'`));
    const storedOrder = () =>
      sql(
        `SELECT string_agg(note_id::text, ',' ORDER BY position) FROM notebook_notes WHERE notebook_id=${bookId}`,
      );
    const directory = page.locator('.admin-notebook-row-title');

    await open.click();
    await search.fill(prefix);
    await pick(24).waitFor();
    await dialog.getByText('访客可在目录中看到已发布的笔记。').waitFor();
    assert(await add.isDisabled());
    await noteRow(24).click();
    assert(await pick(24).isChecked());
    const selectPage = dialog.getByRole('checkbox', { name: '全选本页可加入的笔记' });
    assert(await selectPage.evaluate((input: HTMLInputElement) => input.indeterminate));
    await selectPage.check();
    await dialog.getByText('已选 20 篇', { exact: true }).waitFor();
    await selectPage.uncheck();
    await pick(24).check();
    await pick(23).check();
    await page.screenshot({
      path: path.join(artifacts, 'notebook-picker-desktop.png'),
      animations: 'disabled',
    });
    const initial = response();
    await add.click();
    assert.equal((await (await initial).json()).added, 2);
    await dialog.waitFor({ state: 'hidden' });
    assert.deepEqual(await directory.allTextContents(), [title(24), title(23)]);
    assert(await open.evaluate((button) => button === document.activeElement));
    await page.getByRole('button', { name: `上移「${title(23)}」` }).click();

    await open.click();
    await search.fill(prefix);
    await noteRow(24).waitFor();
    const joined = dialog.locator('li').filter({ hasText: title(24) });
    assert.match((await joined.textContent()) ?? '', /已加入/);
    await dialog.getByRole('radio', { name: '未加入', exact: true }).click();
    await pick(22).check();
    assert.equal(await joined.count(), 0);
    await dialog.getByRole('button', { name: '下一页', exact: true }).click();
    await pick(1).focus();
    await page.keyboard.press('Space');
    assert(await pick(1).isChecked());
    await search.fill(title(3));
    await pick(3).waitFor();
    await dialog.getByRole('radio', { name: '已选', exact: true }).click();
    assert.deepEqual(await dialog.locator('.notebook-picker-row-title').allTextContents(), [
      title(22),
      title(1),
    ]);

    // 选择后被另一页面移入回收站的笔记让整批失败，恢复后可用原勾选重试。
    const [last, staleEditor] = idsFor(1, 22);
    const trashed: Note = await api('act_on_note', {
      id: last,
      expected_version: 1,
      action: 'trash',
    });
    const failed = response();
    await add.click();
    assert.equal((await failed).ok(), false);
    await dialog
      .getByRole('alert')
      .getByText(/已移入回收站/)
      .waitFor();
    assert.equal(storedOrder(), idsFor(24, 23).join(','));
    assert(await pick(22).isChecked());
    assert(await pick(1).isChecked());
    await api('act_on_note', { id: last, expected_version: trashed.version, action: 'restore' });

    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' });
    await page.screenshot({
      path: path.join(artifacts, 'notebook-picker-mobile.png'),
      animations: 'disabled',
    });
    const retried = response();
    await add.click();
    assert.equal((await (await retried).json()).added, 2);
    await dialog.waitFor({ state: 'hidden' });
    assert.deepEqual(await directory.allTextContents(), [
      title(23),
      title(24),
      title(22),
      title(1),
    ]);
    await page.locator('.admin-notebook-item-meta').filter({ hasText: /4 篇/ }).waitFor();
    assert.equal(storedOrder(), idsFor(24, 23, 22, 1).join(','));
    await page.getByRole('button', { name: '保存', exact: true }).click();
    await page.getByRole('button', { name: '已保存', exact: true }).waitFor();
    assert.equal(storedOrder(), idsFor(23, 24, 22, 1).join(','));

    const savedNote = await api('save_note', {
      before_notebooks: [],
      draft: {
        id: staleEditor,
        expected_version: 1,
        kind: 'topic',
        title: title(22),
        content_md: '从较早打开的编辑器保存正文',
        notebook_ids: [],
      },
    });
    assert.deepEqual(savedNote.notebook_ids, [bookId]);
    const duplicate = await api('add_notebook_notes', { id: bookId, note_ids: idsFor(22, 1) });
    assert.equal(duplicate.added, 0);
    assert.equal(storedOrder(), idsFor(23, 24, 22, 1).join(','));
    pass('notebook picker preserves cross-page selection and directory edits on batch add');
  } finally {
    sql(
      `DELETE FROM notebooks WHERE title='${bookTitle}'; DELETE FROM notes WHERE slug LIKE '${prefix}-%'`,
    );
  }
}

async function main() {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  await context.addCookies([{ name: 'session', value: process.env.ADMIN_SESSION!, url: base }]);
  const page = await context.newPage();
  page.setDefaultTimeout(30000);
  page.on('pageerror', (e) => report.errors.push(e.message));
  page.on('dialog', (dialog) => dialog.accept());
  let delaySave = false,
    failSave = false,
    delayHistory = false;
  await page.route('https://fonts.googleapis.com/**', (r) => r.fulfill({ body: '' }));
  await page.route('**/api/**', async (route) => {
    const name = endpoint(route.request().url());
    report.requests.push({ name, body: route.request().postData() });
    if (name.startsWith('savenote')) {
      if (failSave) {
        failSave = false;
        await route.fulfill({ status: 503, body: 'Regression save failure' });
        return;
      }
      if (delaySave) {
        delaySave = false;
        await new Promise((resolve) => setTimeout(resolve, 700));
      }
    }
    if (name.startsWith('getownednote') && delayHistory) {
      delayHistory = false;
      await new Promise((resolve) => setTimeout(resolve, 700));
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
    const started = page.waitForRequest((r) => endpoint(r.url()).startsWith('savenote'));
    await page.getByLabel('笔记标题').fill('提交中的版本');
    await started;
    await page.getByLabel('笔记标题').fill('保存期间继续输入');
    await saved(page);
    assert.equal(title(id), '保存期间继续输入');
    assert.equal(await page.getByLabel('笔记标题').inputValue(), '保存期间继续输入');
    pass('delayed save preserves new typing and subsequently saves it');

    failSave = true;
    await page.getByLabel('笔记标题').fill('失败后仍保留');
    await page.getByRole('alert').waitFor();
    assert.equal(await page.getByLabel('笔记标题').inputValue(), '失败后仍保留');
    const retry = page.waitForResponse((r) => endpoint(r.url()).startsWith('savenote') && r.ok());
    await page.getByRole('button', { name: '保存草稿', exact: true }).click();
    await retry;
    await saved(page);
    assert.equal(title(id), '失败后仍保留');
    pass('failed save retains content and explicit retry persists it');

    await page.getByRole('button', { name: '发布到前台', exact: true }).click();
    await page.getByRole('button', { name: '更新公开版', exact: true }).waitFor();
    const published = note().published_version;
    await page.getByRole('button', { name: '收录当前版本', exact: true }).click();
    await page.getByRole('button', { name: '移出', exact: true }).waitFor();
    const knowledge = note().knowledge_version;
    await page.getByLabel('笔记标题').fill('只修改工作稿');
    const modified = page.waitForResponse(
      (r) => endpoint(r.url()).startsWith('savenote') && r.ok(),
    );
    await modified;
    await saved(page);
    assert.equal(note().published_version, published);
    assert.equal(note().knowledge_version, knowledge);
    pass('autosave leaves public and knowledge snapshots unchanged');

    const restore = page.getByRole('button', { name: `恢复 v${initialVersion}`, exact: true });
    await restore.waitFor();
    delayHistory = true;
    const fetching = page.waitForRequest((r) => endpoint(r.url()).startsWith('getownednote'));
    await restore.click();
    await fetching;
    await page.getByLabel('笔记标题').fill('历史加载时继续编辑');
    await page.getByRole('alert').filter({ hasText: '恢复期间有新的编辑' }).waitFor();
    assert.equal(await page.getByLabel('笔记标题').inputValue(), '历史加载时继续编辑');
    pass('delayed history fetch cannot overwrite newer local edits');
    await page.getByRole('button', { name: '保存草稿', exact: true }).click();
    await saved(page);
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

    await page.evaluate(
      (route) => (window as ScriptWindow).__routeTransitions.push(route),
      `/admin/notes/edit/${id}`,
    );
    await page.locator('#note-tiptap .ProseMirror').waitFor();
    for (const variant of [
      { name: 'desktop-dark', width: 1440, height: 1000, scheme: 'dark', motion: 'no-preference' },
      { name: 'mobile-light', width: 390, height: 844, scheme: 'light', motion: 'reduce' },
    ] as const) {
      await page.setViewportSize({ width: variant.width, height: variant.height });
      await page.emulateMedia({ colorScheme: variant.scheme, reducedMotion: variant.motion });
      await page.screenshot({
        path: path.join(artifacts, `notes-${variant.name}.png`),
        fullPage: true,
      });
    }
    delaySave = true;
    const leavingSave = page.waitForRequest((r) => endpoint(r.url()).startsWith('savenote'));
    await page.getByLabel('笔记标题').fill('离开旧笔记时的请求');
    await leavingSave;
    await page.evaluate(() => (window as ScriptWindow).__routeTransitions.push('/admin/notes/new'));
    await page.waitForURL('**/admin/notes/new');
    await page.locator('#note-tiptap .ProseMirror').waitFor();
    await page.waitForTimeout(1000);
    assert.equal(await page.getByLabel('笔记标题').inputValue(), '');
    assert.equal(await page.locator('#note-tiptap .ProseMirror').textContent(), '');
    pass('leaving during save cannot populate the next editor with an old response');
    await page.setViewportSize({ width: 1440, height: 1000 });
    await batchNotes(page);
    await batchNotebooks(page);
    assert.deepEqual(report.errors, []);
  } finally {
    fs.writeFileSync(path.join(artifacts, 'notes.json'), JSON.stringify(report, null, 2));
    await browser.close();
  }
}
main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
