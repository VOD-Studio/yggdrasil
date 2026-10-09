#!/usr/bin/env bun
// Exercise the real Make dependency graph without Rust/JS compilation or downloads.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const root = fs.mkdtempSync(path.join(os.tmpdir(), 'yggdrasil-dev-assets-test-'));
const packages = ['shared', 'codemirror-editor', 'lightbox', 'mermaid-renderer',
  'tiptap-editor', 'xterm-terminal', 'yggdrasil-core'];
const outputs = {
  'codemirror-editor': ['codemirror/editor.js', 'codemirror/editor.js.map'],
  lightbox: ['lightbox/lightbox.js', 'lightbox/lightbox.js.map', 'lightbox/lightbox.css'],
  'mermaid-renderer': ['mermaid/mermaid.js', 'mermaid/mermaid.js.map'],
  'tiptap-editor': ['tiptap/editor.js', 'tiptap/editor.js.map', 'tiptap/editor.css'],
  'xterm-terminal': ['xterm/terminal.js', 'xterm/terminal.js.map', 'xterm/terminal.css'],
  'yggdrasil-core': ['yggdrasil-core/yggdrasil-core.js', 'yggdrasil-core/yggdrasil-core.js.map',
    'yggdrasil-core/yggdrasil-core.css'],
};
function write(file, content = '') {
  const target = path.join(root, file);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, content);
}
function step(expected) {
  write('commands.log');
  const result = spawnSync('make', ['dev-assets'], { cwd: root, encoding: 'utf8',
    env: { ...process.env, PATH: `${path.join(root, 'bin')}:${process.env.PATH}` } });
  assert.equal(result.status, 0, result.stderr + result.stdout);
  const commands = fs.readFileSync(path.join(root, 'commands.log'), 'utf8').trim().split('\n');
  assert.deepEqual(commands.filter(line => line.startsWith('build ')).sort(),
    expected.map(name => `build ${name}`).sort(), result.stdout);
  assert.equal(commands.filter(line => line === 'install').length, 1);
  assert.equal(commands.filter(line => line === 'css').length, 1);
  return commands;
}
async function edit(file, content = 'changed') {
  // GNU Make 3.81 on macOS can use second-resolution timestamps.
  await new Promise(resolve => setTimeout(resolve, 1100));
  write(file, content);
}
async function main() {
  write('Makefile', fs.readFileSync(path.join(__dirname, '../Makefile'), 'utf8'));
  for (const file of ['Cargo.toml', 'Cargo.lock', 'build.rs', 'src/bin/generate_highlight_css.rs',
    'input.css', 'libs/package.json', 'libs/bun.lock', 'libs/bunfig.toml',
    'libs/tsconfig.base.json', 'libs/patches/example.patch',
    'themes/Catppuccin Latte.tmTheme', 'themes/Catppuccin Mocha.tmTheme']) write(file);
  for (const name of packages) {
    write(`libs/${name}/package.json`);
    write(`libs/${name}/tsconfig.json`);
    write(`libs/${name}/src/index.ts`);
  }
  const mock = `#!${process.execPath}
const fs = require('node:fs');
const path = require('node:path');
const root = ${JSON.stringify(root)};
const outputs = ${JSON.stringify(outputs)};
const command = path.basename(process.argv[1]);
const args = process.argv.slice(2);
function write(file) {
  const target = path.join(root, file);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, 'built');
}
function record(line) { fs.appendFileSync(path.join(root, 'commands.log'), line + '\\n'); }
if (command === 'bun' && args[0] === 'install') {
  record('install');
  for (const file of ['libs/node_modules/.bun/marker', 'libs/node_modules/katex/dist/katex.min.css',
    'libs/node_modules/katex/dist/fonts/example.woff2']) {
    if (!fs.existsSync(path.join(root, file))) write(file);
  }
} else if (command === 'bun') {
  const name = path.basename(process.cwd());
  record('build ' + name);
  if (fs.existsSync(path.join(root, 'fail-' + name))) process.exit(1);
  for (const file of outputs[name] || []) write('public/' + file);
} else if (command === 'cargo') {
  record('highlight'); write('public/highlight.css');
} else if (command === 'tailwindcss') {
  record('css'); write('public/style.css');
} else if (command === 'rustup') console.log('llvm-tools-aarch64-apple-darwin');
`;
  for (const tool of ['bun', 'cargo', 'tailwindcss', 'rustup', 'dx']) {
    write(`bin/${tool}`, mock);
    fs.chmodSync(path.join(root, `bin/${tool}`), 0o755);
  }
  assert.ok(step(packages).includes('highlight'));
  assert.ok(!step([]).includes('highlight'));
  await edit('libs/tiptap-editor/src/index.ts');
  step(['tiptap-editor']);
  await edit('libs/shared/src/index.ts');
  step(packages.filter(name => name !== 'mermaid-renderer'));
  fs.unlinkSync(path.join(root, 'public/codemirror/editor.js'));
  step(['codemirror-editor']);
  await edit('libs/lightbox/src/removed.ts');
  step(['lightbox']);
  await new Promise(resolve => setTimeout(resolve, 1100));
  fs.unlinkSync(path.join(root, 'libs/lightbox/src/removed.ts'));
  step(['lightbox']);
  await edit('libs/lightbox/vite.config.ts');
  step(['lightbox']);
  await new Promise(resolve => setTimeout(resolve, 1100));
  fs.unlinkSync(path.join(root, 'libs/lightbox/vite.config.ts'));
  step(['lightbox']);
  fs.unlinkSync(path.join(root, 'public/katex/fonts/example.woff2'));
  step([]);
  assert.ok(fs.existsSync(path.join(root, 'public/katex/fonts/example.woff2')));
  await edit('libs/bun.lock');
  assert.ok(!step(packages).includes('highlight'));
  await edit('themes/Catppuccin Latte.tmTheme');
  assert.ok(step([]).includes('highlight'));
  fs.rmSync(path.join(root, 'libs/node_modules'), { recursive: true });
  step(packages);
  await edit('libs/tiptap-editor/src/index.ts');
  write('fail-tiptap-editor');
  const before = fs.statSync(path.join(root, 'target/dev-assets/tiptap-editor.stamp')).mtimeMs;
  const failed = spawnSync('make', ['dev-assets'], { cwd: root, encoding: 'utf8',
    env: { ...process.env, PATH: `${path.join(root, 'bin')}:${process.env.PATH}` } });
  assert.notEqual(failed.status, 0);
  assert.equal(fs.statSync(path.join(root, 'target/dev-assets/tiptap-editor.stamp')).mtimeMs, before);
  fs.unlinkSync(path.join(root, 'fail-tiptap-editor'));
  step(['tiptap-editor']);
  step([]);
  console.log('PASS: unchanged assets skip compilation; source, shared code, dependencies, missing outputs and failed builds invalidate correctly');
}
main().finally(() => fs.rmSync(root, { recursive: true, force: true })).catch(error => {
  console.error(error);
  process.exitCode = 1;
});
