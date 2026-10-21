import { afterEach, describe, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { brotliCompressSync } from 'node:zlib';
import { bootstrapRefs, preloadWasm, verifyWasmPreload, wasmBootstrap } from './release-bootstrap.cjs';

const template = (flavor) => `<!DOCTYPE html><html><head>
<link rel="stylesheet" href="/style.css">
<script src="/yggdrasil-core/yggdrasil-core.js"></script>
<link rel="preload" as="script" href="/./assets/${flavor}.js" crossorigin>
</head><body><div id="main"></div>
<script type="module" async src="/./assets/${flavor}.js"></script></body></html>`;
const entry = (flavor) => `init({module_or_path:"/./assets/${flavor}_bg.wasm"});`;
const readEntry = () => entry('reader');
const dirs = [];
afterEach(() => { for (const dir of dirs.splice(0)) rmSync(dir, { recursive: true, force: true }); });

describe('release WASM bootstrap', () => {
  test('follows the module entry and places its normalized WASM preload before blocking resources', () => {
    const html = preloadWasm(template('reader'), readEntry);
    expect(wasmBootstrap(html, readEntry)).toEqual({ entry: '/assets/reader.js', wasm: '/assets/reader_bg.wasm' });
    expect(bootstrapRefs(html)).toEqual(['/assets/reader.js', '/assets/reader_bg.wasm']);
    expect(html.indexOf('as="fetch"')).toBeLessThan(html.indexOf('rel="stylesheet"'));
    expect(() => verifyWasmPreload(html, '/assets/reader_bg.wasm')).not.toThrow();
    expect(preloadWasm(html, readEntry)).toBe(html);
  });

  test('rejects missing or ambiguous bootstrap instead of picking an arbitrary directory asset', () => {
    expect(() => preloadWasm(template('reader'), () => 'init();')).toThrow('实际为 0 个');
    expect(() => preloadWasm(template('reader'), () => entry('reader') + entry('full'))).toThrow('实际为 2 个');
    expect(() => preloadWasm(template('reader').replace('type="module"', ''), readEntry)).toThrow('module 入口');
    expect(() => preloadWasm(template('reader').replace('</body>', '<script type="module" src="/assets/other.js"></script></body>'), readEntry)).toThrow('module 入口');
  });

  test('rejects nonlocal or escaped WASM paths and incomplete heads', () => {
    for (const ref of ['https://cdn.invalid/main.wasm', '/assets/../../secret.wasm', '/assets/a.wasm?v=1']) {
      expect(() => preloadWasm(template('reader'), () => `init({module_or_path:"${ref}"});`)).toThrow();
    }
    expect(() => preloadWasm(template('reader').replace('</head>', ''), readEntry)).toThrow('head');
  });

  test('supports single quoted, spaced entry properties and unrelated fetch preloads', () => {
    const html = template('reader').replace('</head>', '<link rel="preload" as="fetch" href="/data.json"></head>');
    expect(() => preloadWasm(html, () => "init({ module_or_path: '/assets/reader_bg.wasm' });")).not.toThrow();
  });

  test('rejects duplicate, misplaced, wrong-flavor and mismatched credential preloads', () => {
    const html = preloadWasm(template('reader'), readEntry);
    const link = html.match(/<link[^>]*as="fetch"[^>]*>/)[0];
    for (const broken of [
      html.replace('</head>', link + '</head>'),
      html.replace(link, '').replace('</body>', link + '</body>'),
      html.replace('href="/assets/reader_bg.wasm"', 'href="/assets/full_bg.wasm"'),
      html.replace('crossorigin="anonymous"', 'crossorigin="use-credentials"'),
      html.replace('crossorigin="anonymous"', ''),
      html.replace('as="fetch"', 'as="script"'),
    ]) expect(() => verifyWasmPreload(broken, '/assets/reader_bg.wasm')).toThrow();
  });
});

function fixture() {
  const dir = mkdtempSync(join(tmpdir(), 'ygg-release-preload-'));
  dirs.push(dir);
  const assembly = join(dir, 'assembly');
  const out = join(dir, 'out');
  for (const flavor of ['full', 'reader']) {
    const snap = join(assembly, flavor);
    const assets = join(snap, 'public', 'assets');
    mkdirSync(assets, { recursive: true });
    writeFileSync(join(snap, 'public', 'index.html'), template(flavor));
    writeFileSync(join(snap, 'public', 'index.html.br'), brotliCompressSync(template(flavor)));
    writeFileSync(join(snap, 'public', 'release-manifest.json'), JSON.stringify({ stale: flavor }));
    writeFileSync(join(snap, 'public', 'release-manifest.json.br'), brotliCompressSync(flavor));
    mkdirSync(join(snap, 'public', 'flavors', 'reader'), { recursive: true });
    writeFileSync(join(snap, 'public', 'flavors', 'reader', 'index.html'), `stale ${flavor}`);
    writeFileSync(join(snap, 'public', 'flavors', 'reader', 'index.html.br'), brotliCompressSync(flavor));
    const files = { [`${flavor}.js`]: entry(flavor), [`${flavor}_bg.wasm`]: 'fixture wasm' };
    for (const [name, data] of Object.entries(files)) {
      writeFileSync(join(assets, name), data);
      writeFileSync(join(assets, name + '.br'), brotliCompressSync(data));
    }
    writeFileSync(join(assets, 'stale_bg.wasm'), 'old incremental output');
    writeFileSync(join(snap, '.manifest.json'), JSON.stringify({ assets: {
      current: Object.keys(files).flatMap((name) => [name, name + '.br']).map((name) => ({ bundled_path: name, options: { add_hash: true } })),
    } }));
  }
  const run = (...args) => spawnSync(process.execPath, [join(import.meta.dir, 'build-release.cjs'), '--assembly', assembly, '--out', out, ...args], { encoding: 'utf8' });
  return { assembly, out, run };
}

describe('dual-flavor release assembly', () => {
  test('records preloads from final templates and ignores stale incremental WASM files', () => {
    const { out, run } = fixture();
    const result = run();
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    const manifest = JSON.parse(readFileSync(join(out, 'public', 'release-manifest.json'), 'utf8'));
    for (const flavor of ['full', 'reader']) {
      const info = manifest.flavors[flavor];
      expect(info.bootstrap_assets).toEqual([`/assets/${flavor}.js`, `/assets/${flavor}_bg.wasm`]);
      const html = readFileSync(join(out, 'public', info.template), 'utf8');
      expect(() => verifyWasmPreload(html, `/assets/${flavor}_bg.wasm`)).not.toThrow();
      expect(info.assets.some((asset) => asset.path.includes('stale'))).toBe(false);
    }
    expect(run('--verify').status).toBe(0);
  });

  test('fails before overwriting an existing release if the referenced WASM is excluded by dx manifest', () => {
    const { assembly, out, run } = fixture();
    expect(run().status).toBe(0);
    const index = join(out, 'public', 'index.html');
    const before = readFileSync(index, 'utf8');
    const sentinel = join(out, 'public', 'previous-release.txt');
    writeFileSync(sentinel, 'must survive a failed assembly');
    const file = join(assembly, 'reader', '.manifest.json');
    const manifest = JSON.parse(readFileSync(file, 'utf8'));
    manifest.assets.current = manifest.assets.current.filter((asset) => !asset.bundled_path.endsWith('.wasm'));
    writeFileSync(file, JSON.stringify(manifest));
    expect(run().status).not.toBe(0);
    expect(readFileSync(index, 'utf8')).toBe(before);
    expect(readFileSync(sentinel, 'utf8')).toBe('must survive a failed assembly');
  });

  test('rejects shared entrypoints before replacing the previous release', () => {
    const { assembly, out, run } = fixture();
    expect(run().status).toBe(0);
    const sentinel = join(out, 'public', 'previous-release.txt');
    writeFileSync(sentinel, 'previous release');
    const full = join(assembly, 'full');
    const reader = join(assembly, 'reader');
    writeFileSync(join(reader, 'public', 'index.html'), template('full') + '\n');
    writeFileSync(join(reader, '.manifest.json'), readFileSync(join(full, '.manifest.json')));
    for (const name of ['full.js', 'full.js.br', 'full_bg.wasm', 'full_bg.wasm.br']) {
      writeFileSync(join(reader, 'public', 'assets', name), readFileSync(join(full, 'public', 'assets', name)));
    }
    const result = run();
    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain('同一入口');
    expect(readFileSync(sentinel, 'utf8')).toBe('previous release');
  });

  test('verification rejects asset content, digest and byte-size mismatches', () => {
    const { out, run } = fixture();
    expect(run().status).toBe(0);
    const manifestFile = join(out, 'public', 'release-manifest.json');
    const originalManifest = readFileSync(manifestFile, 'utf8');
    for (const field of ['sha256', 'bytes']) {
      const manifest = JSON.parse(originalManifest);
      manifest.flavors.full.assets[0][field] = field === 'bytes' ? 0 : 'bad digest';
      writeFileSync(manifestFile, JSON.stringify(manifest));
      expect(run('--verify').status).not.toBe(0);
    }
    writeFileSync(manifestFile, originalManifest);
    writeFileSync(join(out, 'public', 'assets', 'full_bg.wasm'), 'changed content');
    expect(run('--verify').status).not.toBe(0);
  });

  test('verification rejects a modified template, stale manifest or wrong-flavor WASM bootstrap', () => {
    const { out, run } = fixture();
    expect(run().status).toBe(0);
    const index = join(out, 'public', 'index.html');
    const original = readFileSync(index, 'utf8');
    writeFileSync(index, original.replace('crossorigin="anonymous"', 'crossorigin="use-credentials"'));
    expect(run('--verify').status).not.toBe(0);
    writeFileSync(index, original);
    const js = join(out, 'public', 'assets', 'full.js');
    writeFileSync(js, entry('reader'));
    expect(run('--verify').status).not.toBe(0);
    writeFileSync(js, entry('full'));
    const manifestFile = join(out, 'public', 'release-manifest.json');
    const manifest = JSON.parse(readFileSync(manifestFile, 'utf8'));
    manifest.flavors.full.bootstrap_assets.pop();
    writeFileSync(manifestFile, JSON.stringify(manifest));
    expect(run('--verify').status).not.toBe(0);
  });
});
