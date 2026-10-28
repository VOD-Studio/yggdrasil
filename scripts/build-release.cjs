#!/usr/bin/env bun
/**
 * cargo-leptos 发布产物装配与校验。
 *
 * 输入（cargo leptos build --release --split 的产物）：
 *   <site>/         —— 站点根（默认 target/site）：public/ 静态资源 + pkg/ 分包前端
 *   <hash>          —— hash.txt（默认 target/release/hash.txt）：js/wasm/css/manifest/split 行
 *   <server>        —— 服务端二进制（可选）
 * 输出：
 *   <out>/public/                  —— 站点根副本 + release-manifest.json
 *   <out>/hash.txt                 —— 服务端启动时从可执行文件同目录读取，必须与 server 同放
 *   <out>/server                   —— 服务端二进制（给出 --server 时）
 *
 * 装配先写入 <out>/.staging-<pid>，完成后逐项 rename 换入 <out>；装配中途失败
 * 不会破坏既有发布目录。<site> 顶层若存在 index.html，说明混入了旧 dx 产物，直接失败。
 *
 * 装配时剥掉 pkg/ 下每个 .wasm 的 `producers` / `target_features` 自定义段（运行时不读）。
 * --split 产出上千个小分包，每个都带这两段约 180 B，一条路由要拉几百个分包，首屏 Brotli
 * 因此多出约 40 KB。不能交给 wasm-opt 的 --strip-* 做：只含这两段的空分包会被剥成 8 字节，
 * cargo-leptos 0.3.11 随后改写分包加载器引用时对短于文件名的 .wasm 下标越界而崩溃。
 * 剥离是确定性的，cargo-leptos 给出的内容哈希文件名仍与内容一一对应。
 *
 * 用法：
 *   bun scripts/build-release.cjs --out target/release-dist --server target/release/yggdrasil
 *   bun scripts/build-release.cjs --verify --out target/release-dist
 *     （precompress 之后跑：校验清单摘要、hash.txt 指向的产物、分包清单闭包与 .br 旁车）
 *
 * 选项：--out（默认 target/release-dist）、--site（默认 target/site）、
 *       --hash（默认 target/release/hash.txt）、--server、--verify、--help。
 */
import { createHash } from 'node:crypto';
import { execSync } from 'node:child_process';
import {
  chmodSync, copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync,
  renameSync, rmSync, statSync, writeFileSync,
} from 'node:fs';
import { join, relative, resolve } from 'node:path';

const ROOT = resolve(import.meta.dir, '..');
const args = process.argv.slice(2);

if (args.includes('--help') || args.includes('-h')) {
  console.log(
    '用法：bun scripts/build-release.cjs [--out DIR] [--site DIR] [--hash FILE] [--server FILE] [--verify]\n' +
      '  装配：<site> → <out>/public，<hash> → <out>/hash.txt，可选 <server> → <out>/server，并写 release-manifest.json\n' +
      '  --verify：校验已装配（并已 precompress）的 <out>',
  );
  process.exit(0);
}

const flag = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] && !args[i + 1].startsWith('--') ? args[i + 1] : fallback;
};

const OUT = resolve(ROOT, flag('--out', 'target/release-dist'));
const SITE = resolve(ROOT, flag('--site', 'target/site'));
const HASH = resolve(ROOT, flag('--hash', 'target/release/hash.txt'));
const SERVER = flag('--server', null);
const VERIFY = args.includes('--verify');

// 与 Makefile precompress 对 pkg/ 的覆盖保持一致：只有这四种带 .br 旁车（.d.ts 等不压）。
const BR_EXT = new Set(['.js', '.wasm', '.json', '.css']);

const sha256 = (buf) => createHash('sha256').update(buf).digest('hex');
const extOf = (name) => name.slice(name.lastIndexOf('.'));

/** 运行时不需要、却会随每个分包重复下载的 wasm 自定义段。 */
const STRIP_CUSTOM_SECTIONS = new Set(['producers', 'target_features']);

/** 读 LEB128 无符号整数，返回 [值, 下一个偏移]。 */
function readUleb(buf, offset) {
  let value = 0;
  let shift = 0;
  for (;;) {
    const byte = buf[offset++];
    value += (byte & 0x7f) * 2 ** shift;
    if (byte < 0x80) return [value, offset];
    shift += 7;
  }
}

