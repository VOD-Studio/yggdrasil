/** 编辑器/终端仅由对应组件请求；同页多个实例共享资源，失败后允许重试。 */
const libraries = {
  tiptap: {
    global: 'TiptapEditor',
    script: '/tiptap/editor.js?v=2',
    style: '/tiptap/editor.css?v=2',
  },
  codemirror: { global: 'CodeMirrorEditor', script: '/codemirror/editor.js?v=2' },
  xterm: {
    global: 'XtermTerminal',
    script: '/xterm/terminal.js?v=2',
    style: '/xterm/terminal.css?v=2',
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
    element.src = library.script;
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
    element.href = library.style;
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
