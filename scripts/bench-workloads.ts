import assert from 'node:assert/strict';
import { median, summarize } from './bench-statistics';

export const NOTES_ROUTE = '/notes';
export const NOTEBOOK_ROUTE = '/notes/book/1';
const NOTEBOOK_PAGE = '/api/list_notebook_entries';
const SEARCH_ROUTE = '/api/search_posts';
const UPDATE_ROUTE = '/api/update_post';
const REFILL_ROUTE = '/post/bench-refill';
const SEARCH_GROUPS = 9;
const SEARCH_POSTS = 6_000;
const NOTE_COUNT = 44;
const BURST_SIZE = 16;
const SAVE_SAMPLES = 6;
const TAG_NAMES = Array.from({ length: 16 }, (_, i) => `bench-tag-${i + 1}`);
const NOTES_MARKER = '基准笔记正文';
const REFILL_MARKER = '基准回填正文';
const NOTE_MARKER = '基准公开笔记本';

export const WORKLOADS = [
  { name: 'notes /notes', kind: 'notes', route: NOTES_ROUTE },
  { name: 'notes /notes/book/1', kind: 'notes', route: NOTEBOOK_ROUTE },
  { name: 'notes /api/list_notebook_entries', kind: 'notes', route: NOTEBOOK_PAGE },
  { name: 'search /api/search_posts', kind: 'search', route: SEARCH_ROUTE },
  { name: 'write /api/update_post unchanged', kind: 'write', route: UPDATE_ROUTE, changed: false },
  { name: 'write /api/update_post changed', kind: 'write', route: UPDATE_ROUTE, changed: true },
  { name: 'cold /post/bench-refill', kind: 'cold', route: REFILL_ROUTE },
] as const;
export type Workload = (typeof WORKLOADS)[number];

export const FIXTURE_SCALE = {
  notes: NOTE_COUNT,
  searchPosts: SEARCH_POSTS,
  searchGroups: SEARCH_GROUPS,
  refillConcurrency: BURST_SIZE,
  tagWriteSamples: SAVE_SAMPLES,
};

export function workloadFixtures(workloads: Workload[], browser: boolean) {
  const has = (kind: Workload['kind']) => workloads.some((item) => item.kind === kind);
  const notes =
    (browser || has('notes')) &&
    `INSERT INTO notebooks(id, owner_id, title, is_public) VALUES(1, 1, '${NOTE_MARKER}', TRUE);
     INSERT INTO notes(id, owner_id, slug, version, published_version, published_at)
       SELECT g, 1, 'bench-note-'||g, 1, 1, NOW()-g*INTERVAL '1 minute'
       FROM generate_series(1, ${NOTE_COUNT}) g;
     INSERT INTO note_revisions(note_id, version, kind, title, content_md, content_html, toc_html, summary, tags, created_at)
       SELECT g, 1, CASE WHEN g%7=0 THEN 'topic' ELSE 'moment' END, '基准笔记 '||g,
         '${NOTES_MARKER}', '<p>${NOTES_MARKER} '||g||'</p>'||repeat('<p>重复段落用于测量笔记流传输。</p>', 45)||
         '<pre><code class="language-text">示例代码</code></pre>',
         CASE WHEN g%7=0 THEN '<ul><li><a href="#section">小节</a></li></ul>' ELSE '' END,
         '笔记摘要 '||g, ARRAY['基准'], NOW()-g*INTERVAL '1 minute'
       FROM generate_series(1, ${NOTE_COUNT}) g;
     INSERT INTO notebook_notes(notebook_id, note_id, position)
       SELECT 1, g, g FROM generate_series(1, ${NOTE_COUNT}) g;`;
  const tags =
    (has('search') || has('write')) &&
    `INSERT INTO tags(id, name) SELECT 100+g, 'bench-tag-'||g FROM generate_series(1, 100) g;`;
  const search =
    has('search') &&
    `INSERT INTO posts(id, author_id, title, slug, summary, content_md, content_html, status, published_at)
       SELECT 10000+g, 1, '基准搜索 benchsearch'||(g%${SEARCH_GROUPS})||' '||g, 'bench-search-'||g,
         '搜索摘要', repeat('代表性的正文检索语料与标点。', 24), '<p>搜索正文</p>',
         'published', NOW()-INTERVAL '90 days'-g*INTERVAL '1 second'
       FROM generate_series(1, ${SEARCH_POSTS}) g;
     INSERT INTO post_tags(post_id, tag_id)
       SELECT 10000+g, 101+(g+k)%100
       FROM generate_series(1, ${SEARCH_POSTS}) g CROSS JOIN generate_series(1, 8) k;`;
  const write =
    (has('write') || has('cold')) &&
    `INSERT INTO posts(id, author_id, title, slug, content_md, content_html, status, published_at)
       VALUES(70, 1, '基准标签保存', 'bench-tag-save', '正文。', '<p>正文。</p>', 'published', NOW()-INTERVAL '80 days');`;
  const writeTags =
    has('write') &&
    'INSERT INTO post_tags(post_id, tag_id) SELECT 70, 100+g FROM generate_series(1, 16) g;';
  const refill =
    has('cold') &&
    `INSERT INTO posts(id, author_id, title, slug, content_md, content_html, status, published_at)
       VALUES(71, 1, '${REFILL_MARKER}', 'bench-refill', '${REFILL_MARKER}',
         repeat('<p>${REFILL_MARKER}</p>', 15000), 'published', NOW()-INTERVAL '80 days');`;
  return [notes, tags, search, write, writeTags, refill].filter(Boolean).join('\n');
}

