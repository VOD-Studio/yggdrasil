/**
 * 主题切换时以点击位置为圆心，让新主题从小圆展开覆盖旧主题。
 * CSS 变量控制圆心与半径，异步换肤组件在新快照之前完成更新。
 * 与页面导航共享所有权，避免被打断的回调改变后续快照或主题。
 */

import { prefersReducedMotion, THEME_CHANGE_EVENT } from '@yggdrasil/shared';
import { beginTransition } from './view-transition-lifecycle';

/**
 * 主题切换自定义事件。
 *
 * 在 VT 回调内(NEW 快照捕获前)同步 dispatch,通知 CodeMirror / xterm 等
 * 命令式换肤的组件同步调 setTheme——它们的背景色不随 .dark class 翻转,
 * 必须在快照前显式换肤,否则圆形展开扫过时看不到变化(OLD/NEW 同色)。
 *
 * 事件 detail: `{ isDark: boolean }`。
 *
 * 事件名与 prefersReducedMotion 由 @yggdrasil/shared 统一定义,各 IIFE 库
 * (codemirror-editor / xterm-terminal / lightbox)共享同一真相源。
 */
export { THEME_CHANGE_EVENT };

function maxCornerDistance(x: number, y: number): number {
  const w = window.innerWidth;
  const h = window.innerHeight;
  const corners = [
    [0, 0],
    [w, 0],
    [0, h],
    [w, h],
  ];
  let max = 0;
  for (const [cx, cy] of corners) {
    const d = Math.hypot(cx - x, cy - y);
    if (d > max) max = d;
  }
  return max;
}

function applyDarkClass(isDark: boolean): void {
  const html = document.documentElement;
  if (isDark) {
    html.classList.add('dark');
  } else {
    html.classList.remove('dark');
  }
}

/**
 * 主题切换 registry:命令式 / 异步换肤组件注册的回调。
 *
 * 与 THEME_CHANGE_EVENT 事件并存:
 * - 事件(CustomEvent):同步 dispatch,供 CodeMirror / xterm 等同步换肤组件用
 *   (它们在 listener 里同步 setTheme,被同一 reflow 捕获进 NEW 快照)。事件拿不到
 *   listener 返回值,fire-and-forget。
 * - registry:回调**可返回 Promise**,调用方(如 VT callback)能 await 它,等异步换肤
 *   组件(如 mermaid 的 render())完成。这是让异步渲染内容参与 VT 动画的关键——
 *   mermaid.render 是异步的,必须在 VT 拍 NEW 快照前完成,否则快照里仍是旧图。
 */
const themeChangeCallbacks = new Set<(isDark: boolean) => Promise<void> | void>();

/** 注册主题切换回调,返回取消注册函数。回调可返回 Promise,调用方会等待它。 */
export function onThemeChange(cb: (isDark: boolean) => Promise<void> | void): () => void {
  themeChangeCallbacks.add(cb);
  return () => themeChangeCallbacks.delete(cb);
}

/**
 * 通知命令式换肤的组件(CodeMirror / xterm / mermaid)切换主题。
 *
 * 双通道:
 * 1. 同步 dispatch THEME_CHANGE_EVENT(给 CodeMirror / xterm,它们在 listener 内
 *    同步 setTheme,被同一 reflow 捕获进 NEW 快照)。
 * 2. 遍历 registry 调每个 cb,收集返回的 Promise,返回聚合 Promise(VT callback
 *    await 它以等 mermaid 等异步换肤完成)。同步组件不返回值,聚合自动忽略。
 *
 * 返回聚合 Promise,调用方可选 await(走 VT 的路径必须 await,瞬切路径可不 await)。
 */
function notifyThemeChange(isDark: boolean): Promise<void> {
  window.dispatchEvent(new CustomEvent(THEME_CHANGE_EVENT, { detail: { isDark } }));
  const promises: Promise<void>[] = [];
  themeChangeCallbacks.forEach((cb) => {
    try {
      const ret = cb(isDark);
      if (ret) promises.push(ret.catch(() => {})); // 单个失败不中断聚合
    } catch {
      // 同步抛错的 cb 忽略,不中断其他回调
    }
  });
  return Promise.all(promises).then(() => {});
}

/**
 * 直接设置 <html> 的 dark class（设置语义，非翻转）。
 *
 * 用于跟随系统偏好变化时的后台同步：系统偏好变化是后台事件，View Transitions
 * 在此上下文下动画不可靠（实测圆形展开不显示，仅瞬切），故跟随系统场景不走
 * startThemeTransition 的 VT 路径，改用此函数直接同步 class，做无动画的瞬切。
 * 手动点击主题按钮仍走 startThemeTransition，保留圆形展开动画。
 *
 * 同步 dispatch 主题变更事件,让命令式换肤的编辑器跟随系统偏好瞬切
 * (与 Dioxus use_effect 幂等共存,后者作兜底)。
 */
export function applyResolvedTheme(isDark: boolean): void {
  // System preference changes also supersede any pending manual theme callback.
  const owner = beginTransition('theme');
  void notifyThemeChange(isDark);
  applyDarkClass(isDark);
  owner.finish();
}

export function startThemeTransition(x: number, y: number): void {
  // Cancellation commits a pending earlier theme before deriving the next target.
  // Two quick clicks therefore toggle twice, even before the first native callback.
  const owner = beginTransition('theme');
  const html = document.documentElement;
  const isDark = !html.classList.contains('dark');
  let committed = false;
  const commit = (): Promise<void> => {
    if (committed) return Promise.resolve();
    committed = true;
    // Notify editors synchronously before flipping the class, then await Mermaid.
    const asyncWork = notifyThemeChange(isDark);
    applyDarkClass(isDark);
    return asyncWork;
  };
  const cleanup = () => {
    html.classList.remove('is-theme-transitioning');
    html.style.removeProperty('--tt-x');
    html.style.removeProperty('--tt-y');
    html.style.removeProperty('--tt-r');
  };
  owner.onCancel(() => {
    // skipTransition() still schedules the callback. Settle the requested color
    // now, so that callback can safely no-op after a newer route/theme takes over.
    void commit();
    cleanup();
  });

  if (typeof document.startViewTransition !== 'function' || prefersReducedMotion()) {
    void commit();
    owner.finish();
    return;
  }

  html.style.setProperty('--tt-x', `${x}px`);
  html.style.setProperty('--tt-y', `${y}px`);
  html.style.setProperty('--tt-r', `${maxCornerDistance(x, y)}px`);
  html.classList.add('is-theme-transitioning');

  try {
    const vt = document.startViewTransition(async () => {
      if (!owner.isCurrent()) return;
      const asyncWork = commit();
      // Capture both CSS-based and imperative editor colors in the same reflow.
      getComputedStyle(document.body).backgroundColor;
      await asyncWork;
    });
    owner.attach(vt);
    const finish = () => {
      if (!owner.isCurrent()) return;
      cleanup();
      owner.finish();
    };
    void vt.finished.then(finish, finish);
  } catch {
    // Native snapshot setup can fail; changing the theme must still succeed.
    void commit();
    cleanup();
    owner.finish();
  }
}
