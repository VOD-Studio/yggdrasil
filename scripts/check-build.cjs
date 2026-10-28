#!/usr/bin/env bun
// 检查完整发布流水线；保留原始输出，不能只依赖各工具的退出码。
import { spawn } from 'node:child_process';
import { closeSync, mkdirSync, openSync, writeSync } from 'node:fs';
import { resolve } from 'node:path';
import { stripVTControlCharacters } from 'node:util';

export function buildDiagnostics(log) {
  return stripVTControlCharacters(log).split(/\r?\n/).filter(line =>
    /(?:^|[\s[(:])(?:warn(?:ing)?s?|errors?)(?=$|[\s\]:[(])/i.test(line) ||
    /(?:^|\s)\(!\)/.test(line),
  );
}

if (import.meta.main) {
  const root = resolve(import.meta.dir, '..');
  const logPath = resolve(root, 'target/build.log');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const log = openSync(logPath, 'w');
  const child = spawn('make', ['build'], {
    cwd: root,
    stdio: ['inherit', 'pipe', 'pipe'],
  });
  const diagnostics = [];
  for (const [source, destination] of [[child.stdout, process.stdout], [child.stderr, process.stderr]]) {
    // 分别识别两条流，避免交错写入把跨 chunk 的诊断词拆散。
    const chunks = [];
    source.on('data', chunk => {
      chunks.push(chunk);
      writeSync(log, chunk);
      destination.write(chunk);
    });
    source.on('end', () => diagnostics.push(...buildDiagnostics(Buffer.concat(chunks).toString('utf8'))));
  }
  child.on('error', error => console.error(`无法启动 make build：${error.message}`));
  child.on('close', (code, signal) => {
    closeSync(log);
    if (diagnostics.length) {
      console.error(`发布构建出现 ${diagnostics.length} 行告警或错误，完整日志：${logPath}`);
      for (const line of diagnostics) console.error(line);
    }
    if (code !== 0 || signal) {
      console.error(`make build 失败（${signal ?? code ?? '未启动'}），完整日志：${logPath}`);
    }
    process.exitCode = code === 0 && !signal && diagnostics.length === 0 ? 0 : 1;
  });
}
