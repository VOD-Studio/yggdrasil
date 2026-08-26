// xterm.js 终端实例：输出专用（无 stdin），配合 SSE 流式渲染容器 stdout/stderr。
//
// 设计约束：
// - disableStdin: true —— 读者侧不可交互输入，纯输出展示。
// - convertEol: true —— 容器输出的 \n 自动转 \r\n（终端换行语义）。
// - stderr 用 ANSI 红色前缀包裹，与 stdout 视觉区分。
//
// 镜像 codemirror-editor/src/editor.ts 的范式：Options 用 class（非 interface）
// 以便 TS 擦除后存活，wasm 侧能用 new XtermOptions() 构造。

import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { DARK_THEME, LIGHT_THEME, type ThemeName } from './themes';

/** 传给 XtermTerminal.create 的配置。
 *  必须是 class（非 interface），以便 TS 擦除后存活，
 *  wasm 侧能用 `new XtermOptions()` 构造，并通过 setter 填充字段。
 */
export class XtermOptions {
  theme?: ThemeName;
  fontFamily?: string;
  fontSize?: number;
  onReady?: () => void;
}

/** ANSI 红色 + reset 包裹 stderr 文本。 */
function red(text: string): string {
  return `\x1b[31m${text}\x1b[0m`;
}

/** xterm.js 终端实例封装。输出专用，提供 stdout/stderr 流式写入与整段写入两种模式。 */
export class TerminalInstance {
  private term: Terminal;
  private fitAddon: FitAddon;
  private resizeObserver?: ResizeObserver;

  constructor(container: HTMLElement, options: XtermOptions) {
    this.term = new Terminal({
      // 容器输出是 \n，终端需要 \r\n 才能回车换行；convertEol 自动转换。
      convertEol: true,
      // 输出专用：不接收键盘输入，禁用光标闪烁。
      disableStdin: true,
      cursorBlink: false,
      cursorStyle: 'bar',
      fontFamily: options.fontFamily ?? 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace',
      fontSize: options.fontSize ?? 13,
      lineHeight: 1.3,
      scrollback: 5000,
      theme: options.theme === 'dark' ? DARK_THEME : LIGHT_THEME,
    });

    this.fitAddon = new FitAddon();
    this.term.loadAddon(this.fitAddon);
    this.term.open(container);
    // 初始 fit，让列宽适配容器宽度。
    this.fitAddon.fit();

    // 容器尺寸变化后自动重新 fit（侧边栏收起/展开、窗口 resize、输出面板展开动画等）；
    // 否则终端会一直沿用挂载时的列宽换行，直到下次重新挂载才刷新。
    // happy-dom 测试环境不实现 ResizeObserver（见 hash-scroll.ts 同款判空），
    // 缺失时降级为仅保留构造时的初始 fit，不影响现有测试断言。
    if (typeof ResizeObserver !== 'undefined') {
      this.resizeObserver = new ResizeObserver(() => {
        this.fitAddon.fit();
      });
      this.resizeObserver.observe(container);
    }

    options.onReady?.();
  }

  /** 流式写入 stdout 块（SSE stdout 事件）。 */
  writeStdout(data: string): void {
    this.term.write(data);
  }

  /** 流式写入 stderr 块（SSE stderr 事件），红色显示。 */
  writeStderr(data: string): void {
    this.term.write(red(data));
  }

  /** 整段写入（轮询兜底路径拿到完整结果时）：清屏后重写 stdout + stderr。 */
  writeAll(stdout: string, stderr: string): void {
    this.term.reset();
    this.term.write(stdout);
    if (stderr) {
      this.term.write(red(stderr));
    }
  }

  /** 切换主题（热切换，无需重建实例）。 */
  setTheme(theme: ThemeName): void {
    this.term.options.theme = theme === 'dark' ? DARK_THEME : LIGHT_THEME;
  }

  /** 手动强制重新计算列宽（容器尺寸变化已由内部 ResizeObserver 自动处理；
   *  这里保留给需要立即刷新的边界场景，如容器从 display:none 切到可见）。 */
  fit(): void {
    this.fitAddon.fit();
  }

  /** 清屏（新一轮运行前调用）。 */
  clear(): void {
    this.term.reset();
  }

  /** 销毁实例，释放 DOM 与事件监听、ResizeObserver。EditorHandle::drop → destroy。 */
  destroy(): void {
    this.resizeObserver?.disconnect();
    this.term.dispose();
  }
}

// 暴露 XtermOptions 到 window，供 wasm-bindgen 用 new XtermOptions()。
// IIFE 的 name 只能挂一个全局（XtermTerminal），故手动 hoist XtermOptions。
declare global {
  interface Window {
    XtermOptions: typeof XtermOptions;
  }
}
window.XtermOptions = XtermOptions;
