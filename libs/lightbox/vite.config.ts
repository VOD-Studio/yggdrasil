import { resolve } from 'node:path';
import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    outDir: resolve(import.meta.dirname, '../../public/lightbox'),
    emptyOutDir: true,
    lib: {
      entry: resolve(import.meta.dirname, 'src/index.ts'),
      name: 'Lightbox',
      fileName: () => 'lightbox.js',
      formats: ['iife'],
    },
    rolldownOptions: {
      output: {
        assetFileNames: 'lightbox.[ext]',
      },
    },
    cssCodeSplit: false,
    minify: true,
    sourcemap: true,
  },
});
