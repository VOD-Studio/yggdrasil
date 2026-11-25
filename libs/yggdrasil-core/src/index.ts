import { initAnchorClick } from './anchor-click';
import { loadBrowserLibrary } from './browser-library';
import type { EnhanceCall } from './enhance-queue';
import { scrollToHash } from './hash-scroll';
import { routeTransitions } from './route-transitions';
import { applyResolvedTheme, startThemeTransition } from './theme-transition';
import { skipActiveAnimation } from './view-transition-lifecycle';

/**
 * 导航与主题能力在 HTML 解析后安装；外壳已内联首次主题和原生事件监听。
 * 正文增强、灯箱由页面各自加载，迟到的初始化调用经队列补执行。
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
    /** 增强调用队列：目标函数未定义时由 bridges::globals 入队，flush 丢弃已可
     * 执行的项。core 无条件建队，队列存在不代表本页会加载 enhance。 */
    __pendingEnhanceCalls: EnhanceCall[];
    __flushEnhanceCalls?: () => void;
  }
}

window.__loadBrowserLibrary = loadBrowserLibrary;
window.__initAnchorClick = initAnchorClick;
window.__scrollToHash = scrollToHash;
window.__startThemeTransition = startThemeTransition;
window.__applyResolvedTheme = applyResolvedTheme;
window.__routeTransitions = routeTransitions;
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
