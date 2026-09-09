/**
 * mermaid 测试：钉住懒加载渲染的扫描、幂等、主题适配与错误回退。
 * 黑盒驱动：通过 window.__initMermaid 入口，mock IntersectionObserver 与 mermaid bundle 加载。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import './index';
import { _resetOverlay } from './mermaid-overlay';

// mock IntersectionObserver：observe 时立即异步触发 isIntersecting 回调，模拟块进视口。
const disconnect = vi.fn();
const observe = vi.fn();
let intersectCallback: ((entries: { isIntersecting: boolean }[]) => void) | null = null;
vi.stubGlobal(
  'IntersectionObserver',
  class {
    constructor(cb: (entries: { isIntersecting: boolean }[]) => void) {
      intersectCallback = cb;
    }
    observe() {
      observe();
      // 立即触发可见，模拟块已在视口内。
      if (intersectCallback) intersectCallback([{ isIntersecting: true }]);
    }
    disconnect() {
      disconnect();
    }
  },
);

describe('initMermaid', () => {
  const mockRender = vi.fn().mockResolvedValue({ svg: '<svg>diagram</svg>' });
  const mockInitialize = vi.fn();

  beforeEach(() => {
    document.body.innerHTML = '';
    mockRender.mockResolvedValue({ svg: '<svg>diagram</svg>' });
    mockRender.mockClear();
    mockInitialize.mockClear();
    observe.mockClear();
    disconnect.mockClear();
    // 注入已加载的 Mermaid API，加载失败与重试由 shared 的测试覆盖。
    window.MermaidRenderer = { initialize: mockInitialize, render: mockRender };
  });
  afterEach(() => {
    delete window.MermaidRenderer;
    document.body.innerHTML = '';
  });

  it('扫描 language-mermaid 块并渲染成 SVG', async () => {
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
    document.body.appendChild(root);

    window.__initMermaid('.post-content', 'light');

    await vi.waitFor(() => {
      expect(root.querySelector('pre')?.innerHTML).toContain('<svg>diagram</svg>');
    });
  });

  it('用 dark 主题初始化 mermaid', async () => {
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
    document.body.appendChild(root);

    window.__initMermaid('.post-content', 'dark');

    await vi.waitFor(() => {
      // base 主题 + darkMode 标志：让 themeVariables 完全控制 Catppuccin 调色板
      expect(mockInitialize).toHaveBeenCalledWith(
        expect.objectContaining({ theme: 'base', darkMode: true }),
      );
    });
  });

  it('亮/暗主题传入对应的 Catppuccin themeVariables', async () => {
    // 暗色：Mocha 调色板（背景 #313244、文字 #cdd6f4）
    const darkRoot = document.createElement('div');
    darkRoot.className = 'post-content';
    darkRoot.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
    document.body.appendChild(darkRoot);
    window.__initMermaid('.post-content', 'dark');
    await vi.waitFor(() => {
      expect(mockInitialize).toHaveBeenCalled();
    });
    const darkCall = mockInitialize.mock.calls.slice(-1)[0]?.[0] as Record<string, unknown>;
    const darkVars = darkCall.themeVariables as Record<string, string>;
    expect(darkVars.background).toBe('#313244');
    expect(darkVars.primaryTextColor).toBe('#cdd6f4');
    expect(darkVars.lineColor).toBe('#a6adc8');

    // 亮色：Latte 调色板（背景 #dce0e8、文字 #4c4f69）
    document.body.innerHTML = '';
    const lightRoot = document.createElement('div');
    lightRoot.className = 'post-content';
    lightRoot.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
    document.body.appendChild(lightRoot);
    window.__initMermaid('.post-content', 'light');
    await vi.waitFor(() => {
      expect(mockInitialize).toHaveBeenCalled();
    });
    const lightCall = mockInitialize.mock.calls.slice(-1)[0]?.[0] as Record<string, unknown>;
    const lightVars = lightCall.themeVariables as Record<string, string>;
    expect(lightVars.background).toBe('#dce0e8');
    expect(lightVars.primaryTextColor).toBe('#4c4f69');
    expect(lightVars.lineColor).toBe('#5c5f77');
  });

  it('幂等：重复调用不重复渲染已处理的块', async () => {
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
    document.body.appendChild(root);

    window.__initMermaid('.post-content', 'light');
    await vi.waitFor(() => {
      expect(root.querySelector('pre')?.dataset.mermaidRendered).toBe('true');
    });
    mockRender.mockClear();

    // 第二次调用（模拟上下篇切换）：不应再次 render
    window.__initMermaid('.post-content', 'light');
    await new Promise((r) => setTimeout(r, 50));
    expect(mockRender).not.toHaveBeenCalled();
  });

  it('非 mermaid 代码块不受影响', () => {
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code class="language-rust">fn main() {}</code></pre>';
    document.body.appendChild(root);

    expect(() => window.__initMermaid('.post-content', 'light')).not.toThrow();
    // 不应尝试渲染 rust 块
    expect(observe).not.toHaveBeenCalled();
  });

  it('selector 未命中时不报错', () => {
    expect(() => window.__initMermaid('.not-exist', 'light')).not.toThrow();
  });

  it('渲染失败时加 mermaid-error class', async () => {
    mockRender.mockRejectedValueOnce(new Error('syntax error'));
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code class="language-mermaid">bad syntax</code></pre>';
    document.body.appendChild(root);

    window.__initMermaid('.post-content', 'light');

    await vi.waitFor(() => {
      expect(root.querySelector('pre')?.classList.contains('mermaid-error')).toBe(true);
    });
  });

  it('主题切换时重渲染已渲染的块', async () => {
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
    document.body.appendChild(root);

    // 首次渲染（light）
    window.__initMermaid('.post-content', 'light');
    await vi.waitFor(() => {
      expect(root.querySelector('pre')?.dataset.mermaidRendered).toBe('true');
    });
    expect(root.querySelector('pre')?.dataset.mermaidTheme).toBe('light');
    const firstRenderCalls = mockRender.mock.calls.length;

    // 主题切换 → dark：应触发重渲染
    window.__initMermaid('.post-content', 'dark');
    await vi.waitFor(() => {
      expect(mockRender.mock.calls.length).toBeGreaterThan(firstRenderCalls);
    });
    expect(mockInitialize).toHaveBeenLastCalledWith(
      expect.objectContaining({ theme: 'base', darkMode: true }),
    );
    expect(root.querySelector('pre')?.dataset.mermaidTheme).toBe('dark');
  });

  it('主题未变时重渲染路径幂等（同主题跳过）', async () => {
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
    document.body.appendChild(root);

    window.__initMermaid('.post-content', 'light');
    await vi.waitFor(() => {
      expect(root.querySelector('pre')?.dataset.mermaidRendered).toBe('true');
    });
    mockRender.mockClear();

    // 同主题再调（模拟上下篇切换复用组件实例、effect 重跑）
    window.__initMermaid('.post-content', 'light');
    await new Promise((r) => setTimeout(r, 50));
    expect(mockRender).not.toHaveBeenCalled();
  });

  it('快速切回已有主题时，过期异步 SVG 不覆盖当前主题', async () => {
    document.body.innerHTML =
      '<div class="post-content"><pre data-mermaid-rendered="true" data-mermaid-theme="light" data-mermaid-source="graph TD; A-->B"><svg>light</svg></pre></div>';
    const pre = document.querySelector('pre')!;
    let resolveDark!: (value: { svg: string }) => void;
    mockRender.mockImplementationOnce(
      () =>
        new Promise<{ svg: string }>((resolve) => {
          resolveDark = resolve;
        }),
    );

    const darkWork = window.__initMermaid('.post-content', 'dark');
    await vi.waitFor(() => expect(mockRender).toHaveBeenCalledOnce());
    await window.__initMermaid('.post-content', 'light');
    resolveDark({ svg: '<svg>stale dark</svg>' });
    await darkWork;

    expect(pre.dataset.mermaidTheme).toBe('light');
    expect(pre.innerHTML).toBe('<svg>light</svg>');
    expect(mockRender).toHaveBeenCalledOnce();
  });

  it('已取消的主题渲染失败不会给当前图表加错误标记', async () => {
    document.body.innerHTML =
      '<div class="post-content"><pre data-mermaid-rendered="true" data-mermaid-theme="light" data-mermaid-source="graph TD; A-->B"><svg>light</svg></pre></div>';
    const pre = document.querySelector('pre')!;
    let rejectDark!: (error: Error) => void;
    mockRender.mockImplementationOnce(
      () =>
        new Promise<{ svg: string }>((_resolve, reject) => {
          rejectDark = reject;
        }),
    );

    const darkWork = window.__initMermaid('.post-content', 'dark');
    await vi.waitFor(() => expect(mockRender).toHaveBeenCalledOnce());
    await window.__initMermaid('.post-content', 'light');
    rejectDark(new Error('superseded render failed'));
    await darkWork;

    expect(pre.classList.contains('mermaid-error')).toBe(false);
    expect(pre.dataset.mermaidTheme).toBe('light');
    expect(pre.innerHTML).toBe('<svg>light</svg>');
  });

  it('渲染完成前离开页面时，不再改写已卸载的图表', async () => {
    document.body.innerHTML =
      '<div class="post-content"><pre data-mermaid-rendered="true" data-mermaid-theme="light" data-mermaid-source="graph TD; A-->B"><svg>light</svg></pre></div>';
    const pre = document.querySelector('pre')!;
    let resolveDark!: (value: { svg: string }) => void;
    mockRender.mockImplementationOnce(
      () =>
        new Promise<{ svg: string }>((resolve) => {
          resolveDark = resolve;
        }),
    );

    const darkWork = window.__initMermaid('.post-content', 'dark');
    await vi.waitFor(() => expect(mockRender).toHaveBeenCalledOnce());
    document.body.innerHTML = '';
    resolveDark({ svg: '<svg>stale dark</svg>' });
    await darkWork;

    expect(pre.innerHTML).toBe('<svg>light</svg>');
    expect(pre.dataset.mermaidTheme).toBe('light');
  });

  it('主题切换重渲染用唯一 render id（避免 mermaid 残留节点冲突）', async () => {
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
    document.body.appendChild(root);

    window.__initMermaid('.post-content', 'light');
    await vi.waitFor(() => {
      expect(root.querySelector('pre')?.dataset.mermaidRendered).toBe('true');
    });
    const firstId = mockRender.mock.calls[0][0];

    window.__initMermaid('.post-content', 'dark');
    await vi.waitFor(() => {
      expect(mockRender.mock.calls.length).toBeGreaterThanOrEqual(2);
    });
    const secondId = mockRender.mock.calls[mockRender.mock.calls.length - 1][0];

    // 两次 render 的 id 必须不同，否则撞上 mermaid 内部残留的 d-前缀节点（#357）
    expect(secondId).not.toBe(firstId);
  });

  it('有 mermaid 块时在空闲期预拉 bundle', () => {
    const ricSpy = vi.fn((cb: () => void) => {
      cb();
      return 1;
    });
    const originalRic = window.requestIdleCallback;
    window.requestIdleCallback = ricSpy as unknown as typeof window.requestIdleCallback;
    try {
      const root = document.createElement('div');
      root.className = 'post-content';
      root.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
      document.body.appendChild(root);

      window.__initMermaid('.post-content', 'light');

      expect(ricSpy).toHaveBeenCalledTimes(1);
    } finally {
      if (originalRic) {
        window.requestIdleCallback = originalRic;
      } else {
        Reflect.deleteProperty(window, 'requestIdleCallback');
      }
    }
  });

  it('无 mermaid 块时不调度空闲预拉', () => {
    const ricSpy = vi.fn((cb: () => void) => {
      cb();
      return 1;
    });
    const originalRic = window.requestIdleCallback;
    window.requestIdleCallback = ricSpy as unknown as typeof window.requestIdleCallback;
    try {
      const root = document.createElement('div');
      root.className = 'post-content';
      root.innerHTML = '<pre><code class="language-rust">fn main() {}</code></pre>';
      document.body.appendChild(root);

      window.__initMermaid('.post-content', 'light');

      expect(ricSpy).not.toHaveBeenCalled();
    } finally {
      if (originalRic) {
        window.requestIdleCallback = originalRic;
      } else {
        Reflect.deleteProperty(window, 'requestIdleCallback');
      }
    }
  });

  it('无 requestIdleCallback 时回退 setTimeout 预拉', () => {
    const originalRic = window.requestIdleCallback;
    Reflect.deleteProperty(window, 'requestIdleCallback');
    const timeoutSpy = vi.spyOn(globalThis, 'setTimeout');
    try {
      const root = document.createElement('div');
      root.className = 'post-content';
      root.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
      document.body.appendChild(root);

      window.__initMermaid('.post-content', 'light');

      expect(timeoutSpy).toHaveBeenCalledWith(expect.any(Function), 200);
    } finally {
      timeoutSpy.mockRestore();
      if (originalRic) window.requestIdleCallback = originalRic;
    }
  });

  it('渲染期间显示加载角标，成功后随 SVG 替换消失', async () => {
    let resolveRender: ((v: { svg: string }) => void) | undefined;
    mockRender.mockImplementation(
      () =>
        new Promise<{ svg: string }>((res) => {
          resolveRender = res;
        }),
    );
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
    document.body.appendChild(root);

    window.__initMermaid('.post-content', 'light');

    // IO mock 同步触发 render，角标在 renderBlock 首个 await 前同步挂上
    const badge = root.querySelector('.mermaid-loading');
    expect(badge).not.toBeNull();
    expect(badge?.textContent).toBe('图表渲染中');

    // mermaid.render 在 await loadMermaid() 之后才调用（微任务后），先等它被调再放行
    await vi.waitFor(() => expect(mockRender).toHaveBeenCalled());
    resolveRender?.({ svg: '<svg>diagram</svg>' });
    await vi.waitFor(() => {
      expect(root.querySelector('pre')?.innerHTML).toContain('<svg>diagram</svg>');
    });
    expect(root.querySelector('.mermaid-loading')).toBeNull();
  });

  it('渲染失败时移除角标（源码与 mermaid-error 回退不变）', async () => {
    mockRender.mockRejectedValue(new Error('syntax error'));
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code class="language-mermaid">bad syntax</code></pre>';
    document.body.appendChild(root);

    window.__initMermaid('.post-content', 'light');

    await vi.waitFor(() => {
      expect(root.querySelector('pre')?.classList.contains('mermaid-error')).toBe(true);
    });
    expect(root.querySelector('.mermaid-loading')).toBeNull();
    expect(root.querySelector('pre code.language-mermaid')).not.toBeNull();
  });

  it('失败后重试成功清除错误标记，恢复图表和放大入口', async () => {
    mockRender.mockRejectedValueOnce(new Error('temporary failure'));
    document.body.innerHTML =
      '<div class="post-content"><pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre></div>';
    const pre = document.querySelector('pre')!;
    window.__initMermaid('.post-content', 'light');
    await vi.waitFor(() => expect(pre.classList.contains('mermaid-error')).toBe(true));

    window.__initMermaid('.post-content', 'light');
    await vi.waitFor(() => expect(pre.dataset.mermaidRendered).toBe('true'));
    expect(pre.classList.contains('mermaid-error')).toBe(false);
    expect(pre.querySelector('svg')).not.toBeNull();
    expect(pre.querySelector('.mermaid-loading')).toBeNull();
    pre.click();
    expect(document.querySelector('.mermaid-overlay')).not.toBeNull();
    _resetOverlay();
  });

  it('主题切换重渲染不挂加载角标', async () => {
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
    document.body.appendChild(root);

    window.__initMermaid('.post-content', 'light');
    await vi.waitFor(() => {
      expect(root.querySelector('pre')?.dataset.mermaidRendered).toBe('true');
    });
    let resolveRender: ((v: { svg: string }) => void) | undefined;
    mockRender.mockImplementation(
      () =>
        new Promise<{ svg: string }>((res) => {
          resolveRender = res;
        }),
    );
    const callsBefore = mockRender.mock.calls.length;

    window.__initMermaid('.post-content', 'dark');

    // rerenderExistingBlocks 同步走到 renderBlock 起点：dataset.mermaidRendered 已设 → 不挂角标
    expect(root.querySelector('.mermaid-loading')).toBeNull();
    // 同上：先等 mermaid.render 被调（微任务后）再放行
    await vi.waitFor(() => expect(mockRender.mock.calls.length).toBeGreaterThan(callsBefore));
    resolveRender?.({ svg: '<svg>dark</svg>' });
    await vi.waitFor(() => {
      expect(root.querySelector('pre')?.dataset.mermaidTheme).toBe('dark');
    });
  });
});

describe('mermaid 放大浮层', () => {
  const mockInitialize = vi.fn();
  const mockRender = vi.fn();

  beforeEach(() => {
    document.body.innerHTML = '';
    mockRender.mockClear();
    mockInitialize.mockClear();
    window.MermaidRenderer = { initialize: mockInitialize, render: mockRender };
  });
  afterEach(() => {
    delete window.MermaidRenderer;
    _resetOverlay();
    document.body.innerHTML = '';
  });

  /** 渲染一个 mermaid 块并返回 <pre>，等待 SVG 注入完成。 */
  async function renderBlock(): Promise<HTMLPreElement> {
    mockRender.mockResolvedValue({
      svg: '<svg viewBox="0 0 400 200"><rect width="400" height="200"/></svg>',
    });
    const root = document.createElement('div');
    root.className = 'post-content';
    root.innerHTML = '<pre><code class="language-mermaid">graph TD; A--&gt;B</code></pre>';
    document.body.appendChild(root);
    window.__initMermaid('.post-content', 'light');
    await vi.waitFor(() => {
      expect(root.querySelector('pre')?.dataset.mermaidRendered).toBe('true');
    });
    return root.querySelector('pre')!;
  }

  it('点击渲染后的 pre 打开浮层', async () => {
    const pre = await renderBlock();
    expect(document.querySelector('.mermaid-overlay')).toBeNull();
    pre.click();
    expect(document.querySelector('.mermaid-overlay')).not.toBeNull();
  });

  it('浮层包含 SVG 克隆', async () => {
    const pre = await renderBlock();
    pre.click();
    const svg = document.querySelector('.mermaid-overlay-content svg');
    expect(svg).not.toBeNull();
    // 剥离了 max-width 约束，按 viewBox 原始尺寸渲染
    expect(svg?.getAttribute('viewBox')).toBe('0 0 400 200');
  });

  it('ESC 关闭浮层', async () => {
    const pre = await renderBlock();
    pre.click();
    expect(document.querySelector('.mermaid-overlay')).not.toBeNull();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    // closeOverlay 有 250ms 飞回动画 + 280ms 兜底定时器才 remove()，用 waitFor 等移除。
    await vi.waitFor(() => {
      expect(document.querySelector('.mermaid-overlay')).toBeNull();
    });
  });

  it('✕ 按钮关闭浮层', async () => {
    const pre = await renderBlock();
    pre.click();
    const closeBtn = document.querySelector('.mermaid-overlay-close') as HTMLButtonElement;
    expect(closeBtn).not.toBeNull();
    closeBtn.click();
    await vi.waitFor(() => {
      expect(document.querySelector('.mermaid-overlay')).toBeNull();
    });
  });

  it('点击背景关闭浮层', async () => {
    const pre = await renderBlock();
    pre.click();
    const overlay = document.querySelector('.mermaid-overlay') as HTMLDivElement;
    // 模拟点击背景（target === overlay）
    overlay.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await vi.waitFor(() => {
      expect(document.querySelector('.mermaid-overlay')).toBeNull();
    });
  });

  it('主题切换重渲染后点击仍可打开浮层', async () => {
    const pre = await renderBlock();
    // 模拟主题切换重渲染
    mockRender.mockResolvedValue({
      svg: '<svg viewBox="0 0 400 200"><rect width="400" height="200"/></svg>',
    });
    window.__initMermaid('.post-content', 'dark');
    await vi.waitFor(() => {
      expect(pre.dataset.mermaidTheme).toBe('dark');
    });
    // overlay 绑定是幂等的——重渲染后仍可点击打开
    pre.click();
    expect(document.querySelector('.mermaid-overlay')).not.toBeNull();
  });
});
