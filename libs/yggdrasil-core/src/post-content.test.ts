/**
 * post-content 测试:钉住代码块 copy 按钮的生成与复制行为。
 * 黑盒驱动:只通过 window.__initPostContent 入口。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import './enhance';

describe('initPostContent', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    vi.restoreAllMocks();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    document.body.innerHTML = '';
  });

  it('为 pre>code 注入 .copy-code 按钮', () => {
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code>console.log(1)</code></pre>';
    document.body.appendChild(root);

    window.__initPostContent('.post-content');

    const btn = root.querySelector('pre .copy-code');
    expect(btn).not.toBeNull();
    expect(btn?.textContent).toBe('copy');
  });

  it('已有 .copy-code 的 pre 不重复注入', () => {
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code>x</code><button class="copy-code">copy</button></pre>';
    document.body.appendChild(root);

    window.__initPostContent('.post-content');

    const btns = root.querySelectorAll('pre .copy-code');
    expect(btns.length).toBe(1);
  });

  it('点击按钮调用 clipboard.writeText 并回显 copied!', () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });

    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code>hello</code></pre>';
    document.body.appendChild(root);

    window.__initPostContent('.post-content');

    const btn = root.querySelector('.copy-code') as HTMLButtonElement;
    btn.click();

    expect(writeText).toHaveBeenCalledWith('hello');
    expect(btn.textContent).toBe('copied!');

    // 2 秒后还原回 'copy'
    vi.advanceTimersByTime(2000);
    expect(btn.textContent).toBe('copy');
  });

  it('selector 未命中时不报错', () => {
    expect(() => window.__initPostContent('.not-exist')).not.toThrow();
  });
});
