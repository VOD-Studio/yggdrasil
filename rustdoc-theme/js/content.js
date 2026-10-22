// 正文块：章节标题、折叠摘要、Trait chip、更多实现、crate 模块分组、hash 定位。

const SECTION_LABELS = {
  fields: '字段',
  variants: '变体',
  implementations: '方法',
  'trait-implementations': '实现的 Trait',
  'synthetic-implementations': '自动实现',
  'blanket-implementations': '通用实现',
  modules: '模块',
  structs: '结构体',
  enums: '枚举',
  functions: '函数',
  constants: '常量',
  implementors: '实现者',
  'synthetic-implementors': '自动实现者',
  'aliased-type': '原类型',
  statics: '静态变量',
  types: '类型别名',
  macros: '宏',
  traits: 'Trait',
  unions: '联合体',
  reexports: '重导出',
  'required-methods': '必需方法',
  'provided-methods': '默认方法',
  'required-associated-types': '必需关联类型',
  'provided-associated-types': '默认关联类型',
  'required-associated-consts': '必需关联常量',
  'provided-associated-consts': '默认关联常量',
  'foreign-impls': '外部类型实现',
};

const SHOW_UNITS = { field: '字段', variant: '变体', item: '条目', method: '方法' };

const MODULE_GROUPS = [
  [
    '界面',
    ['pages', 'components', 'hooks', 'router', 'navigation', 'context', 'theme', 'bridges'],
  ],
  [
    '服务端',
    [
      'api',
      'auth',
      'db',
      'cache',
      'config',
      'middleware',
      'mcp',
      'og',
      'ssr_cache',
      'startup',
      'tasks',
      'highlight',
      'infra',
    ],
  ],
  ['共享', ['models', 'utils', 'build_info']],
  ['其他', []],
];

const CHEVRON_RIGHT =
  '<svg class="lucide doc-more-chevron" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="m9 18 6-6-6-6"/></svg>';

const TRAIT_LISTS = '#trait-implementations-list, #synthetic-implementations-list';

function hashTarget() {
  if (!location.hash) return null;
  let id = location.hash.slice(1);
  try {
    id = decodeURIComponent(id);
  } catch {
    // 保持原样
  }
  return id ? document.getElementById(id) : null;
}

function count(selector, root) {
  return (root || document).querySelectorAll(selector).length;
}

function sectionCount(h2) {
  const next = h2.nextElementSibling;
  switch (h2.id) {
    case 'fields':
      return count('#main-content > span.structfield');
    case 'variants':
      return count('.variants > section.variant');
    case 'implementations':
      return count('#implementations-list section.method');
    case 'trait-implementations':
    case 'blanket-implementations': {
      const list = document.getElementById(`${h2.id}-list`);
      return list ? list.children.length : 0;
    }
    case 'synthetic-implementations':
      return count('#synthetic-implementations-list > section.impl');
    default:
      if (next?.matches('dl.item-table')) return count(':scope > dt', next);
      return 0;
  }
}

function localizeSections() {
  for (const h2 of document.querySelectorAll('#main-content h2.section-header[id]')) {
    const label = SECTION_LABELS[h2.id];
    const text = Array.from(h2.childNodes).find((n) => n.nodeType === Node.TEXT_NODE);
    if (label && text && text.nodeValue !== label) text.nodeValue = label;
    if (h2.querySelector(':scope > .doc-count')) continue;
    const n = sectionCount(h2);
    if (!n) continue;
    const badge = document.createElement('span');
    badge.className = 'doc-count';
    badge.textContent = String(n);
    const anchor = h2.querySelector(':scope > a.anchor');
    h2.insertBefore(badge, anchor);
  }
}

function localizeSummaries() {
  for (const span of document.querySelectorAll('summary.hideme > span')) {
    const raw = span.textContent.trim();
    let next = raw;
    if (raw === 'Expand description') {
      next = '展开说明';
    } else {
      const m = /^Show (\d+) (field|variant|item|method)s?$/.exec(raw);
      if (m) next = `展开 ${m[1]} 个${SHOW_UNITS[m[2]]}`;
    }
    if (next !== raw) span.textContent = next;
  }
}

function chipSections() {
  const result = [];
  for (const list of document.querySelectorAll(TRAIT_LISTS)) {
    for (const child of list.children) {
      const isDetails = child.tagName === 'DETAILS';
      const section = isDetails ? child.querySelector(':scope > summary > section.impl') : child;
      if (!section?.matches('section.impl')) continue;
      result.push({ el: child, section, isDetails });
    }
  }
  return result;
}

