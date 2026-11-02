import { iifeLib } from '../vite-iife.ts';

// P5 §8.2 拆分：core（启动必需，阻塞执行）与 enhance（增强，按需 defer，见
// vite.enhance.config.ts）两个 IIFE。iife 格式不支持多入口共享 chunk，两条配置
// 独立打包；共享模块（@yggdrasil/shared）在两个 IIFE 里各存一份，均为无状态工具，
// 重复安全。带状态的 core 模块（主题 registry、anchor-click 幂等位）绝不能被
// enhance 复制，enhance.ts 只经 window 全局桥接消费 core 能力。
// core 先行构建并清空输出目录；enhance 随后追加（emptyOutDir: false）。
export default iifeLib({
  dir: import.meta.dirname,
  out: 'yggdrasil-core',
  name: 'YggdrasilCore',
  file: 'yggdrasil-core',
});
