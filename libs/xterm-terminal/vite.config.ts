import { resolve } from 'node:path';
import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    // 输出直写 public/xterm/，Dioxus 直接托管，无需拷贝步骤。
    outDir: resolve(import.meta.dirname, '../../public/xterm'),
    emptyOutDir: true,
    lib: {
      entry: resolve(import.meta.dirname, 'src/index.ts'),
      // IIFE 产物挂在 window.XtermTerminal 上，Rust 侧用 Reflect::get 取。
      name: 'XtermTerminal',
      fileName: () => 'terminal.js',
      formats: ['iife'],
    },
    rolldownOptions: {
      output: {
        // 默认导出（对象字面量 { create() }）成为 window.XtermTerminal。
        exports: 'default',
        assetFileNames: 'terminal.[ext]',
      },
    },
    cssCodeSplit: false,
    minify: true,
    sourcemap: true,
  },
});
