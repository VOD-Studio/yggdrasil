import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { expect, it } from 'vitest';
import { hljsThemeCss } from './code-palette';

it('matches the committed Tiptap code-highlight.css', () => {
  const path = resolve(import.meta.dirname, '../../tiptap-editor/src/code-highlight.css');
  expect(readFileSync(path, 'utf8')).toBe(hljsThemeCss());
});
