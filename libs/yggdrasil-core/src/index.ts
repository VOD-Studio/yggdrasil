import { initAnchorClick } from './anchor-click';
import { loadBrowserLibrary } from './browser-library';
import type { EnhanceCall } from './enhance-queue';
import { scrollToHash } from './hash-scroll';
import { initPageReveal } from './page-entry';
import { routeTransitions } from './route-transitions';
import { applyResolvedTheme, onThemeChange, startThemeTransition } from './theme-transition';
import { skipActiveAnimation } from './view-transition-lifecycle';
import './style.css';

/**
 * P5 §8.2 拆分边界：本入口只保留启动必需代码（导航、主题、锚点、pagereveal），
 * 阻塞执行、不加 async。文章/图鉴/changelog 的增强函数在 `enhance.ts` 入口，
 * 由页面组件按需以 `defer` 注入（SSR 直直接烤进 head，先于 hydration；
 * SPA 首次注入为 async，由下方队列兜底到增强脚本加载完成）。
 */
declare global {
  interface Window {
    __loadBrowserLibrary: typeof loadBrowserLibrary;
    __initAnchorClick: () => void;
    __scrollToHash: () => void;
    __startThemeTransition: (x: number, y: number) => void;
    __applyResolvedTheme: (isDark: boolean) => void;
    __routeTransitions: typeof routeTransitions;
    /** 打开弹窗前调用：过渡快照画在顶层之上，会盖住模态面板。 */
    __skipViewTransition: typeof skipActiveAnimation;
    /** enhance 入口订阅 core 主题 registry 的桥接：两个 IIFE 各持一份 registry 会让
     * mermaid 收不到通知，必须共用 core 这份。 */
    __subscribeThemeChange: typeof onThemeChange;
    /** 增强调用队列：目标函数未定义时由 invoke_optional_global 入队，flush 丢弃已可
     * 执行的项。core 无条件建队，队列存在不代表本页会加载 enhance。 */
    __pendingEnhanceCalls: EnhanceCall[];
    __flushEnhanceCalls: () => void;
  }
}

window.__loadBrowserLibrary = loadBrowserLibrary;
window.__initAnchorClick = initAnchorClick;
window.__scrollToHash = scrollToHash;
window.__startThemeTransition = startThemeTransition;
window.__applyResolvedTheme = applyResolvedTheme;
window.__routeTransitions = routeTransitions;
window.__subscribeThemeChange = onThemeChange;
window.__skipViewTransition = skipActiveAnimation;

// dev HTML 可能内联两次 IIFE：队列与 flush 都要求幂等（保留已入队项）。
window.__pendingEnhanceCalls ??= [];
window.__flushEnhanceCalls ??= () => {
  const queue = window.__pendingEnhanceCalls;
  if (!queue.length) return;
  // 只执行已就绪的函数，其余保留在队里等对应脚本加载（各 IIFE 尾部都会 flush）。
  window.__pendingEnhanceCalls = queue.filter((call) => {
    const entry = (window as unknown as Record<string, unknown>)[call.name];
    if (typeof entry !== 'function') return true;
    (entry as (...args: unknown[]) => void)(...call.args);
    return false;
  });
};

// Same-page links also appear on the homepage, before any article is mounted.
initAnchorClick();

// Cross-document transitions can reveal before the WASM router connects.
initPageReveal();
