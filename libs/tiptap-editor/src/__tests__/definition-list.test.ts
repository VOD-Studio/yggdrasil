// @vitest-environment happy-dom

import { Editor } from '@tiptap/core';
import { afterEach, describe, expect, it } from 'vitest';
import { buildExtensions } from '../editor-extensions';

/** 定义列表：解析、往返，以及不误伤相邻块的边界。 */

let editor: Editor | null = null;

function load(md: string): Editor {
  editor = new Editor({
    element: document.body,
    extensions: buildExtensions({ variant: 'full', getCoordinator: () => null }),
  });
  editor.commands.setContent(md, { contentType: 'markdown', emitUpdate: false });
  return editor;
}

function count(e: Editor, type: string): number {
  let n = 0;
  e.state.doc.descendants((node) => {
    if (node.type.name === type) n += 1;
    return true;
  });
  return n;
}

afterEach(() => {
  editor?.destroy();
  editor = null;
});

describe('DefinitionList', () => {
  it('多组定义列表解析为 1 个 dl（2 dt / 3 dd）并往返逐字相同', () => {
    const md = '术语 A\n: 术语 A 的定义，用冒号缩进。\n\n术语 B\n: 第一个定义。\n: 第二个定义。';
    const e = load(md);
    expect(count(e, 'definitionList')).toBe(1);
    expect(count(e, 'definitionTerm')).toBe(2);
    expect(count(e, 'definitionDescription')).toBe(3);
    expect(e.getMarkdown().trimEnd()).toBe(md);
  });

  it('段落后紧跟术语+定义时保持普通段落', () => {
    const md = '导语\n术语\n: 定义';
    const e = load(md);
    expect(count(e, 'definitionList')).toBe(0);
    expect(e.getMarkdown().trimEnd()).toBe(md);
  });

  it('标题行后的冒号行不是定义列表', () => {
    const e = load('# 标题\n: x');
    expect(count(e, 'definitionList')).toBe(0);
  });
});
