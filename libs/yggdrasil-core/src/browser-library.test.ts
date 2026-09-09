import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

let load: typeof import('./browser-library').loadBrowserLibrary;

beforeEach(async () => {
  vi.resetModules();
  ({ loadBrowserLibrary: load } = await import('./browser-library'));
  const append = document.head.appendChild.bind(document.head);
  vi.spyOn(document.head, 'appendChild').mockImplementation(<T extends Node>(node: T): T => {
    // 手动控制加载完成/失败，避免 happy-dom 发出真实请求。
    if (node instanceof HTMLScriptElement) node.type = 'application/json';
    if (node instanceof HTMLLinkElement) node.rel = 'test-stylesheet';
    return append(node);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  document.head.innerHTML = '';
  for (const name of ['TiptapEditor', 'CodeMirrorEditor', 'XtermTerminal']) {
    Reflect.deleteProperty(window, name);
  }
});

describe('浏览器库按需加载', () => {
  it('导入模块不加载任何编辑器或终端资源', () => {
    expect(document.head.childElementCount).toBe(0);
  });

  it('多个组件共用一次加载，等待 JS 和 CSS 都完成后才允许挂载', async () => {
    const first = load('tiptap');
    expect(load('tiptap')).toBe(first);
    expect(document.querySelectorAll('script')).toHaveLength(1);
    expect(document.querySelectorAll('link')).toHaveLength(1);
    let finished = false;
    void first.then(() => {
      finished = true;
    });
    Reflect.set(window, 'TiptapEditor', {});
    document.querySelector('script')!.dispatchEvent(new Event('load'));
    await Promise.resolve();
    expect(finished).toBe(false);
    document.querySelector('link')!.dispatchEvent(new Event('load'));
    await first;
    expect(finished).toBe(true);
    expect(load('tiptap')).toBe(first);
  });

  it('CodeMirror 不依赖独立 CSS，也不会带入 Tiptap 或终端', async () => {
    const work = load('codemirror');
    expect(document.querySelector('script')?.getAttribute('src')).toBe('/codemirror/editor.js?v=2');
    expect(document.querySelector('link')).toBeNull();
    Reflect.set(window, 'CodeMirrorEditor', {});
    document.querySelector('script')!.dispatchEvent(new Event('load'));
    await work;
  });

  it.each(['error', 'load'])(
    '脚本 %s 后未提供运行时时清理资源，下一次调用可重试',
    async (event) => {
      const failed = expect(load('codemirror')).rejects.toThrow();
      document.querySelector('script')!.dispatchEvent(new Event(event));
      await failed;
      expect(document.head.childElementCount).toBe(0);
      const retry = load('codemirror');
      Reflect.set(window, 'CodeMirrorEditor', {});
      document.querySelector('script')!.dispatchEvent(new Event('load'));
      await retry;
    },
  );

  it('CSS 失败后重试会复用已经加载的运行时，不重复执行脚本', async () => {
    const failed = expect(load('xterm')).rejects.toThrow();
    Reflect.set(window, 'XtermTerminal', {});
    document.querySelector('script')!.dispatchEvent(new Event('load'));
    document.querySelector('link')!.dispatchEvent(new Event('error'));
    await failed;
    expect(document.head.childElementCount).toBe(0);
    const retry = load('xterm');
    expect(document.querySelector('script')).toBeNull();
    expect(document.querySelector('link')?.getAttribute('href')).toBe('/xterm/terminal.css?v=2');
    document.querySelector('link')!.dispatchEvent(new Event('load'));
    await retry;
  });
});
