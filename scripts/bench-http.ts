#!/usr/bin/env bun
// release 产物 HTTP 宏基准：一次性 PostgreSQL + 真实 release 服务端，oha 以固定到达率压公开路由。
// SSR 缓存命中（cached）与未命中（ssr，带 ?bench=ssr 绕过 HTML 缓存）分开测；支持同机 A/B
// ABBA 交替对比，并用精确置换检验（Mann–Whitney U）判定差异是否显著。
// 语料与 benches/markdown.rs 共用：benches/fixtures/*.md（4 篇线上文章）+ 56 篇填充。
//
// 用法：
//   make build && make bench-http                         # 单 target
//   make build RELEASE_OUT=target/bench-base               # 另一份产物（基准版本）
//   BENCH_TARGETS=target/bench-base,target/release-dist make bench-http   # A/B（A=前者，B=后者）
//
// 环境变量（默认值）：
//   BENCH_TARGETS            逗号分隔 1–2 个产物目录（target/release-dist），依次标为 A、B；
//                            每个目录需有 server、hash.txt、public/release-manifest.json
//   BENCH_ROUNDS             测量轮数 5..10（5）；5 是精确检验能做到 p<0.05 的最小轮数
//   BENCH_DURATION_SECS      每次测量秒数（10）
//   BENCH_WARMUP_SECS        每轮重启后，每个 target 每个场景的预热秒数（3）
//   BENCH_CALIBRATION_SECS   A 的闭环容量校准秒数（10）
//   BENCH_LOAD_FACTOR        固定到达率 = 容量 × 该系数，0<f<1（0.5）
//   BENCH_CONNECTIONS        oha 并发连接数（32）
//   BENCH_ONLY               按场景名过滤的正则（空）
//   BENCH_RATES_FROM         上一次的 results.json；按场景名复用 rate，跳过校准（空）
//   BENCH_SERVER_CPUS / BENCH_DB_CPUS / BENCH_CLIENT_CPUS
//                            taskset / --cpuset-cpus 的 CPU 组（0-3 / 4,5 / 6,7,14,15）。默认值按
//                            Ryzen 7 5800（CPU n 与 n+8 为 SMT 兄弟线程）分配；三组必须两两不相交。
// 依赖：oha（cargo install --locked oha@1.16.0）、taskset、curl、psql、docker。
// 基准前请关闭 make dev / 浏览器等后台负载，并把被钉住的 CPU 调成 performance governor。
import { createHash } from 'node:crypto';
import * as fs from 'node:fs';
import * as net from 'node:net';
import * as os from 'node:os';
import * as path from 'node:path';

const ROOT = path.resolve(__dirname, '..');
const PG_IMAGE = 'postgres:16-alpine';
const FIXTURES = ['plain-prose', 'tables-prose', 'math-heavy', 'all-features'];
const FILLER_COUNT = 56;
const ADMIN_TOKEN = 'bench-http-admin';
const ROUTES = ['/', '/archives', ...FIXTURES.map((name) => `/post/bench-${name}`)];
// 渲染后 HTML 超过 ssr_cache.rs 的 MAX_PAGE_BYTES（1 MiB）的页面永远不进 SSR 缓存（math-heavy 约
// 1.26 MB，all-features 约 2.4 MB），生产里就是每次都完整 SSR，所以只测 ssr 场景，不存在 cached 场景。
const UNCACHEABLE_ROUTES = ['/post/bench-math-heavy', '/post/bench-all-features'];
const CALIBRATION_MIN_RATE = 1;
const SIZE_TOLERANCE = 0.05;
const METRICS = ['p50', 'p90', 'p99', 'p999', 'rps'] as const;
type Metric = (typeof METRICS)[number];

// ---------- 配置 ----------

function intEnv(name: string, fallback: number, min: number, max = Number.POSITIVE_INFINITY) {
  const raw = process.env[name];
  if (raw === undefined || raw === '') return fallback;
  const n = Number(raw);
  if (!Number.isInteger(n) || n < min || n > max) {
    throw new Error(
      `${name} 必须是 ${min}..${max === Number.POSITIVE_INFINITY ? '∞' : max} 的整数，当前 ${raw}`,
    );
  }
  return n;
}

