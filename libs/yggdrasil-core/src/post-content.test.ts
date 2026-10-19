/**
 * post-content 测试:钉住代码块复制按钮的生成与复制行为。
 * 黑盒驱动:只通过 window.__initPostContent 入口。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import './index';

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
    expect(btn?.getAttribute('aria-label')).toBe('复制代码');
    expect(btn?.querySelector('svg')).not.toBeNull();
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

  it('点击按钮调用 clipboard.writeText 并回显已复制', () => {
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
    expect(btn.getAttribute('aria-label')).toBe('已复制');
    expect(btn.dataset.copied).toBe('true');

    // 2 秒后还原
    vi.advanceTimersByTime(2000);
    expect(btn.getAttribute('aria-label')).toBe('复制代码');
    expect(btn.dataset.copied).toBeUndefined();
  });

  it('selector 未命中时不报错', () => {
    expect(() => window.__initPostContent('.not-exist')).not.toThrow();
  });
});
