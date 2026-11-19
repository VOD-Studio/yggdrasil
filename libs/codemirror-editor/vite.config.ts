import { resolve } from 'node:path';
import { defineConfig } from 'vite';

const release = process.env.YGG_RELEASE_ASSETS === '1';

// 语言与 Vim 共享编辑器核心中的 CodeMirror 模块实例，确保扩展与 Compartment 兼容。
export default defineConfig({
  base: './',
  build: {
    outDir: resolve(import.meta.dirname, '../../public/codemirror'),
    emptyOutDir: true,
    sourcemap: !release,
    manifest: !release,
    lib: {
      entry: resolve(import.meta.dirname, 'src/index.ts'),
      formats: ['es'],
      fileName: 'editor-module',
    },
    rolldownOptions: {
      output: { chunkFileNames: 'chunks/[name].[hash:12].js' },
    },
  },
});