function parseCpus(name: string, spec: string, cpuCount: number): number[] {
  const out: number[] = [];
  for (const part of spec.split(',')) {
    const m = /^(\d+)(?:-(\d+))?$/.exec(part.trim());
    if (!m) throw new Error(`${name}=${spec} 无法解析，写法如 0-3 或 4,5`);
    const from = Number(m[1]);
    const to = m[2] === undefined ? from : Number(m[2]);
    if (to < from) throw new Error(`${name}=${spec} 范围倒置`);
    for (let i = from; i <= to; i++) out.push(i);
  }
  for (const cpu of out) {
    if (cpu >= cpuCount) {
      throw new Error(
        `${name}=${spec} 含 CPU${cpu}，本机只有 ${cpuCount} 个逻辑 CPU；请设置 BENCH_SERVER_CPUS/BENCH_DB_CPUS/BENCH_CLIENT_CPUS`,
      );
    }
  }
  return [...new Set(out)];
}

interface Config {
  targets: string[];
  rounds: number;
  durationSecs: number;
  warmupSecs: number;
  calibrationSecs: number;
  loadFactor: number;
  connections: number;
  only: string;
  ratesFrom: string;
  cpus: { server: string; db: string; client: string };
  clientCpuCount: number;
}

function loadConfig(): Config {
  const loadFactor = Number(process.env.BENCH_LOAD_FACTOR || 0.5);
  if (!(loadFactor > 0 && loadFactor < 1)) {
    throw new Error(`BENCH_LOAD_FACTOR 必须满足 0<f<1，当前 ${process.env.BENCH_LOAD_FACTOR}`);
  }
  const targets = (process.env.BENCH_TARGETS || 'target/release-dist')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  if (targets.length < 1 || targets.length > 2) {
    throw new Error(`BENCH_TARGETS 需要 1 或 2 个目录，当前 ${targets.length} 个`);
  }
  const cpuCount = os.cpus().length;
  const spec = {
    server: process.env.BENCH_SERVER_CPUS || '0-3',
    db: process.env.BENCH_DB_CPUS || '4,5',
    client: process.env.BENCH_CLIENT_CPUS || '6,7,14,15',
  };
  const groups = {
    server: parseCpus('BENCH_SERVER_CPUS', spec.server, cpuCount),
    db: parseCpus('BENCH_DB_CPUS', spec.db, cpuCount),
    client: parseCpus('BENCH_CLIENT_CPUS', spec.client, cpuCount),
  };
  const names = Object.keys(groups) as (keyof typeof groups)[];
  for (let i = 0; i < names.length; i++) {
    for (let j = i + 1; j < names.length; j++) {
      const shared = groups[names[i]].filter((c) => groups[names[j]].includes(c));
      if (shared.length > 0) {
        throw new Error(
          `CPU 组 ${names[i]} 与 ${names[j]} 相交（CPU${shared.join(',')}）；请设置 BENCH_SERVER_CPUS/BENCH_DB_CPUS/BENCH_CLIENT_CPUS 使三组互不相交`,
        );
      }
    }
  }
  return {
    targets,
    rounds: intEnv('BENCH_ROUNDS', 5, 5, 10),
    durationSecs: intEnv('BENCH_DURATION_SECS', 10, 1),
    warmupSecs: intEnv('BENCH_WARMUP_SECS', 3, 1),
    calibrationSecs: intEnv('BENCH_CALIBRATION_SECS', 10, 1),
    loadFactor,
    connections: intEnv('BENCH_CONNECTIONS', 32, 1),
    only: process.env.BENCH_ONLY || '',
    ratesFrom: process.env.BENCH_RATES_FROM || '',
    cpus: spec,
    clientCpuCount: groups.client.length,
  };
}

// ---------- 运行时与清理 ----------

const runtime = fs.mkdtempSync(path.join(os.tmpdir(), 'ygg-bench-http-'));
const container = `ygg-bench-http-${process.pid}-${Date.now()}`;
const liveServers = new Set<Bun.Subprocess>();
let child: Bun.Subprocess | null = null;
let containerStarted = false;
let cleaned = false;
let artifacts = '';

