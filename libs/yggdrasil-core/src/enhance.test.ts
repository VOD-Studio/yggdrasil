/**
 * enhance 入口测试：钉住 P5 §8.2 的全局注册契约与增强调用队列语义。
 *
 * 生产顺序是 core（阻塞）先于 enhance（defer）执行，测试按同一顺序 import 两个
 * 入口，让注册与 flush 走真实代码路径。
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import './index';
import './enhance';

const ENHANCE_GLOBALS = [
  '__initPostContent',
  '__initMermaid',
  '__initTocSidebar',
  '__disposeTocSidebar',
  '__initChangelogNav',
  '__disposeChangelogNav',
  '__initShowcaseMasonry',
  '__disposeShowcaseMasonry',
] as const;

describe('enhance entry registrations', () => {
  beforeEach(() => {
    window.__pendingEnhanceCalls = [];
    document.body.innerHTML = '';
  });

  it('注册全部增强函数', () => {
    for (const name of ENHANCE_GLOBALS) {
      expect(typeof window[name], name).toBe('function');
    }
  });

  it('flush 只执行已就绪的函数，未就绪的保留在队列里', () => {
    const initToc = vi.spyOn(window, '__initTocSidebar').mockImplementation(() => {});
    window.__pendingEnhanceCalls.push(
      { name: '__initTocSidebar', args: [] },
      { name: '__initMermaid', args: ['.post-content', 'dark'] },
      { name: '__initPostContent', args: ['.post-content'] },
    );
    // 卸掉一个实现，模拟对应脚本尚未加载。
    const mermaid = window.__initMermaid;
    delete (window as unknown as Record<string, unknown>).__initMermaid;

    window.__flushEnhanceCalls();

    expect(initToc).toHaveBeenCalledTimes(1);
    expect(window.__pendingEnhanceCalls.map((call) => call.name)).toEqual(['__initMermaid']);

    // 对应脚本随后加载完成：注册回来再 flush，滞留项补执行。
    window.__initMermaid = mermaid;
    window.__flushEnhanceCalls();
    expect(window.__pendingEnhanceCalls).toHaveLength(0);
  });

  it('flush 幂等：重复调用不会重复执行已消费的调用', () => {
    const masonry = vi.spyOn(window, '__initShowcaseMasonry').mockImplementation(() => {});
    window.__pendingEnhanceCalls.push({ name: '__initShowcaseMasonry', args: [] });
    window.__flushEnhanceCalls();
    window.__flushEnhanceCalls();
    expect(masonry).toHaveBeenCalledTimes(1);
  });

  it('core 的主题桥接接通同一 registry：经桥接订阅的回调能收到主题通知', () => {
    // mermaid.ts 顶层即这样订阅；applyResolvedTheme 走 core registry 通知全部回调。
    const seen: boolean[] = [];
    const unsubscribe = window.__subscribeThemeChange((isDark) => {
      seen.push(isDark);
    });
    window.__applyResolvedTheme(true);
    window.__applyResolvedTheme(false);
    unsubscribe();
    window.__applyResolvedTheme(true);
    expect(seen).toEqual([true, false]);
  });
});
