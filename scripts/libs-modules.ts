import * as path from 'node:path';
import type * as Playwright from 'playwright';

/** libs/node_modules 下的路径（scripts 的 npm 依赖装在 libs workspace）。 */
export function libsModule(relative: string): string {
  return path.join(import.meta.dir, '../libs/node_modules', relative);
}

/** PLAYWRIGHT_MODULE 优先，否则使用 libs/node_modules/playwright。 */
export function loadPlaywright(): typeof Playwright {
  return require(process.env.PLAYWRIGHT_MODULE || libsModule('playwright')) as typeof Playwright;
}
