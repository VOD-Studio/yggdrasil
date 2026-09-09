/**
 * 跨 IIFE 库共享的类型、常量与工具函数。
 *
 * 这些库各自打包成独立 IIFE（不能 import 彼此），但它们的主题切换逻辑
 * 需要约定相同的事件名与类型定义。本包作为单一真相源，由各库以 workspace
 * dependency 引入，Vite 构建时 inline 进各自的 IIFE bundle。
 */

/** 主题名称：亮色 / 暗色。 */
export type ThemeName = 'light' | 'dark';

/**
 * 主题切换事件名。
 *
 * yggdrasil-core 在切换主题时 dispatch 此事件；codemirror-editor / xterm-terminal
 * 监听它以热切换编辑器主题。所有库必须用同一个字符串字面量。
 */
export const THEME_CHANGE_EVENT = 'yggdrasil:theme-change';

/**
 * 检测用户是否在系统层面启用了「减少动态效果」（prefers-reduced-motion）。
 *
 * 用于决定是否跳过 View Transitions / 动画，直接切换。
 */
export function prefersReducedMotion(): boolean {
  return !!window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

// ---------------------------------------------------------------------------
// Mermaid Catppuccin 主题变量
// ---------------------------------------------------------------------------
//
// 这是 mermaid 渲染配色的单一真相源。前台 yggdrasil-core 与后台 tiptap-editor
// 都从这里取,保证「编辑器预览 = 线上文章页」视觉一致(各 IIFE 不能 import 彼此,
// 但都能 inline 本包)。
//
// mermaid 把颜色烤进 SVG 内联 style,无法靠 CSS 原地改主题,故须在 initialize 时注入。
// 用 `theme: 'base'`(非 'default'/'dark'):base 主题不硬编码颜色,themeVariables 能完全
// 控制调色板;'default' 主题硬编码 mainBkg=#ECECFF 等会阻断覆盖。
//
// 设计哲学:极简卡片化——节点用 surface 色阶(卡片感)、边框/连线用 subtext 色阶(克制)、
// 文字用 primary text。不滥用强调色,绿/紫等 accent 留给作者用 classDef 手动强调。
// hex 值取自 themes/Catppuccin Latte.tmTheme 与 Catppuccin Mocha.tmTheme。
//
// 覆盖的字段涵盖 flowchart / sequence / class 三类图(测试文章用到的全部类型)。

/** mermaid themeVariables 的字段集合(mermaid 实际接受更多字段,这里只列项目用到的)。 */
export type MermaidThemeVariables = Record<string, unknown>;

/** Latte(亮)主题:节点 surface 色阶、文字 #4c4f69、连线 subtext1 #5c5f77。 */
export const MERMAID_LATTE_VARS: MermaidThemeVariables = {
  background: '#dce0e8', // = --color-paper-code-block,图背景与 pre 无缝衔接
  // 节点填充:surface 色阶递进(主/次/三级),卡片质感
  primaryColor: '#e6e9ef',
  secondaryColor: '#ccd0da',
  tertiaryColor: '#bcc0cc',
  mainBkg: '#e6e9ef',
  nodeBkg: '#e6e9ef',
  secondBkg: '#ccd0da',
  // 节点边框:surface1 偏冷灰
  primaryBorderColor: '#bcc0cc',
  secondaryBorderColor: '#acb0be',
  tertiaryBorderColor: '#9ca0b0',
  nodeBorder: '#bcc0cc',
  clusterBorder: '#bcc0cc',
  labelBoxBorderColor: '#bcc0cc',
  // 文字:primary text #4c4f69
  primaryTextColor: '#4c4f69',
  secondaryTextColor: '#5c5f77',
  tertiaryTextColor: '#6c6f85',
  textColor: '#4c4f69',
  nodeTextColor: '#4c4f69',
  titleColor: '#4c4f69',
  classText: '#4c4f69',
  labelTextColor: '#4c4f69',
  // 连线/箭头:subtext1 #5c5f77
  lineColor: '#5c5f77',
  defaultLinkColor: '#5c5f77',
  arrowheadColor: '#5c5f77',
  // 时序图
  actorBkg: '#e6e9ef',
  actorBorder: '#bcc0cc',
  actorTextColor: '#4c4f69',
  actorLineColor: '#5c5f77',
  signalColor: '#5c5f77',
  signalTextColor: '#4c4f69',
  loopTextColor: '#4c4f69',
  sequenceNumberColor: '#eff1f5',
  activationBkgColor: '#ccd0da',
  activationBorderColor: '#bcc0cc',
  // 边标签 / 子图背景
  edgeLabelBackground: '#eff1f5',
  labelBoxBkgColor: '#eff1f5',
  clusterBkg: 'rgba(239, 241, 245, 0.5)',
  // 注释:低饱和黄(Latte yellow #df8e1d)
  noteBkgColor: 'rgba(223, 142, 29, 0.15)',
  noteBorderColor: '#df8e1d',
  noteTextColor: '#4c4f69',
  // 字体:与正文 sans 对齐,中文友好
  fontFamily:
    "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Sans SC', 'PingFang SC', 'Microsoft YaHei', sans-serif",
  fontSize: '16px',
};

/** Mocha(暗)主题:节点 surface 色阶、文字 #cdd6f4、连线 subtext0 #a6adc8。 */
export const MERMAID_MOCHA_VARS: MermaidThemeVariables = {
  background: '#313244', // = --color-paper-code-block(暗)
  primaryColor: '#45475a',
  secondaryColor: '#585b70',
  tertiaryColor: '#1e1e2e',
  mainBkg: '#45475a',
  nodeBkg: '#45475a',
  secondBkg: '#585b70',
  primaryBorderColor: '#585b70',
  secondaryBorderColor: '#45475a',
  tertiaryBorderColor: '#313244',
  nodeBorder: '#585b70',
  clusterBorder: '#585b70',
  labelBoxBorderColor: '#585b70',
  primaryTextColor: '#cdd6f4',
  secondaryTextColor: '#bac2de',
  tertiaryTextColor: '#a6adc8',
  textColor: '#cdd6f4',
  nodeTextColor: '#cdd6f4',
  titleColor: '#cdd6f4',
  classText: '#cdd6f4',
  labelTextColor: '#cdd6f4',
  lineColor: '#a6adc8',
  defaultLinkColor: '#a6adc8',
  arrowheadColor: '#a6adc8',
  actorBkg: '#45475a',
  actorBorder: '#585b70',
  actorTextColor: '#cdd6f4',
  actorLineColor: '#a6adc8',
  signalColor: '#a6adc8',
  signalTextColor: '#cdd6f4',
  loopTextColor: '#cdd6f4',
  sequenceNumberColor: '#1e1e2e',
  activationBkgColor: '#585b70',
  activationBorderColor: '#6c7086',
  edgeLabelBackground: '#1e1e2e',
  labelBoxBkgColor: '#1e1e2e',
  clusterBkg: 'rgba(30, 30, 46, 0.5)',
  noteBkgColor: 'rgba(249, 226, 175, 0.12)',
  noteBorderColor: '#f9e2af',
  noteTextColor: '#cdd6f4',
  fontFamily:
    "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Sans SC', 'PingFang SC', 'Microsoft YaHei', sans-serif",
  fontSize: '16px',
};

/** 按主题返回对应 Catppuccin themeVariables 的副本(调用方可安全 mutate)。 */
export function mermaidThemeVarsFor(theme: ThemeName): MermaidThemeVariables {
  return theme === 'dark' ? { ...MERMAID_MOCHA_VARS } : { ...MERMAID_LATTE_VARS };
}

/** 前台正文与编辑器使用的 Mermaid API 子集。 */
export type MermaidApi = {
  initialize: (config: Record<string, unknown>) => void;
  render: (id: string, text: string) => Promise<{ svg: string }>;
};

declare global {
  interface Window {
    MermaidRenderer?: MermaidApi;
    // shared 会内联进不同 IIFE；加载中的 Promise 必须放在 window 才能跨库复用。
    __yggdrasilMermaidPromise?: Promise<MermaidApi>;
  }
}

/** 按需加载独立 bundle，同页并发共用一次请求，失败后允许重试。 */
export function loadMermaidRenderer(): Promise<MermaidApi> {
  if (window.MermaidRenderer) return Promise.resolve(window.MermaidRenderer);
  if (!window.__yggdrasilMermaidPromise) {
    const script = document.createElement('script');
    script.src = '/mermaid/mermaid.js';
    window.__yggdrasilMermaidPromise = new Promise<MermaidApi>((resolve, reject) => {
      script.onload = () => {
        if (window.MermaidRenderer) resolve(window.MermaidRenderer);
        else reject(new Error('mermaid bundle loaded but window.MermaidRenderer undefined'));
      };
      script.onerror = () => reject(new Error('failed to load /mermaid/mermaid.js'));
      document.head.appendChild(script);
    }).catch((error) => {
      script.remove();
      delete window.__yggdrasilMermaidPromise;
      throw error;
    });
  }
  return window.__yggdrasilMermaidPromise;
}

/** 渲染配置、SVG 修正与错误容器清理由两端共用；挂载与取消仍由调用方处理。 */
export async function renderMermaidSvg(
  mermaid: MermaidApi,
  id: string,
  source: string,
  theme: ThemeName,
): Promise<string> {
  mermaid.initialize({
    startOnLoad: false,
    theme: 'base',
    darkMode: theme === 'dark',
    securityLevel: 'strict',
    flowchart: {
      curve: 'basis',
      diagramPadding: 16,
      useMaxWidth: true,
      htmlLabels: true,
    },
    themeVariables: mermaidThemeVarsFor(theme),
  });
  try {
    const { svg } = await mermaid.render(id, source);
    // 防止 foreignObject 默认 overflow:hidden 裁切多行标签的文字下沿。
    return svg.replace(/<foreignObject(?=[\s>])/g, '<foreignObject overflow="visible"');
  } catch (error) {
    // Mermaid 语法错误时会把临时错误 SVG 留在 body 中。
    document.getElementById(`d${id}`)?.remove();
    throw error;
  }
}
