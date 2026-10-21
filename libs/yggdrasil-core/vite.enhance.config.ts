import { resolve } from 'node:path';
import { defineConfig } from 'vite';

// P5 §8.2 增强 IIFE：文章、图鉴、changelog 的增强函数（见 src/enhance.ts）。
// 在 core 构建之后追加（不清空输出目录）；本入口不 import 任何 CSS——共享样式
// 由 core 入口随页面全局加载，避免增强脚本晚到时样式缺失。
export default defineConfig({
  build: {
    outDir: resolve(import.meta.dirname, '../../public/yggdrasil-core'),
    emptyOutDir: false,
    lib: {
      entry: resolve(import.meta.dirname, 'src/enhance.ts'),
      name: 'YggdrasilEnhance',
      fileName: () => 'yggdrasil-enhance.js',
      formats: ['iife'],
    },
    cssCodeSplit: false,
    minify: true,
    sourcemap: true,
  },
});
