import { describe, expect, test } from 'bun:test';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { stripVTControlCharacters } from 'node:util';
import { buildDiagnostics } from './check-build.ts';

describe('发布构建诊断门禁', () => {
  test.each([
    'warning: unused import',
    'warning[E0308]: mismatched types',
    '\x1b[33mwarning\x1b[0m: broken intra-doc link',
    '2026-10-27T00:00:00Z  WARN wasm_split: overlong segment',
    'warn: peer dependency mismatch',
    '▲ [WARNING] unsupported CSS selector',
    '(!) Some chunks are larger than the limit',
    'error: could not compile yggdrasil',
    '[ERROR] Build failed',
    'Error: asset missing',
    'make: *** [Makefile:114: build] Error 2',
    '1 warning generated.',
  ])('拒绝诊断：%s', (line) => {
    expect(buildDiagnostics(`正常输出\n${line}\n构建结束`)).toEqual([
      stripVTControlCharacters(line),
    ]);
  });

  test('正常输出中的文件路径、类型名和命令不是诊断', () => {
    expect(
      buildDiagnostics(
        [
          'Compiling yggdrasil v1.0.1',
          'Generated target/doc/yggdrasil/api/error/index.html',
          'Bundled src/error.ts → public/error.js',
          'ApiError ServerFnError error_reason warnings.rs',
          'Finished release profile [optimized]',
        ].join('\r\n'),
      ),
    ).toEqual([]);
  });
});

test.each([
  [
    '双流交错且诊断跨 chunk',
    `
    process.stderr.write('war');
    setTimeout(() => {
      console.log('normal output');
      setTimeout(() => process.stderr.write('ning: split diagnostic\\n'), 30);
    }, 30);
  `,
    1,
  ],
  ['无诊断但构建失败', "console.log('compile stopped'); process.exit(23);", 1],
  ['正常构建', "console.log('Finished release profile');", 0],
])('命令退出状态：%s', (name, source, exitCode) => {
  const root = mkdtempSync(join(tmpdir(), 'ygg-build-gate-'));
  try {
    mkdirSync(join(root, 'scripts'));
    mkdirSync(join(root, 'bin'));
    copyFileSync(join(import.meta.dir, 'check-build.ts'), join(root, 'scripts/check-build.ts'));
    writeFileSync(join(root, 'bin/make'), `#!${Bun.which('bun')}\n${source}`, { mode: 0o755 });
    const result = Bun.spawnSync(['bun', join(root, 'scripts/check-build.ts')], {
      env: { ...process.env, PATH: `${join(root, 'bin')}:${process.env.PATH}` },
      stdout: 'pipe',
      stderr: 'pipe',
    });
    expect(result.exitCode).toBe(exitCode);
    if (name === '双流交错且诊断跨 chunk') {
      expect(result.stderr.toString()).toContain('warning: split diagnostic');
    } else if (exitCode === 1) {
      expect(readFileSync(join(root, 'target/build.log'), 'utf8')).toBe('compile stopped\n');
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
