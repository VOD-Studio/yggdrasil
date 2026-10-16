import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // prefersReducedMotion 读 window.matchMedia，需要 happy-dom。
    environment: 'happy-dom',
    include: ['src/**/*.test.{ts,js}'],
  },
});
