#!/usr/bin/env bun
// Reproduce dx's LLVM post-processing with the project's WASM release strip setting.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const repo = path.join(__dirname, '..');
const manifest = fs.readFileSync(path.join(repo, 'Cargo.toml'), 'utf8');
const lock = fs.readFileSync(path.join(repo, 'Cargo.lock'), 'utf8');
const version = lock.match(/\[\[package\]\]\s+name = "wasm-bindgen"\s+version = "([^"]+)"/)[1];
function stripSetting(name) {
  const section = manifest.split(`[profile.${name}]`)[1]?.split(/^\[/m)[0] || '';
  const strip = section.match(/^strip\s*=\s*(false|true|"[^"]+")/m);
  if (strip) return JSON.parse(strip[1]);
  const parent = section.match(/^inherits\s*=\s*"([^"]+)"/m);
  return parent ? stripSetting(parent[1]) : false;
}
function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: 'utf8', ...options });
  assert.equal(result.status, 0, result.error?.message || result.stderr || result.stdout);
  return result.stdout.trim();
}
const cli = process.env.WASM_BINDGEN_CLI || path.join(
  process.env.DX_HOME || path.join(os.homedir(), '.local/share/.dx'),
  `tools/wasm-bindgen-${version}/wasm-bindgen`,
);
assert.equal(run(cli, ['--version']), `wasm-bindgen ${version}`, 'Run make wasm-bindgen-cache first');
const sysroot = run('rustc', ['--print', 'sysroot']);
const host = run('rustc', ['-vV']).match(/^host: (.+)$/m)[1];
const objcopy = path.join(sysroot, 'lib/rustlib', host, 'bin/llvm-objcopy');
const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'yggdrasil-wasm-metadata-test-'));
try {
  fs.mkdirSync(path.join(temp, 'src'));
  fs.writeFileSync(path.join(temp, 'Cargo.toml'), `[package]
name = "wasm_metadata_probe"
version = "0.0.0"
edition = "2021"
[lib]
crate-type = ["cdylib"]
[dependencies]
wasm-bindgen = "=${version}"
[profile.release]
strip = false
debug = false
opt-level = "z"
`);
  fs.writeFileSync(path.join(temp, 'src/lib.rs'),
    'use wasm_bindgen::prelude::*;\n#[wasm_bindgen]\npub fn answer() -> u32 { 42 }\n');
  run('cargo', ['build', '--offline', '--manifest-path', path.join(temp, 'Cargo.toml'),
    '--release', '--target', 'wasm32-unknown-unknown'], {
    env: { ...process.env, CARGO_TARGET_DIR: path.join(temp, 'target'), RUSTC_WRAPPER: '' },
  });
  const wasm = path.join(temp, 'target/wasm32-unknown-unknown/release/wasm_metadata_probe.wasm');
  const strip = stripSetting('wasm-release');
  const flag = strip === 'debuginfo' ? '--strip-debug'
    : strip === true || strip === 'symbols' ? '--strip-all' : null;
  if (flag) run(objcopy, [flag, wasm, wasm]);
  run(cli, [wasm, '--target', 'web', '--out-dir', path.join(temp, 'bindings')]);
  assert.ok(fs.existsSync(path.join(temp, 'bindings/wasm_metadata_probe_bg.wasm')));
  console.log('PASS: WASM release post-processing preserves wasm-bindgen metadata');
} finally {
  fs.rmSync(temp, { recursive: true, force: true });
}
