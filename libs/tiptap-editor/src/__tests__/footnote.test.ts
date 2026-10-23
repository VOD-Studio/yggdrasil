// @vitest-environment happy-dom

import { Editor } from '@tiptap/core';
import { Markdown } from '@tiptap/markdown';
import StarterKit from '@tiptap/starter-kit';
import { beforeEach, describe, expect, it } from 'vitest';

import { FootnoteDef, FootnoteNumbering, FootnoteRef } from '../footnote';

/**
 * 脚注节点回归测试。
 *
 * 新模型:[^id] 解析成 atom 节点(footnoteRef/footnoteDef),序列化走
 * renderMarkdown 直接拼字面量,绕过 @tiptap/markdown 的 escapeMarkdownSyntax。
 * 旧版 unescapeFootnoteSyntax 后处理已移除——不再需要。
 */

function makeEditor() {
  return new Editor({
    element: document.body,
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
      Markdown,
      FootnoteRef,
      FootnoteDef,
      FootnoteNumbering,
    ],
    content: '',
  });
}

/** 遍历文档,收集指定类型节点的 attrs。 */
function collectNodes(editor: Editor, type: string): Array<Record<string, unknown>> {
  const found: Array<Record<string, unknown>> = [];
  editor.state.doc.descendants((node) => {
    if (node.type.name === type) {
      found.push({ ...node.attrs });
    }
    return true;
  });
  return found;
}

/** 从 editor.storage 取脚注编号表(storage 上无类型,需断言)。 */
function getNumbering(editor: Editor): Map<string, number> {
  const storage = editor.storage as unknown as {
    footnoteNumbering: { numbering: Map<string, number> };
  };
  return storage.footnoteNumbering.numbering;
}

function getDefinitions(editor: Editor): Map<string, string> {
  const storage = editor.storage as unknown as {
    footnoteNumbering: { definitions: Map<string, string> };
  };
  return storage.footnoteNumbering.definitions;
}

describe('脚注节点 - 解析与序列化', () => {
  let editor: Editor;

  beforeEach(() => {
    editor = makeEditor();
  });

  it('引用 [^1] 解析成 footnoteRef 节点,不再被转义', () => {
    editor.commands.setContent('正文[^1]结束', { contentType: 'markdown' });
    const refs = collectNodes(editor, 'footnoteRef');
    expect(refs).toHaveLength(1);
    expect(refs[0]?.label).toBe('1');
    // 关键:序列化输出不应含转义形式 \[^1\]
    const md = editor.getMarkdown();
    expect(md).toContain('[^1]');
    expect(md).not.toContain('\\[^1\\]');
  });

  it('定义 [^1]: 内容 解析成 footnoteDef 节点并往返无损', () => {
    editor.commands.setContent('正文[^1]\n\n[^1]: 这是脚注内容', {
      contentType: 'markdown',
    });
    const defs = collectNodes(editor, 'footnoteDef');
    expect(defs).toHaveLength(1);
    expect(defs[0]?.label).toBe('1');
    expect(defs[0]?.content).toBe('这是脚注内容');

    const md = editor.getMarkdown();
    expect(md).toContain('[^1]: 这是脚注内容');
  });

  it('多行定义(缩进续行)被收集为单个 content', () => {
    const src = ['正文[^a]', '', '[^a]: 第一行', '    第二行', '    第三行', ''].join('\n');
    editor.commands.setContent(src, { contentType: 'markdown' });
    const defs = collectNodes(editor, 'footnoteDef');
    expect(defs).toHaveLength(1);
    expect(defs[0]?.content).toBe('第一行\n第二行\n第三行');
  });

  it('多行定义序列化后用 4 空格缩进续行(GFM 格式)', () => {
    const src = ['正文[^a]', '', '[^a]: 第一行', '    第二行', ''].join('\n');
    editor.commands.setContent(src, { contentType: 'markdown' });
    const md = editor.getMarkdown();
    // 首行无缩进,续行 4 空格——与 pulldown-cmark GFM 续行规则一致。
    expect(md).toContain('[^a]: 第一行');
    expect(md).toContain('    第二行');
  });

  it('label 含空格也能正确解析与序列化', () => {
    editor.commands.setContent('正文[^my note]', { contentType: 'markdown' });
    const refs = collectNodes(editor, 'footnoteRef');
    expect(refs[0]?.label).toBe('my note');
    expect(editor.getMarkdown()).toContain('[^my note]');
  });

  it('普通链接 [text](url) 不被脚注 tokenizer 误吞', () => {
    editor.commands.setContent('链接[示例](http://x.com)此处', {
      contentType: 'markdown',
    });
    const refs = collectNodes(editor, 'footnoteRef');
    expect(refs).toHaveLength(0);
    const md = editor.getMarkdown();
    expect(md).toContain('[示例](http://x.com)');
    // 也不应被转义成 \[示例\]
    expect(md).not.toContain('\\[示例\\]');
  });

  it('多次引用同一 label 产生多个 footnoteRef 节点', () => {
    editor.commands.setContent('前[^1]后[^1]再[^2]', { contentType: 'markdown' });
    const refs = collectNodes(editor, 'footnoteRef');
    expect(refs).toHaveLength(3);
    expect(refs.map((r) => r.label)).toEqual(['1', '1', '2']);
  });
});

describe('脚注编号 - 实时分配', () => {
  let editor: Editor;

  beforeEach(() => {
    editor = makeEditor();
  });

  it('编号按引用首次出现顺序分配', () => {
    editor.commands.setContent('先[^b]后[^a]', { contentType: 'markdown' });
    const numbering = getNumbering(editor);
    // b 先出现→1, a 后出现→2
    expect(numbering.get('b')).toBe(1);
    expect(numbering.get('a')).toBe(2);
  });

  it('重复引用同一 label 共享同一编号', () => {
    editor.commands.setContent('前[^1]中[^1]后[^2]', { contentType: 'markdown' });
    const numbering = getNumbering(editor);
    expect(numbering.get('1')).toBe(1);
    expect(numbering.get('2')).toBe(2);
  });

  it('删除引用后编号重排', () => {
    editor.commands.setContent('A[^x]B[^y]', { contentType: 'markdown' });
    expect(getNumbering(editor).get('x')).toBe(1);
    expect(getNumbering(editor).get('y')).toBe(2);

    // 删除第一个引用 [^x](用全量替换简化)。
    editor.commands.setContent('AB[^y]', { contentType: 'markdown' });
    const numbering = getNumbering(editor);
    expect(numbering.has('x')).toBe(false);
    // y 现在是唯一的,编号应是 1。
    expect(numbering.get('y')).toBe(1);
  });

  it('定义内容收集到 definitions 供引用预览', () => {
    editor.commands.setContent('正文[^1]\n\n[^1]: 定义内容', {
      contentType: 'markdown',
    });
    expect(getDefinitions(editor).get('1')).toBe('定义内容');
  });
  it('多段脚注定义保持为一个 footnoteDef,引用前不被换行切断', () => {
    editor.commands.setContent('文末[^a]。\n\n[^a]: 第一段。\n    续行。\n\n    第二段。\n', {
      contentType: 'markdown',
    });
    const defs = collectNodes(editor, 'footnoteDef');
    expect(defs).toHaveLength(1);
    expect(defs[0].content).toBe('第一段。\n续行。\n\n第二段。');
    expect(collectNodes(editor, 'codeBlock')).toHaveLength(0);
    const md = editor.getMarkdown();
    expect(md).toContain('文末[^a]。');
    expect(md).toContain('\n\n    第二段。');
  });
});
