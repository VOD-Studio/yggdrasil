import { resolve } from 'node:path';
import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    outDir: resolve(import.meta.dirname, '../../public/tiptap'),
    emptyOutDir: false,
    sourcemap: process.env.YGG_RELEASE_ASSETS !== '1',
    lib: {
      entry: {
        'math-renderer': resolve(import.meta.dirname, 'src/math-renderer.ts'),
        'code-highlighter': resolve(import.meta.dirname, 'src/code-highlighter.ts'),
      },
      formats: ['es'],
    },
    rolldownOptions: { output: { minify: true } },
  },
});
