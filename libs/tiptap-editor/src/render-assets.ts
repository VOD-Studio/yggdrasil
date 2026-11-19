import { assetUrl } from '@yggdrasil/shared';

declare const __YGG_TIPTAP_ASSETS__: boolean;

function lazyRenderer<T>(read: (attempt: number) => Promise<T>) {
  let ready: T | undefined;
  let pending: Promise<T> | undefined;
  let attempt = 0;
  return {
    get current() {
      return ready;
    },
    load(): Promise<T> {
      pending ??= read(attempt)
        .then((renderer) => {
          ready = renderer;
          return renderer;
        })
        .catch((error) => {
          pending = undefined;
          attempt++;
          throw error;
        });
      return pending;
    },
  };
}

function fromPublic<T>(path: string, attempt: number): Promise<T> {
  const href = assetUrl(path);
  // 浏览器会缓存模块加载失败，重试时使用新地址。
  const url = attempt ? `${href}${href.includes('?') ? '&' : '?'}retry=${attempt}` : href;
  return import(/* @vite-ignore */ url) as Promise<T>;
}

// 两个 IIFE 请求同一地址，浏览器复用下载和模块实例。单测走同一份源码。
export const mathRenderer = lazyRenderer(
  typeof __YGG_TIPTAP_ASSETS__ !== 'undefined'
    ? async (attempt) => {
        const renderer = await fromPublic<typeof import('./math-renderer')>(
          '/tiptap/math-renderer.js',
          attempt,
        );
        await renderer.ensureStyle();
        return renderer;
      }
    : () => import('./math-renderer'),
);
export const codeHighlighter = lazyRenderer(
  typeof __YGG_TIPTAP_ASSETS__ !== 'undefined'
    ? (attempt) =>
        fromPublic<typeof import('./code-highlighter')>('/tiptap/code-highlighter.js', attempt)
    : () => import('./code-highlighter'),
);
