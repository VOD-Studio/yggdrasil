import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { expect, it } from 'vitest';
import { hljsThemeCss, tmThemeFiles } from './code-palette';

// 生成产物入库：改了取色却没重新生成时在这里失败。
it('matches the committed themes/*.tmTheme', () => {
  for (const { file, content } of tmThemeFiles()) {
    const path = resolve(import.meta.dirname, '../../../themes', file);
    expect(readFileSync(path, 'utf8'), file).toBe(content);
  }
});

it('matches the committed Tiptap code-highlight.css', () => {
  const path = resolve(import.meta.dirname, '../../tiptap-editor/src/code-highlight.css');
  expect(readFileSync(path, 'utf8')).toBe(hljsThemeCss());
});
