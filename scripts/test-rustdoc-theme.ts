#!/usr/bin/env bun
// rustdoc 站点主题的只读浏览器回归，覆盖 make doc 注入的导航、主题与布局。
// Prerequisites: run `make doc` first and serve the site (default http://localhost:8080).
// VT_BASE=http://localhost:8080 RUSTDOC_QA_DIR=/tmp/yggdrasil-rustdoc-qa \
//   PLAYWRIGHT_MODULE=/path/to/playwright CHROMIUM_PATH=/usr/bin/chromium bun scripts/test-rustdoc-theme.ts
// Writes screenshots (struct page, crate index, source page, search results) and report.json to RUSTDOC_QA_DIR.
// Exits non-zero if any check or any pageerror fails.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import type * as Playwright from 'playwright';

const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright') as typeof Playwright;
const base = (process.env.VT_BASE || 'http://localhost:8080').replace(/\/$/, '');
const output = process.env.RUSTDOC_QA_DIR || '/tmp/yggdrasil-rustdoc-qa';

const STRUCT_URL = `${base}/doc/yggdrasil/models/note/struct.Note.html`;
const CRATE_URL = `${base}/doc/yggdrasil/index.html`;
const MODULE_URL = `${base}/doc/yggdrasil/models/index.html`;
const SOURCE_URL = `${base}/doc/src/yggdrasil/models/note.rs.html#49-82`;

interface Variant {
  name: string;
  width: number;
  height: number;
  colorScheme: 'light' | 'dark';
  reducedMotion: 'no-preference' | 'reduce';
}
interface CheckResult {
  variant: string;
  check: string;
  ok: boolean;
  error?: string;
}
const VARIANTS: Variant[] = [
  {
    name: 'desktop-light',
    width: 1440,
    height: 1000,
    colorScheme: 'light',
    reducedMotion: 'no-preference',
  },
  {
    name: 'desktop-dark',
    width: 1440,
    height: 1000,
    colorScheme: 'dark',
    reducedMotion: 'no-preference',
  },
  { name: 'mobile-light', width: 390, height: 844, colorScheme: 'light', reducedMotion: 'reduce' },
];

const ENGLISH_TYPENAMES = new Set([
  'keyword',
  'primitive type',
  'module',
  'extern crate',
  're-export',
  'struct',
  'enum',
  'function',
  'type alias',
  'static',
  'trait',
  'attribute macro',
  'trait alias',
  'trait method',
  'method',
  'struct field',
  'enum variant',
  'macro',
  'assoc type',
  'constant',
  'assoc const',
  'union',
  'foreign type',
  'existential type',
  'derive macro',
  'attribute',
]);

const overflowFree = (page: Playwright.Page) =>
  page.evaluate(() => document.documentElement.scrollWidth <= innerWidth);
const settle = async (page: Playwright.Page, url: string) => {
  await page.goto(url, { waitUntil: 'load' });
  await page.locator('.site-header').first().waitFor();
};

// Runs one named check; a failure is recorded (not thrown) so later checks still run.
function makeRunner(variant: Variant, results: CheckResult[]) {
  return async (name: string, fn: () => Promise<void>) => {
    try {
      await fn();
      results.push({ variant: variant.name, check: name, ok: true });
      console.log(`  ok   ${variant.name}: ${name}`);
    } catch (error) {
      const message = String(error instanceof Error ? error.message : error);
      results.push({ variant: variant.name, check: name, ok: false, error: message });
      console.error(
        `  FAIL ${variant.name}: ${name}\n       ${message.split('\n').join('\n       ')}`,
      );
    }
  };
}

