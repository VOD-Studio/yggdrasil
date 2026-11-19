import { assetUrl } from '@yggdrasil/shared';
import katex from 'katex';
import 'katex/contrib/mhchem';

export const renderToString = katex.renderToString;

let style: Promise<void> | undefined;

/** 编辑器需要自己的样式链接，正文链接会在 SPA 切页时卸载。 */
export function ensureStyle(): Promise<void> {
  if (style) return style;
  const href = assetUrl('/katex/katex.min.css');
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  style = new Promise<void>((resolve, reject) => {
    link.onload = () => resolve();
    link.onerror = () => {
      link.remove();
      style = undefined;
      reject(new Error(`无法加载 ${href}`));
    };
    document.head.appendChild(link);
  });
  return style;
}
