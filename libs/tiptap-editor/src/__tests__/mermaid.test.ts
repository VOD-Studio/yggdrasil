// @vitest-environment happy-dom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * mermaid 代码块预览测试。
 *
 * mock `./mermaid` 模块的 renderMermaid/getCurrentTheme,
 * 绕过真实 mermaid 运行时(happy-dom 不支持 SVG 引擎)。重点验证 NodeView 的
 * 生命周期:预览区创建/移除、debounce、竞态取消、主题重渲染、错误态、清理。
 */

const mockRenderMermaid = vi.fn();
const mockGetCurrentTheme = vi.fn().mockReturnValue('light');

vi.mock('../mermaid', () => ({
  renderMermaid: (...args: unknown[]) => mockRenderMermaid(...args),
  getCurrentTheme: () => mockGetCurrentTheme(),
}));

// THEME_CHANGE_EVENT 实际从 @yggdrasil/shared import,需一并 stub(vi.mock 对裸模块名)。
vi.mock('@yggdrasil/shared', () => ({
  THEME_CHANGE_EVENT: 'yggdrasil:theme-change',
}));

const CODEBLOCK_TYPE = { name: 'codeBlock' };

function mockNode(language: string, textContent = '') {
  return {
    type: CODEBLOCK_TYPE,
    attrs: { language },
    textContent,
  } as any;
}

function mockEditor() {
  return { storage: {} } as any;
}

// 动态 import,确保 vi.mock 生效。
const { CodeBlockNodeView } = await import('../code-block-view');

/** 追踪所有测试创建的 view,afterEach 统一 destroy,避免 window 事件监听器跨测试泄漏。 */
const views: Array<{ destroy: () => void }> = [];
function makeView(opts: { node: any; editor: any }) {
  const view = new CodeBlockNodeView({ ...opts, getPos: undefined } as any);
  views.push(view);
  return view;
}

