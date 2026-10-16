// 生成 src/components/ui/icons_data.rs：站点用到的 Lucide 图标。
import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { iconsRust } from '../src/icons';

const require = createRequire(import.meta.url);
const nodes = require('lucide-static/icon-nodes.json');
const { version } = JSON.parse(readFileSync(require.resolve('lucide-static/package.json'), 'utf8'));
const target = fileURLToPath(new URL('../../../src/components/ui/icons_data.rs', import.meta.url));
writeFileSync(target, iconsRust(nodes, version));
console.log(`wrote ${target}`);
