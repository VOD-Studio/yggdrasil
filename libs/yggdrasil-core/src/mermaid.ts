/** 服务端已渲染浅/深两份 SVG（见 src/api/markdown/mermaid），这里只绑定点击放大。 */
import { attachOverlayTrigger } from './mermaid-overlay';

export function initMermaid(selector: string): void {
  document
    .querySelector(selector)
    ?.querySelectorAll<HTMLElement>('figure.mermaid-diagram')
    .forEach(attachOverlayTrigger);
}
