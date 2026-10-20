import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // Bound workers on shared CI runners; local development retains Vitest defaults.
    maxWorkers: process.env.CI ? 2 : undefined,
    // 默认 happy-dom：index.ts 的行为测试需要真实 DOM（点击/keydown/scroll/IO）。
    // geometry.test.ts 是纯函数，用文件内 // @vitest-environment node 指令保持 node 环境。
    environment: 'happy-dom',
    include: ['src/**/*.test.ts'],
  },
});
