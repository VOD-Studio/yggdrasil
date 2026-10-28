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
  },
  codemirror: { global: 'CodeMirrorEditor', script: '/codemirror/editor.js' },
  xterm: {
    global: 'XtermTerminal',
    script: '/xterm/terminal.js',
    style: '/xterm/terminal.css',
  },
} as const;

export type BrowserLibrary = keyof typeof libraries;
const pending = new Map<BrowserLibrary, Promise<void>>();

export function loadBrowserLibrary(name: BrowserLibrary): Promise<void> {
  const existing = pending.get(name);
  if (existing) return existing;
  const library = libraries[name];
  if (!library) return Promise.reject(new Error(`Unknown browser library: ${name}`));

  const elements: HTMLElement[] = [];
  const script = new Promise<void>((resolve, reject) => {
    if (Reflect.get(window, library.global)) {
      resolve();
      return;
    }
    const element = document.createElement('script');
    element.src = assetUrl(library.script);
    element.onload = () => {
      if (Reflect.get(window, library.global)) resolve();
      else reject(new Error(`${library.global} unavailable after loading`));
    };
    element.onerror = () => reject(new Error(`Failed to load ${library.script}`));
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
  const work = Promise.all([script, style])
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
