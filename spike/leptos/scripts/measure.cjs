#!/usr/bin/env bun
// 首屏 WASM/JS 体积：取 SSR HTML 里 HydrationScripts 预加载的 /pkg/ 资源（即该路由
// 直接打开时浏览器必须下载的集合），按发布快照里的原文件与 .br 旁车求和。
// 用法：bun scripts/measure.cjs <site-dir> <base-url> <route>...
const { readFileSync, statSync, existsSync, readdirSync } = require('node:fs');
const { join } = require('node:path');

const [siteDir, base, ...routes] = process.argv.slice(2);
if (!siteDir || !base || routes.length === 0) {
  console.error('用法：bun scripts/measure.cjs <site-dir> <base-url> <route>...');
  process.exit(1);
}

const size = (rel) => statSync(join(siteDir, rel)).size;
const brSize = (rel) => (existsSync(join(siteDir, `${rel}.br`)) ? size(`${rel}.br`) : null);

function pkgRefs(html) {
  const refs = new Set();
  for (const m of html.matchAll(/(?:href|src)="(\/pkg\/[^"]+\.(?:wasm|js))"/g)) refs.add(m[1]);
  // HydrationScripts 的内联模块脚本里还会 import 主 JS 并 fetch 主 WASM。
  for (const m of html.matchAll(/["'](\/pkg\/[^"']+\.(?:wasm|js))["']/g)) refs.add(m[1]);
  return [...refs].map((r) => r.replace(/^\//, ''));
}

const sum = (list, f) => list.reduce((acc, rel) => acc + (f(rel) ?? 0), 0);
const rows = [];
for (const route of routes) {
  const html = await (await fetch(new URL(route, base))).text();
  const refs = pkgRefs(html);
  const wasm = refs.filter((r) => r.endsWith('.wasm'));
  const js = refs.filter((r) => r.endsWith('.js'));
  rows.push({
    route,
    wasm_files: wasm.length,
    wasm_raw: sum(wasm, size),
    wasm_br: sum(wasm, brSize),
    js_raw: sum(js, size),
    js_br: sum(js, brSize),
    files: refs,
  });
}

const pkg = readdirSync(join(siteDir, 'pkg')).filter((f) => f.endsWith('.wasm'));
const all = pkg.map((f) => `pkg/${f}`);
const main = all.filter((f) => /^pkg\/ygg_spike\.[^.]+\.wasm$/.test(f));
console.log(
  JSON.stringify(
    {
      total_wasm: { files: all.length, raw: sum(all, size), br: sum(all, brSize) },
      main_wasm: { file: main[0], raw: sum(main, size), br: sum(main, brSize) },
      chunks: all
        .filter((f) => !main.includes(f))
        .map((f) => ({ file: f.replace('pkg/', ''), raw: size(f), br: brSize(f) })),
      routes: rows,
    },
    null,
    2,
  ),
);
