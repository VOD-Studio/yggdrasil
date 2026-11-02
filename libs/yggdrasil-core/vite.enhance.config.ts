import { iifeLib } from '../vite-iife.ts';

// P5 §8.2 增强 IIFE：文章、图鉴、changelog 的增强函数（见 src/enhance.ts）。
// 在 core 构建之后追加（不清空输出目录）；本入口不 import 任何 CSS——共享样式
// 由 core 入口随页面全局加载，避免增强脚本晚到时样式缺失。
export default iifeLib({
  dir: import.meta.dirname,
  out: 'yggdrasil-core',
  name: 'YggdrasilEnhance',
  file: 'yggdrasil-enhance',
  entry: 'src/enhance.ts',
  emptyOutDir: false,
});