async function run(args: string[], env: NodeJS.ProcessEnv = process.env) {
  child = Bun.spawn(args, { cwd: ROOT, env, stdout: 'inherit', stderr: 'inherit' });
  const code = await child.exited;
  child = null;
  if (code !== 0) throw new Error(`${args[0]} exited ${code}`);
}
async function capture(args: string[], env: NodeJS.ProcessEnv = process.env) {
  const proc = Bun.spawn(args, { env, stdout: 'pipe', stderr: 'pipe' });
  const [output, error, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  if (code !== 0) throw new Error(`${args[0]} exited ${code}: ${error.trim()}`);
  return output.trim();
}
async function cleanup() {
  if (cleaned) return;
  cleaned = true;
  if (child) {
    child.kill();
    await child.exited;
  }
  for (const server of liveServers) {
    server.kill();
    await server.exited;
  }
  if (containerStarted) await capture(['docker', 'rm', '-f', container]).catch(() => {});
  fs.rmSync(runtime, { recursive: true, force: true });
}
for (const signal of ['SIGINT', 'SIGTERM'])
  process.on(signal, async () => {
    await cleanup();
    process.exit(1);
  });

async function freePort() {
  const listener = net.createServer();
  const listening = Promise.withResolvers<void>();
  listener.listen(0, '127.0.0.1', listening.resolve);
  await listening.promise;
  const port = (listener.address() as net.AddressInfo).port;
  const closed = Promise.withResolvers<void>();
  listener.close(() => closed.resolve());
  await closed.promise;
  return port;
}
async function waitFor(check: () => Promise<boolean>, description: string, attempts = 120) {
  for (let i = 0; i < attempts; i++) {
    if (await check()) return;
    await Bun.sleep(500);
  }
  throw new Error(`Timed out waiting for ${description}; see ${artifacts}`);
}

// ---------- 统计 ----------

const median = (values: number[]) => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};
const round3 = (n: number) => Math.round(n * 1000) / 1000;

/** 精确置换检验（双侧 Mann–Whitney U）：枚举 C(n+m, n) 种划分。 */
function mannWhitneyP(a: number[], b: number[]): number {
  const n = a.length;
  const total = n + b.length;
  const pooled = [...a, ...b];
  const score = pooled.map((x) => pooled.map((y) => (x < y ? 1 : x === y ? 0.5 : 0)));
  const center = (n * b.length) / 2;
  const observed = Math.abs(
    a.reduce((s, _, i) => s + b.reduce((t, _, j) => t + score[i][n + j], 0), 0) - center,
  );
  let count = 0;
  let all = 0;
  const inA: boolean[] = new Array(total).fill(false);
  const visit = (start: number, chosen: number) => {
    if (chosen === n) {
      let u = 0;
      for (let i = 0; i < total; i++) {
        if (!inA[i]) continue;
        for (let j = 0; j < total; j++) if (!inA[j]) u += score[i][j];
      }
      all++;
      if (Math.abs(u - center) >= observed - 1e-9) count++;
      return;
    }
    for (let i = start; i <= total - (n - chosen); i++) {
      inA[i] = true;
      visit(i + 1, chosen + 1);
      inA[i] = false;
    }
  };
  visit(0, 0);
  return count / all;
}

// ---------- 类型 ----------

interface Target {
  label: 'A' | 'B';
  dir: string;
  releaseId: string;
  gitHash: string;
  dirty: boolean;
  database: string;
  base: string;
  port: number;
  proc: Bun.Subprocess | null;
}
interface Scenario {
  name: string;
  url: string;
  mode: 'cached' | 'ssr';
  route: string;
}
interface RunResult {
  round: number;
  p50: number;
  p90: number;
  p99: number;
  p999: number;
  rps: number;
  bytesPerRequest: number;
  saturated: boolean;
}
interface Probe {
  bytes: number;
  title: string;
}
interface OhaJson {
  summary: { requestsPerSec: number; sizePerRequest: number };
  latencyPercentiles: Record<string, number>;
  statusCodeDistribution: Record<string, number>;
  errorDistribution: Record<string, number>;
}

const readText = (file: string) => {
  try {
    return fs.readFileSync(file, 'utf8').trim();
  } catch {
    return null;
  }
};

function cpuList(spec: string): number[] {
  return spec.split(',').flatMap((part) => {
    const [from, to = from] = part.split('-').map(Number);
    return Array.from({ length: to - from + 1 }, (_, i) => from + i);
  });
}

// ---------- 主流程 ----------

