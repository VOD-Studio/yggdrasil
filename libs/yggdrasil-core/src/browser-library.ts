import { assetUrl } from '@yggdrasil/shared';

/**
 * 编辑器/终端仅由对应组件请求；同页多个实例共享资源，失败后允许重试。
 * 地址的版本号由服务端按内容生成（`assetUrl`），这几个路径须列在 `src/static_assets.rs` 的 `ALL`。
 */
const libraries = {
  tiptap: {
    global: 'TiptapEditor',
    script: '/tiptap/editor.js',
    style: '/tiptap/editor.css',
    math: true,
  },
  'tiptap-comment': {
    global: 'TiptapCommentEditor',
    script: '/tiptap/comment.js',
    style: '/tiptap/comment.css',
    math: true,
  },
  codemirror: { global: 'CodeMirrorEditor', script: '/codemirror/editor-module.js', module: true },
  xterm: {
    global: 'XtermTerminal',
    script: '/xterm/terminal.js',
    style: '/xterm/terminal.css',
  },
} as const;

export type BrowserLibrary = keyof typeof libraries;
const pending = new Map<BrowserLibrary, Promise<void>>();
let katexStyle: Promise<void> | undefined;
let codeMirrorRetry = 0;

function katexLink(href = assetUrl('/katex/katex.min.css')): HTMLLinkElement | null {
  return document.querySelector<HTMLLinkElement>(`link[rel="stylesheet"][href="${href}"]`);
}

function loadKatexStyle(): Promise<void> {
  if (katexStyle) return katexStyle;
  const href = assetUrl('/katex/katex.min.css');
  if (katexLink(href)?.sheet) return Promise.resolve();

  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  katexStyle = new Promise<void>((resolve, reject) => {
    link.onload = () => resolve();
    link.onerror = () => {
      link.remove();
      katexStyle = undefined;
      reject(new Error(`Failed to load ${href}`));
    };
    document.head.appendChild(link);
  });
  return katexStyle;
}

export function loadBrowserLibrary(name: BrowserLibrary): Promise<void> {
  const library = libraries[name];
  if (!library) return Promise.reject(new Error(`Unknown browser library: ${name}`));
  const existing = pending.get(name);
  if (existing) {
    // 正文声明的 KaTeX 样式随路由卸载，之前已加载的编辑器也要重新确认样式。
    if (!('math' in library) || katexStyle || katexLink()?.sheet) return existing;
    const work = Promise.all([existing, loadKatexStyle()])
      .then(() => {})
      .catch((error) => {
        pending.delete(name);
        throw error;
      });
    pending.set(name, work);
    return work;
  }

  const elements: HTMLElement[] = [];
  const script = new Promise<void>((resolve, reject) => {
    if (Reflect.get(window, library.global)) {
      resolve();
      return;
    }
    const element = document.createElement('script');
    const href = assetUrl(library.script);
    const isModule = 'module' in library;
    if (isModule) element.type = 'module';
    // 浏览器会在文档的模块表中缓存加载失败，用新地址重试入口。
    element.src =
      isModule && codeMirrorRetry > 0
        ? `${href}${href.includes('?') ? '&' : '?'}retry=${codeMirrorRetry}`
        : href;
    const fail = (error: Error) => {
      if (isModule) codeMirrorRetry++;
      reject(error);
    };
    element.onload = () => {
      if (Reflect.get(window, library.global)) resolve();
      else fail(new Error(`${library.global} unavailable after loading`));
    };
    element.onerror = () => fail(new Error(`Failed to load ${library.script}`));
    elements.push(element);
    document.head.appendChild(element);
  });
  const style = new Promise<void>((resolve, reject) => {
    if (!('style' in library)) {
      resolve();
      return;
    }
    const element = document.createElement('link');
    element.rel = 'stylesheet';
    element.href = assetUrl(library.style);
    element.onload = () => resolve();
    element.onerror = () => reject(new Error(`Failed to load ${library.style}`));
    elements.push(element);
    document.head.appendChild(element);
  });
  const work = Promise.all([script, style, 'math' in library ? loadKatexStyle() : undefined])
    .then(() => {})
    .catch((error) => {
      elements.forEach((element) => {
        element.remove();
      });
      pending.delete(name);
      throw error;
    });
  pending.set(name, work);
  return work;
}
