import { initTocSidebar } from '../../libs/yggdrasil-core/src/toc-sidebar.ts';

const X_ICON =
  '<svg class="lucide" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>';

function collectEntries() {
  const entries = [];
  for (const h of document.querySelectorAll('#main-content > h2.section-header[id]')) {
    const clone = h.cloneNode(true);
    for (const a of clone.querySelectorAll('a.anchor, .doc-count')) a.remove();
    const text = clone.textContent.trim();
    if (text) entries.push({ id: h.id, text, el: h });
  }
  const more = document.getElementById('more-impls');
  if (more) {
    const entry = { id: 'more-impls', text: '自动与通用实现', el: more };
    const at = entries.findIndex(
      (e) => more.compareDocumentPosition(e.el) & Node.DOCUMENT_POSITION_FOLLOWING,
    );
    if (at === -1) entries.push(entry);
    else entries.splice(at, 0, entry);
  }
  return entries;
}

function buildToc(entries) {
  const root = document.createElement('div');
  root.className = 'post-toc';
  root.setAttribute('data-toc-root', 'true');

  const items = entries
    .map((e) => {
      const li = document.createElement('li');
      const a = document.createElement('a');
      a.setAttribute('href', `#${e.id}`);
      a.textContent = e.text;
      li.append(a);
      return li.outerHTML;
    })
    .join('');

  root.innerHTML =
    '<div class="toc-progress-line" aria-hidden="true"></div>' +
    '<button id="doc-toc-pill" class="toc-pill" type="button" aria-expanded="false" aria-controls="doc-toc">' +
    '<svg class="toc-pill-ring" viewBox="0 0 22 22" aria-hidden="true"><circle cx="11" cy="11" r="8.5"></circle><circle class="toc-pill-ring-value" cx="11" cy="11" r="8.5" pathLength="100"></circle></svg>' +
    '<span class="toc-pill-label"><span class="toc-pill-title">本页</span><span class="toc-pill-current" data-toc-current="true"></span></span>' +
    '<span class="toc-pill-percent tabular-nums" data-toc-percent="true"></span>' +
    '</button>' +
    '<div class="toc-scrim" aria-hidden="true"></div>' +
    '<nav id="doc-toc" class="toc-sidebar" aria-label="本页" tabindex="-1">' +
    '<div class="toc-sidebar-head"><span class="toc-sidebar-title">本页</span>' +
    `<button class="toc-sidebar-close" type="button" aria-label="关闭目录">${X_ICON}</button></div>` +
    `<div class="toc-sidebar-body scroll-fade-y"><ul>${items}</ul></div>` +
    '<div class="toc-sidebar-progress"><div class="toc-sidebar-progress-text"><span>已读</span><span class="tabular-nums" data-toc-percent="true"></span></div>' +
    '<div class="toc-sidebar-progress-track" aria-hidden="true"><div class="toc-sidebar-progress-bar"></div></div></div>' +
    '</nav>';
  return root;
}

export function initToc() {
  const limiter = document.querySelector('.width-limiter');
  if (!limiter) return;
  const entries = collectEntries();
  if (entries.length < 2) return;

  const root = buildToc(entries);
  limiter.append(root);
  limiter.classList.add('post-layout');

  const pill = root.querySelector('#doc-toc-pill');
  const nav = root.querySelector('#doc-toc');

  const setOpen = (open, restoreFocus) => {
    root.classList.toggle('is-open', open);
    pill.setAttribute('aria-expanded', String(open));
    if (open) nav.focus({ preventScroll: true });
    else if (restoreFocus) pill.focus({ preventScroll: true });
  };

  pill.addEventListener('click', () => setOpen(!root.classList.contains('is-open'), false));
  root.querySelector('.toc-scrim').addEventListener('click', () => setOpen(false, true));
  root.querySelector('.toc-sidebar-close').addEventListener('click', () => setOpen(false, true));
  for (const a of nav.querySelectorAll('.toc-sidebar-body a')) {
    a.addEventListener('click', () => setOpen(false, false));
  }
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && root.classList.contains('is-open')) {
      e.preventDefault();
      setOpen(false, true);
    }
  });

  initTocSidebar('#doc-toc', '#main-content');
}
