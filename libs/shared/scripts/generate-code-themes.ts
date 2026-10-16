// 生成 themes/*.tmTheme：文章代码高亮（syntect → public/highlight.css）的配色来源。
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { tmThemeFiles } from '../src/code-palette';

for (const { file, content } of tmThemeFiles()) {
  const target = fileURLToPath(new URL(`../../../themes/${file}`, import.meta.url));
  writeFileSync(target, content);
  console.log(`wrote ${target}`);
}
