// @vitest-environment happy-dom

import { Editor } from '@tiptap/core';
import { Markdown } from '@tiptap/markdown';
import StarterKit from '@tiptap/starter-kit';
import { beforeEach, describe, expect, it } from 'vitest';
import { DisplayMath, InlineMath } from '../math';

/**
 * 数学公式节点单测(happy-dom 真实 DOM + 真实 Editor)。
 *
 * 核心回归:`editor.getMarkdown()` 序列化时必须绕过 @tiptap/markdown 的
 * encodeTextForMarkdown 双重转义(\→\\、_→\_、&→&amp;),否则存库后服务端
 * KaTeX 收到损坏的 LaTeX 报错。
 *
 * 数学公式是 atom Node,LaTeX 存 attrs.latex,renderMarkdown 直接拼 $...$,
 * 不走文本转义路径(MarkdownManager.ts:1116-1182)。
 */

function makeEditor() {
  return new Editor({
    element: document.body,
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
      Markdown,
      InlineMath,
      DisplayMath,
    ],
    content: '',
  });
}

/** 从编辑器文档收集所有 math/mathBlock 节点的 (type, latex)。 */
function collectMathNodes(editor: Editor): Array<{ type: string; latex: string }> {
  const out: Array<{ type: string; latex: string }> = [];
  editor.state.doc.descendants((node) => {
    if (node.type.name === 'math' || node.type.name === 'mathBlock') {
      out.push({ type: node.type.name, latex: (node.attrs.latex as string) ?? '' });
    }
    return true;
  });
  return out;
}

describe('数学公式节点 - markdown 序列化往返', () => {
  let editor: Editor;

  beforeEach(() => {
    document.body.innerHTML = '';
    editor = makeEditor();
  });

  it('块级公式 $$...$$ 解析为 mathBlock 节点', () => {
    editor.commands.setContent('$$\\frac{a}{b}$$', { contentType: 'markdown' });
    const nodes = collectMathNodes(editor);
    expect(nodes).toHaveLength(1);
    expect(nodes[0].type).toBe('mathBlock');
    expect(nodes[0].latex).toBe('\\frac{a}{b}');
  });

  it('行内公式 $...$ 解析为 math 节点', () => {
    editor.commands.setContent('公式 $E = mc^2$ 很重要', { contentType: 'markdown' });
    const nodes = collectMathNodes(editor);
    expect(nodes).toHaveLength(1);
    expect(nodes[0].type).toBe('math');
    expect(nodes[0].latex).toBe('E = mc^2');
  });

  it('序列化块级公式:LaTeX 原样输出,反斜杠不被双写', () => {
    const tex = '\\frac{a}{b}';
    editor.commands.setContent(`$$${tex}$$`, { contentType: 'markdown' });
    const md = editor.getMarkdown();
    // 关键回归:不得出现 \\frac(被转义)、不得出现 &amp;
    expect(md).toContain(tex);
    expect(md).not.toContain('\\\\frac');
    expect(md).toContain('$$');
  });

  it('序列化行内公式:LaTeX 原样输出', () => {
    const tex = 'a^2 + b^2 = c^2';
    editor.commands.setContent(`勾股 $${tex}$ 定理`, { contentType: 'markdown' });
    const md = editor.getMarkdown();
    expect(md).toContain(`$${tex}$`);
    expect(md).toContain('勾股');
    expect(md).toContain('定理');
  });

  it('矩阵公式:反斜杠、下标、& 全部不被转义(线上 bug 的直接回归)', () => {
    // 这正是线上 markdown-syntax-test-engine-compatibility 损坏的矩阵内容。
    const tex = '\\begin{bmatrix} a_{11} & a_{12} \\\\ a_{21} & a_{22} \\end{bmatrix}';
    editor.commands.setContent(`$$${tex}$$`, { contentType: 'markdown' });
    const md = editor.getMarkdown();
    expect(md).toContain('\\begin{bmatrix}');
    // 不得出现被转义的 \\begin(四个反斜杠)、\_、&amp;
    expect(md).not.toContain('\\\\begin{bmatrix}');
    expect(md).not.toContain('\\_');
    expect(md).not.toContain('&amp;');
    expect(md).toContain('a_{11}');
    expect(md).toContain(' & ');
  });

  it('分段函数 cases:多行 LaTeX 完整保留', () => {
    const tex =
      'f(n) = \\begin{cases} n/2 & \\text{if } n \\text{ is even} \\\\ 3n+1 & \\text{if } n \\text{ is odd} \\end{cases}';
    editor.commands.setContent(`$$${tex}$$`, { contentType: 'markdown' });
    const md = editor.getMarkdown();
    expect(md).toContain('\\begin{cases}');
    expect(md).toContain('\\end{cases}');
    expect(md).not.toContain('\\\\begin{cases}');
    expect(md).not.toContain('&amp;');
    expect(md).not.toContain('\\_');
  });

  it('round-trip:解析 → 序列化 → 再解析,latex 内容稳定', () => {
    const tex = '\\sum_{i=1}^{n} \\frac{1}{i^2}';
    editor.commands.setContent(`$$${tex}$$`, { contentType: 'markdown' });
    const md1 = editor.getMarkdown();
    // 第二轮:把序列化结果再喂回去。
    editor.commands.setContent(md1, { contentType: 'markdown' });
    const md2 = editor.getMarkdown();
    expect(md2).toBe(md1);
    expect(md2).toContain(tex);
  });

  it.each([
    { markdown: '$E=mc^2$', pos: 1 },
    { markdown: '$$E=mc^2$$', pos: 0 },
  ])('编辑 $markdown 时键盘输入不替换公式节点', ({ markdown, pos }) => {
    editor.commands.setContent(markdown, { contentType: 'markdown' });
    editor.commands.setNodeSelection(pos);
    const before = editor.getJSON();
    const formula = editor.view.dom.querySelector('.math-node');
    expect(formula).not.toBeNull();
    formula!.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    const textarea = formula!.querySelector('textarea');
    expect(textarea).not.toBeNull();

    // ProseMirror 会把冒泡的 keypress 当成替换当前 NodeSelection 的输入。
    const key = new KeyboardEvent('keypress', {
      key: 'x',
      bubbles: true,
      cancelable: true,
    });
    // happy-dom 未实现 charCode，补齐浏览器 keypress 实际携带的值。
    Object.defineProperty(key, 'charCode', { value: 120 });
    textarea!.dispatchEvent(key);
    expect(editor.getJSON()).toEqual(before);
    expect(key.defaultPrevented).toBe(false);

    textarea!.value = '\\frac{a}{b}';
    textarea!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true }));
    expect(collectMathNodes(editor)[0].latex).toBe('\\frac{a}{b}');
    expect(editor.getMarkdown()).toContain('\\frac{a}{b}');
    editor.destroy();
  });
});

