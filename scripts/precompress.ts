#!/usr/bin/env bun
import { randomUUID } from 'node:crypto';
import { mkdir, readdir, readFile, rename, rm, stat, utimes, writeFile } from 'node:fs/promises';
import { availableParallelism } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { brotliCompress, constants } from 'node:zlib';

const root = process.argv[2];
if (!root) {
  throw new Error('用法：bun scripts/precompress.ts <发布站点目录>');
}

// 图片与字体已有压缩，.d.ts 和调试映射也无需生成旁车。
const asset = /\.(?:js|css|wasm|svg|html|json|xml|webmanifest)$/;
const directories = [root];
const files: string[] = [];
for (const dir of directories) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      directories.push(path);
    } else if (entry.isFile() && asset.test(entry.name)) {
      files.push(path);
    }
  }
}
const compress = promisify(brotliCompress);
const quality = 11;
// 编码器版本和质量参与缓存寻址。大小提示从哈希所覆盖的原件字节得到。
const cacheDir = join(
  process.env.YGG_PRECOMPRESS_CACHE_DIR ?? 'target/precompress-cache',
  `brotli-${quality}-${Bun.revision}`,
);
await mkdir(cacheDir, { recursive: true });
const inFlight = new Map<string, Promise<Buffer>>();
let compressed = 0;

async function readOrCompress(cacheFile: string, data: Buffer): Promise<Buffer> {
  try {
    return await readFile(cacheFile);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
  }
  const encoded = await compress(data, {
    params: {
      [constants.BROTLI_PARAM_QUALITY]: quality,
      [constants.BROTLI_PARAM_SIZE_HINT]: data.length,
    },
  });
  const temporary = `${cacheFile}.${randomUUID()}.tmp`;
  try {
    await writeFile(temporary, encoded);
    await rename(temporary, cacheFile);
  } finally {
    await rm(temporary, { force: true });
  }
  compressed++;
  return encoded;
}

function cachedCompression(data: Buffer): Promise<Buffer> {
  const hash = new Bun.CryptoHasher('sha256').update(data).digest('hex');
  const active = inFlight.get(hash);
  if (active) return active;
  const work = readOrCompress(join(cacheDir, `${hash}.br`), data).finally(() => inFlight.delete(hash));
  inFlight.set(hash, work);
  return work;
}

const pending = files.values();
let failed = false;

// 限制同时编码的文件数，避免质量 11 的编码器占用过多内存。
const results = await Promise.allSettled(
  Array.from({ length: Math.min(availableParallelism(), 4) }, async () => {
    for (const path of pending) {
      if (failed) break;
      const sidecar = `${path}.br`;
      const temporary = `${sidecar}.${process.pid}.tmp`;
      try {
        const data = await readFile(path);
        const { atimeMs, mtimeMs } = await stat(path);
        const encoded = await cachedCompression(data);
        await writeFile(temporary, encoded);
        // 与原文件使用相同的修改时间，供 ServeDir 生成 Last-Modified。
        await utimes(temporary, atimeMs / 1000, mtimeMs / 1000);
        await rename(temporary, sidecar);
      } catch (error) {
        failed = true;
        await rm(temporary, { force: true });
        throw new Error(`预压缩失败：${path}`, { cause: error });
      }
    }
  }),
);
const failure = results.find((result) => result.status === 'rejected');
if (failure) throw failure.reason;

console.log(
  `已生成 ${files.length} 个 Brotli 旁车（${root}），新压缩 ${compressed} 个，复用 ${files.length - compressed} 个`,
);
