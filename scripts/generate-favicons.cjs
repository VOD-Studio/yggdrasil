#!/usr/bin/env bun
// 生成网站图标：衬线 Y 加陶土橙方点（见 docs/v1.0-重设计方案.md「网站图标」）。
//
// 输出到 public/：
// - favicon.svg：浅色为默认，prefers-color-scheme: dark 时换深底浅字；
// - favicon.ico：16 / 32（加粗）与 48 三种尺寸的 PNG 条目，供不支持 SVG 图标的浏览器；
// - apple-touch-icon.png：180px 满底色，由系统裁圆角；
// - icon-192.png / icon-512.png：满底色并留 10% 安全边距，供安装为应用时裁切。
//
// 用法：bun scripts/generate-favicons.cjs（需要 libs/ 里已安装的 Playwright，
// 或用 PLAYWRIGHT_MODULE 指定）。
const fs = require('node:fs');
const path = require('node:path');

const { chromium } = require(process.env.PLAYWRIGHT_MODULE || path.join(__dirname, '../libs/node_modules/playwright'));

const PUBLIC = path.join(__dirname, '../public');
const LIGHT = { bg: '#f5f5f3', fg: '#151515', ring: 'rgba(0,0,0,0.12)' };
const DARK = { bg: '#0f0f0e', fg: '#f2f1ee', ring: null };
const CLAY = '#d97757';

/** 64 网格里的衬线 Y：左臂粗、右臂细、竖笔，加三处衬线；bold 供 16 / 32px 使用。 */
function letterPath(bold) {
  const [stemL, stemR] = bold ? [24.8, 31.6] : [25.4, 30.8];
  const arm = bold ? 11 : 9.6;
  const thin = bold ? 4.2 : 3;
  const serif = bold ? 3 : 2.3;
  const top = 13;
  const base = 51;
  const joint = 33.5;
  const f = (n) => Number(n.toFixed(1));
  return [
    `M14.2 ${top} L${f(14.2 + arm)} ${top} L${stemR} ${joint} L${stemL} ${f(joint + 1.8)} Z`,
    `M${f(39.6 - thin)} ${top} L39.6 ${top} L${stemR} ${f(joint + 1.2)} L${f(stemR - thin - 0.6)} ${joint} Z`,
    `M${stemL} ${joint - 1} L${stemR} ${joint - 1} L${stemR} ${f(base - serif + 0.2)} L${stemL} ${f(base - serif + 0.2)} Z`,
    `M10.4 ${top} L27.6 ${top} L27.6 ${f(top + serif)} L10.4 ${f(top + serif)} Z`,
    `M33.4 ${top} L43.4 ${top} L43.4 ${f(top + serif)} L33.4 ${f(top + serif)} Z`,
    `M18.8 ${f(base - serif)} L37.4 ${f(base - serif)} L37.4 ${base} L18.8 ${base} Z`,
  ].join(' ');
}

const DOT = `<rect x="45" y="41" width="10" height="10" rx="2.5" fill="${CLAY}"/>`;

/** 单一配色的图标；full 为满底色（应用图标），safe 为安全边距比例。 */
function iconSvg({ colors, bold = false, full = false, safe = 0, size = 64 }) {
  const glyph = `<path fill="${colors.fg}" d="${letterPath(bold)}"/>${DOT}`;
  const scale = 1 - 2 * safe;
  const body = safe
    ? `<g transform="translate(${64 * safe} ${64 * safe}) scale(${scale})">${glyph}</g>`
    : glyph;
  const bg = full
    ? `<rect width="64" height="64" fill="${colors.bg}"/>`
    : `<rect width="64" height="64" rx="14" fill="${colors.bg}"/>`;
  const ring =
    !full && colors.ring
      ? `<rect x="0.5" y="0.5" width="63" height="63" rx="13.5" fill="none" stroke="${colors.ring}"/>`
      : '';
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 64 64">${bg}${ring}${body}</svg>`;
}

/** 浏览器标签页用的 SVG：跟随系统明暗切换配色。 */
function faviconSvg() {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
<style>
.bg{fill:${LIGHT.bg}}.ring{stroke:${LIGHT.ring}}.fg{fill:${LIGHT.fg}}
@media (prefers-color-scheme: dark){.bg{fill:${DARK.bg}}.ring{stroke:transparent}.fg{fill:${DARK.fg}}}
</style>
<rect class="bg" width="64" height="64" rx="14"/>
<rect class="ring" x="0.5" y="0.5" width="63" height="63" rx="13.5" fill="none"/>
<path class="fg" d="${letterPath(true)}"/>
${DOT}
</svg>
`;
}

/** 把若干 PNG 打包成 ICO（PNG 条目，Vista 起支持）。 */
function ico(entries) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(entries.length, 4);
  const dir = Buffer.alloc(16 * entries.length);
  let offset = 6 + dir.length;
  entries.forEach(({ size, png }, i) => {
    const o = i * 16;
    dir.writeUInt8(size >= 256 ? 0 : size, o);
    dir.writeUInt8(size >= 256 ? 0 : size, o + 1);
    dir.writeUInt8(0, o + 2);
    dir.writeUInt8(0, o + 3);
    dir.writeUInt16LE(1, o + 4);
    dir.writeUInt16LE(32, o + 6);
    dir.writeUInt32LE(png.length, o + 8);
    dir.writeUInt32LE(offset, o + 12);
    offset += png.length;
  });
  return Buffer.concat([header, dir, ...entries.map((e) => e.png)]);
}

async function main() {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  const render = async (svg, size) => {
    await page.setViewportSize({ width: size, height: size });
    await page.setContent(`<html><body style="margin:0;background:transparent">${svg}</body></html>`);
    return page.screenshot({ omitBackground: true, clip: { x: 0, y: 0, width: size, height: size } });
  };
  try {
    fs.writeFileSync(path.join(PUBLIC, 'favicon.svg'), faviconSvg());
    const entries = [];
    for (const size of [16, 32, 48]) {
      const svg = iconSvg({ colors: LIGHT, bold: size <= 32, size });
      entries.push({ size, png: await render(svg, size) });
    }
    fs.writeFileSync(path.join(PUBLIC, 'favicon.ico'), ico(entries));
    fs.writeFileSync(
      path.join(PUBLIC, 'apple-touch-icon.png'),
      await render(iconSvg({ colors: LIGHT, full: true, safe: 0.06, size: 180 }), 180),
    );
    for (const size of [192, 512]) {
      fs.writeFileSync(
        path.join(PUBLIC, `icon-${size}.png`),
        await render(iconSvg({ colors: LIGHT, full: true, safe: 0.1, size }), size),
      );
    }
  } finally {
    await browser.close();
  }
  console.log('wrote favicon.svg, favicon.ico, apple-touch-icon.png, icon-192.png, icon-512.png');
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