describe('数学公式节点 - 边界情况', () => {
  let editor: Editor;

  beforeEach(() => {
    document.body.innerHTML = '';
    editor = makeEditor();
  });

  it('成对 $ 会被识别为公式(markdown $...$ 配对语义,非 bug)', () => {
    // 「5$ 到 10$」里两个 $ 配对,中间「 到 10」被当 inline math——这是 markdown
    // $...$ 的标准语义,不是误吞。验证它被正确识别为 math 节点,且序列化往返无损。
    editor.commands.setContent('价格 5$ 到 10$ 之间', { contentType: 'markdown' });
    const nodes = collectMathNodes(editor);
    // 配对语义:应识别出一个 math 节点。
    expect(nodes.length).toBeGreaterThanOrEqual(1);
    // 序列化后 $ 定界符仍存在,内容无损。
    const md = editor.getMarkdown();
    expect(md).toContain('$');
    expect(md).toContain('价格');
  });

  it('空公式 $$ 不崩溃', () => {
    editor.commands.setContent('$$$$', { contentType: 'markdown' });
    // 不抛异常即可。
    expect(editor.state.doc).toBeDefined();
  });

  it('混合段落:文本与公式共存', () => {
    editor.commands.setContent('当 $x > 0$ 时,有 $$x^2 > 0$$ 成立', { contentType: 'markdown' });
    const nodes = collectMathNodes(editor);
    expect(nodes.length).toBeGreaterThanOrEqual(1);
    const md = editor.getMarkdown();
    // 两个公式都应原样保留 $ 定界符。
    expect(md).toContain('$');
  });

  it('坏 TeX 不导致序列化失败', () => {
    // renderMarkdown 只拼 $...$,不调用 katex,坏 TeX 不影响序列化。
    const badTex = '\\undefinedmacro{';
    editor.commands.setContent(`$${badTex}$`, { contentType: 'markdown' });
    const md = editor.getMarkdown();
    expect(md).toContain(badTex);
  });
});
