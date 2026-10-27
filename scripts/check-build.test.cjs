import { describe, expect, test } from 'bun:test';
import { buildDiagnostics } from './check-build.cjs';

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
  ])('拒绝诊断：%s', line => {
    expect(buildDiagnostics(`正常输出\n${line}\n构建结束`)).toEqual([line.replaceAll(/\x1b\[[0-9;]*m/g, '')]);
  });

  test('正常输出中的文件路径、类型名和命令不是诊断', () => {
    expect(buildDiagnostics([
      'Compiling yggdrasil v1.0.1',
      'Generated target/doc/yggdrasil/api/error/index.html',
      'Bundled src/error.ts → public/error.js',
      'ApiError ServerFnError error_reason warnings.rs',
      'Finished release profile [optimized]',
    ].join('\r\n'))).toEqual([]);
  });
});
