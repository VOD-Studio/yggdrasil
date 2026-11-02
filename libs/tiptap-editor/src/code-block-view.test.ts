import type { ViewMutationRecord } from '@tiptap/pm/view';
import { describe, expect, it, vi } from 'vitest';
import { CodeBlockNodeView } from './code-block-view';

/**
 * 模块级共享 sentinel：对齐 upload-image.test.ts 范式。
 * 真实 ProseMirror 每个 schema 的 NodeType 是单例，同类节点引用必等；
 * mock 用共享引用模拟此语义，使实现的纯引用比较能命中。
 */
const CODEBLOCK_TYPE = { name: 'codeBlock' };
const PARAGRAPH_TYPE = { name: 'paragraph' };

/** 构造最小 mock node，只含 NodeView 需要的字段。 */
function mockNode(language: string, textContent = '') {
  return {
    type: CODEBLOCK_TYPE,
    attrs: { language },
    textContent,
  } as any;
}

/** 构造最小 mock editor，含 storage。 */
function mockEditor(onRunCode?: (opts: any) => Promise<string>) {
  return {
    storage: onRunCode ? { __onRunCode: onRunCode } : {},
  } as any;
}

/**
 * CodeBlockNodeView 测试。
 *
 * 验证：DOM 结构、语言标签、运行按钮显隐、update 刷新、contentDOM 指向 code。
 */
