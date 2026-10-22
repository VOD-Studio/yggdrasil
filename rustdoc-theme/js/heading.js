// 标题区：源码位置、本地化文字、crate 首页元信息。

const SRC_RE = /\/src\/[^/]+\/(.+)\.html#(\d+)(?:-(\d+))?$/;

function localizeSource() {
  const link = document.querySelector('.main-heading .sub-heading a.src');
  if (link) {
    const match = SRC_RE.exec(link.getAttribute('href') || '');
    if (match) {
      const meta = document.createElement('span');
      meta.className = 'doc-src-meta';
      meta.textContent = match[3]
        ? `${match[1]}:${match[2]}–${match[3]}`
        : `${match[1]}:${match[2]}`;
      link.before(meta);
    }
    link.textContent = '源码';
  }
  document.querySelectorAll('a.src.rightside').forEach((a) => {
    a.textContent = '源码';
  });
}

function setTitle(selector, title) {
  const el = document.querySelector(selector);
  if (el) el.setAttribute('title', title);
}

function addHeroMeta() {
  if (!document.body.classList.contains('crate')) return;
  const heading = document.querySelector('.main-heading');
  if (!heading) return;
  const versionEl = document.querySelector('.sidebar-crate .version');
  const version = versionEl ? versionEl.textContent.trim() : '';
  const modules = document.getElementById('modules');
  let count = 0;
  if (modules) {
    let node = modules.nextElementSibling;
    while (node && !(node.matches && node.matches('dl.item-table'))) {
      node = node.nextElementSibling;
    }
    if (node) count = node.querySelectorAll(':scope > dt').length;
  }
  const p = document.createElement('p');
  p.className = 'doc-hero-meta';
  p.textContent = `${version ? `v${version} · ` : ''}${count} 个模块`;
  heading.prepend(p);
}

export function localizeHeading() {
  localizeSource();
  setTitle('#copy-path', '复制条目路径');
  setTitle('#toggle-all-docs', '折叠/展开全部说明（Shift 同时折叠 impl）');
  setTitle('.settings-menu > a', '设置');
  setTitle('.help-menu > a', '帮助');
  addHeroMeta();
  const h1 = document.querySelector('.main-heading h1');
  if (h1 && h1.textContent.trim() === 'List of all items') {
    h1.textContent = '全部条目';
  }
}
