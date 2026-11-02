import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { expect, it } from 'vitest';
import { CUSTOM_ICONS, ICONS, iconBody, iconsRust, variantName } from './icons';

const require = createRequire(import.meta.url);
const nodes = require('lucide-static/icon-nodes.json');
const { version } = require('lucide-static/package.json');

// 生成产物入库：改了图标清单却没重新生成时在这里失败。
it('matches the committed src/components/ui/icons_data.rs', () => {
  const file = resolve(import.meta.dirname, '../../../src/components/ui/icons_data.rs');
  expect(readFileSync(file, 'utf8')).toBe(iconsRust(nodes, version));
});

it('keeps icon names unique and present in lucide-static', () => {
  expect(new Set(ICONS).size).toBe(ICONS.length);
  for (const slug of ICONS) expect(nodes[slug] ?? CUSTOM_ICONS[slug], slug).toBeDefined();
});

it('converts slugs and nodes', () => {
  expect(variantName('arrow-up-right')).toBe('ArrowUpRight');
  expect(iconBody([['circle', { cx: 11, cy: 11, r: 8, key: 'a' }]])).toBe(
    '<circle cx="11" cy="11" r="8"/>',
  );
});

it('refuses icons missing from lucide-static', () => {
  expect(() => iconsRust({}, '0.0.0')).toThrow(/缺少图标/);
});