describe('CodeBlockNodeView mermaid 预览', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockRenderMermaid.mockReset();
    mockGetCurrentTheme.mockReturnValue('light');
  });
  afterEach(() => {
    // 统一清理:destroy 每个 view(移除主题监听 + 清 timer),再切回真实 timer。
    for (const v of views.splice(0)) v.destroy();
    vi.useRealTimers();
  });

  it('mermaid 块:构造后出现预览区,debounce 后渲染 SVG', async () => {
    mockRenderMermaid.mockResolvedValue({ svg: '<svg>flow</svg>' });
    const view = makeView({
      node: mockNode('mermaid', 'graph TD\n  A-->B'),
      editor: mockEditor(),
    } as any);

    // 预览区立即存在,显示加载态。
    const preview = view.dom.querySelector('.tiptap-codeblock-mermaid-preview');
    expect(preview).not.toBeNull();
    expect(preview?.classList.contains('mermaid-loading')).toBe(true);

    // 推进 debounce 500ms,flush 微任务。
    await vi.advanceTimersByTimeAsync(500);
    expect(mockRenderMermaid).toHaveBeenCalledWith('graph TD\n  A-->B', 'light');
    expect(preview?.innerHTML).toBe('<svg>flow</svg>');
    expect(preview?.classList.contains('mermaid-loading')).toBe(false);
  });

  it('非 mermaid 块(python):无预览区', () => {
    const view = makeView({
      node: mockNode('python', 'print(1)'),
      editor: mockEditor(),
    } as any);
    expect(view.dom.querySelector('.tiptap-codeblock-mermaid-preview')).toBeNull();
  });

  it('源码变化(update)触发 debounce 重渲染', async () => {
    mockRenderMermaid.mockResolvedValue({ svg: '<svg>v2</svg>' });
    const view = makeView({
      node: mockNode('mermaid', 'graph TD\n  A-->B'),
      editor: mockEditor(),
    } as any);
    await vi.advanceTimersByTimeAsync(500); // 首次渲染

    // 源码变化(update 传入新 node)。
    view.update(mockNode('mermaid', 'graph TD\n  A-->C') as any);
    await vi.advanceTimersByTimeAsync(500);
    expect(mockRenderMermaid).toHaveBeenLastCalledWith('graph TD\n  A-->C', 'light');
    expect(view.dom.querySelector('.tiptap-codeblock-mermaid-preview')?.innerHTML).toBe(
      '<svg>v2</svg>',
    );
  });

  it('language 从 python 切到 mermaid:创建预览区并渲染', async () => {
    mockRenderMermaid.mockResolvedValue({ svg: '<svg>m</svg>' });
    const view = makeView({
      node: mockNode('python', 'x'),
      editor: mockEditor(),
    } as any);
    expect(view.dom.querySelector('.tiptap-codeblock-mermaid-preview')).toBeNull();

    view.update(mockNode('mermaid', 'graph TD\n  A-->B') as any);
    expect(view.dom.querySelector('.tiptap-codeblock-mermaid-preview')).not.toBeNull();
    await vi.advanceTimersByTimeAsync(500);
    expect(mockRenderMermaid).toHaveBeenCalled();
  });

  it('language 从 mermaid 切到 python:移除预览区', async () => {
    mockRenderMermaid.mockResolvedValue({ svg: '<svg>m</svg>' });
    const view = makeView({
      node: mockNode('mermaid', 'graph TD\n  A-->B'),
      editor: mockEditor(),
    } as any);
    await vi.advanceTimersByTimeAsync(500);

    view.update(mockNode('python', 'x') as any);
    expect(view.dom.querySelector('.tiptap-codeblock-mermaid-preview')).toBeNull();
  });

  it('渲染失败:预览区显示错误 + mermaid-error class', async () => {
    mockRenderMermaid.mockResolvedValue({ error: 'syntax error' });
    const view = makeView({
      node: mockNode('mermaid', 'bad syntax'),
      editor: mockEditor(),
    } as any);
    await vi.advanceTimersByTimeAsync(500);

    const preview = view.dom.querySelector('.tiptap-codeblock-mermaid-preview');
    expect(preview?.classList.contains('mermaid-error')).toBe(true);
    expect(preview?.textContent).toContain('syntax error');
  });

  it('竞态:连续两次源码改动,只保留最后一次渲染结果', async () => {
    // 第一次渲染 pending(慢),第二次立即 resolve——验证慢的过期结果不覆盖快的。
    let resolveFirst!: (v: { svg: string }) => void;
    mockRenderMermaid
      .mockImplementationOnce(
        () =>
          new Promise((r) => {
            resolveFirst = () => r({ svg: '<svg>first</svg>' });
          }),
      )
      .mockResolvedValueOnce({ svg: '<svg>second</svg>' });

    const view = makeView({
      node: mockNode('mermaid', 'v1'),
      editor: mockEditor(),
    } as any);
    await vi.advanceTimersByTimeAsync(500); // 触发首次(v1),pending

    // 立即改源码(v2),token 推进,首次结果应被丢弃。
    view.update(mockNode('mermaid', 'v2') as any);
    await vi.advanceTimersByTimeAsync(500); // 触发第二次(v2),resolve
    resolveFirst({ svg: '<svg>first</svg>' }); // 第一次才 resolve(过期)

    await vi.advanceTimersByTimeAsync(0);
    const preview = view.dom.querySelector('.tiptap-codeblock-mermaid-preview');
    // 第二次结果胜出,first 被丢弃。
    expect(preview?.innerHTML).toBe('<svg>second</svg>');
  });

  it('主题切换事件触发重渲染', async () => {
    mockRenderMermaid.mockResolvedValue({ svg: '<svg>m</svg>' });
    makeView({
      node: mockNode('mermaid', 'graph TD\n  A-->B'),
      editor: mockEditor(),
    } as any);
    await vi.advanceTimersByTimeAsync(500);

    mockGetCurrentTheme.mockReturnValue('dark');
    mockRenderMermaid.mockClear();
    window.dispatchEvent(new Event('yggdrasil:theme-change'));
    await vi.advanceTimersByTimeAsync(500);

    expect(mockRenderMermaid).toHaveBeenCalledWith('graph TD\n  A-->B', 'dark');
  });

  it('destroy 清理预览区与主题监听', async () => {
    mockRenderMermaid.mockResolvedValue({ svg: '<svg>m</svg>' });
    const removeSpy = vi.spyOn(window, 'removeEventListener');
    const view = makeView({
      node: mockNode('mermaid', 'graph TD\n  A-->B'),
      editor: mockEditor(),
    } as any);
    await vi.advanceTimersByTimeAsync(500);

    view.destroy();
    expect(view.dom.querySelector('.tiptap-codeblock-mermaid-preview')).toBeNull();
    expect(removeSpy).toHaveBeenCalledWith('yggdrasil:theme-change', expect.any(Function));

    // destroy 后主题事件不再触发渲染。
    mockRenderMermaid.mockClear();
    window.dispatchEvent(new Event('yggdrasil:theme-change'));
    await vi.advanceTimersByTimeAsync(500);
    expect(mockRenderMermaid).not.toHaveBeenCalled();
    removeSpy.mockRestore();
  });
});