interface Sample {
  ttfbMs: number;
  elapsedMs: number;
  decodedBytes: number;
  cache: string | null;
}

export interface WorkloadRun {
  round: number;
  endpoint: string;
  samples: Sample[];
  sqlCalls: number;
  cacheMisses: number | null;
  burstMs: number | null;
  medianMs: number | null;
}

interface Target {
  base: string;
  sql: (source: string) => Promise<string>;
  token: string;
}

async function request(target: Target, route: string, input?: object, admin = false) {
  const headers: Record<string, string> = { 'Accept-Encoding': 'identity' };
  if (input) {
    headers.Origin = target.base;
    headers['Content-Type'] = 'application/json';
  }
  if (admin) headers.Cookie = `session=${target.token}`;
  const start = performance.now();
  const response = await fetch(target.base + route, {
    method: input ? 'POST' : 'GET',
    headers,
    body: input ? JSON.stringify(input) : undefined,
    signal: AbortSignal.timeout(30_000),
  });
  const ttfbMs = performance.now() - start;
  const body = await response.arrayBuffer();
  const sample: Sample = {
    ttfbMs,
    elapsedMs: performance.now() - start,
    decodedBytes: body.byteLength,
    cache: response.headers.get('x-ssr-cache'),
  };
  return { status: response.status, sample, text: () => new TextDecoder().decode(body) };
}

async function countQueries(target: Target, pattern: string) {
  return Number(
    await target.sql(
      `SELECT COALESCE(sum(calls), 0)::bigint FROM pg_stat_statements
      WHERE dbid=(SELECT oid FROM pg_database WHERE datname=current_database()) AND query ~* '${pattern}';`,
    ),
  );
}

async function saveTags(target: Target, tags: string[]) {
  const response = await request(
    target,
    UPDATE_ROUTE,
    {
      post_id: 70,
      title: '基准标签保存',
      slug: 'bench-tag-save',
      content_md: '正文。',
      status: 'published',
      tags,
      summary: null,
      cover_image: null,
    },
    true,
  );
  assert.equal(response.status, 200, UPDATE_ROUTE);
  assert.equal(JSON.parse(response.text()).success, true, UPDATE_ROUTE);
  return response.sample;
}

async function sampleNotes(target: Target, workload: Workload, samples: Sample[]) {
  const route = workload.route;
  const entries = route === NOTEBOOK_PAGE;
  const requestEntries = (endpoint: string) =>
    request(
      target,
      endpoint,
      endpoint === NOTEBOOK_PAGE
        ? { id: 1, page: 2 }
        : {
            filter: { notebook_id: 1, page: 2 },
          },
    );
  // 旧版本使用通用笔记列表接口。探测在计数前完成，并记录实际测量的接口。
  const legacy = entries && [404, 405].includes((await requestEntries(route)).status);
  const endpoint = legacy ? '/api/list_notes' : route;
  if (legacy) await requestEntries(endpoint);
  const pattern = 'FROM (notes n|notebooks b|notebook_notes bn)';
  const before = await countQueries(target, pattern);
  for (let i = 0; i < 4; i++) {
    const response = entries ? await requestEntries(endpoint) : await request(target, route);
    assert.equal(response.status, 200, endpoint);
    if (entries) {
      const body = JSON.parse(response.text());
      assert.equal(body.notes.length, 20, endpoint);
      assert.equal(body.total, NOTE_COUNT, endpoint);
    } else {
      assert(response.text().includes(route === NOTES_ROUTE ? NOTES_MARKER : NOTE_MARKER), route);
    }
    samples.push(response.sample);
  }
  return { endpoint, sqlCalls: (await countQueries(target, pattern)) - before };
}

