import { THEME_CHANGE_EVENT } from '@yggdrasil/shared';
import { CodeMirrorInstance, EditorOptions } from './editor';
import type { ThemeName } from './themes';

/**
 * 主题切换事件由 @yggdrasil/shared 统一定义。
 *
 * yggdrasil-core 在 VT 回调内(NEW 快照捕获前)同步 dispatch 此事件,
 * 让 CodeMirror 同步 reconfigure 主题——否则圆形展开扫过编辑器区域时
 * OLD/NEW 快照同色(背景由 catppuccin Extension 注入,不随 .dark 翻转),
 * 看不到变化,动画结束后才瞬切。
 */

/**
 * 模块入口：暴露对象字面量 { create } 作为默认导出。
 * IIFE 产物挂在 window.CodeMirrorEditor 上，由 Rust 侧用 Reflect::get 取
 * （对象字面量，不能用 wasm-bindgen 的 extern fn——那会被编成函数调用而失败）。
 */
const CodeMirrorEditor = {
  _instances: new Map<string, CodeMirrorInstance>(),

  create(
    containerId: string,
    options: EditorOptions = new EditorOptions(),
  ): CodeMirrorInstance | null {
    const container = document.getElementById(containerId);
    if (!container) return null;

    // 销毁同 id 的旧实例
    const existing = this._instances.get(containerId);
    if (existing) {
      existing.destroy();
    }

    let instance!: CodeMirrorInstance;
    instance = new CodeMirrorInstance(container, options, () => {
      if (this._instances.get(containerId) === instance) this._instances.delete(containerId);
    });
    this._instances.set(containerId, instance);
    return instance;
  },
};

/**
 * 订阅主题切换事件:VT 回调内同步 dispatch 时,遍历所有存活实例调 setTheme。
 *
 * 必须在模块加载时注册一次(IIFE 顶层),确保任何时刻 dispatch 都能命中。
 * 单实例异常用 try/catch 隔离,避免一个实例失败中断其他实例换肤。
 * 与 Dioxus use_effect 驱动的 set_theme 幂等共存(reconfigure 相同主题是 no-op)。
 */
if (typeof window !== 'undefined') {
  window.addEventListener(THEME_CHANGE_EVENT, (event) => {
    const detail = (event as CustomEvent).detail as { isDark: boolean } | undefined;
    if (!detail) return;
    const theme: ThemeName = detail.isDark ? 'dark' : 'light';
    CodeMirrorEditor._instances.forEach((instance) => {
      try {
        instance.setTheme(theme);
      } catch (e) {
        console.error('[CodeMirrorEditor] setTheme failed during theme change:', e);
      }
    });
  });
}

export default CodeMirrorEditor;
