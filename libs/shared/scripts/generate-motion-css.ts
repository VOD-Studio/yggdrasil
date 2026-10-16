// 生成 styles/motion.css：四档弹簧的 linear() 缓动与收敛时长。
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { motionCss } from '../src/spring';

const target = fileURLToPath(new URL('../../../styles/motion.css', import.meta.url));
writeFileSync(target, motionCss());
console.log(`wrote ${target}`);
