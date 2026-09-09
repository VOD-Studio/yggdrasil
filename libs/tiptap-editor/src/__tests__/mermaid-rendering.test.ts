import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderMermaid } from '../mermaid';

afterEach(() => {
  delete window.MermaidRenderer;
  document.body.innerHTML = '';
});

describe('编辑器 Mermaid 渲染适配', () => {
  it('经共享渲染器返回修正的 SVG，并使用独立且唯一的 id', async () => {
    const render = vi.fn().mockResolvedValue({ svg: '<svg><foreignObject /></svg>' });
    window.MermaidRenderer = { initialize: vi.fn(), render };
    expect(await renderMermaid('graph TD; A-->B', 'light')).toEqual({
      svg: '<svg><foreignObject overflow="visible" /></svg>',
    });
    await renderMermaid('graph TD; A-->C', 'dark');
    const ids = render.mock.calls.map(([id]) => id as string);
    expect(ids.every((id) => id.startsWith('tiptap-mermaid-'))).toBe(true);
    expect(new Set(ids).size).toBe(2);
  });

  it.each([new Error('invalid diagram'), 'invalid diagram'])(
    '错误转为预览提示并清理残留 SVG',
    async (error) => {
      window.MermaidRenderer = {
        initialize: vi.fn(),
        render: vi.fn(async (id: string) => {
          const temporary = document.createElement('div');
          temporary.id = `d${id}`;
          document.body.appendChild(temporary);
          throw error;
        }),
      };
      expect(await renderMermaid('invalid', 'light')).toEqual({ error: 'invalid diagram' });
      expect(document.body.childElementCount).toBe(0);
    },
  );
});
