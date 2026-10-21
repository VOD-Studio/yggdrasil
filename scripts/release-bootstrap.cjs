// dx 0.7 的入口 JS 才包含 WASM URL；只跟随当前模板的 module 入口，不扫描历史产物。
const LOCAL_ASSET = /^\/(?:\.\/)?(?:assets|wasm)\//;

function attributes(tag) {
  const result = new Map();
  const re = /(?:^|\s)([^\s=/>]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/g;
  for (const match of tag.matchAll(re)) {
    result.set(match[1].toLowerCase(), match[2] ?? match[3] ?? match[4] ?? '');
  }
  return result;
}

function localAsset(ref) {
  if (!LOCAL_ASSET.test(ref)) throw new Error(`入口必须引用本地 assets/ 或 wasm/ 资源：${ref}`);
  const url = new URL(ref, 'http://release.invalid');
  if (!LOCAL_ASSET.test(url.pathname) || url.search || url.hash) {
    throw new Error(`不支持的客户端资源 URL：${ref}`);
  }
  return url.pathname;
}

export function bootstrapRefs(html) {
  const refs = new Set();
  for (const tag of html.matchAll(/<(?:script|link)\b[^>]*>/gi)) {
    const attrs = attributes(tag[0]);
    const ref = attrs.get('src') ?? attrs.get('href');
    if (ref && LOCAL_ASSET.test(ref)) refs.add(localAsset(ref));
  }
  return [...refs].sort();
}

export function wasmBootstrap(html, readAsset) {
  const entries = new Set();
  for (const tag of html.matchAll(/<script\b[^>]*>/gi)) {
    const attrs = attributes(tag[0]);
    const src = attrs.get('src');
    if (attrs.get('type') === 'module' && src && LOCAL_ASSET.test(src)) {
      entries.add(localAsset(src));
    }
  }
  if (entries.size !== 1) {
    throw new Error(`预期一个 dx module 入口，实际为 ${entries.size} 个`);
  }
  const [entry] = entries;
  const js = readAsset(entry);
  const wasm = new Set();
  for (const match of js.matchAll(/\bmodule_or_path\s*:\s*(["'])([^"']+\.wasm)\1/g)) {
    wasm.add(localAsset(match[2]));
  }
  if (wasm.size !== 1) {
    throw new Error(`入口 ${entry} 必须明确引用一个 WASM，实际为 ${wasm.size} 个`);
  }
  return { entry, wasm: [...wasm][0] };
}

function wasmPreloads(html) {
  const links = [];
  for (const tag of html.matchAll(/<link\b[^>]*>/gi)) {
    const attrs = attributes(tag[0]);
    const href = attrs.get('href') ?? '';
    const rel = (attrs.get('rel') ?? '').toLowerCase().split(/\s+/);
    if (rel.includes('preload') && (attrs.get('type') === 'application/wasm' || /\.wasm(?:[?#]|$)/.test(href))) {
      links.push({ tag: tag[0], attrs });
    }
  }
  return links;
}

export function verifyWasmPreload(html, wasm) {
  const head = html.match(/<head\b[^>]*>([\s\S]*?)<\/head\s*>/i);
  if (!head) throw new Error('模板缺少完整 head');
  const links = wasmPreloads(html);
  if (links.length !== 1) throw new Error(`预期一个 WASM preload，实际为 ${links.length} 个`);
  const { tag, attrs } = links[0];
  if (!head[1].includes(tag)
    || localAsset(attrs.get('href') ?? '') !== wasm
    || attrs.get('as') !== 'fetch'
    || attrs.get('type') !== 'application/wasm'
    || !attrs.has('crossorigin')
    || !['', 'anonymous'].includes(attrs.get('crossorigin'))) {
    throw new Error('WASM preload 的 URL、head 位置或 fetch/CORS 属性不匹配');
  }
}

export function preloadWasm(html, readAsset) {
  const { wasm } = wasmBootstrap(html, readAsset);
  if (wasmPreloads(html).length) {
    verifyWasmPreload(html, wasm);
    return html;
  }
  if (!/<head\b[^>]*>/i.test(html)) throw new Error('模板缺少 head');
  // 与 wasm-bindgen 的 fetch(url) 匹配：CORS + same-origin credentials。
  const link = `<link rel="preload" as="fetch" type="application/wasm" href="${wasm}" crossorigin="anonymous">`;
  const result = html.replace(/<head\b[^>]*>/i, (head) => `${head}\n        ${link}`);
  verifyWasmPreload(result, wasm);
  return result;
}
