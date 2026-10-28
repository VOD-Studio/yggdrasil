#!/usr/bin/env bun
import { execSync } from 'node:child_process';
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
 * 装配时还会按最终内容重命名入口三件套 `yggdrasil.<h>.js`、`yggdrasil.<h>.wasm` 与
 * `__wasm_split.<h>.js`，并在 hash.txt 写入 `entry: content-addressed`，服务端据此给入口
 * immutable 缓存头（原因见 contentAddressEntries）。
 *
 * 用法：
 *   bun scripts/build-release.ts --out target/release-dist --server target/release/yggdrasil
 *   bun scripts/build-release.ts --verify --out target/release-dist
 *     （precompress 之后跑：校验清单摘要、hash.txt 指向的产物、分包清单闭包与 .br 旁车）
 *
 * 选项：--out（默认 target/release-dist）、--site（默认 target/site）、
 *       --hash（默认 target/release/hash.txt）、--server、--verify、--help。
 */
import { createHash } from 'node:crypto';
import {
  chmodSync,
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { join, relative, resolve } from 'node:path';

const ROOT = resolve(import.meta.dir, '..');
const args = process.argv.slice(2);

if (args.includes('--help') || args.includes('-h')) {
  console.log(
    '用法：bun scripts/build-release.ts [--out DIR] [--site DIR] [--hash FILE] [--server FILE] [--verify]\n' +
      '  装配：<site> → <out>/public，<hash> → <out>/hash.txt，可选 <server> → <out>/server，并写 release-manifest.json\n' +
      '  --verify：校验已装配（并已 precompress）的 <out>',
  );
  process.exit(0);
}

const flag = <T extends string | null>(name: string, fallback: T): string | T => {
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

const sha256 = (buf: Buffer) => createHash('sha256').update(buf).digest('hex');
const extOf = (name: string) => name.slice(name.lastIndexOf('.'));

/** 运行时不需要、却会随每个分包重复下载的 wasm 自定义段。 */
const STRIP_CUSTOM_SECTIONS = new Set(['producers', 'target_features']);

/** 读 LEB128 无符号整数，返回 [值, 下一个偏移]。 */
function readUleb(buf: Buffer, offset: number): [number, number] {
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
function stripCustomSections(buf: Buffer) {
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

/** 把 buf 中所有 from 原地替换为等长的 to，返回替换次数。 */
function replaceEqualLength(buf: Buffer, from: string, to: string) {
  const needle = Buffer.from(from);
  const replacement = Buffer.from(to);
  if (needle.length !== replacement.length) throw new Error(`替换前后长度不同：${from} → ${to}`);
  let count = 0;
  for (let at = buf.indexOf(needle); at >= 0; at = buf.indexOf(needle, at + needle.length)) {
    replacement.copy(buf, at);
    count++;
  }
  return count;
}

/** 入口三件套的哈希段（hash.txt 的 js / wasm / split 行）。 */
type EntryHashes = { js: string; wasm: string; split: string };

const entryFiles = (h: EntryHashes) => ({
  js: `yggdrasil.${h.js}.js`,
  wasm: `yggdrasil.${h.wasm}.wasm`,
  split: `__wasm_split.${h.split}.js`,
});

/**
 * 由入口三件套的内容推出新的哈希段；与文件当前叫什么无关，对已重命名的产物再算一次结果不变。
 *
 * 三者互相引用：入口 JS 与入口 wasm 引用加载器文件名，加载器静态导入入口 JS。先把这些引用
 * 换成等长占位符再合并求摘要，三个新名字都由同一摘要派生，任一文件变化则三个名字一起变。
 * 新哈希与 cargo-leptos 一样取 22 位 base64url，wasm 中的加载器名可以原地等长替换。
 */
function entryContentHashes(pkgDir: string, current: EntryHashes): EntryHashes {
  const names = entryFiles(current);
  const blank = (name: string) => '#'.repeat(name.length);
  const js = readFileSync(join(pkgDir, names.js));
  const wasm = readFileSync(join(pkgDir, names.wasm));
  const split = readFileSync(join(pkgDir, names.split));
  const refs = [
    ['入口 JS 引用加载器', replaceEqualLength(js, names.split, blank(names.split))],
    ['入口 wasm 引用加载器', replaceEqualLength(wasm, names.split, blank(names.split))],
    ['加载器导入入口 JS', replaceEqualLength(split, names.js, blank(names.js))],
  ] as const;
  for (const [what, count] of refs) {
    if (count === 0) throw new Error(`${what}的文件名没找到，cargo-leptos 产物结构变了`);
  }
  const digest = createHash('sha256');
  for (const [label, buf] of [
    ['js', js],
    ['wasm', wasm],
    ['split', split],
  ] as const) {
    digest.update(`${label}\0${buf.length}\0`).update(buf);
  }
  const root = digest.digest();
  const derive = (label: string) =>
    createHash('sha256').update(label).update(root).digest('base64url').slice(0, 22);
  return { js: derive('js'), wasm: derive('wasm'), split: derive('split') };
}

/**
 * 按最终内容重命名入口三件套并改写 hash.txt。
 *
 * cargo-leptos 先按内容给入口改名，之后才把加载器文件名写进入口；只改懒加载代码的发布
 * 会出现「入口同名不同内容」，入口因此只能 no-cache。EdgeOne 不缓存 no-cache 资源，也不把
 * 条件请求转给源站，回访每页都要整包重下主 wasm。内容寻址之后入口可以与分包一样 immutable。
 */
function contentAddressEntries(pkgDir: string, hashTxtPath: string) {
  const text = readFileSync(hashTxtPath, 'utf8');
  const parsed = parseHashTxt(text);
  if (!parsed.js || !parsed.wasm || !parsed.split) {
    throw new Error('hash.txt 缺少 js / wasm / split 行，无法给入口内容寻址');
  }
  const current: EntryHashes = { js: parsed.js, wasm: parsed.wasm, split: parsed.split };
  const next = entryContentHashes(pkgDir, current);
  const from = entryFiles(current);
  const to = entryFiles(next);

  const js = readFileSync(join(pkgDir, from.js));
  const wasm = readFileSync(join(pkgDir, from.wasm));
  const split = readFileSync(join(pkgDir, from.split));
  replaceEqualLength(js, from.split, to.split);
  replaceEqualLength(wasm, from.split, to.split);
  replaceEqualLength(split, from.js, to.js);
  for (const key of ['js', 'wasm', 'split'] as const) rmSync(join(pkgDir, from[key]));
  writeFileSync(join(pkgDir, to.js), js);
  writeFileSync(join(pkgDir, to.wasm), wasm);
  writeFileSync(join(pkgDir, to.split), split);

  const lines = text
    .split('\n')
    .filter((line) => line.trim() !== '' && !/^\s*entry\s*:/.test(line))
    .map((line) => {
      const key = line.slice(0, line.indexOf(':')).trim();
      return key === 'js' || key === 'wasm' || key === 'split' ? `${key}: ${next[key]}` : line;
    });
  writeFileSync(hashTxtPath, [...lines, 'entry: content-addressed', ''].join('\n'));
  return { from, to };
}

function walk(dir: string, base = dir, acc: string[] = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, entry.name);
    if (entry.isDirectory()) walk(p, base, acc);
    else acc.push(relative(base, p));
  }
  return acc;
}

function sh(cmd: string) {
  try {
    return execSync(cmd, { cwd: ROOT, encoding: 'utf8' }).trim();
  } catch {
    return '';
  }
}

/** 解析 hash.txt：`key: value` 逐行，未知键忽略。 */
function parseHashTxt(text: string) {
  const out: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const i = line.indexOf(':');
    if (i < 0) continue;
    out[line.slice(0, i).trim()] = line.slice(i + 1).trim();
  }
  return out;
}

const hashFields = (h: Record<string, string>) => ({
  js: h.js ?? null,
  wasm: h.wasm ?? null,
  css: h.css ?? null,
  manifest: h.manifest ?? null,
  split: h.split ?? null,
  entry: h.entry ?? null,
});

if (VERIFY) {
  const publicDir = join(OUT, 'public');
  const problems: string[] = [];
  const fail = (msg: string) => problems.push(msg);
  const pkgDir = join(publicDir, 'pkg');

  let manifest: { assets?: { path: string; bytes: number; sha256: string }[] } | null = null;
  try {
    manifest = JSON.parse(readFileSync(join(publicDir, 'release-manifest.json'), 'utf8'));
  } catch (error) {
    fail(
      `无法读取 ${join(publicDir, 'release-manifest.json')}：${error instanceof Error ? error.message : String(error)}`,
    );
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
  let hashes: Record<string, string> = {};
  try {
    hashes = parseHashTxt(readFileSync(join(OUT, 'hash.txt'), 'utf8'));
  } catch (error) {
    fail(
      `无法读取 ${join(OUT, 'hash.txt')}：${error instanceof Error ? error.message : String(error)}`,
    );
  }
  const need = (rel: string, why: string) => {
    if (!existsSync(join(pkgDir, rel))) fail(`${why}缺失：pkg/${rel}`);
  };
  if (!hashes.js) fail('hash.txt 缺少 js 行');
  else need(`yggdrasil.${hashes.js}.js`, 'hash.txt js 行指向的产物');
  if (!hashes.wasm) fail('hash.txt 缺少 wasm 行');
  else need(`yggdrasil.${hashes.wasm}.wasm`, 'hash.txt wasm 行指向的产物');
  if (hashes.css) need(`yggdrasil.${hashes.css}.css`, 'hash.txt css 行指向的产物');
  if (hashes.split) need(`__wasm_split.${hashes.split}.js`, 'hash.txt split 行指向的产物');
  if (hashes.entry !== 'content-addressed') {
    fail('hash.txt 缺少 entry: content-addressed（入口未按内容重命名）');
  } else if (hashes.js && hashes.wasm && hashes.split) {
    const current = { js: hashes.js, wasm: hashes.wasm, split: hashes.split };
    try {
      const expected = entryContentHashes(pkgDir, current);
      if (JSON.stringify(expected) !== JSON.stringify(current)) {
        fail(`入口文件名与内容不符：期望 ${JSON.stringify(expected)}`);
      }
    } catch (error) {
      fail(`入口内容寻址校验失败：${error instanceof Error ? error.message : String(error)}`);
    }
  }
  if (hashes.manifest) {
    const rel = `__wasm_split_manifest.${hashes.manifest}.json`;
    need(rel, 'hash.txt manifest 行指向的产物');
    try {
      // 值是不含扩展名的分包文件名，实际文件为 `<name>.wasm`。
      const split: Record<string, string[]> = JSON.parse(readFileSync(join(pkgDir, rel), 'utf8'));
      for (const [module, chunks] of Object.entries(split)) {
        for (const chunk of chunks) {
          if (!existsSync(join(pkgDir, `${chunk}.wasm`))) {
            fail(`分包清单 ${module} 引用的文件缺失：pkg/${chunk}.wasm`);
          }
        }
      }
    } catch (error) {
      if (existsSync(join(pkgDir, rel)))
        fail(
          `分包清单无法解析：pkg/${rel}：${error instanceof Error ? error.message : String(error)}`,
        );
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
const isDir = (p: string) => statSync(p, { throwIfNoEntry: false })?.isDirectory();
const isFile = (p: string) => statSync(p, { throwIfNoEntry: false })?.isFile();

if (!isDir(SITE)) {
  console.error(`缺少站点目录：${SITE}\n先运行 make build-frontend`);
  process.exit(1);
}
if (!isDir(join(SITE, 'pkg'))) {
  console.error(`站点目录缺少 pkg/：${SITE}\n先运行 make build-frontend`);
  process.exit(1);
}
if (existsSync(join(SITE, 'index.html'))) {
  console.error(
    `${join(SITE, 'index.html')} 存在：疑似混入旧 dx 产物，拒绝发布（清理 ${SITE} 后重新构建）`,
  );
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
  const entries = contentAddressEntries(pkgDir, join(staging, 'hash.txt'));
  const assets = walk(pkgDir)
    .filter((rel) => !rel.endsWith('.br'))
    .map((rel) => {
      const buf = readFileSync(join(pkgDir, rel));
      return { path: `/pkg/${rel}`, sha256: sha256(buf), bytes: buf.length };
    })
    .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));

  const hashFile = parseHashTxt(readFileSync(join(staging, 'hash.txt'), 'utf8'));
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
  writeFileSync(
    join(stagingPublic, 'release-manifest.json'),
    JSON.stringify(manifestOut, null, 2) + '\n',
  );

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
      `  入口内容寻址：${entries.to.js}、${entries.to.wasm}、${entries.to.split}\n` +
      `  hash.txt${SERVER ? '、server' : ''} 已就位；清单 release-manifest.json (${manifestOut.release_id})`,
  );
} finally {
  rmSync(staging, { recursive: true, force: true });
  rmSync(trash, { recursive: true, force: true });
}
