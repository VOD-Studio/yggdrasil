// @vitest-environment happy-dom

import { Editor } from '@tiptap/core';
import { afterEach, describe, expect, it } from 'vitest';
import { buildExtensions } from '../editor-extensions';

/** GFM 提示块：解析、往返、HTML 结构与普通引用块的区分。 */

let editor: Editor | null = null;

function load(md: string): Editor {
  editor = new Editor({
    element: document.body,
    extensions: buildExtensions({ variant: 'full', getCoordinator: () => null }),
  });
  editor.commands.setContent(md, { contentType: 'markdown', emitUpdate: false });
  return editor;
}

function kinds(e: Editor, type: string): string[] {
  const out: string[] = [];
  e.state.doc.descendants((node) => {
    if (node.type.name === type) out.push(String(node.attrs.kind ?? ''));
    return true;
  });
  return out;
}

afterEach(() => {
  editor?.destroy();
  editor = null;
});

describe('GfmAlert', () => {
  it('五种提示块解析为 gfmAlert 并带正确 kind', () => {
    const md = ['NOTE', 'TIP', 'IMPORTANT', 'WARNING', 'CAUTION']
      .map((k) => `> [!${k}]\n> 内容 ${k}`)
      .join('\n\n');
    const e = load(md);
    expect(kinds(e, 'gfmAlert')).toEqual(['note', 'tip', 'important', 'warning', 'caution']);
    expect(e.getMarkdown().trimEnd()).toBe(md);
  });

  it('多段提示块往返逐字相同', () => {
    const md = '> [!TIP]\n> 段落一\n>\n> 段落二';
    expect(load(md).getMarkdown().trimEnd()).toBe(md);
  });

  it('小写标记序列化为大写', () => {
    expect(load('> [!note]\n> x').getMarkdown().trimEnd()).toBe('> [!NOTE]\n> x');
  });

  it('getHTML 输出与前台一致的 blockquote class', () => {
    expect(load('> [!WARNING]\n> x').getHTML()).toContain(
      '<blockquote class="markdown-alert-warning">',
    );
  });

  it('普通引用仍是 blockquote', () => {
    const e = load('> 引用');
    expect(kinds(e, 'gfmAlert')).toEqual([]);
    expect(kinds(e, 'blockquote')).toHaveLength(1);
  });
});
