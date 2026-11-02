// 默认 happy-dom（vitest-base）：index.ts 的行为测试需要真实 DOM（点击/keydown/scroll/IO）。
// geometry.test.ts 是纯函数，用文件内 // @vitest-environment node 指令保持 node 环境。
export { default } from '../vitest-base.ts';
