#!/usr/bin/env bun
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE);
const base = process.env.ADMIN_BASE;
const artifacts = process.env.REGRESSION_ARTIFACT_DIR;
const report = { passed: [], errors: [] };
const pass = name => { report.passed.push(name); console.log('PASS', name); };
async function main() {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  await context.addCookies([{ name: 'session', value: process.env.ADMIN_SESSION, url: base }]);
  await context.addInitScript(() => {
    window.__runnerStreams = [];
    const Original = window.EventSource;
    window.EventSource = class extends Original {
      constructor(url, ...args) {
        if (window.__runnerFailSSE && String(url).includes('/api/exec/stream')) throw new Error('Regression SSE constructor failure');
        super(url, ...args);
        this.record = { closed: false, listeners: new Set(), stdout: '', stderr: '' };
        window.__runnerStreams.push(this.record);
        super.addEventListener('stdout', e => this.record.stdout += e.data);
        super.addEventListener('stderr', e => this.record.stderr += e.data);
      }
      addEventListener(name, callback, ...args) { this.record.listeners.add(callback); return super.addEventListener(name, callback, ...args); }
      removeEventListener(name, callback, ...args) { this.record.listeners.delete(callback); return super.removeEventListener(name, callback, ...args); }
      close() { this.record.closed = true; return super.close(); }
    };
  });
  const page = await context.newPage();
  page.setDefaultTimeout(45000);
  page.on('pageerror', e => report.errors.push(e.message));
  await page.route('https://fonts.googleapis.com/**', r => r.fulfill({ body: '' }));
  async function code(source) {
    await page.evaluate(source => window.CodeMirrorEditor._instances.values().next().value.setValue(source), source);
  }
  const closed = () => page.waitForFunction(() => window.__runnerStreams.every(s => s.closed && s.listeners.size === 0));
  const run = page.getByRole('button', { name: '运行', exact: true });
  try {
    await page.goto(base + '/admin/runner');
    await page.locator('.cm-editor').waitFor();
    await code('print("regression stdout")\nimport sys\nprint("regression stderr", file=sys.stderr)');
    await run.click();
    await page.getByText(/耗时:.*状态: 成功/).waitFor(); await closed();
    assert(await page.evaluate(() => window.__runnerStreams.at(-1).stdout.includes('regression stdout')));
    assert(await page.evaluate(() => window.__runnerStreams.at(-1).stderr.includes('regression stderr')));
    pass('real Python container streams stdout/stderr and releases all SSE callbacks on success');

    await code('raise RuntimeError("regression nonzero")');
    await run.click(); await page.getByText(/耗时:.*运行错误/).waitFor(); await closed();
    pass('nonzero exit displays failure and releases the stream');

    await code('import time\ntime.sleep(8)');
    await run.click(); await page.getByText(/耗时:.*状态: 超时/).waitFor(); await closed();
    pass('real container timeout produces a terminal state and releases the stream');

    await page.evaluate(() => {
      window.__runnerFailSSE = true;
      window.__runnerPollOutput = '';
      const terminal = window.XtermTerminal._instances.values().next().value;
      const write = terminal.writeAll.bind(terminal);
      terminal.writeAll = (stdout, stderr) => { window.__runnerPollOutput = stdout + stderr; write(stdout, stderr); };
    });
    await code('print("poll fallback")');
    await run.click(); await page.getByText(/耗时:.*状态: 成功/).waitFor();
    assert(await page.evaluate(() => window.__runnerPollOutput.includes('poll fallback')));
    await page.evaluate(() => { window.__runnerFailSSE = false; });
    pass('SSE construction failure falls back to real result polling and writes terminal output');
    await page.screenshot({ path: path.join(artifacts, 'runner-success.png'), fullPage: true });

    await code('import time\nprint("before leaving", flush=True)\ntime.sleep(3)');
    await run.click();
    await page.waitForFunction(() => window.__runnerStreams.some(s => !s.closed));
    await page.evaluate(() => window.__routeTransitions.push('/admin/notes'));
    await page.getByLabel('快速记录').waitFor(); await closed();
    assert.deepEqual(await page.evaluate(() => [window.CodeMirrorEditor._instances.size, window.XtermTerminal._instances.size]), [0, 0]);
    await page.waitForTimeout(3500);
    pass('leaving mid-run closes SSE and destroys editor/terminal without late updates');

    await page.evaluate(() => window.__routeTransitions.push('/admin/runner'));
    await page.locator('.cm-editor').waitFor();
    await code('print("after re-entry")');
    await run.click(); await page.getByText(/耗时:.*状态: 成功/).waitFor(); await closed();
    assert.equal(await page.evaluate(() => window.CodeMirrorEditor._instances.size), 1);
    assert.equal(await page.evaluate(() => window.XtermTerminal._instances.size), 1);
    pass('re-entry creates one editor and terminal and can execute again');
    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' });
    await page.screenshot({ path: path.join(artifacts, 'runner-mobile-dark.png'), fullPage: true });
    assert.deepEqual(report.errors, []);
  } finally {
    fs.writeFileSync(path.join(artifacts, 'runner.json'), JSON.stringify(report, null, 2));
    await browser.close();
  }
}
main().catch(e => { console.error(e); process.exitCode = 1; });
