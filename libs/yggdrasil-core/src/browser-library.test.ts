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
  for (const name of ['TiptapEditor', 'TiptapCommentEditor', 'CodeMirrorEditor', 'XtermTerminal']) {
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
    expect(document.querySelectorAll('link')).toHaveLength(2);
    let finished = false;
    void first.then(() => {
      finished = true;
    });
    Reflect.set(window, 'TiptapEditor', {});
    document.querySelector('script')!.dispatchEvent(new Event('load'));
    await Promise.resolve();
    expect(finished).toBe(false);
    for (const link of document.querySelectorAll('link')) link.dispatchEvent(new Event('load'));
    await first;
    expect(finished).toBe(true);
    expect(load('tiptap')).toBe(first);
  });

  it('评论和写作编辑器共用一次 KaTeX 样式加载', async () => {
    const writer = load('tiptap');
    const comment = load('tiptap-comment');
    expect(document.querySelectorAll('link[href="/katex/katex.min.css"]')).toHaveLength(1);
    Reflect.set(window, 'TiptapEditor', {});
    Reflect.set(window, 'TiptapCommentEditor', {});
    for (const node of document.head.children) node.dispatchEvent(new Event('load'));
    await Promise.all([writer, comment]);
  });

  it('借用正文样式后，路由卸载样式时仍可再次打开编辑器', async () => {
    document.head.insertAdjacentHTML(
      'beforeend',
      '<link rel="stylesheet" href="/katex/katex.min.css" id="markdown-style">',
    );
    const ssrStyle = document.getElementById('markdown-style')!;
    Object.defineProperty(ssrStyle, 'sheet', { value: {} });
    vi.spyOn(document, 'querySelector').mockReturnValueOnce(ssrStyle);
    Reflect.set(window, 'TiptapCommentEditor', {});
    const first = load('tiptap-comment');
    expect(document.querySelectorAll('link[href="/katex/katex.min.css"]')).toHaveLength(1);
    document.querySelector('link[href="/tiptap/comment.css"]')!.dispatchEvent(new Event('load'));
    await first;

    ssrStyle.remove();
    const again = load('tiptap-comment');
    expect(again).not.toBe(first);
    document.querySelector('link[href="/katex/katex.min.css"]')!.dispatchEvent(new Event('load'));
    await again;
  });

  it('CodeMirror 不依赖独立 CSS，也不会带入 Tiptap 或终端', async () => {
    document.head.insertAdjacentHTML(
      'beforeend',
      '<script type="application/json" id="ygg-asset-versions">{"/codemirror/editor-module.js":"c0de"}</script>',
    );
    const work = load('codemirror');
    expect(document.querySelector('script[src]')?.getAttribute('src')).toBe(
      '/codemirror/editor-module.js?v=c0de',
    );
    expect(document.querySelector('link')).toBeNull();
    Reflect.set(window, 'CodeMirrorEditor', {});
    document.querySelector('script[src]')!.dispatchEvent(new Event('load'));
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
      expect(document.querySelector('script')?.getAttribute('src')).toBe(
        '/codemirror/editor-module.js?retry=1',
      );
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
    expect(document.querySelector('link')?.getAttribute('href')).toBe('/xterm/terminal.css');
    document.querySelector('link')!.dispatchEvent(new Event('load'));
    await retry;
  });
});
