import { expect, test } from 'bun:test';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  symlinkSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { brotliDecompressSync } from 'node:zlib';

function precompress(site: string, cache: string) {
  return Bun.spawnSync([process.execPath, join(import.meta.dir, 'precompress.ts'), site], {
    env: { ...process.env, PATH: '', YGG_PRECOMPRESS_CACHE_DIR: cache },
    stdout: 'pipe',
    stderr: 'pipe',
  });
}

test('发布预压缩覆盖静态资源，保留原件与修改时间', () => {
  const root = mkdtempSync(join(tmpdir(), 'ygg-precompress-'));
  try {
    const site = join(root, 'public');
    const outside = join(root, 'outside');
    mkdirSync(join(site, '含空格的 目录'), { recursive: true });
    mkdirSync(outside);
    const assets = [
      '含空格的 目录/app.js',
      'empty.css',
      'entry.wasm',
      'icon.svg',
      'index.html',
      'data.json',
      'feed.xml',
      'manifest.webmanifest',
    ];
    const ignored = ['app.js.map', 'entry.d.ts', 'icon.png', 'font.woff2'];
    const mtime = new Date('2026-01-01T00:00:00Z');
    for (const file of [...assets, ...ignored]) {
      const path = join(site, file);
      writeFileSync(path, file === 'empty.css' ? '' : '原文件内容');
      utimesSync(path, mtime, mtime);
    }
    writeFileSync(join(outside, 'external.css'), '目录外的文件');
    symlinkSync(outside, join(site, 'link'));
    symlinkSync(join(site, 'empty.css'), join(site, 'link.css'));
    writeFileSync(join(site, 'index.html.br'), '旧旁车');

    const run = () => precompress(site, join(root, 'cache'));
    const result = run();
    expect(result.stderr.toString()).toBe('');
    expect(result.exitCode).toBe(0);
    for (const file of assets) {
      const path = join(site, file);
      expect(readFileSync(path, 'utf8')).toBe(file === 'empty.css' ? '' : '原文件内容');
      expect(brotliDecompressSync(readFileSync(`${path}.br`))).toEqual(readFileSync(path));
      expect(statSync(`${path}.br`).mtimeMs).toBe(mtime.getTime());
    }
    for (const file of [...ignored, 'link.css', 'link/external.css']) {
      expect(existsSync(join(site, `${file}.br`))).toBe(false);
    }

    const blocked = join(site, 'index.html.br');
    rmSync(blocked);
    mkdirSync(blocked);
    const failed = run();
    expect(failed.exitCode).not.toBe(0);
    expect(failed.stderr.toString()).toMatch(/预压缩失败.*index\.html/);
    const files = readdirSync(site, { recursive: true, encoding: 'utf8' });
    expect(files.some((file) => file.endsWith('.tmp'))).toBe(false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('跨发布目录复用旁车，文件内容变化时重新压缩', () => {
  const root = mkdtempSync(join(tmpdir(), 'ygg-precompress-cache-'));
  try {
    const first = join(root, 'first');
    const second = join(root, 'second');
    const cache = join(root, 'cache');
    mkdirSync(first);
    mkdirSync(second);
    writeFileSync(join(first, 'a.js'), 'aaa');
    writeFileSync(join(first, 'b.js'), 'aaa');
    const cold = precompress(first, cache);
    expect(cold.exitCode).toBe(0);
    expect(cold.stdout.toString()).toContain('新压缩 1 个，复用 1 个');

    const file = join(second, 'c.js');
    writeFileSync(file, 'aaa');
    const mtime = new Date('2026-02-01T00:00:00Z');
    utimesSync(file, mtime, mtime);
    const warm = precompress(second, cache);
    expect(warm.exitCode).toBe(0);
    expect(warm.stdout.toString()).toContain('新压缩 0 个，复用 1 个');
    expect(readFileSync(`${file}.br`)).toEqual(readFileSync(join(first, 'a.js.br')));
    expect(statSync(`${file}.br`).mtimeMs).toBe(mtime.getTime());

    writeFileSync(file, 'bbb');
    utimesSync(file, mtime, mtime);
    const changed = precompress(second, cache);
    expect(changed.exitCode).toBe(0);
    expect(changed.stdout.toString()).toContain('新压缩 1 个，复用 0 个');
    expect(brotliDecompressSync(readFileSync(`${file}.br`)).toString()).toBe('bbb');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
