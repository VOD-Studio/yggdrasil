// @vitest-environment happy-dom

import { Editor, type JSONContent } from '@tiptap/core';
import { afterEach, describe, expect, it } from 'vitest';
import { buildExtensions } from '../editor-extensions';

/**
 * 文章编辑器（full 变体）Markdown 往返保真回归：
 * 载入 Markdown → getMarkdown() 应逐字还原（或固定在渲染等价的写法），且再载入一次不变。
 * 服务端往返守卫（roundtrip.rs）对这里没覆盖的写法兜底。
 */

let editor: Editor | null = null;

function load(md: string): Editor {
  editor?.destroy();
  editor = new Editor({
    element: document.createElement('div'),
    extensions: buildExtensions({ variant: 'full', getCoordinator: () => null }),
    content: '',
  });
  editor.commands.setContent(md, { contentType: 'markdown', emitUpdate: false });
  return editor;
}

function roundtrip(md: string): string {
  return load(md).getMarkdown().trimEnd();
}

/** 往返结果必须稳定：输出再载入后不变。 */
function expectStable(md: string): void {
  const once = roundtrip(md);
  expect(roundtrip(once)).toBe(once);
}

function collect(node: JSONContent, pred: (n: JSONContent) => boolean, out: JSONContent[] = []) {
  if (pred(node)) out.push(node);
  node.content?.forEach((c) => {
    collect(c, pred, out);
  });
  return out;
}

const nodesOfType = (ed: Editor, type: string) =>
  collect(ed.getJSON(), (n) => n.type === type).length;

const marksOfType = (ed: Editor, type: string) =>
  collect(ed.getJSON(), (n) => (n.marks ?? []).some((m) => m.type === type)).length;

afterEach(() => {
  editor?.destroy();
  editor = null;
});

describe('下标 / 上标', () => {
  const md = '上标 E = mc ^2^，下标 H ~2~ O';

  it('逐字往返并渲染成 sup / sub', () => {
    const ed = load(md);
    expect(ed.getMarkdown().trimEnd()).toBe(md);
    expect(ed.getHTML()).toContain('<sup>2</sup>');
    expect(ed.getHTML()).toContain('<sub>2</sub>');
    expectStable(md);
  });

  it('词内 ~ 与 ^ 保持字面量，写回为渲染等价的转义', () => {
    const ed = load('H~2~O 与 x^2^');
    expect(ed.getMarkdown().trimEnd()).toBe('H\\~2\\~O 与 x^2\\^');
    for (const mark of ['strike', 'subscript', 'superscript']) {
      expect(marksOfType(ed, mark)).toBe(0);
    }
    expectStable('H~2~O 与 x^2^');
  });
});

describe('转义与实体', () => {
  it('CJK 标点旁的转义不丢', () => {
    const md = '转义字符：\\*这不是斜体\\*、\\`这不是行内代码\\`';
    const ed = load(md);
    expect(ed.getMarkdown().trimEnd()).toBe(md);
    expect(marksOfType(ed, 'italic')).toBe(0);
    expectStable(md);
  });

  it('HTML 实体逐字保留，编辑器显示解码字符', () => {
    const md = 'HTML 实体：&copy; &amp; &lt; &gt; &nbsp;x';
    const ed = load(md);
    expect(ed.getMarkdown().trimEnd()).toBe(md);
    expect(ed.getHTML()).toContain('©');
    expectStable(md);
  });

  it('未知实体 &foo; 是字面文本，序列化为 &amp;foo;', () => {
    expect(roundtrip('a &foo; b')).toBe('a &amp;foo; b');
  });
});

describe('行内 HTML', () => {
  it('成对标签变成可见格式并逐字往返', () => {
    const md =
      '<kbd>Ctrl</kbd> + <kbd>C</kbd>，<mark>高亮文本</mark>，<abbr title="HyperText Markup Language">HTML</abbr>，H<sub>2</sub>O，E=mc<sup>2</sup>';
    const ed = load(md);
    expect(ed.getMarkdown().trimEnd()).toBe(md);
    expect(ed.getHTML()).toContain('<kbd>Ctrl</kbd>');
    expect(ed.getHTML()).toContain('<abbr title="HyperText Markup Language">HTML</abbr>');
    expectStable(md);
  });

  it('其他行内原始 HTML 原样保留为原子节点', () => {
    const md =
      '原始 HTML：<abbr title="…"> 的 `title` 会留下，<time datetime="2026-10-23">x</time>';
    const ed = load(md);
    expect(ed.getMarkdown().trimEnd()).toBe(md);
    expect(nodesOfType(ed, 'rawInlineHtml')).toBe(3);
    expectStable(md);
  });
});

describe('<details> 折叠块', () => {
  const md = [
    '<details>',
    '<summary>查看补充说明</summary>',
    '',
    '标签与内容之间要保留空行，内部的 Markdown 与公式才会被解析：',
    '',
    '$$',
    '\\ce{2H2 + O2 -> 2H2O}',
    '$$',
    '',
    '</details>',
  ].join('\n');

  it('成为节点并原样往返', () => {
    const ed = load(md);
    expect(nodesOfType(ed, 'details')).toBe(1);
    expect(nodesOfType(ed, 'mathBlock')).toBe(1);
    expect(ed.getMarkdown().trimEnd()).toBe(md);
    expectStable(md);
  });

  it('保留 open 等开标签属性', () => {
    const open = md.replace('<details>', '<details open>');
    expect(roundtrip(open)).toBe(open);
  });
});

describe('链接', () => {
  it('参考式链接与定义行原样往返', () => {
    const md =
      '- 参考 [写作指南][guide]、[guide][] 与 [guide]\n\n[guide]: /about/writing "站内写作指南"';
    const ed = load(md);
    expect(ed.getMarkdown().trimEnd()).toBe(md);
    expect(nodesOfType(ed, 'linkDefinition')).toBe(1);
    expectStable(md);
  });

  it('内联链接不受影响', () => {
    const md = '[文字](https://example.com "标题")';
    expect(roundtrip(md)).toBe(md);
  });

  it('裸 URL 与裸邮箱不自动成链', () => {
    const md = '裸 URL https://example.com/plain 与 hi@example.com';
    const ed = load(md);
    expect(ed.getMarkdown().trimEnd()).toBe(md);
    expect(marksOfType(ed, 'link')).toBe(0);
  });
});

describe('列表', () => {
  it.each([
    [
      '松散编号列表带代码块',
      '1. 第一项带代码块：\n\n   ```python\n   print(1)\n   ```\n\n2. 第二项',
    ],
    ['松散无序列表', '- a\n\n- b'],
    ['紧凑无序列表', '- a\n- b'],
    ['起始编号', '3. a\n4. b'],
  ])('%s 逐字往返', (_name, md) => {
    expect(roundtrip(md)).toBe(md);
    expectStable(md);
  });
});

describe('块级公式', () => {
  it.each([
    ['多行写法', '$$\n\\sum_{i=1}^{n} i\n$$'],
    ['单行写法', '$$\\frac{a}{b}$$'],
  ])('%s 保持原样', (_name, md) => {
    expect(roundtrip(md)).toBe(md);
    expectStable(md);
  });
});
