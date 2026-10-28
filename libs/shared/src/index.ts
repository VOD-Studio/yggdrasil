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
// Mermaid 主题变量
// ---------------------------------------------------------------------------
//
// 这是 mermaid 渲染配色的单一真相源。前台 yggdrasil-core 与后台 tiptap-editor
// 都从这里取,保证「编辑器预览 = 线上文章页」视觉一致(各 IIFE 不能 import 彼此,
// 但都能 inline 本包)。
//
// mermaid 把颜色烤进 SVG 内联 style,无法靠 CSS 原地改主题,故须在 initialize 时注入,
// 也因此只能写具体色值,不能用 CSS 变量;取值对应 styles/theme.css 的 token。
// 用 `theme: 'base'`(非 'default'/'dark'):base 主题不硬编码颜色,themeVariables 能完全
// 控制调色板;'default' 主题硬编码 mainBkg=#ECECFF 等会阻断覆盖。
//
// 设计:节点是浮在代码块底色上的卡片(surface 色阶 + 发丝线边框),文字用 fg,连线用
// fg-3;注释用陶土浅底。不滥用强调色,强调留给作者用 classDef 手动指定。
//
// 覆盖的字段涵盖 flowchart / sequence / class 三类图(测试文章用到的全部类型)。

/** mermaid themeVariables 的字段集合(mermaid 实际接受更多字段,这里只列项目用到的)。 */
export type MermaidThemeVariables = Record<string, unknown>;

interface MermaidTones {
  /** 图背景,等于代码块底色 surface-1。 */
  background: string;
  node: string;
  nodeAlt: string;
  nodeSoft: string;
  border: string;
  borderStrong: string;
  text: string;
  textSecondary: string;
  textTertiary: string;
  line: string;
  cluster: string;
  noteBackground: string;
  noteBorder: string;
}

const MERMAID_FONT =
  "-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, 'PingFang SC', 'Hiragino Sans GB', 'Noto Sans SC', 'Microsoft YaHei', sans-serif";

function mermaidVars(t: MermaidTones): MermaidThemeVariables {
  return {
    background: t.background,
    // 节点填充:surface 色阶递进(主/次/三级),卡片质感
    primaryColor: t.node,
    secondaryColor: t.nodeAlt,
    tertiaryColor: t.nodeSoft,
    mainBkg: t.node,
    nodeBkg: t.node,
    secondBkg: t.nodeAlt,
    primaryBorderColor: t.border,
    secondaryBorderColor: t.border,
    tertiaryBorderColor: t.border,
    nodeBorder: t.border,
    clusterBorder: t.border,
    labelBoxBorderColor: t.border,
    primaryTextColor: t.text,
    secondaryTextColor: t.textSecondary,
    tertiaryTextColor: t.textTertiary,
    textColor: t.text,
    nodeTextColor: t.text,
    titleColor: t.text,
    classText: t.text,
    labelTextColor: t.text,
    lineColor: t.line,
    defaultLinkColor: t.line,
    arrowheadColor: t.line,
    // 时序图
    actorBkg: t.node,
    actorBorder: t.border,
    actorTextColor: t.text,
    actorLineColor: t.line,
    signalColor: t.line,
    signalTextColor: t.text,
    loopTextColor: t.text,
    sequenceNumberColor: t.background,
    activationBkgColor: t.nodeAlt,
    activationBorderColor: t.borderStrong,
    // 边标签 / 子图背景
    edgeLabelBackground: t.background,
    labelBoxBkgColor: t.background,
    clusterBkg: t.cluster,
    // 注释:陶土浅底
    noteBkgColor: t.noteBackground,
    noteBorderColor: t.noteBorder,
    noteTextColor: t.text,
    fontFamily: MERMAID_FONT,
    fontSize: '16px',
  };
}

/** 浅色:白色节点浮在 surface-1 上。 */
const MERMAID_LIGHT_VARS: MermaidThemeVariables = mermaidVars({
  background: '#fafaf8',
  node: '#ffffff',
  nodeAlt: '#f5f5f3',
  nodeSoft: '#efefec',
  border: '#dcdbd7',
  borderStrong: '#c4c3be',
  text: '#151515',
  textSecondary: '#525252',
  textTertiary: '#6b6b69',
  line: '#6b6b69',
  cluster: 'rgba(0, 0, 0, 0.02)',
  noteBackground: '#f7e8e1',
  noteBorder: '#d97757',
});

/** 深色:暖调深灰节点,surface 色阶递进。 */
const MERMAID_DARK_VARS: MermaidThemeVariables = mermaidVars({
  background: '#161615',
  node: '#20201e',
  nodeAlt: '#262624',
  nodeSoft: '#1a1a19',
  border: '#3a3936',
  borderStrong: '#4f4e4a',
  text: '#f2f1ee',
  textSecondary: '#a9a7a1',
  textTertiary: '#8b8984',
  line: '#a9a7a1',
  cluster: 'rgba(255, 255, 255, 0.03)',
  noteBackground: 'rgba(217, 119, 87, 0.15)',
  noteBorder: '#d97757',
});

/** 按主题返回对应 themeVariables 的副本(调用方可安全 mutate)。 */
export function mermaidThemeVarsFor(theme: ThemeName): MermaidThemeVariables {
  return theme === 'dark' ? { ...MERMAID_DARK_VARS } : { ...MERMAID_LIGHT_VARS };
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

/** 服务端下发的资源版本表元素 id，与 `src/static_assets.rs` 的 `VERSIONS_ELEMENT_ID` 一致。 */
export const ASSET_VERSIONS_ID = 'ygg-asset-versions';

/**
 * 带内容版本号的资源地址：`/mermaid/mermaid.js` → `/mermaid/mermaid.js?v=<摘要>`。
 * 版本号由服务端按文件内容生成，带版本号的地址可被浏览器与 CDN 长期缓存；
 * 表里没有（或页面不是服务端渲染的）时原样返回，服务端对不带版本号的地址要求每次校验。
 */
export function assetUrl(path: string): string {
  const text = document.getElementById(ASSET_VERSIONS_ID)?.textContent;
  let version: unknown;
  try {
    version = text ? (JSON.parse(text) as Record<string, unknown>)[path] : undefined;
  } catch {
    version = undefined;
  }
  return typeof version === 'string' ? `${path}?v=${version}` : path;
}

/** 按需加载独立 bundle，同页并发共用一次请求，失败后允许重试。 */
export function loadMermaidRenderer(): Promise<MermaidApi> {
  if (window.MermaidRenderer) return Promise.resolve(window.MermaidRenderer);
  if (!window.__yggdrasilMermaidPromise) {
    const script = document.createElement('script');
    script.src = assetUrl('/mermaid/mermaid.js');
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
    // Mermaid 12 改用 ELK/neo 默认值；保留现有文章的布局与节点外观。
    layout: 'dagre',
    look: 'classic',
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

export * from './code-palette';
