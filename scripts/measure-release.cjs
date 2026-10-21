#!/usr/bin/env bun
// P0 发布产物测量：只读扫描一次 dx release 客户端产物目录，输出 JSON 与人工摘要。
// 每个 artifact 记录原始字节、Brotli 旁车字节与 sha256，并附构建环境快照
// （git 提交、未提交 diff 摘要、Cargo.lock 摘要、工具链版本），保证结果可归属。
// 用法：
//   bun scripts/measure-release.cjs --dir target-p0/dx/yggdrasil/release/web/public
// 报告写入 target/regressions/<时间戳>-release-measure/（report.json + report.txt）。
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { createHash } = require('node:crypto');

const root = path.resolve(__dirname, '..');

// ── 参数 ─────────────────────────────────────────────────────────
function argDir() {
  const i = process.argv.indexOf('--dir');
  if (i >= 0) {
    assert(process.argv[i + 1], '--dir 需要一个产物目录参数');
    return path.resolve(root, process.argv[i + 1]);
  }
  // 默认测量正式 release 布局；不存在时报错，绝不静默测量过期产物。
  const fallback = path.join(root, 'target/dx/yggdrasil/release/web/public');
  assert(fs.existsSync(fallback), `未找到产物目录：请用 --dir 指定，或先完成 release 构建（${fallback}）`);
  return fallback;
}
const dir = argDir();
assert(fs.existsSync(path.join(dir, 'assets')), `${dir} 下没有 assets/，不像 dx 客户端产物目录`);

// ── 工具 ─────────────────────────────────────────────────────────
function sh(cmd, args) {
  try {
    return execFileSync(cmd, args, { cwd: root, encoding: 'utf8' }).trim();
  } catch {
    return null; // 工具缺失只记录 null，不阻断测量
  }
}
const sha256 = (file) => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const bytes = (file) => fs.statSync(file).size;

// ── 环境快照 ─────────────────────────────────────────────────────
const nightly = (sh('grep', ['-s', '^WASM_NIGHTLY', 'Makefile']) || '').split(':=')[1]?.trim() || 'nightly-2026-10-19';
function gitDiffSummary() {
  try {
    const diff = execFileSync('git', ['diff'], { cwd: root, encoding: 'utf8', maxBuffer: 1 << 26 });
    return {
      dirtyFiles: diff ? diff.split(/^diff --git /m).length - 1 : 0,
      diffSha256: diff ? sha256ish(diff) : null,
    };
  } catch {
    return { dirtyFiles: null, diffSha256: null };
  }
}
function sha256ish(text) {
  return createHash('sha256').update(text).digest('hex').slice(0, 16);
}

const meta = {
  measuredAt: new Date().toISOString(),
  buildDir: path.relative(root, dir),
  git: {
    commit: sh('git', ['rev-parse', 'HEAD']),
    subject: sh('git', ['log', '-1', '--format=%s']),
    ...gitDiffSummary(),
  },
  lockfile: (() => {
    const p = path.join(root, 'Cargo.lock');
    return fs.existsSync(p) ? { sha256: sha256(p), bytes: bytes(p) } : null;
  })(),
  toolchain: {
    rustcStable: sh('rustc', ['--version']),
    rustcNightly: sh('rustup', ['run', nightly, 'rustc', '--version']),
    wasmNightly: nightly,
    dx: sh('dx', ['--version']),
    wasmOpt: sh('wasm-opt', ['--version']),
    brotli: sh('brotli', ['--version']),
    bun: sh('bun', ['--version']),
  },
};

// ── 产物扫描：assets/ 下 dx 生成的 wasm + 胶水 JS，public/ 下的库 bundle ──
function scan(sub, filter) {
  const abs = path.join(dir, sub);
  if (!fs.existsSync(abs)) return [];
  return fs.readdirSync(abs)
    .filter(filter)
    .sort()
    .map((name) => {
      const file = path.join(abs, name);
      const brFile = file + '.br';
      return {
        path: `${sub}/${name}`,
        bytes: bytes(file),
        sha256: sha256(file),
        brotliBytes: fs.existsSync(brFile) ? bytes(brFile) : null,
        brotliSha256: fs.existsSync(brFile) ? sha256(brFile) : null,
      };
    });
}

const report = {
  meta,
  // dx 生成物：*_bg-*.wasm（wasm-bindgen 后 + wasm-opt -Oz）与同名入口胶水 JS
  clientAssets: scan('assets', (n) => /\.(wasm|js)$/.test(n)),
  // 仓库自带 bundle（yggdrasil-core / codemirror / tiptap / xterm / lightbox）
  publicBundles: scan('.', (n) => n.endsWith('.js')),
};

// ── 输出 ─────────────────────────────────────────────────────────
const outDir = path.join(root, 'target/regressions', `${Date.now()}-release-measure`);
fs.mkdirSync(outDir, { recursive: true });
fs.writeFileSync(path.join(outDir, 'report.json'), JSON.stringify(report, null, 2));

const kib = (n) => (n == null ? '—' : (n / 1024).toFixed(1) + ' KiB');
let text = `# Release 产物测量 — ${meta.measuredAt}\n`;
text += `构建目录: ${meta.buildDir}\n`;
text += `git: ${meta.git.commit?.slice(0, 12) || '?'} ${meta.git.subject || ''}（未提交文件改动: ${meta.git.dirtyFiles ?? '?'}）\n`;
text += `rustc: ${meta.toolchain.rustcStable} | nightly: ${meta.toolchain.rustcNightly}\n`;
text += `dx: ${meta.toolchain.dx} | wasm-opt: ${meta.toolchain.wasmOpt} | brotli: ${meta.toolchain.brotli}\n\n`;
text += `| 产物 | 原始 | Brotli | sha256(前 12) |\n| --- | ---: | ---: | --- |\n`;
for (const a of report.clientAssets) {
  text += `| ${a.path} | ${kib(a.bytes)} | ${kib(a.brotliBytes)} | ${a.sha256.slice(0, 12)} |\n`;
}
for (const a of report.publicBundles) {
  text += `| ${a.path} | ${kib(a.bytes)} | ${kib(a.brotliBytes)} | ${a.sha256.slice(0, 12)} |\n`;
}
fs.writeFileSync(path.join(outDir, 'report.txt'), text);
console.log(text);
console.log(`报告已写入 ${path.relative(root, outDir)}`);
