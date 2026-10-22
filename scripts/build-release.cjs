#!/usr/bin/env bun
/**
 * P3 双客户端产物合并与发布清单、P5 WASM preload（plan §6.4/§6.5/§8.1）。
 *
 * 输入：make snapshot-flavor 产出的两个 flavor 快照
 *   <assembly>/<flavor>/public/          —— 该 flavor dx 构建完成、restore-webp 之后的 public
 *   <assembly>/<flavor>/.manifest.json   —— dx 的资产清单（可选，用于剔除增量构建的陈旧哈希产物）
 * 输出：
 *   <out>/public/                        —— 合并后的最终发布目录（先清空再装配）
 *     index.html                         —— full 客户端模板
 *     flavors/reader/index.html          —— reader 客户端模板
 *     release-manifest.json              —— 发布清单（release_id、源码摘要、每 flavor 资产闭包）
 *
 * 合并规则（plan §6.4 第 5/6 步）：
 *   - 同路径同内容 → 复用一份；
 *   - 同路径不同内容 → 硬失败（禁止 cp -r 式静默覆盖）；
 *   - assets/ 与 wasm/ 下只收录 dx 清单列出的文件（无清单则全收），
 *     避免把上次构建遗留的陈旧哈希产物带进发布。
 *
 * 用法：
 *   bun scripts/build-release.cjs --out target/dx/yggdrasil/release/web
 *   bun scripts/build-release.cjs --verify --out target/dx/yggdrasil/release/web
 *     （precompress 之后跑：额外校验每个文本资产都有 .br 旁车）
 */
import { createHash } from 'node:crypto';
import { execSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, renameSync, rmSync, statSync, mkdirSync, writeFileSync, copyFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { bootstrapRefs, preloadWasm, verifyWasmPreload, wasmBootstrap } from './release-bootstrap.cjs';

const ROOT = resolve(import.meta.dir, '..');
const args = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] && !args[i + 1].startsWith('--') ? args[i + 1] : fallback;
};
const hasFlag = (name) => args.includes(name);

const OUT = resolve(ROOT, flag('--out', 'target/dx/yggdrasil/release/web'));
const ASSEMBLY = resolve(ROOT, flag('--assembly', 'target/release-assembly'));
const FLAVORS = (flag('--flavors', 'full,reader') || '').split(',').map((s) => s.trim()).filter(Boolean);
const VERIFY = hasFlag('--verify');

const TEXT_EXT = new Set(['.js', '.css', '.wasm', '.html', '.json', '.svg', '.xml']);
const HASHED_DIRS = new Set(['assets', 'wasm']); // dx 生成、按内容哈希命名的目录

const sha256 = (buf) => createHash('sha256').update(buf).digest('hex');

function walk(dir, base = dir, acc = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, entry.name);
    if (entry.isDirectory()) walk(p, base, acc);
    else acc.push(relative(base, p));
  }
  return acc;
}

function sh(cmd) {
  try {
    return execSync(cmd, { cwd: ROOT, encoding: 'utf8' }).trim();
  } catch {
    return '';
  }
}

/**
 * dx 资产清单里登记的相对 public 路径集合。
 * - 普通拷贝项：bundled_path 形如 "../codemirror/editor.js"（相对 web/ 目录）。
 * - 构建产物（add_hash: true，本次编译生成的 wasm/胶水 JS）：bundled_path 只含
 *   文件名，实际落在 public/assets/ 下——登记为 "assets/<bundled_path>"。
 * 合并时 assets/、wasm/ 只收录集合内的文件，剔除增量构建遗留的陈旧哈希产物。
 */
