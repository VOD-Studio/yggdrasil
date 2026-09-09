import { loadMermaidRenderer, renderMermaidSvg, type ThemeName } from '@yggdrasil/shared';

// 与前台使用不同前缀，避免同页 SVG/marker id 冲突。
let renderCounter = 0;

/** 编辑器返回错误文本；防抖、主题切换和过期结果取消由 CodeBlockNodeView 管理。 */
export async function renderMermaid(
  source: string,
  theme: ThemeName,
): Promise<{ svg: string } | { error: string }> {
  try {
    const mermaid = await loadMermaidRenderer();
    const svg = await renderMermaidSvg(mermaid, `tiptap-mermaid-${++renderCounter}`, source, theme);
    return { svg };
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
}

/**
 * 读取当前站点主题(前台用 documentElement 的 .dark class 标记暗色)。
 * NodeView 首次渲染与主题切换重渲染时调用。
 */
export function getCurrentTheme(): ThemeName {
  return document.documentElement.classList.contains('dark') ? 'dark' : 'light';
}
