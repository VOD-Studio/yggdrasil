import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // Bound workers on shared CI runners; local development retains Vitest defaults.
    maxWorkers: process.env.CI ? 2 : undefined,
    environment: 'happy-dom',
    include: ['src/**/*.test.{ts,js}'],
  },
});
