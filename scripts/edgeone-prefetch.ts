#!/usr/bin/env bun
// 发版后把线上页面引用的 WASM 分包、入口与版本化静态资源预热到 EdgeOne 中间层。
// 内容寻址的文件名每次发版都会换，第一批访客本来要逐个回源；预热后只剩边缘到中间层一跳。
// 地址取自线上首页（HTML 里的入口与版本表、分包加载器里的全部 .wasm），与部署方式无关。
//
// 用法：bun scripts/edgeone-prefetch.ts [--site https://rua.plus] [--expect-hash <commit>] [--dry-run]
// 环境变量：EDGEONE_ZONE_ID（站点 ID，必填）；TENCENTCLOUD_SECRET_ID / TENCENTCLOUD_SECRET_KEY，
// 未设置时读 ~/.tccli/default.credential。--expect-hash 与首页 x-yggdrasil-hash 不符时不预热
// （新版本还没接流量）。预热带 Accept-Encoding: br：EdgeOne 按编码分别缓存，浏览器都请求 br。
import { createHash, createHmac } from 'node:crypto';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';

const HOST = 'teo.tencentcloudapi.com';
const VERSION = '2022-09-01';
/** CreatePrefetchTask 单次最多 1000 条（DescribeContentQuota 的 prefetch_url Batch）。 */
const BATCH = 1000;

interface Credentials {
  secretId: string;
  secretKey: string;
}

function credentials(): Credentials {
  const { TENCENTCLOUD_SECRET_ID: secretId, TENCENTCLOUD_SECRET_KEY: secretKey } = process.env;
  if (secretId && secretKey) return { secretId, secretKey };
  const file = path.join(os.homedir(), '.tccli/default.credential');
  if (fs.existsSync(file)) {
    const saved = JSON.parse(fs.readFileSync(file, 'utf8')) as Partial<Credentials>;
    if (saved.secretId && saved.secretKey) {
      return { secretId: saved.secretId, secretKey: saved.secretKey };
    }
  }
  throw new Error('缺少腾讯云凭据：设置 TENCENTCLOUD_SECRET_ID/KEY 或配置 tccli');
}

const sha256 = (data: string) => createHash('sha256').update(data).digest('hex');
const hmac = (key: string | Buffer, data: string) =>
  createHmac('sha256', key).update(data).digest();

/** 以 TC3-HMAC-SHA256 签名调用 teo 接口，返回 Response 字段；接口报错时抛出。 */
export async function teo(
  action: string,
  payload: Record<string, unknown>,
  auth = credentials(),
): Promise<Record<string, unknown>> {
  const body = JSON.stringify(payload);
  const timestamp = Math.floor(Date.now() / 1000);
  const date = new Date(timestamp * 1000).toISOString().slice(0, 10);
  const contentType = 'application/json; charset=utf-8';
  const signedHeaders = 'content-type;host;x-tc-action';
  const canonical = [
    'POST',
    '/',
    '',
    `content-type:${contentType}\nhost:${HOST}\nx-tc-action:${action.toLowerCase()}\n`,
    signedHeaders,
    sha256(body),
  ].join('\n');
  const scope = `${date}/teo/tc3_request`;
  const stringToSign = ['TC3-HMAC-SHA256', timestamp, scope, sha256(canonical)].join('\n');
  const signing = hmac(hmac(hmac(`TC3${auth.secretKey}`, date), 'teo'), 'tc3_request');
  const signature = createHmac('sha256', signing).update(stringToSign).digest('hex');
  const response = await fetch(`https://${HOST}/`, {
    method: 'POST',
    headers: {
      Authorization: `TC3-HMAC-SHA256 Credential=${auth.secretId}/${scope}, SignedHeaders=${signedHeaders}, Signature=${signature}`,
      'Content-Type': contentType,
      'X-TC-Action': action,
      'X-TC-Timestamp': String(timestamp),
      'X-TC-Version': VERSION,
    },
    body,
  });
  const json = (await response.json()) as {
    Response: Record<string, unknown> & { Error?: { Code: string; Message: string } };
  };
  const error = json.Response.Error;
  if (error) throw new Error(`${action} 失败：${error.Code} ${error.Message}`);
  return json.Response;
}

/** 线上首页引用的全部可长期缓存资源地址（绝对地址）。 */
export async function collectTargets(site: string, expectHash?: string): Promise<string[]> {
  const page = await fetch(`${site}/`, { headers: { 'Accept-Encoding': 'br' } });
  if (!page.ok) throw new Error(`GET ${site}/ 返回 ${page.status}`);
  const live = page.headers.get('x-yggdrasil-hash') ?? '';
  if (expectHash && live !== expectHash) {
    throw new Error(`线上版本 ${live || '未知'} 不是 ${expectHash}，跳过预热`);
  }
  const html = await page.text();

  const paths = new Set<string>();
  for (const [, file] of html.matchAll(/(?:href|src)="(\/pkg\/[^"?#]+\.(?:js|wasm))"/g)) {
    paths.add(file!);
  }
  const loader = [...paths].find((p) => /^\/pkg\/__wasm_split\.[^/]+\.js$/.test(p));
  if (!loader) throw new Error('首页没有分包加载器 __wasm_split.*.js');
  const loaderSource = await (await fetch(`${site}${loader}`)).text();
  for (const [, file] of loaderSource.matchAll(/"\.\/([^"/]+\.wasm)"/g)) {
    paths.add(`/pkg/${file}`);
  }

  const versions = html.match(/<script[^>]*\bid="ygg-asset-versions"[^>]*>([^<]*)<\/script>/)?.[1];
  for (const [asset, version] of Object.entries(
    versions ? (JSON.parse(versions) as Record<string, string>) : {},
  )) {
    paths.add(`${asset}?v=${version}`);
  }
  return [...paths].sort().map((p) => `${site}${p}`);
}

async function main() {
  const args = process.argv.slice(2);
  const option = (name: string) => {
    const index = args.indexOf(name);
    return index >= 0 ? args[index + 1] : undefined;
  };
  const site = (option('--site') ?? 'https://rua.plus').replace(/\/$/, '');
  const targets = await collectTargets(site, option('--expect-hash'));
  console.log(`==> 预热 ${targets.length} 个地址（${site}）`);
  if (args.includes('--dry-run')) {
    for (const target of targets) console.log(target);
    return;
  }
  const zone = process.env.EDGEONE_ZONE_ID;
  if (!zone) throw new Error('缺少 EDGEONE_ZONE_ID');
  const auth = credentials();
  for (let i = 0; i < targets.length; i += BATCH) {
    const result = await teo(
      'CreatePrefetchTask',
      {
        ZoneId: zone,
        Targets: targets.slice(i, i + BATCH),
        Headers: [{ Name: 'Accept-Encoding', Value: 'br' }],
      },
      auth,
    );
    const failed = (result.FailedList as unknown[] | undefined) ?? [];
    console.log(`  任务 ${String(result.JobId)}，失败 ${failed.length} 条`);
    if (failed.length > 0) console.log(JSON.stringify(failed));
  }
}

if (import.meta.main) {
  main().catch((error: unknown) => {
    console.error(`error: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  });
}
