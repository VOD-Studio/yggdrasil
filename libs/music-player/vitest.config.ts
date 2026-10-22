import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    maxWorkers: process.env.CI ? 2 : undefined,
    environment: 'happy-dom',
    include: ['src/**/*.test.ts'],
  },
});
