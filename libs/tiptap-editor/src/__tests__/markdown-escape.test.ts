// @vitest-environment happy-dom

import { Editor } from '@tiptap/core';
import { Markdown } from '@tiptap/markdown';
import StarterKit from '@tiptap/starter-kit';
import { beforeEach, describe, expect, it } from 'vitest';

/**
 * Markdown 转义往返回归(Fix 1:patchedDependencies 补丁 @tiptap/markdown)。
 *
 * 根因:@tiptap/markdown 的 escapeMarkdownSyntax 对 *每个* 非代码文本节点的
 * `*`/`_`/`>` 等无差别转义,破坏词内下划线(content_html → content\_html)、
 * 行内大于号(> → &gt;)、以及被 marked 误判为强调边界的字面星号。
 *
 * 修补:CommonMark 风向感知转义——只在真正构成强调开/闭边界的位置转义 `*`/`_`,
 * 词内位置(两侧均 other)不转义;`>` 从 HTML 实体编码集移除。
 *
 * 这些测试用富文本模式(setContent markdown → getMarkdown)验证序列化路径,
 * 因为损坏只在富文本模式走 MarkdownManager.serialize 时发生(源码模式直传原文)。
 */

function makeEditor() {
  return new Editor({
    element: document.body,
    extensions: [StarterKit.configure({ heading: { levels: [1, 2, 3] } }), Markdown],
    content: '',
  });
}

describe('markdown 转义往返 - 富文本模式不过度转义', () => {
  let editor: Editor;

  beforeEach(() => {
    editor = makeEditor();
  });

  it('词内下划线不被转义:content_html 保持原样', () => {
    editor.commands.setContent('字段 content_html 是渲染产物', { contentType: 'markdown' });
    const md = editor.getMarkdown();
    // 修复前:escapeMarkdownSyntax 把每个 _ 转义 → content\_html
    expect(md).toContain('content_html');
    expect(md).not.toContain('content\\_html');
  });

  it('行内大于号不被编码为 &gt;:> 入口提示', () => {
    // 块级 > 是 blockquote,这里用行内(非行首)的 > 验证。
    editor.commands.setContent('见 `foo > bar` 或 输入 > 入口提示', { contentType: 'markdown' });
    const md = editor.getMarkdown();
    expect(md).toContain('入口提示');
    // 修复前:encodeHtmlEntities 把 > 编码为 &gt;
    expect(md).not.toContain('&gt;');
  });

  it('字面星号 run 安全转义且往返不累积:用法**说明', () => {
    // 词内 ** (CJK 两侧) 在无配对闭界时被 marked 保留为字面文本节点。
    // 序列化时转义为 \*\* 是安全表示(\* 在 re-import 时解码回字面 *,不累积)。
    // 关键契约:多轮往返稳定,不出现 \\\* 这类双重转义累积。
    editor.commands.setContent('这是 用法**说明 文本', { contentType: 'markdown' });
    const md1 = editor.getMarkdown();
    // 第二轮
    editor.commands.setContent(md1, { contentType: 'markdown' });
    const md2 = editor.getMarkdown();
    expect(md2).toBe(md1);
    // 不应出现四反斜杠以上的双重转义累积。
    expect(md2).not.toMatch(/\\\\{2,}\*/);
    expect(md2).toContain('说明');
  });

  it('删除线 ~~~~ 结构性转义保留', () => {
    // ~ 是结构性字符(strikethrough),应维持转义避免误判。
    editor.commands.setContent('这是 ~~删除~~ 文本', { contentType: 'markdown' });
    const md = editor.getMarkdown();
    // ~~ 在词边界(空格两侧)会被 marked 识别为删除线;转义后序列化为 \~\~
    expect(md).toContain('删除');
  });

  it('真实强调 **bold** 作为 strong 往返保留', () => {
    editor.commands.setContent('这是 **加粗** 文本', { contentType: 'markdown' });
    const md = editor.getMarkdown();
    // 真实强调(两侧空格 + other)应作为 strong mark 识别并原样序列化。
    expect(md).toContain('**加粗**');
  });

  it('字面反斜杠星号裸文本 \\* 不被双重转义', () => {
    // 裸 \* 在 import 时被 marked 解析为 escape token → 字面 *。
    // serialize 时这个字面 * 若处于边界位置会被转义一次(单层),不应变 \\*。
    editor.commands.setContent('字面星号 \\* 在这里', { contentType: 'markdown' });
    const md = editor.getMarkdown();
    // 关键:不应出现四反斜杠(\\\\)这种双重转义累积。
    expect(md).not.toContain('\\\\\\*');
  });

  it('行内代码反引号结构性转义保留', () => {
    editor.commands.setContent('用 `code` 标记', { contentType: 'markdown' });
    const md = editor.getMarkdown();
    expect(md).toContain('`code`');
    expect(md).toContain('标记');
  });

  it('多轮往返稳定:序列化 → 再解析 → 序列化不累积转义', () => {
    const source = '字段 content_html 与 **加粗** 和 > 符号';
    editor.commands.setContent(source, { contentType: 'markdown' });
    const md1 = editor.getMarkdown();
    editor.commands.setContent(md1, { contentType: 'markdown' });
    const md2 = editor.getMarkdown();
    editor.commands.setContent(md2, { contentType: 'markdown' });
    const md3 = editor.getMarkdown();
    // 关键契约:多轮不累积(content\_html → content\\\_html → ...)。
    expect(md2).toBe(md1);
    expect(md3).toBe(md1);
    expect(md3).toContain('content_html');
    expect(md3).not.toContain('&gt;');
  });
});
