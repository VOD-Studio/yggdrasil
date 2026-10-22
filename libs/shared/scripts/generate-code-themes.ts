// 生成代码配色产物：themes/*.tmTheme（文章代码高亮，syntect → public/highlight.css 的配色来源）
// 与 Tiptap 编辑器代码块的 highlight.js 配色。
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { hljsThemeCss, tmThemeFiles } from '../src/code-palette';

for (const { file, content } of tmThemeFiles()) {
  const target = fileURLToPath(new URL(`../../../themes/${file}`, import.meta.url));
  writeFileSync(target, content);
  console.log(`wrote ${target}`);
}

const hljsTarget = fileURLToPath(
  new URL('../../tiptap-editor/src/code-highlight.css', import.meta.url),
);
writeFileSync(hljsTarget, hljsThemeCss());
console.log(`wrote ${hljsTarget}`);
