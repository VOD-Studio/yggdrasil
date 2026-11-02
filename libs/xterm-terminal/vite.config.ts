import { iifeLib } from '../vite-iife.ts';

// 输出直写 public/xterm/，由 cargo-leptos 的 assets-dir 拷进站点根，无需拷贝步骤。
// IIFE 产物挂在 window.XtermTerminal 上；默认导出（对象字面量 { create() }）即该全局。
export default iifeLib({
  dir: import.meta.dirname,
  out: 'xterm',
  name: 'XtermTerminal',
  file: 'terminal',
  defaultExport: true,
});
