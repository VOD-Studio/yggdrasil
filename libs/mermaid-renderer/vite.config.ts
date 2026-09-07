import { resolve } from 'node:path';
import { defineConfig } from 'vite';

// mermaid 独立 IIFE bundle：~1MB，不打进 yggdrasil-core（避免拖累每篇文章首屏）。
// 由 yggdrasil-core 的 mermaid.ts 在 IntersectionObserver 视口可见时动态 import。
// 输出到 public/mermaid/mermaid.js，dx build 会作为静态资源拷贝。
export default defineConfig({
  build: {
    outDir: resolve(import.meta.dirname, '../../public/mermaid'),
    emptyOutDir: true,
    lib: {
      entry: resolve(import.meta.dirname, 'src/index.ts'),
      name: 'MermaidRenderer',
      fileName: () => 'mermaid.js',
      // IIFE 输出单文件，包含 Mermaid 的动态导入依赖。
      formats: ['iife'],
    },
    cssCodeSplit: false,
    minify: true,
    sourcemap: true,
  },
});
