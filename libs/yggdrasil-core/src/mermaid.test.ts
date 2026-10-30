/**
 * mermaid 测试：服务端已渲染浅/深两份 SVG，入口只给图表绑定点击放大。
 * 黑盒驱动：通过 window.__initMermaid 入口。
 */
import { afterEach, describe, expect, it } from 'vitest';
import './enhance';
import { _resetOverlay } from './mermaid-overlay';

describe('initMermaid', () => {
  afterEach(() => {
    _resetOverlay();
    document.body.innerHTML = '';
  });

  it('点击服务端渲染的图表打开放大浮层', () => {
    document.body.innerHTML =
      '<div class="post-content"><figure class="mermaid-diagram">' +
      '<div class="mermaid-light"><svg viewBox="0 0 400 200"><rect width="10" height="10"/></svg></div>' +
      '<div class="mermaid-dark"><svg viewBox="0 0 400 200"><rect width="10" height="10"/></svg></div>' +
      '</figure></div>';
    window.__initMermaid('.post-content');
    document.querySelector<HTMLElement>('figure.mermaid-diagram')?.click();
    expect(document.querySelector('.mermaid-overlay-content svg')).not.toBeNull();
  });
});