async function main() {
  const config = loadConfig();
  for (const tool of ['oha', 'taskset', 'curl', 'psql', 'docker']) {
    if (Bun.which(tool)) continue;
    throw new Error(
      tool === 'oha'
        ? '缺少 oha：cargo install --locked oha@1.16.0（或 pacman -S oha）'
        : `缺少 ${tool}`,
    );
  }
  const only = config.only ? new RegExp(config.only) : null;

  const targets: Target[] = config.targets.map((dir, i) => {
    const abs = path.resolve(ROOT, dir);
    for (const required of [
      path.join(abs, 'server'),
      path.join(abs, 'hash.txt'),
      path.join(abs, 'public/release-manifest.json'),
    ]) {
      if (!fs.existsSync(required)) throw new Error(`缺少 ${required}，先运行 make build`);
    }
    const manifest = JSON.parse(
      fs.readFileSync(path.join(abs, 'public/release-manifest.json'), 'utf8'),
    );
    const label = i === 0 ? 'A' : 'B';
    return {
      label,
      dir: abs,
      releaseId: String(manifest.release_id),
      gitHash: String(manifest.source?.git_hash),
      dirty: Boolean(manifest.source?.dirty),
      database: `ygg_bench_${label.toLowerCase()}`,
      base: '',
      port: 0,
      proc: null,
    };
  });

  const scenarios: Scenario[] = [
    ...(['cached', 'ssr'] as const).flatMap((mode) =>
      ROUTES.map((route) => ({
        name: `${mode} ${route}`,
        url: mode === 'cached' ? route : `${route}?bench=ssr`,
        mode,
        route,
      })),
    ),
  ]
    .filter((s) => s.mode === 'ssr' || !UNCACHEABLE_ROUTES.includes(s.route))
    .filter((s) => !only || only.test(s.name));
  if (scenarios.length === 0) throw new Error(`BENCH_ONLY=${config.only} 没有匹配任何场景`);

  let rates: Record<string, number> | null = null;
  if (config.ratesFrom) {
    const previous = JSON.parse(fs.readFileSync(path.resolve(ROOT, config.ratesFrom), 'utf8'));
    rates = {};
    for (const s of scenarios) {
      const found = (previous.scenarios as { name: string; rate: number }[]).find(
        (p) => p.name === s.name,
      );
      if (!found) throw new Error(`BENCH_RATES_FROM 缺少场景 ${s.name}`);
      rates[s.name] = found.rate;
    }
  }

  // 环境元数据与告警（不中止）。
  const pinned = [
    ...new Set([
      ...cpuList(config.cpus.server),
      ...cpuList(config.cpus.db),
      ...cpuList(config.cpus.client),
    ]),
  ].sort((a, b) => a - b);
  const governors: Record<string, string | null> = {};
  for (const cpu of pinned) {
    const governor = readText(`/sys/devices/system/cpu/cpu${cpu}/cpufreq/scaling_governor`);
    governors[`cpu${cpu}`] = governor;
    if (governor !== 'performance') {
      console.warn(`警告：CPU${cpu} governor=${governor ?? '未知'}，结果噪声会变大`);
    }
  }
  const loadavg = Number((readText('/proc/loadavg') || '0').split(' ')[0]);
  if (loadavg > 1.0) {
    console.warn(`警告：系统负载 ${loadavg}，请关闭 make dev/浏览器等后台负载`);
  }
  const ohaVersion = await capture(['oha', '--version']).catch(() => 'unknown');

  const stamp = new Date().toISOString().replace(/[:.]/g, '-');
  artifacts = path.join(ROOT, 'target/bench', `${stamp}-http`);
  fs.mkdirSync(path.join(artifacts, 'oha'), { recursive: true });

  // 数据库：一个容器，每个 target 一个独立库（各自用自己的渲染器生成 content_html）。
  containerStarted = true;
  await capture([
    'docker',
    'run',
    '-d',
    '--name',
    container,
    '--cpuset-cpus',
    config.cpus.db,
    '-e',
    'POSTGRES_PASSWORD=bench',
    '-p',
    '127.0.0.1::5432',
    PG_IMAGE,
  ]);
  const pgPort = (await capture(['docker', 'port', container, '5432/tcp'])).split(':').at(-1);
  const pg = {
    ...process.env,
    PGHOST: '127.0.0.1',
    PGPORT: pgPort,
    PGUSER: 'postgres',
    PGPASSWORD: 'bench',
  };
  console.log(`等待 PostgreSQL（${PG_IMAGE}）…`);
  await waitFor(async () => {
    const p = Bun.spawn(['pg_isready'], { env: pg, stdout: 'ignore', stderr: 'ignore' });
    return (await p.exited) === 0;
  }, 'PostgreSQL');
  for (const t of targets) await run(['createdb', t.database], pg);

  // 服务端：每个 target 一个进程，A/B 同时常驻（空闲的几乎不占 CPU）；每轮会重启，见测量段。
  const startServer = async (t: Target, tag: string) => {
    if (t.port === 0) {
      t.port = await freePort();
      t.base = `http://127.0.0.1:${t.port}`;
    }
    const cwd = path.join(runtime, `app-${t.label}`);
    fs.mkdirSync(path.join(cwd, 'uploads'), { recursive: true });
    const env: Record<string, string | undefined> = {
      ...pg,
      DATABASE_URL: `postgresql://postgres:bench@127.0.0.1:${pgPort}/${t.database}`,
      IP: '127.0.0.1',
      PORT: String(t.port),
      APP_BASE_URL: t.base,
      COOKIE_SECURE: 'false',
      SSR_CACHE_SECS: '3600',
      COMPRESSION_ALGORITHMS: 'br, gzip',
      LEPTOS_SITE_ROOT: path.join(t.dir, 'public'),
      MCP_TOKEN_ENC_KEY: '42'.repeat(32),
      ADMIN_USERNAME: '',
      ADMIN_PASSWORD: '',
      ADMIN_EMAIL: '',
      RUST_LOG: 'info',
      YGG_PREVIOUS_PUBLIC_DIR: undefined,
    };
    const proc = Bun.spawn(['taskset', '-c', config.cpus.server, path.join(t.dir, 'server')], {
      cwd,
      env,
      stdout: Bun.file(path.join(artifacts, `server-${t.label}-${tag}.log`)),
      stderr: Bun.file(path.join(artifacts, `server-${t.label}-${tag}-error.log`)),
    });
    t.proc = proc;
    liveServers.add(proc);
    await waitFor(async () => {
      if (proc.exitCode !== null) throw new Error(`服务端 ${t.label} 已退出；见 ${artifacts}`);
      return fetch(`${t.base}/readyz`)
        .then((r) => r.ok)
        .catch(() => false);
    }, `服务端 ${t.label}`);
  };
  const stopServer = async (t: Target) => {
    if (!t.proc) return;
    t.proc.kill();
    await t.proc.exited;
    liveServers.delete(t.proc);
    t.proc = null;
  };
  for (const t of targets) {
    await startServer(t, 'r0');
    console.log(
      `启动服务端 ${t.label}：${t.base}（${t.releaseId} ${t.gitHash.slice(0, 8)}${t.dirty ? '-dirty' : ''}）`,
    );
  }

  // 种子数据 + 用真实渲染管线生成 content_html。
  const bodies = FIXTURES.map((name) =>
    fs.readFileSync(path.join(ROOT, 'benches/fixtures', `${name}.md`), 'utf8'),
  );
  for (const body of bodies) {
    if (body.includes('$ygg_bench$')) throw new Error('语料含 dollar quote 标记 $ygg_bench$');
  }
  const tokenHash = createHash('sha256').update(ADMIN_TOKEN).digest('hex');
  const quote = (s: string) => `$ygg_bench$${s}$ygg_bench$`;
  const fixturePosts = FIXTURES.map(
    (
      name,
      k,
    ) => `INSERT INTO posts(id,author_id,title,slug,summary,content_md,content_html,status,published_at)
      VALUES(${k + 1},1,${quote(`基准：${name}`)},'bench-${name}',${quote('基准语料')},${quote(bodies[k])},NULL,'published',NOW() - INTERVAL '${k + 1} minutes');`,
  ).join('\n');
  const total = FIXTURES.length + FILLER_COUNT;
  const sql = `INSERT INTO users(id,username,email,password_hash,role) VALUES(1,'benchadmin','bench@test.invalid','unused','admin');
    INSERT INTO sessions(user_id,token_hash,expires_at) VALUES(1,'${tokenHash}',NOW()+INTERVAL '1 hour');
    INSERT INTO tags(id,name) VALUES(1,'bench');
    ${fixturePosts}
    INSERT INTO posts(id,author_id,title,slug,summary,content_md,content_html,status,published_at)
      SELECT ${FIXTURES.length}+g,1,'基准填充 '||lpad(g::text,2,'0'),'bench-filler-'||lpad(g::text,2,'0'),${quote('基准语料')},${quote(bodies[0])},NULL,'published',NOW() - (g||' days')::interval
      FROM generate_series(1,${FILLER_COUNT}) g;
    INSERT INTO post_tags(post_id,tag_id) SELECT id,1 FROM posts;
    SELECT setval('users_id_seq',1); SELECT setval('posts_id_seq',${total}); SELECT setval('tags_id_seq',1);`;
  const seedFile = path.join(runtime, 'seed.sql');
  fs.writeFileSync(seedFile, sql);
  for (const t of targets) {
    await run(
      ['psql', '-q', '-o', '/dev/null', '-v', 'ON_ERROR_STOP=1', '-d', t.database, '-f', seedFile],
      pg,
    );
    let cursor: number | null = 0;
    let rebuilt = 0;
    let failed = 0;
    const errors: string[] = [];
    while (cursor !== null) {
      const response = await fetch(`${t.base}/api/rebuild_content_html`, {
        method: 'POST',
        headers: {
          Cookie: `session=${ADMIN_TOKEN}`,
          Origin: t.base,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ rebuild_all: false, after_id: cursor }),
      });
      const text = await response.text();
      if (!response.ok)
        throw new Error(`rebuild_content_html ${t.label}: ${response.status} ${text}`);
      const result = JSON.parse(text) as {
        rebuilt: number;
        failed: number;
        errors: string[];
        next_after_id: number | null;
      };
      rebuilt += result.rebuilt;
      failed += result.failed;
      errors.push(...result.errors);
      cursor = result.next_after_id;
    }
    if (rebuilt !== total || failed !== 0) {
      throw new Error(
        `渲染 ${t.label}：rebuilt=${rebuilt}/${total} failed=${failed} ${errors.join(' | ')}`,
      );
    }
    const missing = await capture(
      [
        'psql',
        '-At',
        '-d',
        t.database,
        '-c',
        'SELECT count(*) FROM posts WHERE content_html IS NULL',
      ],
      pg,
    );
    if (missing !== '0') throw new Error(`渲染 ${t.label}：仍有 ${missing} 篇 content_html 为空`);
    console.log(`已渲染 ${t.label}：${rebuilt} 篇`);
  }

  // ---------- 探针与 oha ----------

  const titleOf = async (url: string) => {
    const response = await fetch(url, { headers: { 'accept-encoding': 'br' } });
    const html = await response.text();
    return /<title[^>]*>([^<]*)<\/title>/.exec(html)?.[1]?.trim() ?? '';
  };
  const curlProbe = async (url: string) => {
    const out = await capture([
      'curl',
      '-sS',
      '-o',
      '/dev/null',
      '-H',
      'accept-encoding: br',
      '-w',
      '%{http_code}|%{size_download}|%header{content-encoding}|%header{x-ssr-cache}',
      url,
    ]);
    const [status, bytes, encoding, cache] = out.split('|');
    return { status, bytes: Number(bytes), encoding, cache };
  };
  async function probe(t: Target, s: Scenario): Promise<Probe> {
    const url = `${t.base}${s.url}`;
    let result = await curlProbe(url);
    if (s.mode === 'cached') {
      const deadline = Date.now() + 20_000;
      while (result.cache !== 'HIT' && Date.now() < deadline) {
        await Bun.sleep(200);
        result = await curlProbe(url);
      }
    }
    const fail = (why: string) => {
      throw new Error(
        `探针失败 ${t.label} ${s.name}：${why}（status=${result.status} encoding=${result.encoding} x-ssr-cache=${result.cache}）`,
      );
    };
    if (result.status !== '200') fail('状态不是 200');
    if (result.encoding !== 'br') fail('content-encoding 不是 br');
    const expected = s.mode === 'cached' ? 'HIT' : 'BYPASS';
    if (result.cache !== expected) {
      const html = await (await fetch(url, { headers: { 'accept-encoding': 'br' } })).text();
      fail(
        `x-ssr-cache 应为 ${expected}；未压缩 HTML ${html.length} 字符（超过 1 MiB 的页面不进 SSR 缓存，见 ssr_cache.rs MAX_PAGE_BYTES）`,
      );
    }
    if (!(result.bytes > 0)) fail('响应体为空');
    const title = await titleOf(url);
    if (!title) fail('页面没有 <title>');
    if (s.mode === 'ssr') {
      const cachedTitle = await titleOf(`${t.base}${s.route}`);
      if (title !== cachedTitle) {
        fail(
          `<title> 与缓存版本不一致：${JSON.stringify(title)} vs ${JSON.stringify(cachedTitle)}`,
        );
      }
    }
    return { bytes: result.bytes, title };
  }

  async function oha(
    t: Target,
    s: Scenario,
    phase: string,
    secs: number,
    rate: number | null,
    probed: Probe,
  ): Promise<OhaJson> {
    const slug = s.name.replace(/[^a-zA-Z0-9]+/g, '-').replace(/^-|-$/g, '');
    const file = path.join(artifacts, 'oha', `${t.label}-${slug}-${phase}.json`);
    const args = [
      'taskset',
      '-c',
      config.cpus.client,
      'oha',
      '--no-tui',
      '--output-format',
      'json',
      '-o',
      file,
      '--worker-threads',
      String(config.clientCpuCount),
      '-c',
      String(config.connections),
      '-z',
      `${secs}s`,
      '-w',
      '--disable-compression',
      '-H',
      'accept-encoding: br',
      ...(rate === null ? [] : ['-q', String(rate), '--latency-correction']),
      `${t.base}${s.url}`,
    ];
    child = Bun.spawn(args, { stdout: 'ignore', stderr: 'pipe' });
    const [stderr, code] = await Promise.all([
      new Response(child.stderr as ReadableStream).text(),
      child.exited,
    ]);
    child = null;
    if (code !== 0)
      throw new Error(`oha ${t.label} ${s.name} ${phase} exited ${code}: ${stderr.trim()}`);
    const json = JSON.parse(fs.readFileSync(file, 'utf8')) as OhaJson;
    const codes = Object.keys(json.statusCodeDistribution);
    if (codes.length !== 1 || codes[0] !== '200') {
      throw new Error(
        `${t.label} ${s.name} ${phase}：状态码分布 ${JSON.stringify(json.statusCodeDistribution)}`,
      );
    }
    if (Object.keys(json.errorDistribution).length > 0) {
      throw new Error(
        `${t.label} ${s.name} ${phase}：错误 ${JSON.stringify(json.errorDistribution)}`,
      );
    }
    const size = json.summary.sizePerRequest;
    if (Math.abs(size - probed.bytes) > probed.bytes * SIZE_TOLERANCE) {
      throw new Error(
        `${t.label} ${s.name} ${phase}：oha 每请求 ${size} B 与探针 br 体积 ${probed.bytes} B 相差超过 ${SIZE_TOLERANCE * 100}%，请求可能没有协商成 br`,
      );
    }
    return json;
  }

  // 校准：只对 A 跑闭环，取容量；这一步同时充当 A 的预热。
  const capacity: Record<string, number | null> = {};
  const rateOf: Record<string, number> = {};
  const probeBytes: Record<string, Record<string, number>> = {};
  if (rates) {
    console.log('复用 BENCH_RATES_FROM 的 rate，跳过校准');
    for (const s of scenarios) {
      rateOf[s.name] = rates[s.name];
      capacity[s.name] = null;
    }
  } else {
    for (const s of scenarios) {
      const probed = await probe(targets[0], s);
      const json = await oha(targets[0], s, 'calibration', config.calibrationSecs, null, probed);
      const capacityRps = json.summary.requestsPerSec;
      const rate = Math.floor(capacityRps * config.loadFactor);
      if (rate < CALIBRATION_MIN_RATE)
        throw new Error(`${s.name}：容量 ${capacityRps} rps，rate 不足 1`);
      capacity[s.name] = capacityRps;
      rateOf[s.name] = rate;
      console.log(`校准 ${s.name}：容量 ${Math.round(capacityRps)} rps → rate ${rate}`);
    }
  }

  // 测量：每轮重启服务端并预热，再 ABBA 交替。
  // 不重启的话，同一进程实例的内存布局/分配器状态会带来固定的几个百分点偏差，各轮并不独立，
  // 置换检验会把它判成显著差异（同一产物 A/A 实测出现过 +4.5% 的“B 更慢”）。
  const runs: Record<string, Record<string, RunResult[]>> = {};
  for (const s of scenarios) runs[s.name] = Object.fromEntries(targets.map((t) => [t.label, []]));
  for (let round = 0; round < config.rounds; round++) {
    for (const t of targets) await stopServer(t);
    for (const t of targets) await startServer(t, `r${round + 1}`);
    for (const t of targets) {
      for (const s of scenarios) {
        const probed = await probe(t, s);
        probeBytes[s.name] ??= {};
        probeBytes[s.name][t.label] = probed.bytes;
        await oha(t, s, `warmup${round + 1}`, config.warmupSecs, rateOf[s.name], probed);
      }
    }
    const order = round % 2 === 0 ? targets : [...targets].reverse();
    for (const s of scenarios) {
      const rate = rateOf[s.name];
      for (const t of order) {
        const probed = await probe(t, s);
        probeBytes[s.name][t.label] = probed.bytes;
        const json = await oha(t, s, `round${round + 1}`, config.durationSecs, rate, probed);
        const ms = (key: string) => {
          const value = json.latencyPercentiles[key];
          if (typeof value !== 'number')
            throw new Error(`${t.label} ${s.name}：oha 缺少 latencyPercentiles.${key}`);
          return round3(value * 1000);
        };
        const rps = json.summary.requestsPerSec;
        const result: RunResult = {
          round: round + 1,
          p50: ms('p50'),
          p90: ms('p90'),
          p99: ms('p99'),
          p999: ms('p99.9'),
          rps: round3(rps),
          bytesPerRequest: json.summary.sizePerRequest,
          saturated: rps < 0.95 * rate,
        };
        if (result.saturated) {
          console.warn(
            `警告：${t.label} ${s.name} 第 ${round + 1} 轮 rps=${Math.round(rps)} < 95% × rate ${rate}，已饱和`,
          );
        }
        runs[s.name][t.label].push(result);
      }
    }
    console.log(`第 ${round + 1}/${config.rounds} 轮完成`);
  }

  // ---------- 汇总 ----------

  const summarize = (list: RunResult[]) =>
    Object.fromEntries(
      METRICS.map((m) => {
        const values = list.map((r) => r[m]);
        return [
          m,
          { median: round3(median(values)), min: Math.min(...values), max: Math.max(...values) },
        ];
      }),
    ) as Record<Metric, { median: number; min: number; max: number }>;

  const scenarioResults = scenarios.map((s) => {
    const summary = Object.fromEntries(
      targets.map((t) => [t.label, summarize(runs[s.name][t.label])]),
    );
    let comparison: Record<string, { deltaPct: number; p: number; verdict: string }> | undefined;
    if (targets.length === 2) {
      comparison = {};
      for (const metric of ['p50', 'p99'] as const) {
        const a = runs[s.name].A.map((r) => r[metric]);
        const b = runs[s.name].B.map((r) => r[metric]);
        const medianA = median(a);
        const deltaPct = ((median(b) - medianA) / medianA) * 100;
        const p = mannWhitneyP(a, b);
        const verdict =
          p < 0.05 && Math.abs(deltaPct) >= 3 ? (deltaPct < 0 ? 'B 更快' : 'B 更慢') : '无显著差异';
        comparison[metric] = { deltaPct: round3(deltaPct), p: round3(p), verdict };
      }
    }
    return {
      name: s.name,
      url: s.url,
      mode: s.mode,
      route: s.route,
      rate: rateOf[s.name],
      capacityRps: capacity[s.name],
      probeBytes: probeBytes[s.name],
      runs: runs[s.name],
      summary,
      comparison,
    };
  });

  const results = {
    meta: {
      startedAt: new Date(Date.now()).toISOString(),
      machine: {
        cpu: os.cpus()[0].model,
        kernel: os.release(),
        governors,
        boost: readText('/sys/devices/system/cpu/cpufreq/boost'),
        loadavg,
      },
      cpus: config.cpus,
      tools: { bun: Bun.version, oha: ohaVersion },
      pgImage: PG_IMAGE,
      config: {
        rounds: config.rounds,
        durationSecs: config.durationSecs,
        warmupSecs: config.warmupSecs,
        calibrationSecs: config.calibrationSecs,
        loadFactor: config.loadFactor,
        connections: config.connections,
        only: config.only || null,
        ratesFrom: config.ratesFrom || null,
      },
    },
    targets: targets.map((t) => ({
      label: t.label,
      dir: t.dir,
      releaseId: t.releaseId,
      gitHash: t.gitHash,
      dirty: t.dirty,
    })),
    scenarios: scenarioResults,
  };
  fs.writeFileSync(path.join(artifacts, 'results.json'), `${JSON.stringify(results, null, 2)}\n`);

  const ms = (n: number) => n.toFixed(3);
  const lines: string[] = [];
  if (targets.length === 1) {
    lines.push(
      '| 场景 | rate | p50 (ms) | p99 (ms) | p99.9 (ms) | rps |',
      '|---|---:|---:|---:|---:|---:|',
    );
    for (const r of scenarioResults) {
      const a = r.summary.A;
      lines.push(
        `| ${r.name} | ${r.rate} | ${ms(a.p50.median)} | ${ms(a.p99.median)} | ${ms(a.p999.median)} | ${a.rps.median.toFixed(1)} |`,
      );
    }
  } else {
    lines.push(
      '| 场景 | rate | A p50 | B p50 | Δp50 | A p99 | B p99 | Δp99 | 判定 |',
      '|---|---:|---:|---:|---:|---:|---:|---:|---|',
    );
    for (const r of scenarioResults) {
      const c = r.comparison as NonNullable<typeof r.comparison>;
      const sign = (n: number) => `${n > 0 ? '+' : ''}${n.toFixed(1)}%`;
      const verdict =
        c.p50.verdict === c.p99.verdict
          ? c.p50.verdict
          : `p50 ${c.p50.verdict} / p99 ${c.p99.verdict}`;
      lines.push(
        `| ${r.name} | ${r.rate} | ${ms(r.summary.A.p50.median)} | ${ms(r.summary.B.p50.median)} | ${sign(c.p50.deltaPct)} | ${ms(r.summary.A.p99.median)} | ${ms(r.summary.B.p99.median)} | ${sign(c.p99.deltaPct)} | ${verdict} |`,
      );
    }
  }
  console.log(`\n${lines.join('\n')}\n\n产物目录：${artifacts}`);
}

main()
  .catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  })
  .finally(cleanup);
