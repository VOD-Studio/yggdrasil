#!/usr/bin/env bun
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE);
const base = process.env.ADMIN_BASE;
const artifacts = process.env.REGRESSION_ARTIFACT_DIR;
async function main() {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  await context.addCookies([{ name: 'session', value: process.env.ADMIN_SESSION, url: base }]);
  const page = await context.newPage();
  page.setDefaultTimeout(30000);
  const report = { passed: [], errors: [] };
  const pass = name => { report.passed.push(name); console.log('PASS', name); };
  page.on('pageerror', e => report.errors.push(e.message));
  await page.route('https://fonts.googleapis.com/**', r => r.fulfill({ body: '' }));
  const modal = page.locator('.tiptap-runnable-modal');
  try {
    await page.goto(base + '/admin/write/1');
    const editor = page.locator('#tiptap-editor .ProseMirror');
    await editor.waitFor();
    await editor.fill('');
    await editor.pressSequentially('/runnable');
    await page.locator('.slash-command-item').filter({ hasText: '可运行代码块' }).waitFor();
    await page.keyboard.press('Enter');
    await modal.waitFor();
    await page.waitForTimeout(100);
    assert(await modal.isVisible());
    pass('slash search and Enter open the actual modal without immediately submitting it');
    await page.locator('#runnable-timeout').fill('7');
    await modal.getByRole('button', { name: '插入', exact: true }).click();
    await modal.waitFor({ state: 'detached' });
    let markdown = await page.evaluate(() => window.TiptapEditor._instances.get('tiptap-editor').getMarkdown());
    assert(markdown.includes('python runnable {"timeout_secs":7'));
    pass('inserting a runnable block serializes the configured timeout');
    const edit = page.getByTitle('点击修改语言与运行配置');
    await edit.click(); await modal.waitFor();
    assert.equal(await page.locator('#runnable-timeout').inputValue(), '7');
    await page.locator('#runnable-timeout').fill('9');
    await modal.getByRole('button', { name: '保存', exact: true }).click();
    markdown = await page.evaluate(() => window.TiptapEditor._instances.get('tiptap-editor').getMarkdown());
    assert(markdown.includes('"timeout_secs":9'));
    pass('editing the block changes its existing runnable configuration');
    for (let i = 0; i < 4; i++) {
      await edit.click(); await modal.waitFor();
      // Flush the previous editor's deferred focus as well as the modal key listener.
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert(await modal.evaluate(element => element.contains(document.activeElement)),
        'reopening retains keyboard focus inside the modal');
      await page.keyboard.press('Escape');
      await modal.waitFor({ state: 'detached' });
    }
    pass('repeated opening and Escape leave no modal overlays');
    await edit.click(); await modal.waitFor();
    await page.screenshot({ path: path.join(artifacts, 'runnable-modal-light.png'), fullPage: true });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' });
    await page.screenshot({ path: path.join(artifacts, 'runnable-modal-mobile-dark.png'), fullPage: true });
    page.once('dialog', dialog => dialog.accept());
    await page.evaluate(() => window.__routeTransitions.push('/admin/posts'));
    await page.waitForURL('**/admin/posts');
    await modal.waitFor({ state: 'detached' });
    await page.waitForFunction(() => window.TiptapEditor._instances.size === 0);
    await page.keyboard.press('Escape');
    assert.deepEqual(report.errors, []);
    pass('unmounting the editor closes its modal and leaves no late keyboard callback');
  } finally {
    fs.writeFileSync(path.join(artifacts, 'slash-menu.json'), JSON.stringify(report, null, 2));
    await browser.close();
  }
}
main().catch(e => { console.error(e); process.exitCode = 1; });
