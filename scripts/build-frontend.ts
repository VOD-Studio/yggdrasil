#!/usr/bin/env bun
import { cpSync, existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';

const assets = resolve(process.env.LEPTOS_ASSETS_DIR ?? 'public');
const docs = join(assets, 'doc');
const env = { ...process.env };
let staging: string | undefined;

try {
  // make doc 留下的开发文档仍可用于开发站点。发布时从其余资源装配，
  // 构建好的文档由最后的 make doc 直接写入发布目录。
  if (existsSync(docs)) {
    mkdirSync('target', { recursive: true });
    staging = mkdtempSync(resolve('target/release-assets-'));
    cpSync(assets, staging, { recursive: true, filter: (file) => file !== docs });
    env.LEPTOS_ASSETS_DIR = staging;
  }

  const build = Bun.spawn(['cargo', 'leptos', 'build', ...process.argv.slice(2)], {
    env,
    stdin: 'inherit',
    stdout: 'inherit',
    stderr: 'inherit',
  });
  process.exitCode = await build.exited;
} finally {
  if (staging) rmSync(staging, { recursive: true, force: true });
}
