/**
 * 主题切换时以点击位置为圆心，让新主题从小圆展开覆盖旧主题。
 * CSS 变量控制圆心与半径，命令式换肤组件在新快照之前同步完成更新。
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
 * 通知命令式换肤的组件(CodeMirror / xterm)切换主题。
 *
 * 同步 dispatch THEME_CHANGE_EVENT,它们在 listener 内同步 setTheme,被同一 reflow
 * 捕获进 NEW 快照。Mermaid 图表由服务端渲染浅/深两份 SVG,靠 CSS 随 .dark 切换,
 * 无需通知。
 */
function notifyThemeChange(isDark: boolean): void {
  window.dispatchEvent(new CustomEvent(THEME_CHANGE_EVENT, { detail: { isDark } }));
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
 * (与 Rust 侧 Effect 幂等共存,后者作兜底)。
 */
export function applyResolvedTheme(isDark: boolean): void {
  // System preference changes also supersede any pending manual theme callback.
  const owner = beginTransition('theme');
  notifyThemeChange(isDark);
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
  const commit = (): void => {
    if (committed) return;
    committed = true;
    // Notify editors synchronously before flipping the class.
    notifyThemeChange(isDark);
    applyDarkClass(isDark);
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
    commit();
    cleanup();
  });

  if (typeof document.startViewTransition !== 'function' || prefersReducedMotion()) {
    commit();
    owner.finish();
    return;
  }

  html.style.setProperty('--tt-x', `${x}px`);
  html.style.setProperty('--tt-y', `${y}px`);
  html.style.setProperty('--tt-r', `${maxCornerDistance(x, y)}px`);
  html.classList.add('is-theme-transitioning');

  try {
    const vt = document.startViewTransition(() => {
      if (!owner.isCurrent()) return;
      commit();
      // Capture both CSS-based and imperative editor colors in the same reflow.
      getComputedStyle(document.body).backgroundColor;
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
    commit();
    cleanup();
    owner.finish();
  }
}
