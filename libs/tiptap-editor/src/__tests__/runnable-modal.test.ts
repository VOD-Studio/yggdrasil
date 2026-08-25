import type { Editor } from '@tiptap/core';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { openRunnableModal } from '../slash-command';

/**
 * openRunnableModal 测试(happy-dom)。
 *
 * 验证:DOM 创建/销毁、预览实时更新、插入回调、关闭路径。
 * editor 用 mock,只校验 setCodeBlock 被以正确 language 调用。
 */

// happy-dom 提供 document.body;每个用例后清理。
afterEach(() => {
  document.body.innerHTML = '';
});

/** 构造 mock editor:记录 chain() 调用链上的 setCodeBlock 参数。 */
function mockEditor() {
  const calls: { language: string }[] = [];
  const chain = {
    focus: vi.fn(() => chain),
    setCodeBlock: vi.fn((attrs: { language: string }) => {
      calls.push(attrs);
      return chain;
    }),
    run: vi.fn(),
  };
  return { editor: { chain: vi.fn(() => chain) } as unknown as Editor, calls };
}

/**
 * 构造支持编辑模式的 mock editor：额外记录 view.dispatch 调用（编辑模式用 setNodeMarkup）。
 */
function mockEditorWithView() {
  const calls: { language: string }[] = [];
  const markupCalls: { pos: number | undefined; language: string }[] = [];
  const chain = {
    focus: vi.fn(() => chain),
    setCodeBlock: vi.fn((attrs: { language: string }) => {
      calls.push(attrs);
      return chain;
    }),
    run: vi.fn(),
  };
  // tr.setNodeMarkup 记录调用，view.dispatch 触发记录
  const tr = {
    setNodeMarkup: vi.fn((pos: number, _type: unknown, attrs: { language: string }) => {
      markupCalls.push({ pos, language: attrs.language });
      return tr;
    }),
  };
  const view = {
    dispatch: vi.fn(() => {}),
  };
  const editor = {
    chain: vi.fn(() => chain),
    view,
    state: { tr },
  };
  return { editor: editor as unknown as Editor, calls, markupCalls };
}

describe('openRunnableModal', () => {
  it('创建后 body 含模态框', () => {
    const { editor } = mockEditor();
    openRunnableModal(editor);
    expect(document.querySelector('.tiptap-runnable-modal')).not.toBeNull();
    expect(document.querySelector('.tiptap-runnable-modal-mask')).not.toBeNull();
  });

  it('改 timeout 后预览更新', () => {
    const { editor } = mockEditor();
    openRunnableModal(editor);
    const timeoutInput = document.querySelector<HTMLInputElement>('#runnable-timeout')!;
    timeoutInput.value = '10';
    timeoutInput.dispatchEvent(new Event('input', { bubbles: true }));
    const preview = document.querySelector('.tiptap-runnable-preview')!;
    expect(preview.textContent).toContain('"timeout_secs":10');
  });

  it('点「插入」调用 setCodeBlock 并销毁弹框', () => {
    const { editor, calls } = mockEditor();
    openRunnableModal(editor);
    // 改 timeout 让 dirty=true
    const timeoutInput = document.querySelector<HTMLInputElement>('#runnable-timeout')!;
    timeoutInput.value = '10';
    timeoutInput.dispatchEvent(new Event('input', { bubbles: true }));
    // 点插入
    const insertBtn = document.querySelector<HTMLButtonElement>(
      '.tiptap-runnable-actions .insert',
    )!;
    insertBtn.click();
    expect(calls).toHaveLength(1);
    expect(calls[0].language).toBe(
      'python runnable {"timeout_secs":10,"memory_mb":256,"allow_network":false}',
    );
    expect(document.querySelector('.tiptap-runnable-modal')).toBeNull();
  });

  it('全默认(不动 overrides)插入 → info string 无 JSON', () => {
    const { editor, calls } = mockEditor();
    openRunnableModal(editor);
    document.querySelector<HTMLButtonElement>('.tiptap-runnable-actions .insert')!.click();
    expect(calls[0].language).toBe('python runnable');
  });

  it('超时输入非法值(0)时「插入」按钮 disabled', () => {
    const { editor, calls } = mockEditor();
    openRunnableModal(editor);
    const timeoutInput = document.querySelector<HTMLInputElement>('#runnable-timeout')!;
    timeoutInput.value = '0';
    timeoutInput.dispatchEvent(new Event('input', { bubbles: true }));
    const insertBtn = document.querySelector<HTMLButtonElement>(
      '.tiptap-runnable-actions .insert',
    )!;
    expect(insertBtn.disabled).toBe(true);
    // 且点插入(即使强制)不应触发——实际 disabled 按钮不接收 click,这里只验状态
    insertBtn.click();
    expect(calls).toHaveLength(0);
  });

  it('超时恢复合法值后「插入」按钮重新启用', () => {
    const { editor } = mockEditor();
    openRunnableModal(editor);
    const timeoutInput = document.querySelector<HTMLInputElement>('#runnable-timeout')!;
    const insertBtn = document.querySelector<HTMLButtonElement>(
      '.tiptap-runnable-actions .insert',
    )!;
    timeoutInput.value = '0';
    timeoutInput.dispatchEvent(new Event('input', { bubbles: true }));
    expect(insertBtn.disabled).toBe(true);
    timeoutInput.value = '10';
    timeoutInput.dispatchEvent(new Event('input', { bubbles: true }));
    expect(insertBtn.disabled).toBe(false);
  });

  it('点「取消」不调用 setCodeBlock 且销毁弹框', () => {
    const { editor, calls } = mockEditor();
    openRunnableModal(editor);
    document.querySelector<HTMLButtonElement>('.tiptap-runnable-actions .cancel')!.click();
    expect(calls).toHaveLength(0);
    expect(document.querySelector('.tiptap-runnable-modal')).toBeNull();
  });

  it('Esc 关闭弹框(不插入)', async () => {
    const { editor, calls } = mockEditor();
    openRunnableModal(editor);
    // keydown 监听延迟注册（setTimeout(0)），需先 flush timer
    await new Promise((r) => setTimeout(r, 0));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(calls).toHaveLength(0);
    expect(document.querySelector('.tiptap-runnable-modal')).toBeNull();
  });

  it('点击遮罩关闭弹框(不插入)', () => {
    const { editor, calls } = mockEditor();
    openRunnableModal(editor);
    const mask = document.querySelector('.tiptap-runnable-modal-mask')!;
    // 点击遮罩本身(非卡片):target = currentTarget
    mask.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(calls).toHaveLength(0);
    expect(document.querySelector('.tiptap-runnable-modal')).toBeNull();
  });
});