async function runVariant(
  browser: Playwright.Browser,
  variant: Variant,
  results: CheckResult[],
  shots: string[],
) {
  const desktop = variant.width >= 1024;
  const context = await browser.newContext({
    viewport: { width: variant.width, height: variant.height },
    colorScheme: variant.colorScheme,
    reducedMotion: variant.reducedMotion,
  });
  const page = await context.newPage();
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.route('https://fonts.googleapis.com/**', (route) =>
    route.fulfill({ status: 200, contentType: 'text/css', body: '' }),
  );
  const check = makeRunner(variant, results);
  const shot = async (name: string, options: Playwright.PageScreenshotOptions = {}) => {
    const file = path.join(output, `${variant.name}-${name}.png`);
    await page.screenshot({ path: file, animations: 'disabled', ...options });
    shots.push(file);
  };

  // 1. Shell
  await settle(page, STRUCT_URL);
  await check('shell: site header visible, rustdoc topbar hidden', async () => {
    assert(await page.locator('.site-header').first().isVisible(), '.site-header not visible');
    assert.equal(
      await page
        .locator('rustdoc-topbar')
        .first()
        .isVisible()
        .catch(() => false),
      false,
      'rustdoc-topbar visible',
    );
  });
  if (desktop) {
    await check('shell: nav current item is 文档', async () => {
      const text = (
        await page.locator('.site-nav a[aria-current=page]').first().innerText()
      ).trim();
      assert.equal(text, '文档');
    });
  }
  await check('shell: no horizontal overflow', async () => {
    assert(await overflowFree(page), 'page overflows horizontally');
  });
  if (desktop) {
    await check('shell: sidebar sticks under header after 1200px scroll', async () => {
      await page.evaluate(() => scrollTo({ top: 1200, behavior: 'instant' }));
      const { top, header } = await page.evaluate(() => ({
        top: document.querySelector('nav.sidebar')!.getBoundingClientRect().top,
        header: document.querySelector('.site-header')!.getBoundingClientRect().height,
      }));
      assert(Math.abs(top - header) <= 1, `sidebar top ${top} vs header height ${header}`);
      await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
    });
  }

  // 2. Localization
  await check('localize: h1 ::before content is 结构体', async () => {
    const content = await page.evaluate(
      () => getComputedStyle(document.querySelector('h1')!, '::before').content,
    );
    assert.equal(content, '"结构体"');
  });
  await check('localize: #fields heading and count', async () => {
    const heading = page.locator('#fields');
    assert(
      (await heading.innerText()).trim().startsWith('字段'),
      'heading does not start with 字段',
    );
    const count = Number((await page.locator('#fields .doc-count').innerText()).replace(/\D/g, ''));
    const fields = await page.locator('#main-content > span.structfield').count();
    assert(fields > 0, 'no struct fields found');
    assert.equal(count, fields);
  });
  await check('localize: no "Show " text in summary.hideme', async () => {
    const texts = await page.locator('summary.hideme span').allInnerTexts();
    const bad = texts.filter((t) => t.trim().startsWith('Show '));
    assert.deepEqual(bad, []);
  });

  // 5. Table of contents
  if (desktop) {
    await check('toc: #doc-toc visible and right of #main-content', async () => {
      const toc = page.locator('#doc-toc');
      assert(await toc.isVisible(), '#doc-toc not visible');
      const { tocLeft, mainRight } = await page.evaluate(() => ({
        tocLeft: document.querySelector('#doc-toc')!.getBoundingClientRect().left,
        mainRight: document.querySelector('#main-content')!.getBoundingClientRect().right,
      }));
      assert(tocLeft >= mainRight - 1, `toc left ${tocLeft} < main right ${mainRight}`);
    });
    await check('toc: scrolling to #trait-implementations activates 实现的 Trait', async () => {
      await page.evaluate(() => document.querySelector('#trait-implementations')!.scrollIntoView());
      await page.waitForTimeout(300);
      const text = (await page.locator('#doc-toc a.active').first().innerText()).trim();
      assert.equal(text, '实现的 Trait');
    });
    await check('toc: data-toc-percent > 0%', async () => {
      const text = await page.locator('[data-toc-percent]').first().innerText();
      assert(parseFloat(text) > 0, `percent text: ${JSON.stringify(text)}`);
    });
    await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
  } else {
    await check('toc: mobile pill opens and Escape closes', async () => {
      const pill = page.locator('#doc-toc-pill');
      assert(await pill.isVisible(), '#doc-toc-pill not visible');
      await pill.click();
      await page.locator('.post-toc.is-open').waitFor({ timeout: 3000 });
      await page.keyboard.press('Escape');
      await page.waitForFunction(() => !document.querySelector('.post-toc.is-open'), null, {
        timeout: 3000,
      });
    });
  }

  // 6. More implementations
  await check('impls: details.doc-more-impls collapsed by default', async () => {
    const details = page.locator('details.doc-more-impls');
    assert((await details.count()) > 0, 'no details.doc-more-impls');
    assert.equal(await details.first().evaluate((el) => (el as HTMLDetailsElement).open), false);
  });
  await check('impls: chips carry data-short including Clone', async () => {
    const shorts = await page
      .locator('#trait-implementations-list [data-short]')
      .evaluateAll((els) => els.map((el) => el.getAttribute('data-short')));
    assert(
      shorts.some((s) => s?.includes('Clone')),
      `data-short values: ${JSON.stringify(shorts.slice(0, 20))}`,
    );
  });
  await check('impls: #impl-Send-for-Note expands details and scrolls into view', async () => {
    await settle(page, `${STRUCT_URL}#impl-Send-for-Note`);
    await page.waitForTimeout(300);
    const state = await page.evaluate(() => {
      const target = document.getElementById('impl-Send-for-Note');
      const details = target?.closest<HTMLDetailsElement>('details.doc-more-impls');
      const rect = target?.getBoundingClientRect();
      return {
        found: !!target,
        inMore: !!details,
        open: details ? details.open : null,
        inView: !!rect && rect.top >= 0 && rect.bottom <= innerHeight,
      };
    });
    assert(state.found, 'target not found');
    assert(state.inMore, 'target not inside details.doc-more-impls');
    assert.equal(state.open, true, 'details not open');
    assert(state.inView, 'target not within viewport');
  });

  // Screenshot: struct page
  await settle(page, STRUCT_URL);
  await shot('struct');

  // 7. Crate index
  await settle(page, CRATE_URL);
  await check('crate: at least 3 .doc-module-group', async () => {
    assert((await page.locator('.doc-module-group').count()) >= 3);
  });
  await check('crate: group dt total equals SIDEBAR_ITEMS.mod, no duplicates', async () => {
    // rustdoc 的 sidebar-items.js 在页面里按需加载后会清掉全局变量，所以直接取文件解析。
    const info = await page.evaluate(async () => {
      const names = [...document.querySelectorAll('.doc-module-group dt')].map((dt) =>
        dt.textContent!.trim(),
      );
      const text = await (await fetch('sidebar-items.js')).text();
      const holder = {} as { SIDEBAR_ITEMS: { mod: unknown[] } };
      new Function('window', text)(holder);
      const items = holder.SIDEBAR_ITEMS;
      return { names, expected: items.mod.length };
    });
    assert.equal(info.names.length, info.expected);
    assert.equal(
      new Set(info.names).size,
      info.names.length,
      `duplicates in ${JSON.stringify(info.names)}`,
    );
  });
  await check('crate: .doc-hero-meta matches /个模块$/', async () => {
    const text = (await page.locator('.doc-hero-meta').first().innerText()).trim();
    assert.match(text, /个模块$/);
  });
  await check(
    'crate: sidebar filter "og" hides non-matching modules and restores on clear',
    async () => {
      const input = page
        .locator('nav.sidebar input[type=search], nav.sidebar input[type=text], nav.sidebar input')
        .first();
      await input.fill('og');
      const visible = await page
        .locator('#rustdoc-modnav li')
        .evaluateAll((items) =>
          items
            .filter(
              (li) =>
                li.getClientRects().length > 0 && getComputedStyle(li).visibility !== 'hidden',
            )
            .map((li) => li.textContent),
        );
      assert(visible.length > 0, 'filter hid every module');
      assert(
        visible.every((text) => text!.includes('og')),
        `visible: ${JSON.stringify(visible)}`,
      );
      await input.fill('');
      const total = await page.locator('#rustdoc-modnav li').count();
      const shown = await page
        .locator('#rustdoc-modnav li')
        .evaluateAll((items) => items.filter((li) => li.getClientRects().length > 0).length);
      assert.equal(shown, total, 'not all modules visible after clearing');
    },
  );
  // 8b. crate index pub marker
  await check('crate: only infra is pub (::after "pub"), api is none', async () => {
    assert.equal(await page.locator('a.mod[href="infra/index.html"]').count(), 1);
    const after = (href: string) =>
      page.evaluate(
        (h) => getComputedStyle(document.querySelector(`a.mod[href="${h}"]`)!, '::after').content,
        href,
      );
    assert.equal(await after('infra/index.html'), '"pub"');
    assert.equal(await after('api/index.html'), 'none');
  });
  await shot('crate');

  // 8a. Module page
  await settle(page, MODULE_URL);
  await check('module: modnav current sub-nav count equals module items', async () => {
    const sub = await page.locator('#rustdoc-modnav li.current .doc-subnav li').count();
    const items = await page.locator('#main-content .item-table dt > a.mod').count();
    assert(items > 0, 'no module items');
    assert.equal(sub, items);
  });

  // 9. Mobile
  if (!desktop) {
    await settle(page, STRUCT_URL);
    await check('mobile: sidebar action opens .sidebar.shown', async () => {
      await page.locator('[data-doc-action=sidebar]').click();
      await page.locator('.sidebar.shown').waitFor({ timeout: 3000 });
      await page
        .locator('[data-doc-action=sidebar]')
        .click()
        .catch(() => {});
    });
    await check('mobile: menu action toggles #doc-nav-menu with Escape', async () => {
      await settle(page, STRUCT_URL);
      await page.locator('[data-doc-action=menu]').click();
      assert(await page.locator('#doc-nav-menu').isVisible(), '#doc-nav-menu not visible');
      await page.keyboard.press('Escape');
      await page.waitForFunction(
        () => document.querySelector('#doc-nav-menu')!.hasAttribute('hidden'),
        null,
        { timeout: 3000 },
      );
    });
    await check('mobile: no horizontal overflow', async () => {
      assert(await overflowFree(page));
    });
  }

  // 10. Source page
  await settle(page, SOURCE_URL);
  await check('source: header, code block visible, no overflow', async () => {
    assert(await page.locator('.site-header').first().isVisible(), '.site-header not visible');
    assert(await page.locator('.example-wrap').first().isVisible(), '.example-wrap not visible');
    assert(await overflowFree(page), 'page overflows horizontally');
  });
  await shot('source');
  await check('source: sidebar button opens #src-sidebar', async () => {
    await page.locator('.src #sidebar-button a').click();
    await page.locator('#src-sidebar').waitFor({ state: 'visible', timeout: 3000 });
  });

  // 4. Search
  await settle(page, STRUCT_URL);
  await check('search: Control+K opens #search and focuses input', async () => {
    await page.keyboard.press('Control+K');
    await page.locator('#search').waitFor({ state: 'visible', timeout: 3000 });
    await page.waitForFunction(() => document.activeElement?.matches('.search-input'), null, {
      timeout: 3000,
    });
  });
  await check('search: results localized (tab 名称, typenames)', async () => {
    await page.keyboard.type('Note');
    await page.locator('.search-results a').first().waitFor({ timeout: 10000 });
    const tab = (await page.locator('#search-tabs button.selected').first().innerText()).trim();
    assert(tab.startsWith('名称'), `selected tab: ${tab}`);
    const names = (await page.locator('.search-results .typename').allInnerTexts()).map((t) =>
      t.trim(),
    );
    assert(names.length > 0, 'no .typename found');
    const bad = names.filter((t) => ENGLISH_TYPENAMES.has(t.toLowerCase()));
    assert.deepEqual(bad, []);
    await shot('search');
  });

  // 3. Theme (desktop)
  if (desktop) {
    await settle(page, STRUCT_URL);
    await check('theme: choosing dark applies and persists', async () => {
      await page.locator('[data-theme-choice=dark]').first().click();
      const state = () =>
        page.evaluate(() => ({
          theme: document.documentElement.dataset.theme,
          dark: document.documentElement.classList.contains('dark'),
          stored: localStorage.getItem('yggdrasil-theme'),
          bg: getComputedStyle(document.body).backgroundColor,
        }));
      let s = await state();
      assert.equal(s.theme, 'dark');
      assert(s.dark, 'html lacks .dark class');
      assert.equal(s.stored, 'dark');
      assert.equal(s.bg, 'rgb(15, 15, 14)');
      await page.reload({ waitUntil: 'load' });
      s = await state();
      assert.equal(s.theme, 'dark', 'dark did not survive reload');
    });
    await check(
      'theme: 跟随系统 clears yggdrasil-theme, sets rustdoc-use-system-theme',
      async () => {
        await page.locator('.site-header [data-theme-choice=system]').first().click();
        const s = await page.evaluate(() => ({
          stored: localStorage.getItem('yggdrasil-theme'),
          system: localStorage.getItem('rustdoc-use-system-theme'),
        }));
        assert.equal(s.stored, null);
        assert.equal(s.system, 'true');
      },
    );
    await check('theme: stale rustdoc ayu settings overridden by yggdrasil light', async () => {
      await page.evaluate(() => {
        localStorage.setItem('rustdoc-theme', 'ayu');
        localStorage.setItem('rustdoc-use-system-theme', 'false');
        localStorage.setItem('yggdrasil-theme', 'light');
      });
      await page.reload({ waitUntil: 'load' });
      assert.equal(await page.evaluate(() => document.documentElement.dataset.theme), 'light');
    });
    await check('theme: settings popover hides #settings #theme', async () => {
      await page.locator('.main-heading .settings-menu > a').first().click();
      await page.locator('#settings').waitFor({ state: 'visible', timeout: 3000 });
      assert.equal(
        await page
          .locator('#settings #theme')
          .first()
          .isVisible()
          .catch(() => false),
        false,
      );
    });
  }

  await check('no pageerror events', async () => {
    assert.deepEqual(errors, []);
  });
  await context.close();
}

async function main() {
  await fs.mkdir(output, { recursive: true });
  const results: CheckResult[] = [];
  const shots: string[] = [];
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
  });
  try {
    for (const variant of VARIANTS) {
      console.log(`== ${variant.name}`);
      try {
        await runVariant(browser, variant, results, shots);
      } catch (error) {
        const message = String(error instanceof Error ? error.stack || error : error);
        results.push({ variant: variant.name, check: 'variant setup', ok: false, error: message });
        console.error(`  FAIL ${variant.name}: variant aborted\n${message}`);
      }
    }
  } finally {
    await browser.close();
  }
  const failed = results.filter((r) => !r.ok);
  await fs.writeFile(
    path.join(output, 'report.json'),
    JSON.stringify(
      {
        base,
        total: results.length,
        failed: failed.length,
        screenshots: shots,
        results,
      },
      null,
      2,
    ),
  );
  if (failed.length) {
    console.error(`\n${failed.length}/${results.length} checks failed:`);
    for (const f of failed) console.error(`  - ${f.variant}: ${f.check}`);
    process.exitCode = 1;
  } else {
    console.log(`\nPASS ${results.length} checks; report: ${path.join(output, 'report.json')}`);
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
