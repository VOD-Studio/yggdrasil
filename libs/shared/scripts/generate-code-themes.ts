// 生成文章和编辑器的代码高亮配色。
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { highlightCss, hljsThemeCss } from '../src/code-palette';

const siteTarget = fileURLToPath(new URL('../../../public/highlight.css', import.meta.url));
mkdirSync(dirname(siteTarget), { recursive: true });
writeFileSync(siteTarget, highlightCss());
console.log(`生成 ${siteTarget}`);

if (!process.argv.includes('--site-only')) {
  const editorTarget = fileURLToPath(
    new URL('../../tiptap-editor/src/code-highlight.css', import.meta.url),
  );
  writeFileSync(editorTarget, hljsThemeCss());
  console.log(`生成 ${editorTarget}`);
}