describe('CodeBlockNodeView', () => {
  it('runnable 块：toolbar 显示语言 + 运行按钮', () => {
    const view = new CodeBlockNodeView({
      node: mockNode('python runnable {"timeout_secs":10}'),
      editor: mockEditor(),
    } as any);
    expect(view.dom.querySelector('.tiptap-codeblock-lang')?.textContent).toBe('python');
    expect(view.dom.querySelector('.tiptap-codeblock-run')).not.toBeNull();
    expect(view.dom.querySelector<HTMLElement>('.tiptap-codeblock-toolbar')?.style.display).toBe(
      '',
    );
    expect(view.dom.classList.contains('has-toolbar')).toBe(true);
  });

  it('普通块(python)：显示语言标签，无运行按钮', () => {
    const view = new CodeBlockNodeView({
      node: mockNode('python'),
      editor: mockEditor(),
    } as any);
    expect(view.dom.querySelector('.tiptap-codeblock-lang')?.textContent).toBe('python');
    expect(view.dom.querySelector('.tiptap-codeblock-run')).toBeNull();
    expect(view.dom.querySelector<HTMLElement>('.tiptap-codeblock-toolbar')?.style.display).toBe(
      '',
    );
    expect(view.dom.classList.contains('has-toolbar')).toBe(true);
  });

  it('无 language 的块：隐藏 toolbar，无运行按钮', () => {
    const view = new CodeBlockNodeView({
      node: mockNode(''),
      editor: mockEditor(),
    } as any);
    expect(view.dom.querySelector('.tiptap-codeblock-run')).toBeNull();
    expect(view.dom.querySelector<HTMLElement>('.tiptap-codeblock-toolbar')?.style.display).toBe(
      'none',
    );
    expect(view.dom.classList.contains('has-toolbar')).toBe(false);
  });

  it('node 语言由空更新为 python 时 update() 显示 toolbar', () => {
    const view = new CodeBlockNodeView({
      node: mockNode(''),
      editor: mockEditor(),
    } as any);
    expect(view.dom.querySelector<HTMLElement>('.tiptap-codeblock-toolbar')?.style.display).toBe(
      'none',
    );
    expect(view.dom.classList.contains('has-toolbar')).toBe(false);

    view.update(mockNode('python'));
    expect(view.dom.querySelector('.tiptap-codeblock-lang')?.textContent).toBe('python');
    expect(view.dom.querySelector<HTMLElement>('.tiptap-codeblock-toolbar')?.style.display).toBe(
      '',
    );
    expect(view.dom.classList.contains('has-toolbar')).toBe(true);

    view.update(mockNode(''));
    expect(view.dom.querySelector<HTMLElement>('.tiptap-codeblock-toolbar')?.style.display).toBe(
      'none',
    );
    expect(view.dom.classList.contains('has-toolbar')).toBe(false);
  });
  it('node 语言变化时 update() 刷新标签', () => {
    const view = new CodeBlockNodeView({
      node: mockNode('python'),
      editor: mockEditor(),
    } as any);
    // 模拟 ProseMirror 调 update：改为 node runnable
    const updated = view.update(mockNode('node runnable'));
    expect(updated).toBe(true);
    expect(view.dom.querySelector('.tiptap-codeblock-lang')?.textContent).toBe('node');
    expect(view.dom.querySelector('.tiptap-codeblock-run')).not.toBeNull();
  });

  it('update 拒绝非同类节点(返回 false)', () => {
    const view = new CodeBlockNodeView({
      node: mockNode('python'),
      editor: mockEditor(),
    } as any);
    const otherNode = { type: PARAGRAPH_TYPE, attrs: {}, textContent: '' } as any;
    expect(view.update(otherNode)).toBe(false);
  });

  it('contentDOM 指向 code 元素(保证 decoration 生效)', () => {
    const view = new CodeBlockNodeView({
      node: mockNode('python'),
      editor: mockEditor(),
    } as any);
    expect(view.contentDOM?.tagName).toBe('CODE');
  });

  it('runnable 块的 code class 只含纯语言名(不抛 InvalidCharacterError)', () => {
    // 回归：完整 info string `python runnable {...}` 含空格，
    // classList.add 会拒绝含空格的 token。必须用 extractLang 提取首 token。
    const view = new CodeBlockNodeView({
      node: mockNode('python runnable {"timeout_secs":10}'),
      editor: mockEditor(),
    } as any);
    expect(view.contentDOM?.classList.contains('language-python')).toBe(true);
    expect(view.contentDOM?.classList.contains('runnable')).toBe(false);
  });

  it('ignoreMutation: contentDOM 内的编辑 mutation 返回 false(让 ProseMirror 处理)', () => {
    // 回归：ignoreMutation 无条件返回 true 会导致 contentDOM 内的输入/退格被忽略，
    // 文档状态与 DOM 失同步，表现为 Backspace 删整块而非删字符。
    const view = new CodeBlockNodeView({
      node: mockNode('python'),
      editor: mockEditor(),
    } as any);
    const code = view.contentDOM!;
    // 模拟 code 内文本节点的 characterData mutation(输入/退格)
    const textNode = document.createTextNode('fn main');
    code.appendChild(textNode);
    const editMutation = {
      type: 'characterData',
      target: textNode,
    } as unknown as ViewMutationRecord;
    expect(view.ignoreMutation(editMutation)).toBe(false);
    // selection mutation 也应交给 ProseMirror
    const selMutation = { type: 'selection', target: code } as unknown as ViewMutationRecord;
    expect(view.ignoreMutation(selMutation)).toBe(false);
  });

  it('ignoreMutation: 工具栏/结果区装饰元素的 mutation 返回 true(忽略)', () => {
    const view = new CodeBlockNodeView({
      node: mockNode('python'),
      editor: mockEditor(),
    } as any);
    // 工具栏(装饰元素)的 childList mutation 应忽略
    const toolbarMutation = {
      type: 'childList',
      target: view.dom.querySelector('.tiptap-codeblock-toolbar')!,
      addedNodes: [] as Node[],
      removedNodes: [] as Node[],
    } as unknown as ViewMutationRecord;
    expect(view.ignoreMutation(toolbarMutation)).toBe(true);
    // contentDOM 自身的 attributes 变化(如高亮改 class)忽略
    const attrMutation = {
      type: 'attributes',
      target: view.contentDOM!,
      attributeName: 'class',
    } as unknown as ViewMutationRecord;
    expect(view.ignoreMutation(attrMutation)).toBe(true);
  });

  it('点击运行按钮调用 onRunCode(storage 回调)', () => {
    const onRunCode = vi.fn(() => Promise.resolve('结果'));
    const view = new CodeBlockNodeView({
      node: mockNode('python runnable {"timeout_secs":10}', 'print(1)'),
      editor: mockEditor(onRunCode),
    } as any);
    view.dom.querySelector<HTMLButtonElement>('.tiptap-codeblock-run')!.click();
    expect(onRunCode).toHaveBeenCalledTimes(1);
    expect(onRunCode).toHaveBeenCalledWith(
      expect.objectContaining({
        language: 'python',
        source: 'print(1)',
        overridesJson: '{"timeout_secs":10}',
      }),
    );
  });

  it('无 onRunCode 时点运行不报错(优雅降级)', () => {
    const view = new CodeBlockNodeView({
      node: mockNode('python runnable', 'print(1)'),
      editor: mockEditor(), // 无 __onRunCode
    } as any);
    expect(() =>
      view.dom.querySelector<HTMLButtonElement>('.tiptap-codeblock-run')!.click(),
    ).not.toThrow();
  });
});
