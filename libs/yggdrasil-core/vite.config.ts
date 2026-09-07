import { resolve } from 'node:path';
import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    outDir: resolve(import.meta.dirname, '../../public/yggdrasil-core'),
    emptyOutDir: true,
    lib: {
      entry: resolve(import.meta.dirname, 'src/index.ts'),
      name: 'YggdrasilCore',
      fileName: () => 'yggdrasil-core.js',
      formats: ['iife'],
    },
    rolldownOptions: {
      output: {
        assetFileNames: 'yggdrasil-core.[ext]',
      },
    },
    cssCodeSplit: false,
    minify: true,
    sourcemap: true,
  },
});
