#!/usr/bin/env bun
// 生成 src/api/markdown/mermaid/font_metrics.bin：服务端 Mermaid 排版用的字宽表。
//
// 用法：bun run mermaid-font-metrics <字体目录>。目录须含 FONTS 列出的 12 个字体文件。
// 每款字体按码点记录前进宽度（千分之一 em，向上取整以保持偏宽），码点连续且宽度相同的合并成一段。
// 文件格式（全部小端）：'YMFM'、u8 版本 1、u8 表数 12；每张表 u16 段数 + 每段 u16 start/end/milli。
// 表顺序与 src/api/markdown/mermaid/metrics.rs 一致：先 6 款常规字重，再 6 款粗体。
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

type Range = readonly [number, number];

const LATIN_RANGES: readonly Range[] = [
  [0x20, 0x52f],
  [0x1e00, 0x1eff],
  [0x2000, 0x2bff],
];

const CJK_RANGES: readonly Range[] = [
  ...LATIN_RANGES,
  [0x2e80, 0x9fff],
  [0xac00, 0xd7af],
  [0xf900, 0xfaff],
  [0xfe30, 0xfe4f],
  [0xff00, 0xffef],
];

/** 表顺序固定：0–5 常规（SF Pro Text、Selawik、Roboto、Liberation Sans、Noto Sans、Noto CJK），6–11 粗体同序。 */
const FONTS: readonly { file: string; cjk: boolean }[] = [
  { file: 'SF-Pro-Text-Regular.otf', cjk: false },
  { file: 'selawk.ttf', cjk: false },
  { file: 'Roboto-Regular.ttf', cjk: false },
  { file: 'LiberationSans-Regular.ttf', cjk: false },
  { file: 'NotoSans-Regular.ttf', cjk: false },
  { file: 'NotoSansCJK-Regular.ttc', cjk: true },
  { file: 'SF-Pro-Text-Bold.otf', cjk: false },
  { file: 'selawkb.ttf', cjk: false },
  { file: 'Roboto-Bold.ttf', cjk: false },
  { file: 'LiberationSans-Bold.ttf', cjk: false },
  { file: 'NotoSans-Bold.ttf', cjk: false },
  { file: 'NotoSansCJK-Bold.ttc', cjk: true },
];

/** 返回 OTF/TTF/TTC 第 `index` 个字形集的码点 → 前进宽度（em）。 */
function parseFont(buf: Uint8Array, index = 0): Map<number, number> {
  const dv = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  let off = 0;
  if (dv.getUint32(0) === 0x74746366 /* 'ttcf' */) off = dv.getUint32(12 + 4 * index);
  const numTables = dv.getUint16(off + 4);
  const tables: Record<string, { o: number; l: number }> = {};
  for (let i = 0; i < numTables; i++) {
    const r = off + 12 + 16 * i;
    const tag = String.fromCharCode(...buf.subarray(r, r + 4));
    tables[tag] = { o: dv.getUint32(r + 8), l: dv.getUint32(r + 12) };
  }
  const head = tables.head;
  const hhea = tables.hhea;
  const hmtx = tables.hmtx;
  const cmap = tables.cmap;
  if (!head || !hhea || !hmtx || !cmap) throw new Error('缺少 head/hhea/hmtx/cmap 表');
  const upem = dv.getUint16(head.o + 18);
  const nH = dv.getUint16(hhea.o + 34);
  const adv = (gid: number) => dv.getUint16(hmtx.o + 4 * Math.min(gid, nH - 1));
  const c = cmap.o;
  const n = dv.getUint16(c + 2);
  let best: { so: number; fmt: number } | null = null;
  for (let i = 0; i < n; i++) {
    const pid = dv.getUint16(c + 4 + 8 * i);
    const eid = dv.getUint16(c + 6 + 8 * i);
    const so = c + dv.getUint32(c + 8 + 8 * i);
    const fmt = dv.getUint16(so);
    if (pid === 3 && eid === 10 && fmt === 12) best = { so, fmt };
    else if (!best && pid === 3 && eid === 1 && fmt === 4) best = { so, fmt };
    else if (!best && pid === 0 && (fmt === 4 || fmt === 12)) best = { so, fmt };
  }
  if (!best) throw new Error('没有可用的 cmap 子表');
  const map = new Map<number, number>();
  if (best.fmt === 12) {
    const g = dv.getUint32(best.so + 12);
    for (let i = 0; i < g; i++) {
      const b = best.so + 16 + 12 * i;
      const s = dv.getUint32(b);
      const e = dv.getUint32(b + 4);
      const sg = dv.getUint32(b + 8);
      for (let cp = s; cp <= e; cp++) map.set(cp, sg + cp - s);
    }
  } else {
    const so = best.so;
    const seg = dv.getUint16(so + 6) / 2;
    const ends = so + 14;
    const starts = ends + 2 * seg + 2;
    const deltas = starts + 2 * seg;
    const ros = deltas + 2 * seg;
    for (let i = 0; i < seg; i++) {
      const e = dv.getUint16(ends + 2 * i);
      const s = dv.getUint16(starts + 2 * i);
      const d = dv.getInt16(deltas + 2 * i);
      const ro = dv.getUint16(ros + 2 * i);
      for (let cp = s; cp <= e && cp !== 0xffff; cp++) {
        let gid: number;
        if (ro === 0) gid = (cp + d) & 0xffff;
        else {
          gid = dv.getUint16(ros + 2 * i + ro + 2 * (cp - s));
          if (gid) gid = (gid + d) & 0xffff;
        }
        if (gid) map.set(cp, gid);
      }
    }
  }
  const res = new Map<number, number>();
  for (const [cp, gid] of map) res.set(cp, adv(gid) / upem);
  return res;
}