function manifestPaths(snapshotDir) {
  const manifestFile = join(snapshotDir, '.manifest.json');
  const paths = new Set();
  if (!existsSync(manifestFile)) return null;
  const manifest = JSON.parse(readFileSync(manifestFile, 'utf8'));
  for (const variants of Object.values(manifest.assets ?? {})) {
    for (const v of variants) {
      if (typeof v.bundled_path !== 'string') continue;
      if (v.options?.add_hash) {
        paths.add('assets/' + v.bundled_path.replace(/^\/+/, ''));
      } else {
        paths.add(v.bundled_path.replace(/^(?:\.\.\/)+/, '').replace(/^\.\//, ''));
      }
    }
  }
  return paths;
}

function bootstrapConflicts(flavorRefs) {
  const owners = new Map();
  const problems = [];
  for (const [flavor, refs] of Object.entries(flavorRefs)) {
    for (const ref of refs) {
      if (owners.has(ref)) {
        problems.push(`flavor ${owners.get(ref)} 与 ${flavor} 的模板引用了同一入口 ${ref}——疑似两次构建产物相同`);
      } else owners.set(ref, flavor);
    }
  }
  return problems;
}

if (VERIFY) {
  // ── 校验模式：模板引用闭包 + 清单一致性 + .br 旁车 ──────────────
  const publicDir = join(OUT, 'public');
  const manifest = JSON.parse(readFileSync(join(publicDir, 'release-manifest.json'), 'utf8'));
  const problems = [];
  const exists = (rel) => existsSync(join(publicDir, rel));

  for (const [flavor, info] of Object.entries(manifest.flavors)) {
    const html = readFileSync(join(publicDir, info.template), 'utf8');
    try {
      const refs = bootstrapRefs(html);
      if (JSON.stringify(refs) !== JSON.stringify([...info.bootstrap_assets].sort())) {
        throw new Error('模板入口引用与发布清单不一致');
      }
      const { wasm } = wasmBootstrap(html, (ref) => readFileSync(join(publicDir, ref), 'utf8'));
      verifyWasmPreload(html, wasm);
      const assets = new Set(info.assets.map((asset) => asset.path));
      for (const ref of refs) {
        if (!assets.has(ref)) throw new Error(`入口不属于本 flavor 资产闭包：${ref}`);
      }
    } catch (error) {
      problems.push(`[${flavor}] ${error.message}`);
    }
    for (const ref of info.bootstrap_assets) {
      if (!exists(ref.replace(/^\//, '').replace(/^\.\//, ''))) {
        problems.push(`[${flavor}] 模板 ${info.template} 引用的产物缺失：${ref}`);
      }
    }
    for (const asset of info.assets) {
      if (!exists(asset.path)) problems.push(`[${flavor}] 清单资产缺失：${asset.path}`);
      else {
        const content = readFileSync(join(publicDir, asset.path));
        if (content.length !== asset.bytes || sha256(content) !== asset.sha256) {
          problems.push(`[${flavor}] 清单资产摘要或大小不匹配：${asset.path}`);
        }
      }
      if (TEXT_EXT.has(asset.path.slice(asset.path.lastIndexOf('.'))) && !exists(asset.path + '.br')) {
        problems.push(`[${flavor}] 文本资产缺 .br 旁车（先跑 precompress）：${asset.path}`);
      }
    }
  }

  // 两份模板不得引用同一份哈希 wasm/js（否则两个 flavor 加载了同一个客户端）。
  problems.push(...bootstrapConflicts(Object.fromEntries(
    Object.entries(manifest.flavors).map(([flavor, info]) => [flavor, info.bootstrap_assets]),
  )));

  if (problems.length) {
    console.error(`\n发布校验失败（${problems.length} 项）：`);
    for (const p of problems) console.error(`  - ${p}`);
    process.exit(1);
  }
  console.log(`发布校验通过：${FLAVORS.join(' / ')} 模板、资产闭包与 .br 旁车完整（${OUT}/public）`);
  process.exit(0);
}

// ── 合并模式 ─────────────────────────────────────────────────────
const snapshots = FLAVORS.map((flavor) => {
  const dir = join(ASSEMBLY, flavor, 'public');
  if (!statSync(dir, { throwIfNoEntry: false })?.isDirectory()) {
    console.error(`缺少 ${flavor} 快照：${dir}\n先运行 make snapshot-flavor FLAVOR=${flavor}`);
    process.exit(1);
  }
  return { flavor, dir, manifest: manifestPaths(join(ASSEMBLY, flavor)) };
});

// 两份模板必须不同（reader 构建若未生效则这里失败，绝不发布假双产物）。
const fullIndex = readFileSync(join(snapshots[0].dir, 'index.html'));
const readerSnap = snapshots.find((s) => s.flavor === 'reader');
if (readerSnap) {
  const readerIndex = readFileSync(join(readerSnap.dir, 'index.html'));
  if (fullIndex.equals(readerIndex)) {
    console.error('reader 模板与 full 模板完全相同——client-reader 未生效或快照过期，拒绝发布');
    process.exit(1);
  }
}

const merged = new Map(); // rel path -> { hash, bytes, srcFile, flavors:Set }
const conflicts = [];
const flavorAssets = Object.fromEntries(FLAVORS.map((f) => [f, []]));

for (const snap of snapshots) {
  for (const rel of walk(snap.dir)) {
    // 模板、模板旁车和发布清单均由最终装配重建，不继承上次发布残留。
    if (/^(?:index\.html|release-manifest\.json)(?:\.br)?$/.test(rel)
      || /^flavors\/[^/]+\/index\.html(?:\.br)?$/.test(rel)) continue;
    const top = rel.split('/')[0];
    if (HASHED_DIRS.has(top) && snap.manifest && !snap.manifest.has(rel)) {
      continue; // dx 清单未登记：上次构建的陈旧哈希产物
    }
    const file = join(snap.dir, rel);
    const buf = readFileSync(file);
    const hash = sha256(buf);
    const prev = merged.get(rel);
    if (prev) {
      if (prev.hash !== hash) {
        conflicts.push(`${rel}\n    ${snap.flavor}: ${hash}\n    ${[...prev.flavors][0]}: ${prev.hash}`);
        continue;
      }
      prev.flavors.add(snap.flavor);
    } else {
      merged.set(rel, { hash, bytes: buf.length, srcFile: file, flavors: new Set([snap.flavor]) });
    }
  }
}
if (conflicts.length) {
  console.error(`合并冲突：同路径不同内容（plan §6.4 禁止静默覆盖）：\n  ${conflicts.join('\n  ')}`);
  process.exit(1);
}

// P5：先从每个模板的真实 module 入口解析 WASM；失败时保留上次发布目录。
const templateOf = (flavor) => (flavor === 'reader' ? 'flavors/reader/index.html' : 'index.html');
const templates = new Map();
for (const snap of snapshots) {
  const readAsset = (ref) => {
    const rel = ref.replace(/^\//, '');
    if (!merged.get(rel)?.flavors.has(snap.flavor)) {
      throw new Error(`[${snap.flavor}] 入口不属于本 flavor 的有效资产：${ref}`);
    }
    return readFileSync(join(snap.dir, rel), 'utf8');
  };
  const html = preloadWasm(readFileSync(join(snap.dir, 'index.html'), 'utf8'), readAsset);
  const refs = bootstrapRefs(html);
  for (const ref of refs) {
    const rel = ref.replace(/^\//, '');
    if (!merged.get(rel)?.flavors.has(snap.flavor)) {
      throw new Error(`[${snap.flavor}] 模板引用的入口不属于本 flavor 的有效资产：${ref}`);
    }
  }
  templates.set(snap.flavor, html);
  flavorAssets[snap.flavor] = refs;
}
const sharedEntrypoints = bootstrapConflicts(flavorAssets);
if (sharedEntrypoints.length) throw new Error(sharedEntrypoints.join('\n'));

// 装配最终 public/（plan §6.7「发布包原子地包含服务端、两套资源和清单」）：
// 先写入同分区的临时目录，模板与清单就绪后两段 rename 原子换入——装配
// 中途失败不会破坏既有发布目录，也绝不存在半成品 public 被部署出去。
const outPublic = join(OUT, 'public');
const stagingPublic = join(OUT, `.public.staging-${process.pid}`);
const trashPublic = join(OUT, `.public.trash-${process.pid}`);
rmSync(stagingPublic, { recursive: true, force: true });
rmSync(trashPublic, { recursive: true, force: true });
mkdirSync(stagingPublic, { recursive: true });
for (const [rel, meta] of merged) {
  const dest = join(stagingPublic, rel);
  mkdirSync(join(dest, '..'), { recursive: true });
  copyFileSync(meta.srcFile, dest);
}
for (const snap of snapshots) {
  const dest = join(stagingPublic, templateOf(snap.flavor));
  mkdirSync(join(dest, '..'), { recursive: true });
  writeFileSync(dest, templates.get(snap.flavor));
}

const flavorsManifest = Object.fromEntries(
  snapshots.map((snap) => {
    const assets = [...merged.entries()]
      .filter(([rel, meta]) => meta.flavors.has(snap.flavor) && HASHED_DIRS.has(rel.split('/')[0]))
      .map(([rel, meta]) => ({ path: '/' + rel, sha256: meta.hash, bytes: meta.bytes }))
      .sort((a, b) => a.path.localeCompare(b.path));
    return [
      snap.flavor,
      { template: templateOf(snap.flavor), bootstrap_assets: flavorAssets[snap.flavor], assets },
    ];
  }),
);

const cargoLock = readFileSync(join(ROOT, 'Cargo.lock'));
const manifestOut = {
  release_id: `${sh('git describe --tags --always --dirty') || 'unknown'}`,
  generated_at: new Date().toISOString(),
  source: {
    git_hash: sh('git rev-parse HEAD') || 'unknown',
    dirty: sh('git status --porcelain').length > 0,
    cargo_lock_sha256: sha256(cargoLock),
    cargo_lock_bytes: cargoLock.length,
  },
  flavors: flavorsManifest,
};
writeFileSync(join(stagingPublic, 'release-manifest.json'), JSON.stringify(manifestOut, null, 2) + '\n');

if (existsSync(outPublic)) renameSync(outPublic, trashPublic);
renameSync(stagingPublic, outPublic);
rmSync(trashPublic, { recursive: true, force: true });

const totalBytes = [...merged.values()].reduce((n, m) => n + m.bytes, 0);
console.log(
  `发布合并完成：${merged.size} 个文件（${(totalBytes / 1024).toFixed(1)} KiB）→ ${relative(ROOT, outPublic)}\n` +
    `  full 模板 index.html；reader 模板 flavors/reader/index.html；清单 release-manifest.json (${manifestOut.release_id})`,
);