/** 去掉 STRIP_CUSTOM_SECTIONS 中的自定义段；其余段原样保留、顺序不变。 */
function stripCustomSections(buf) {
  if (buf.length < 8 || buf.readUInt32LE(0) !== 0x6d736100) throw new Error('不是 wasm 模块');
  const kept = [buf.subarray(0, 8)];
  let offset = 8;
  while (offset < buf.length) {
    const start = offset;
    const id = buf[offset];
    const [size, contentStart] = readUleb(buf, offset + 1);
    const end = contentStart + size;
    if (end > buf.length) throw new Error('wasm 段长度越界');
    let drop = false;
    if (id === 0) {
      const [nameLen, nameStart] = readUleb(buf, contentStart);
      drop = STRIP_CUSTOM_SECTIONS.has(buf.toString('utf8', nameStart, nameStart + nameLen));
    }
    if (!drop) kept.push(buf.subarray(start, end));
    offset = end;
  }
  return Buffer.concat(kept);
}

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

/** 解析 hash.txt：`key: value` 逐行，未知键忽略。 */
function parseHashTxt(text) {
  const out = {};
  for (const line of text.split('\n')) {
    const i = line.indexOf(':');
    if (i < 0) continue;
    out[line.slice(0, i).trim()] = line.slice(i + 1).trim();
  }
  return out;
}

const hashFields = (h) => ({
  js: h.js ?? null,
  wasm: h.wasm ?? null,
  css: h.css ?? null,
  manifest: h.manifest ?? null,
  split: h.split ?? null,
});

if (VERIFY) {
  const publicDir = join(OUT, 'public');
  const problems = [];
  const fail = (msg) => problems.push(msg);
  const pkgDir = join(publicDir, 'pkg');

  let manifest = null;
  try {
    manifest = JSON.parse(readFileSync(join(publicDir, 'release-manifest.json'), 'utf8'));
  } catch (error) {
    fail(`无法读取 ${join(publicDir, 'release-manifest.json')}：${error.message}`);
  }

  if (manifest) {
    for (const asset of manifest.assets ?? []) {
      const file = join(publicDir, asset.path);
      if (!existsSync(file)) {
        fail(`清单资产缺失：${asset.path}`);
        continue;
      }
      const content = readFileSync(file);
      if (content.length !== asset.bytes || sha256(content) !== asset.sha256) {
        fail(`清单资产摘要或大小不匹配：${asset.path}`);
      }
    }
  }

  // hash.txt（以 <out>/hash.txt 为准）指向的产物。
  let hashes = {};
  try {
    hashes = parseHashTxt(readFileSync(join(OUT, 'hash.txt'), 'utf8'));
  } catch (error) {
    fail(`无法读取 ${join(OUT, 'hash.txt')}：${error.message}`);
  }
  const need = (rel, why) => {
    if (!existsSync(join(pkgDir, rel))) fail(`${why}缺失：pkg/${rel}`);
  };
  if (!hashes.js) fail('hash.txt 缺少 js 行');
  else need(`yggdrasil.${hashes.js}.js`, 'hash.txt js 行指向的产物');
  if (!hashes.wasm) fail('hash.txt 缺少 wasm 行');
  else need(`yggdrasil.${hashes.wasm}.wasm`, 'hash.txt wasm 行指向的产物');
  if (hashes.css) need(`yggdrasil.${hashes.css}.css`, 'hash.txt css 行指向的产物');
  if (hashes.split) need(`__wasm_split.${hashes.split}.js`, 'hash.txt split 行指向的产物');
  if (hashes.manifest) {
    const rel = `__wasm_split_manifest.${hashes.manifest}.json`;
    need(rel, 'hash.txt manifest 行指向的产物');
    try {
      // 值是不含扩展名的分包文件名，实际文件为 `<name>.wasm`。
      const split = JSON.parse(readFileSync(join(pkgDir, rel), 'utf8'));
      for (const [module, chunks] of Object.entries(split)) {
        for (const chunk of chunks) {
          if (!existsSync(join(pkgDir, `${chunk}.wasm`))) {
            fail(`分包清单 ${module} 引用的文件缺失：pkg/${chunk}.wasm`);
          }
        }
      }
    } catch (error) {
      if (existsSync(join(pkgDir, rel))) fail(`分包清单无法解析：pkg/${rel}：${error.message}`);
    }
  }

  // pkg/ 下每个 js/wasm/json/css 都要有 .br 旁车（先跑 precompress）。
  if (existsSync(pkgDir)) {
    const files = new Set(readdirSync(pkgDir));
    for (const name of files) {
      if (name.endsWith('.br') || !BR_EXT.has(extOf(name))) continue;
      if (!files.has(`${name}.br`)) fail(`缺 .br 旁车（先跑 precompress）：pkg/${name}`);
    }
  } else {
    fail(`缺少 ${pkgDir}`);
  }

  if (problems.length) {
    console.error(`\n发布校验失败（${problems.length} 项）：`);
    for (const p of problems) console.error(`  - ${p}`);
    process.exit(1);
  }
  console.log(`发布校验通过：清单摘要、hash.txt 产物、分包清单闭包与 .br 旁车完整（${OUT}）`);
  process.exit(0);
}

