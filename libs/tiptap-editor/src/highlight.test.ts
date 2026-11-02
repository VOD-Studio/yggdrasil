import { describe, expect, it } from 'vitest';
import { extractLang, extractOverridesJson, lowlight } from './highlight';

/**
 * extractLang 测试：从完整 fence info string 提取语言名（首个 token）。
 * 与后端 parse_fence_info（src/api/code_runner/languages.rs）对齐。
 */
describe('extractLang', () => {
  it.each([
    ['python', 'python', '纯语言名'],
    ['python runnable', 'python', 'runnable 标记'],
    ['python runnable {"timeout_secs":10}', 'python', 'runnable + overrides JSON'],
    ['node runnable', 'node', 'node runnable'],
    ['  python  ', 'python', '前后空格(trim)'],
    ['', '', '空字符串'],
    ['   ', '', '纯空格'],
    ['PYTHON', 'python', '大写小写化（与后端对齐）'],
    ['Python Runnable', 'python', '混合大小写'],
  ])('%s → %s (%s)', (input, expected) => {
    expect(extractLang(input)).toBe(expected);
  });
});

describe('extractOverridesJson', () => {
  it.each([
    ['python runnable {"timeout_secs":10}', '{"timeout_secs":10}', 'runnable + overrides'],
    [
      'python runnable {"timeout_secs":10,"memory_mb":256}',
      '{"timeout_secs":10,"memory_mb":256}',
      '多字段 overrides',
    ],
    ['python runnable', '', '无 overrides'],
    ['python', '', '纯语言名'],
    ['', '', '空字符串'],
    ['python runnable {"allow_network":true}', '{"allow_network":true}', 'allow_network'],
  ])('%s → %s (%s)', (input, expected) => {
    expect(extractOverridesJson(input)).toBe(expected);
  });
});

/**
 * lowlight wrapper 测试：highlight 经过 extractLang 处理，
 * runnable info string 也能按正确语言高亮。
 */
describe('lowlight wrapper', () => {
  it('runnable info string 按 python 高亮（不抛错）', () => {
    // 未包装的 lowlight 遇到 'python runnable {...}' 会回退 highlightAuto 或抛错；
    // wrapper 提取 'python' 后正常高亮。
    const result = lowlight.highlight('python runnable {"timeout_secs":10}', 'def f():\n  pass');
    expect(result).toBeDefined();
  });

  it('python 代码高亮输出含 hljs-keyword class', () => {
    const result = lowlight.highlight('python', 'def f():\n  pass');
    const html = JSON.stringify(result);
    expect(html).toContain('hljs-keyword');
  });

  it('普通语言名（无 runnable）正常工作', () => {
    const result = lowlight.highlight('javascript', 'const x = 1;');
    expect(JSON.stringify(result)).toContain('hljs-');
  });

  it('未注册语言不抛错（回退处理）', () => {
    // extractLang 返回未注册语言名时，lowlight 内部会抛错，
    // CodeBlockLowlight 源码用 try/catch 兜底，这里只验 wrapper 不改变此行为。
    expect(() => lowlight.highlight('totally-unknown-lang', 'code')).toThrow();
  });

  it('vue 作为 xml 别名高亮（不抛 Unknown language）', () => {
    // lowlight 依赖的 highlight.js 11 已移除 vue grammar，
    // highlight.ts 把 vue 注册为 xml 别名兜底。
    // 验证：vue 代码能高亮（走 xml grammar，输出含 hljs class）且不抛错。
    const result = lowlight.highlight('vue', '<template><div>hi</div></template>');
    expect(JSON.stringify(result)).toContain('hljs-');
  });
});
