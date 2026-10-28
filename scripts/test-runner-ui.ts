#!/usr/bin/env bun
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import type * as Playwright from 'playwright';

const { chromium } = require(process.env.PLAYWRIGHT_MODULE!) as typeof Playwright;
type StreamListener =
  | EventListenerOrEventListenerObject
  | ((this: EventSource, event: MessageEvent<unknown>) => unknown)
  | null;
interface StreamRecord {
  closed: boolean;
  listeners: Set<StreamListener>;
  stdout: string;
  stderr: string;
}
type ScriptWindow = Window &
  typeof globalThis & {
    __runnerStreams: StreamRecord[];
    __runnerFailSSE: boolean;
    __runnerPollOutput: string;
    __routeTransitions: { push(route: string): void };
    CodeMirrorEditor: { _instances: Map<string, { setValue(source: string): void }> };
    XtermTerminal: { _instances: Map<string, { writeAll(stdout: string, stderr: string): void }> };
  };
const base = process.env.ADMIN_BASE!;
const artifacts = process.env.REGRESSION_ARTIFACT_DIR!;
const report: { passed: string[]; errors: string[] } = { passed: [], errors: [] };
const pass = (name: string) => {
  report.passed.push(name);
  console.log('PASS', name);
};
async function main() {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  await context.addCookies([{ name: 'session', value: process.env.ADMIN_SESSION!, url: base }]);
  await context.addInitScript(() => {
    (window as ScriptWindow).__runnerStreams = [];
    const Original = (window as ScriptWindow).EventSource;
    (window as ScriptWindow).EventSource = class extends Original {
      record: StreamRecord;
      constructor(url: string | URL, ...args: [EventSourceInit?]) {
        if ((window as ScriptWindow).__runnerFailSSE && String(url).includes('/api/exec/stream'))
          throw new Error('Regression SSE constructor failure');
        super(url, ...args);
        this.record = { closed: false, listeners: new Set(), stdout: '', stderr: '' };
        (window as ScriptWindow).__runnerStreams.push(this.record);
        super.addEventListener(
          'stdout',
          (e) => (this.record.stdout += (e as MessageEvent<string>).data),
        );
        super.addEventListener(
          'stderr',
          (e) => (this.record.stderr += (e as MessageEvent<string>).data),
        );
      }
      override addEventListener(
        name: string,
        callback: StreamListener,
        ...args: [(boolean | AddEventListenerOptions)?]
      ) {
        this.record.listeners.add(callback);
        return super.addEventListener(name, callback as EventListener, ...args);
      }
      override removeEventListener(
        name: string,
        callback: StreamListener,
        ...args: [(boolean | EventListenerOptions)?]
      ) {
        this.record.listeners.delete(callback);
        return super.removeEventListener(name, callback as EventListener, ...args);
      }
      override close() {
        this.record.closed = true;
        return super.close();
      }
    };
  });
  const page = await context.newPage();
  page.setDefaultTimeout(45000);
  page.on('pageerror', (e) => report.errors.push(e.message));
  await page.route('https://fonts.googleapis.com/**', (r) => r.fulfill({ body: '' }));
  async function code(source: string) {
    await page.evaluate(
      (source) =>
        (window as ScriptWindow).CodeMirrorEditor._instances
          .values()
          .next()
          .value!.setValue(source),
      source,
    );
  }
  const closed = () =>
    page.waitForFunction(() =>
      (window as ScriptWindow).__runnerStreams.every((s) => s.closed && s.listeners.size === 0),
    );
  // 跑完后按钮变成「再次运行」；状态先进入运行中（is-live），再落到结果标签。
  const run = page.getByRole('button', { name: /^(运行|再次运行)$/ });
  const status = page.locator('.runner-status');
  async function runUntil(label: string) {
    await run.click();
    await page.locator('.runner-status.is-live').waitFor();
    await status.filter({ hasText: new RegExp(`^${label}$`) }).waitFor();
  }
  try {
    await page.goto(base + '/admin/runner');
    await page.locator('.cm-editor').waitFor();
    await code(
      'print("regression stdout")\nimport sys\nprint("regression stderr", file=sys.stderr)',
    );
    await runUntil('成功');
    await closed();
    assert(
      await page.evaluate(() =>
        (window as ScriptWindow).__runnerStreams.at(-1)!.stdout.includes('regression stdout'),
      ),
    );
    assert(
      await page.evaluate(() =>
        (window as ScriptWindow).__runnerStreams.at(-1)!.stderr.includes('regression stderr'),
      ),
    );
    pass('real Python container streams stdout/stderr and releases all SSE callbacks on success');

    await code('raise RuntimeError("regression nonzero")');
    await runUntil('运行错误');
    await closed();
    pass('nonzero exit displays failure and releases the stream');

    await code('import time\ntime.sleep(8)');
    await runUntil('超时');
    await closed();
    pass('real container timeout produces a terminal state and releases the stream');

    await page.evaluate(() => {
      (window as ScriptWindow).__runnerFailSSE = true;
      (window as ScriptWindow).__runnerPollOutput = '';
      const terminal = (window as ScriptWindow).XtermTerminal._instances.values().next().value!;
      const write = terminal.writeAll.bind(terminal);
      terminal.writeAll = (stdout, stderr) => {
        (window as ScriptWindow).__runnerPollOutput = stdout + stderr;
        write(stdout, stderr);
      };
    });
    await code('print("poll fallback")');
    await runUntil('成功');
    assert(
      await page.evaluate(() =>
        (window as ScriptWindow).__runnerPollOutput.includes('poll fallback'),
      ),
    );
    await page.evaluate(() => {
      (window as ScriptWindow).__runnerFailSSE = false;
    });
    pass('SSE construction failure falls back to real result polling and writes terminal output');
    await page.screenshot({ path: path.join(artifacts, 'runner-success.png'), fullPage: true });

    await code('import time\nprint("before leaving", flush=True)\ntime.sleep(3)');
    await run.click();
    await page.waitForFunction(() =>
      (window as ScriptWindow).__runnerStreams.some((s) => !s.closed),
    );
    await page.evaluate(() => (window as ScriptWindow).__routeTransitions.push('/admin/notes'));
    await page.getByLabel('快速记录').waitFor();
    await closed();
    assert.deepEqual(
      await page.evaluate(() => [
        (window as ScriptWindow).CodeMirrorEditor._instances.size,
        (window as ScriptWindow).XtermTerminal._instances.size,
      ]),
      [0, 0],
    );
    await page.waitForTimeout(3500);
    pass('leaving mid-run closes SSE and destroys editor/terminal without late updates');

    await page.evaluate(() => (window as ScriptWindow).__routeTransitions.push('/admin/runner'));
    await page.locator('.cm-editor').waitFor();
    await code('print("after re-entry")');
    await runUntil('成功');
    await closed();
    assert.equal(
      await page.evaluate(() => (window as ScriptWindow).CodeMirrorEditor._instances.size),
      1,
    );
    assert.equal(
      await page.evaluate(() => (window as ScriptWindow).XtermTerminal._instances.size),
      1,
    );
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
main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