// ── 装配模式 ─────────────────────────────────────────────────────
const isDir = (p) => statSync(p, { throwIfNoEntry: false })?.isDirectory();
const isFile = (p) => statSync(p, { throwIfNoEntry: false })?.isFile();

if (!isDir(SITE)) {
  console.error(`缺少站点目录：${SITE}\n先运行 make build-frontend`);
  process.exit(1);
}
if (!isDir(join(SITE, 'pkg'))) {
  console.error(`站点目录缺少 pkg/：${SITE}\n先运行 make build-frontend`);
  process.exit(1);
}
if (existsSync(join(SITE, 'index.html'))) {
  console.error(`${join(SITE, 'index.html')} 存在：疑似混入旧 dx 产物，拒绝发布（清理 ${SITE} 后重新构建）`);
  process.exit(1);
}
if (!isFile(HASH)) {
  console.error(`缺少 hash.txt：${HASH}\n先运行 make build-frontend`);
  process.exit(1);
}
if (SERVER && !isFile(resolve(ROOT, SERVER))) {
  console.error(`缺少服务端二进制：${resolve(ROOT, SERVER)}`);
  process.exit(1);
}

mkdirSync(OUT, { recursive: true });
const staging = join(OUT, `.staging-${process.pid}`);
const trash = join(OUT, `.trash-${process.pid}`);
rmSync(staging, { recursive: true, force: true });
rmSync(trash, { recursive: true, force: true });
mkdirSync(staging, { recursive: true });

try {
  const stagingPublic = join(staging, 'public');
  cpSync(SITE, stagingPublic, { recursive: true });
  copyFileSync(HASH, join(staging, 'hash.txt'));
  if (SERVER) {
    copyFileSync(resolve(ROOT, SERVER), join(staging, 'server'));
    chmodSync(join(staging, 'server'), 0o755);
  }

  const pkgDir = join(stagingPublic, 'pkg');
  let strippedBytes = 0;
  for (const rel of walk(pkgDir).filter((r) => r.endsWith('.wasm'))) {
    const file = join(pkgDir, rel);
    const buf = readFileSync(file);
    const out = stripCustomSections(buf);
    if (out.length !== buf.length) {
      writeFileSync(file, out);
      strippedBytes += buf.length - out.length;
    }
  }
  const assets = walk(pkgDir)
    .filter((rel) => !rel.endsWith('.br'))
    .map((rel) => {
      const buf = readFileSync(join(pkgDir, rel));
      return { path: `/pkg/${rel}`, sha256: sha256(buf), bytes: buf.length };
    })
    .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));

  const hashFile = parseHashTxt(readFileSync(HASH, 'utf8'));
  const cargoLock = readFileSync(join(ROOT, 'Cargo.lock'));
  const manifestOut = {
    release_id: sh('git describe --tags --always --dirty') || 'unknown',
    generated_at: new Date().toISOString(),
    source: {
      git_hash: sh('git rev-parse HEAD') || 'unknown',
      dirty: sh('git status --porcelain').length > 0,
      cargo_lock_sha256: sha256(cargoLock),
      cargo_lock_bytes: cargoLock.length,
    },
    hash_txt: hashFields(hashFile),
    assets,
  };
  writeFileSync(join(stagingPublic, 'release-manifest.json'), JSON.stringify(manifestOut, null, 2) + '\n');

  // 逐项原子换入：旧项先移入 trash，新项 rename 就位，最后清理。
  mkdirSync(trash, { recursive: true });
  for (const name of ['public', 'hash.txt', 'server']) {
    const next = join(staging, name);
    if (!existsSync(next)) continue;
    const current = join(OUT, name);
    if (existsSync(current)) renameSync(current, join(trash, name));
    renameSync(next, current);
  }
  console.log(
    `发布装配完成：${assets.length} 个 pkg 资产 → ${relative(ROOT, join(OUT, 'public'))}` +
      `（剥离 wasm 自定义段 ${strippedBytes} B）\n` +
      `  hash.txt${SERVER ? '、server' : ''} 已就位；清单 release-manifest.json (${manifestOut.release_id})`,
  );
} finally {
  rmSync(staging, { recursive: true, force: true });
  rmSync(trash, { recursive: true, force: true });
}
