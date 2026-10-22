import { resolve } from 'node:path';
import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    outDir: resolve(import.meta.dirname, '../../public/music-player'),
    emptyOutDir: true,
    lib: {
      entry: resolve(import.meta.dirname, 'src/index.ts'),
      name: 'YggdrasilMusicPlayer',
      fileName: () => 'player.js',
      formats: ['iife'],
    },
    minify: true,
    sourcemap: true,
  },
});
