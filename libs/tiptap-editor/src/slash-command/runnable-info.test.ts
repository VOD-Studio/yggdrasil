import { describe, expect, it } from 'vitest';
import { buildRunnableInfo } from '../slash-command';

/**
 * buildRunnableInfo 测试:把弹框收集的配置转成 fence info string。
 *
 * dirty=false(全默认) → 'python runnable'(省略 JSON)
 * dirty=true → 'python runnable {"timeout_secs":N,"memory_mb":M,"allow_network":B}'
 * JSON 字段顺序固定:timeout → memory → network
 */

describe('buildRunnableInfo', () => {
  const base = {
    lang: 'python',
    timeoutSecs: 5,
    memoryMb: 256,
    allowNetwork: false,
  } as const;

  describe('dirty=false(全默认,省略 JSON)', () => {
    it('python → "python runnable"', () => {
      expect(buildRunnableInfo({ ...base, dirty: false })).toBe('python runnable');
    });
    it('node → "node runnable"', () => {
      expect(buildRunnableInfo({ ...base, lang: 'node', dirty: false })).toBe('node runnable');
    });
  });

  describe('dirty=true(写 JSON)', () => {
    it('改了 timeout,JSON 含全部 3 项', () => {
      expect(buildRunnableInfo({ ...base, timeoutSecs: 10, dirty: true })).toBe(
        'python runnable {"timeout_secs":10,"memory_mb":256,"allow_network":false}',
      );
    });
    it('node + 改了 allow_network', () => {
      expect(buildRunnableInfo({ ...base, lang: 'node', allowNetwork: true, dirty: true })).toBe(
        'node runnable {"timeout_secs":5,"memory_mb":256,"allow_network":true}',
      );
    });
    it('JSON 字段顺序固定(timeout→memory→network)', () => {
      const out = buildRunnableInfo({ ...base, dirty: true });
      const jsonStart = out.indexOf('{');
      const jsonEnd = out.lastIndexOf('}') + 1;
      const keys = Object.keys(JSON.parse(out.slice(jsonStart, jsonEnd)));
      expect(keys).toEqual(['timeout_secs', 'memory_mb', 'allow_network']);
    });
  });
});