function collapseChips() {
  const target = hashTarget();
  for (const { el, isDetails } of chipSections()) {
    if (isDetails && !(target && el.contains(target))) el.removeAttribute('open');
  }
}

function setupChips() {
  for (const { section } of chipSections()) {
    const header = section.querySelector('h3.code-header');
    if (!header) continue;
    const full = header.textContent.replace(/\s+/g, ' ').trim();
    const trait = header.querySelector('a.trait');
    const short = trait ? trait.textContent.trim() : full;
    section.setAttribute('data-short', short);
    section.setAttribute('title', full);
  }
  collapseChips();
  window.addEventListener('load', collapseChips);
}

function groupMoreImpls() {
  const synthetic = document.getElementById('synthetic-implementations');
  const blanket = document.getElementById('blanket-implementations');
  const first = synthetic || blanket;
  if (!first || document.getElementById('more-impls')) return;

  const details = document.createElement('details');
  details.className = 'doc-more-impls';
  details.id = 'more-impls';

  const parts = [];
  const syntheticCount = count('#synthetic-implementations-list > section.impl');
  const blanketList = document.getElementById('blanket-implementations-list');
  const blanketCount = blanketList ? blanketList.children.length : 0;
  if (syntheticCount) parts.push(`自动实现 ${syntheticCount}`);
  if (blanketCount) parts.push(`通用实现 ${blanketCount}`);

  const summary = document.createElement('summary');
  summary.innerHTML = CHEVRON_RIGHT;
  const label = document.createElement('span');
  label.textContent = parts.join(' · ');
  summary.append(label);
  details.append(summary);

  first.before(details);
  for (const id of ['synthetic-implementations', 'synthetic-implementations-list', 'blanket-implementations', 'blanket-implementations-list']) {
    const el = document.getElementById(id);
    if (el) details.append(el);
  }
}

export function localizeContent() {
  localizeSections();
  localizeSummaries();
  setupChips();
  groupMoreImpls();
}

export function groupCrateModules() {
  if (!document.body.classList.contains('crate')) return;
  const modules = document.getElementById('modules');
  const table = modules?.nextElementSibling;
  if (!table?.matches('dl.item-table')) return;

  const buckets = MODULE_GROUPS.map(() => []);
  const other = MODULE_GROUPS.length - 1;
  for (const dt of table.querySelectorAll(':scope > dt')) {
    const link = dt.querySelector('a.mod');
    const name = link ? link.textContent.replace(/\s+/g, '') : '';
    let index = MODULE_GROUPS.findIndex(([, names]) => names.includes(name));
    if (index < 0) index = other;
    const dd = dt.nextElementSibling?.tagName === 'DD' ? dt.nextElementSibling : null;
    buckets[index].push(dd ? [dt, dd] : [dt]);
  }

  const wrap = document.createElement('div');
  wrap.className = 'doc-module-groups';
  MODULE_GROUPS.forEach(([title], i) => {
    if (!buckets[i].length) return;
    const section = document.createElement('section');
    section.className = 'doc-module-group';
    const h3 = document.createElement('h3');
    h3.append(title);
    const badge = document.createElement('span');
    badge.className = 'doc-count';
    badge.textContent = String(buckets[i].length);
    h3.append(badge);
    const dl = document.createElement('dl');
    dl.className = 'item-table';
    for (const nodes of buckets[i]) dl.append(...nodes);
    section.append(h3, dl);
    wrap.append(section);
  });
  table.replaceWith(wrap);
}

function reveal() {
  const target = hashTarget();
  if (!target) return;
  const ancestors = [];
  for (let el = target.parentElement; el; el = el.parentElement) {
    if (el.tagName === 'DETAILS') ancestors.push(el);
  }
  // Chrome 会在锚点跳转时自己展开 details，展开动画还没跑完它就按中间高度滚动。
  // 先关掉过渡（会让进行中的过渡直接到终点），展开后立即滚动，并在下一帧再对齐一次。
  for (const el of ancestors) {
    el.setAttribute('data-instant', '');
    el.open = true;
  }
  target.scrollIntoView();
  if (ancestors.length) {
    requestAnimationFrame(() => {
      target.scrollIntoView();
      requestAnimationFrame(() => ancestors.forEach((el) => el.removeAttribute('data-instant')));
    });
  }
}

export function initHashReveal() {
  reveal();
  window.addEventListener('hashchange', reveal);
}
