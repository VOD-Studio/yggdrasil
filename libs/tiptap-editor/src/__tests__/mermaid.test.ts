// @vitest-environment happy-dom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CodeBlockNodeView } from '../code-block-view';

/**
 * mermaid 代码块预览测试。
 *
 * 渲染由宿主经 editor.storage.__onRenderMermaid 注入（调服务端），这里用 vi.fn 代替。
 * 重点验证 NodeView 的生命周期:预览区创建/移除、debounce、竞态取消、错误态、清理。
 */

const CODEBLOCK_TYPE = { name: 'codeBlock' };

type ViewProps = ConstructorParameters<typeof CodeBlockNodeView>[0];
type PMNode = ViewProps['node'];

// 最小桩：NodeView 只读取 type / attrs.language / textContent 与 editor.storage。
function mockNode(language: string, textContent = ''): PMNode {
  return { type: CODEBLOCK_TYPE, attrs: { language }, textContent } as unknown as PMNode;
}

const onRender = vi.fn<(source: string) => Promise<string>>();

function mockEditor(withCallback = true): ViewProps['editor'] {
  const storage = withCallback ? { __onRenderMermaid: onRender } : {};
  return { storage } as unknown as ViewProps['editor'];
}

/** 追踪所有测试创建的 view,afterEach 统一 destroy,避免 timer 跨测试泄漏。 */
const views: Array<{ destroy: () => void }> = [];
function makeView(opts: { node: PMNode; editor: ViewProps['editor'] }) {
  const view = new CodeBlockNodeView({ ...opts, getPos: undefined } as unknown as ViewProps);
  views.push(view);
  return view;
}

const PREVIEW = '.tiptap-codeblock-mermaid-preview';

describe('CodeBlockNodeView mermaid 预览', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    onRender.mockReset();
  });
  afterEach(() => {
    for (const v of views.splice(0)) v.destroy();
    vi.useRealTimers();
  });

  it('mermaid 块:构造后出现预览区,debounce 后渲染 SVG', async () => {
    onRender.mockResolvedValue('<div class="mermaid-light"><svg>flow</svg></div>');
    const view = makeView({ node: mockNode('mermaid', 'graph TD\n  A-->B'), editor: mockEditor() });

    // 预览区立即存在,显示加载态。
    const preview = view.dom.querySelector(PREVIEW);
    expect(preview).not.toBeNull();
    expect(preview?.classList.contains('mermaid-loading')).toBe(true);

    await vi.advanceTimersByTimeAsync(500);
    expect(onRender).toHaveBeenCalledWith('graph TD\n  A-->B');
    expect(preview?.innerHTML).toBe('<div class="mermaid-light"><svg>flow</svg></div>');
    expect(preview?.classList.contains('mermaid-loading')).toBe(false);
  });

  it('非 mermaid 块(python):无预览区', () => {
    const view = makeView({ node: mockNode('python', 'print(1)'), editor: mockEditor() });
    expect(view.dom.querySelector(PREVIEW)).toBeNull();
  });

  it('storage 无回调:防抖后移除预览区', async () => {
    const view = makeView({
      node: mockNode('mermaid', 'graph TD\n  A-->B'),
      editor: mockEditor(false),
    });
    expect(view.dom.querySelector(PREVIEW)).not.toBeNull();
    await vi.advanceTimersByTimeAsync(500);
    expect(view.dom.querySelector(PREVIEW)).toBeNull();
  });

  it('源码变化(update)触发 debounce 重渲染', async () => {
    onRender.mockResolvedValue('<svg>v2</svg>');
    const view = makeView({ node: mockNode('mermaid', 'graph TD\n  A-->B'), editor: mockEditor() });
    await vi.advanceTimersByTimeAsync(500); // 首次渲染

    view.update(mockNode('mermaid', 'graph TD\n  A-->C'));
    await vi.advanceTimersByTimeAsync(500);
    expect(onRender).toHaveBeenLastCalledWith('graph TD\n  A-->C');
    expect(view.dom.querySelector(PREVIEW)?.innerHTML).toBe('<svg>v2</svg>');
  });

  it('language 从 python 切到 mermaid:创建预览区并渲染', async () => {
    onRender.mockResolvedValue('<svg>m</svg>');
    const view = makeView({ node: mockNode('python', 'x'), editor: mockEditor() });
    expect(view.dom.querySelector(PREVIEW)).toBeNull();

    view.update(mockNode('mermaid', 'graph TD\n  A-->B'));
    expect(view.dom.querySelector(PREVIEW)).not.toBeNull();
    await vi.advanceTimersByTimeAsync(500);
    expect(onRender).toHaveBeenCalled();
  });

  it('language 从 mermaid 切到 python:移除预览区', async () => {
    onRender.mockResolvedValue('<svg>m</svg>');
    const view = makeView({ node: mockNode('mermaid', 'graph TD\n  A-->B'), editor: mockEditor() });
    await vi.advanceTimersByTimeAsync(500);

    view.update(mockNode('python', 'x'));
    expect(view.dom.querySelector(PREVIEW)).toBeNull();
  });

  it('渲染失败:预览区显示错误消息 + mermaid-error class', async () => {
    onRender.mockRejectedValue(new Error('图表渲染失败：x'));
    const view = makeView({ node: mockNode('mermaid', 'bad syntax'), editor: mockEditor() });
    await vi.advanceTimersByTimeAsync(500);

    const preview = view.dom.querySelector(PREVIEW);
    expect(preview?.classList.contains('mermaid-error')).toBe(true);
    expect(preview?.classList.contains('mermaid-loading')).toBe(false);
    expect(preview?.textContent).toBe('图表渲染失败：x');
  });

  it('竞态:连续两次源码改动,只保留最后一次渲染结果', async () => {
    // 第一次渲染 pending(慢),第二次立即 resolve——验证慢的过期结果不覆盖快的。
    // 库目标低于 ES2024，没有 Promise.withResolvers，只能用 executor 取出 resolve。
    let resolveFirst!: (html: string) => void;
    const first = new Promise<string>((resolve) => {
      resolveFirst = resolve;
    });
    onRender.mockImplementationOnce(() => first).mockResolvedValueOnce('<svg>second</svg>');

    const view = makeView({ node: mockNode('mermaid', 'v1'), editor: mockEditor() });
    await vi.advanceTimersByTimeAsync(500); // 触发首次(v1),pending

    view.update(mockNode('mermaid', 'v2'));
    await vi.advanceTimersByTimeAsync(500); // 触发第二次(v2),resolve
    resolveFirst('<svg>first</svg>'); // 第一次才 resolve(过期)

    await vi.advanceTimersByTimeAsync(0);
    expect(view.dom.querySelector(PREVIEW)?.innerHTML).toBe('<svg>second</svg>');
  });

  it('destroy 移除预览区,之后不再渲染', async () => {
    onRender.mockResolvedValue('<svg>m</svg>');
    const view = makeView({ node: mockNode('mermaid', 'graph TD\n  A-->B'), editor: mockEditor() });

    view.destroy();
    expect(view.dom.querySelector(PREVIEW)).toBeNull();
    await vi.advanceTimersByTimeAsync(500);
    expect(onRender).not.toHaveBeenCalled();
  });
});
