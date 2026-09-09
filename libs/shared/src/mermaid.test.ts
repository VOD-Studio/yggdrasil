import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { loadMermaidRenderer, type MermaidApi, renderMermaidSvg } from './index';

beforeEach(() => {
  const append = document.head.appendChild.bind(document.head);
  vi.spyOn(document.head, 'appendChild').mockImplementation(<T extends Node>(node: T): T => {
    // 测试手动触发 load/error，不让 happy-dom 尝试网络加载。
    if (node instanceof HTMLScriptElement) node.type = 'application/json';
    return append(node);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  delete window.MermaidRenderer;
  delete window.__yggdrasilMermaidPromise;
  document.querySelectorAll('script[src="/mermaid/mermaid.js"]').forEach((script) => {
    script.remove();
  });
  document.body.innerHTML = '';
});

const renderer = (): MermaidApi => ({
  initialize: vi.fn(),
  render: vi.fn().mockResolvedValue({ svg: '<svg><foreignObject>标签</foreignObject></svg>' }),
});

describe('Mermaid bundle 加载', () => {
  it('复用已加载的运行时，不注入脚本', async () => {
    window.MermaidRenderer = renderer();
    expect(await loadMermaidRenderer()).toBe(window.MermaidRenderer);
    expect(document.querySelector('script[src="/mermaid/mermaid.js"]')).toBeNull();
  });

  it('独立模块副本同时加载也只发出一次请求', async () => {
    // 两个独立 IIFE 各自内联 shared，模块变量不能充当跨库缓存。
    vi.resetModules();
    const otherBundle = await import('./index');
    expect(otherBundle.loadMermaidRenderer).not.toBe(loadMermaidRenderer);
    const first = loadMermaidRenderer();
    const second = otherBundle.loadMermaidRenderer();
    expect(second).toBe(first);
    const scripts = document.querySelectorAll('script[src="/mermaid/mermaid.js"]');
    expect(scripts).toHaveLength(1);
    window.MermaidRenderer = renderer();
    scripts[0]?.dispatchEvent(new Event('load'));
    expect(await first).toBe(window.MermaidRenderer);
    expect(await second).toBe(window.MermaidRenderer);
  });

  it.each([
    ['error', 'failed to load /mermaid/mermaid.js'],
    ['load', 'mermaid bundle loaded but window.MermaidRenderer undefined'],
  ])('%s 失败后清理脚本，并允许重新加载', async (event, message) => {
    const failed = expect(loadMermaidRenderer()).rejects.toThrow(message);
    document.querySelector('script[src="/mermaid/mermaid.js"]')?.dispatchEvent(new Event(event));
    await failed;
    expect(document.querySelector('script[src="/mermaid/mermaid.js"]')).toBeNull();
    const retry = loadMermaidRenderer();
    window.MermaidRenderer = renderer();
    document.querySelector('script[src="/mermaid/mermaid.js"]')?.dispatchEvent(new Event('load'));
    expect(await retry).toBe(window.MermaidRenderer);
  });
});

describe('Mermaid SVG 渲染', () => {
  it.each(['light', 'dark'] as const)('统一 %s 配色、安全配置和多行标签修正', async (theme) => {
    const api = renderer();
    const svg = await renderMermaidSvg(api, 'diagram-1', 'graph TD; A-->B', theme);
    expect(api.initialize).toHaveBeenCalledWith({
      startOnLoad: false,
      theme: 'base',
      darkMode: theme === 'dark',
      securityLevel: 'strict',
      flowchart: { curve: 'basis', diagramPadding: 16, useMaxWidth: true, htmlLabels: true },
      themeVariables: expect.objectContaining({
        primaryTextColor: theme === 'dark' ? '#cdd6f4' : '#4c4f69',
      }),
    });
    expect(api.render).toHaveBeenCalledWith('diagram-1', 'graph TD; A-->B');
    expect(svg).toBe('<svg><foreignObject overflow="visible">标签</foreignObject></svg>');
  });

  it('语法错误只清理本次渲染的临时容器，并保留原始异常', async () => {
    const error = new Error('syntax error');
    const api = renderer();
    api.render = vi.fn().mockRejectedValue(error);
    document.body.innerHTML = '<div id="ddiagram-1"></div><div id="ddiagram-2"></div>';
    await expect(renderMermaidSvg(api, 'diagram-1', 'invalid', 'light')).rejects.toBe(error);
    expect(document.getElementById('ddiagram-1')).toBeNull();
    expect(document.getElementById('ddiagram-2')).not.toBeNull();
  });
});
