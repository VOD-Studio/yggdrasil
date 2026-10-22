// 左栏：标签翻译、子导航、模块筛选框。#rustdoc-modnav 由 main.js 动态生成，需等待。

const LABELS = {
  Modules: '模块',
  Structs: '结构体',
  Enums: '枚举',
  Functions: '函数',
  Constants: '常量',
  Statics: '静态变量',
  'Type Aliases': '类型别名',
  Macros: '宏',
  Traits: 'Trait',
  Unions: '联合体',
  'Re-exports': '重导出',
  Crates: 'Crate',
  'All Items': '全部条目',
};

const SEARCH_ICON =
  '<svg class="lucide" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="m21 21-4.34-4.34"/><circle cx="11" cy="11" r="8"/></svg>';

function translate(el) {
  const key = el.textContent.trim();
  if (Object.prototype.hasOwnProperty.call(LABELS, key)) {
    el.textContent = LABELS[key];
  }
}

function translateModnavHeading(h2) {
  const text = h2.textContent.trim();
  if (!text.startsWith('In ')) return;
  let rest = text.slice(3);
  if (rest.startsWith('crate ')) {
    rest = rest.slice(6);
  } else {
    rest = rest.split('::').join(' / ');
  }
  h2.textContent = rest;
}

function addSubnav(modnav) {
  const body = document.body;
  if (!body.classList.contains('mod') || body.classList.contains('sys')) return;
  const current = modnav.querySelector('li.current');
  if (!current || current.querySelector('.doc-subnav')) return;
  const links = document.querySelectorAll('#main-content .item-table > dt > a.mod');
  if (links.length === 0) return;
  const ul = document.createElement('ul');
  ul.className = 'doc-subnav';
  links.forEach((link) => {
    const li = document.createElement('li');
    const a = document.createElement('a');
    a.setAttribute('href', link.getAttribute('href'));
    a.textContent = link.textContent;
    li.appendChild(a);
    ul.appendChild(li);
  });
  current.appendChild(ul);
}

function applyFilter(query) {
  const q = query.toLowerCase().trim();
  const roots = document.querySelectorAll('#rustdoc-modnav, .sidebar-elems > ul.block');
  roots.forEach((root) => {
    const lists = root.matches('ul') ? [root] : Array.from(root.querySelectorAll('ul'));
    lists.forEach((ul) => {
      if (ul.classList.contains('doc-subnav')) return;
      let any = false;
      Array.from(ul.children).forEach((li) => {
        if (li.tagName !== 'LI') return;
        const own = li.querySelector(':scope > a');
        let match = q === '' || (own ? own : li).textContent.toLowerCase().includes(q);
        const sub = li.querySelector(':scope > .doc-subnav');
        if (sub) {
          let subAny = false;
          Array.from(sub.children).forEach((s) => {
            const subMatch = q === '' || s.textContent.toLowerCase().includes(q);
            s.hidden = !subMatch;
            if (subMatch) subAny = true;
          });
          sub.hidden = q !== '' && !subAny;
          if (subAny) match = true;
        }
        li.hidden = !match;
        if (match) any = true;
      });
      ul.hidden = q !== '' && !any;
      const prev = ul.previousElementSibling;
      if (prev && prev.tagName === 'H3') prev.hidden = ul.hidden;
    });
  });
}

function buildFilter() {
  const crate = document.querySelector('.sidebar-crate');
  if (!crate || document.getElementById('doc-filter-input')) return;
  const form = document.createElement('form');
  form.className = 'doc-filter';
  form.setAttribute('role', 'search');
  form.innerHTML =
    SEARCH_ICON +
    '<label class="doc-sr-only" for="doc-filter-input">筛选模块</label>' +
    '<input id="doc-filter-input" type="search" placeholder="筛选模块" autocomplete="off">';
  const input = form.querySelector('input');
  form.addEventListener('submit', (e) => e.preventDefault());
  input.addEventListener('input', () => applyFilter(input.value));
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      input.value = '';
      applyFilter('');
    }
  });
  crate.after(form);
}

function enhanceSidebar(modnav) {
  modnav.querySelectorAll('h3').forEach(translate);
  modnav.querySelectorAll('h2').forEach(translateModnavHeading);
  addSubnav(modnav);
}

export function initSidebar() {
  const allTypes = document.getElementById('all-types');
  if (allTypes) translate(allTypes);
  buildFilter();

  const modnav = document.getElementById('rustdoc-modnav');
  if (!modnav) return;
  if (modnav.querySelector('ul')) {
    enhanceSidebar(modnav);
    return;
  }
  const observer = new MutationObserver(() => {
    if (modnav.querySelector('ul')) {
      observer.disconnect();
      enhanceSidebar(modnav);
    }
  });
  observer.observe(modnav, { childList: true, subtree: true });
  window.addEventListener('load', () => observer.disconnect());
}
