import { resolve } from 'node:path';
import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    outDir: resolve(import.meta.dirname, '../../public/tiptap'),
    emptyOutDir: true,
    lib: {
      entry: resolve(import.meta.dirname, 'src/index.ts'),
      name: 'TiptapEditor',
      fileName: () => 'editor.js',
      formats: ['iife'],
    },
    rolldownOptions: {
      output: {
        exports: 'default',
        assetFileNames: 'editor.[ext]',
      },
    },
    cssCodeSplit: false,
    minify: true,
    sourcemap: true,
  },
});