/**
 * 编辑模式测试：openRunnableModal(editor, editPos, currentInfo) 回填 + 原地更新。
 */
describe('openRunnableModal 编辑模式', () => {
  it('回填当前语言（python runnable + overrides）', () => {
    const { editor } = mockEditorWithView();
    openRunnableModal(editor, 0, 'python runnable {"timeout_secs":10,"memory_mb":512}');
    const langTrigger = document.querySelector<HTMLButtonElement>('.tiptap-select-trigger')!;
    const timeoutInput = document.querySelector<HTMLInputElement>('#runnable-timeout')!;
    const memInput = document.querySelector<HTMLInputElement>('#runnable-memory')!;
    expect(langTrigger.textContent).toContain('Python');
    expect(timeoutInput.value).toBe('10');
    expect(memInput.value).toBe('512');
  });

  it('语言列表与代码运行器注册表同步', () => {
    const { editor } = mockEditorWithView();
    openRunnableModal(editor);
    document.querySelector<HTMLButtonElement>('.tiptap-select-trigger')!.click();
    const options = [...document.querySelectorAll<HTMLElement>('.tiptap-select-option')].map(
      (option) => option.textContent?.trim(),
    );
    expect(options).toEqual(['Python', 'Node.js', 'Go', 'Rust', 'Bun (TS)']);
  });

  it('语言下拉使用项目化 listbox 控件', () => {
    const { editor } = mockEditorWithView();
    openRunnableModal(editor);
    const trigger = document.querySelector<HTMLButtonElement>('.tiptap-select-trigger')!;
    trigger.click();
    expect(trigger.getAttribute('aria-haspopup')).toBe('listbox');
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    expect(document.querySelector('.tiptap-select-panel')).not.toBeNull();
  });

  it('标题为「编辑」、按钮为「保存」', () => {
    const { editor } = mockEditorWithView();
    openRunnableModal(editor, 0, 'node runnable');
    expect(document.querySelector('.tiptap-runnable-modal-title')?.textContent).toBe(
      '编辑可运行代码块',
    );
    expect(document.querySelector('.tiptap-runnable-actions .insert')?.textContent).toBe('保存');
  });

  it('保存时用 setNodeMarkup 原地更新（非 setCodeBlock 新建）', () => {
    const { editor, calls, markupCalls } = mockEditorWithView();
    openRunnableModal(editor, 5, 'python runnable');
    document.querySelector<HTMLButtonElement>('.tiptap-runnable-actions .insert')!.click();
    expect(calls).toHaveLength(0); // 编辑模式不走 setCodeBlock
    expect(markupCalls).toHaveLength(1);
    expect(markupCalls[0].pos).toBe(5);
    expect(markupCalls[0].language).toBe('python runnable');
  });

  it('改语言后保存，新语言写入', () => {
    const { editor, markupCalls } = mockEditorWithView();
    openRunnableModal(editor, 5, 'python runnable');
    document.querySelector<HTMLButtonElement>('.tiptap-select-trigger')!.click();
    const nodeOption = [...document.querySelectorAll<HTMLElement>('.tiptap-select-option')].find(
      (option) => option.textContent?.trim() === 'Node.js',
    )!;
    nodeOption.click();
    document.querySelector<HTMLButtonElement>('.tiptap-runnable-actions .insert')!.click();
    expect(markupCalls[0].language).toBe('node runnable');
  });
});
