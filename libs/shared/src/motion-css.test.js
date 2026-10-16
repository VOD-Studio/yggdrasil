import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { expect, it } from 'vitest';
import { motionCss } from './spring';

// 生成产物入库：参数改了却没重新生成时在这里失败。
it('matches the committed styles/motion.css', () => {
  const file = resolve(import.meta.dirname, '../../../styles/motion.css');
  expect(readFileSync(file, 'utf8')).toBe(motionCss());
});