/** 限定码点范围后按 (start, end, milli) 游程编码。 */
function encodeRuns(
  widths: Map<number, number>,
  ranges: readonly Range[],
): [number, number, number][] {
  const runs: [number, number, number][] = [];
  for (const [lo, hi] of ranges) {
    for (let cp = lo; cp <= hi; cp++) {
      const em = widths.get(cp);
      if (em === undefined) continue;
      const milli = Math.ceil(em * 1000);
      if (milli > 0xffff) throw new Error(`U+${cp.toString(16)} 宽度超出 u16：${milli}`);
      const last = runs.at(-1);
      if (last && last[1] === cp - 1 && last[2] === milli) last[1] = cp;
      else runs.push([cp, cp, milli]);
    }
  }
  return runs;
}

const dir = process.argv[2];
if (!dir) {
  console.error('用法：bun run mermaid-font-metrics <字体目录>');
  process.exit(1);
}
const missing = FONTS.map((f) => f.file).filter((file) => !existsSync(join(dir, file)));
if (missing.length > 0) {
  console.error(`字体目录 ${dir} 缺少：${missing.join(' ')}`);
  process.exit(1);
}

const tables = FONTS.map(({ file, cjk }) => {
  const runs = encodeRuns(
    parseFont(readFileSync(join(dir, file)), 0),
    cjk ? CJK_RANGES : LATIN_RANGES,
  );
  if (runs.length > 0xffff) throw new Error(`${file} 段数超出 u16：${runs.length}`);
  return { file, runs };
});

const size = 6 + tables.reduce((sum, t) => sum + 2 + 6 * t.runs.length, 0);
const out = new Uint8Array(size);
const dv = new DataView(out.buffer);
out.set(new TextEncoder().encode('YMFM'), 0);
dv.setUint8(4, 1);
dv.setUint8(5, tables.length);
let at = 6;
for (const { runs } of tables) {
  dv.setUint16(at, runs.length, true);
  at += 2;
  for (const [start, end, milli] of runs) {
    dv.setUint16(at, start, true);
    dv.setUint16(at + 2, end, true);
    dv.setUint16(at + 4, milli, true);
    at += 6;
  }
}

const target = fileURLToPath(
  new URL('../../../src/api/markdown/mermaid/font_metrics.bin', import.meta.url),
);
writeFileSync(target, out);
for (const { file, runs } of tables) console.log(`${file}\t${runs.length} 段`);
console.log(`共 ${tables.reduce((sum, t) => sum + t.runs.length, 0)} 段，${size} 字节 → ${target}`);