async function sampleSearch(target: Target, samples: Sample[]) {
  const search = async (group: number) => {
    const response = await request(target, SEARCH_ROUTE, { query: `benchsearch${group}` });
    assert.equal(response.status, 200, SEARCH_ROUTE);
    const body = JSON.parse(response.text());
    assert.equal(body.posts.length, 50, SEARCH_ROUTE);
    assert(
      body.posts.every(
        (post: { slug: string; tags: string[] }) =>
          post.slug.startsWith('bench-search-') && post.tags.length === 8,
      ),
      SEARCH_ROUTE,
    );
    return response.sample;
  };
  await search(0);
  const pattern = 'word_similarity';
  const before = await countQueries(target, pattern);
  for (let i = 1; i < SEARCH_GROUPS; i++) samples.push(await search(i));
  const calls = (await countQueries(target, pattern)) - before;
  assert.equal(calls, samples.length, '每个搜索词应执行一次 PostgreSQL 检索');
  return calls;
}

async function sampleWrites(target: Target, changed: boolean, samples: Sample[]) {
  await saveTags(target, TAG_NAMES);
  const pattern =
    '^(INSERT INTO (post_tags|tags)|DELETE FROM post_tags|SELECT id FROM tags|SELECT t.name, t.id FROM tags)';
  const before = await countQueries(target, pattern);
  const alternate = [...TAG_NAMES.slice(0, -1), 'bench-tag-17'];
  let tags = TAG_NAMES;
  for (let i = 0; i < SAVE_SAMPLES; i++) {
    tags = changed && i % 2 === 0 ? alternate : TAG_NAMES;
    samples.push(await saveTags(target, tags));
  }
  const calls = (await countQueries(target, pattern)) - before;
  const saved = JSON.parse(
    await target.sql(
      'SELECT json_agg(t.name ORDER BY t.name) FROM tags t JOIN post_tags pt ON pt.tag_id=t.id WHERE pt.post_id=70;',
    ),
  );
  assert.deepEqual(saved, [...tags].sort());
  return calls;
}

async function sampleRefill(target: Target, samples: Sample[]) {
  const initial = await request(target, REFILL_ROUTE);
  assert.equal(initial.status, 200, REFILL_ROUTE);
  const warm = await request(target, REFILL_ROUTE);
  assert.equal(warm.sample.cache, 'HIT', REFILL_ROUTE);
  // 同一实例更新已发布的文章后同步失效公开 HTML。失效后直接发出并发请求。
  await saveTags(target, []);
  const pattern = 'FROM posts';
  const before = await countQueries(target, pattern);
  const start = performance.now();
  const responses = await Promise.all(
    Array.from({ length: BURST_SIZE }, () => request(target, REFILL_ROUTE)),
  );
  const burstMs = performance.now() - start;
  const calls = (await countQueries(target, pattern)) - before;
  for (const response of responses) {
    assert.equal(response.status, 200, REFILL_ROUTE);
    assert(['HIT', 'MISS'].includes(response.sample.cache ?? ''), REFILL_ROUTE);
    assert(response.text().includes(REFILL_MARKER), REFILL_ROUTE);
    samples.push(response.sample);
  }
  const cacheMisses = samples.filter((sample) => sample.cache === 'MISS').length;
  assert(cacheMisses > 0, '并发回填必须从失效状态开始');
  return { cacheMisses, burstMs, sqlCalls: calls };
}

export async function measureWorkload(
  target: Target,
  workload: Workload,
  round: number,
): Promise<WorkloadRun> {
  const samples: Sample[] = [];
  let endpoint: string = workload.route;
  let sqlCalls: number;
  let cacheMisses = null;
  let burstMs = null;
  switch (workload.kind) {
    case 'notes':
      ({ endpoint, sqlCalls } = await sampleNotes(target, workload, samples));
      break;
    case 'search':
      sqlCalls = await sampleSearch(target, samples);
      break;
    case 'write':
      sqlCalls = await sampleWrites(target, workload.changed, samples);
      break;
    case 'cold':
      ({ cacheMisses, burstMs, sqlCalls } = await sampleRefill(target, samples));
      break;
  }
  return {
    round,
    endpoint,
    samples,
    sqlCalls,
    cacheMisses,
    burstMs,
    medianMs: median(samples.map((sample) => sample.elapsedMs)),
  };
}

export function summarizeWorkload(runs: WorkloadRun[]) {
  return {
    elapsedMs: summarize(runs.map((run) => run.medianMs)),
    ttfbMs: summarize(runs.map((run) => median(run.samples.map((sample) => sample.ttfbMs)))),
    decodedBytes: summarize(
      runs.map((run) => median(run.samples.map((sample) => sample.decodedBytes))),
    ),
    sqlCallsPerRequest: summarize(runs.map((run) => run.sqlCalls / run.samples.length)),
    cacheMisses: summarize(runs.map((run) => run.cacheMisses)),
    burstMs: summarize(runs.map((run) => run.burstMs)),
  };
}
