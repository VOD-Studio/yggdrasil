// 站点顶栏与页脚交互：主题三档、搜索、侧栏开合、手机抽屉、回到顶部。
// 主题模型在 doc-boot.js（window.yggdrasilDocTheme），这里只负责按钮。

const SHEET_EXIT_MS = 200;

function themeApi() {
  return window.yggdrasilDocTheme;
}

function initThemeButtons() {
  const api = themeApi();
  const buttons = Array.from(document.querySelectorAll('[data-theme-choice]'));
  if (!api || buttons.length === 0) return;

  const refresh = () => {
    const current = api.mode();
    for (const b of buttons) {
      const on = b.dataset.themeChoice === current;
      b.setAttribute('aria-checked', String(on));
      b.tabIndex = on ? 0 : -1;
    }
  };

  for (const b of buttons) {
    b.addEventListener('click', () => {
      api.apply(b.dataset.themeChoice);
      refresh();
    });
    b.addEventListener('keydown', (e) => {
      if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
      const group = Array.from(b.parentElement.querySelectorAll('[data-theme-choice]'));
      const step = e.key === 'ArrowRight' ? 1 : -1;
      const next = group[(group.indexOf(b) + step + group.length) % group.length];
      e.preventDefault();
      next.focus();
      api.apply(next.dataset.themeChoice);
      refresh();
    });
  }
  refresh();
}

function openSearch() {
  if (window.searchState) window.searchState.focus();
}

function initSearchTriggers() {
  for (const el of document.querySelectorAll('[data-doc-action=search]')) {
    el.addEventListener('click', openSearch);
  }
  document.addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      openSearch();
    }
  });
}

function initSidebarToggle() {
  const btn = document.querySelector('[data-doc-action=sidebar]');
  if (!btn) return;
  const isSource = document.body.classList.contains('src');
  const sync = () => {
    const open = isSource
      ? document.documentElement.classList.contains('src-sidebar-expanded')
      : Boolean(document.querySelector('.sidebar.shown'));
    btn.setAttribute('aria-expanded', String(open));
  };
  btn.addEventListener('click', () => {
    const target = isSource
      ? document.querySelector('#sidebar-button > a')
      : document.querySelector('rustdoc-topbar .sidebar-menu-toggle');
    if (target) target.click();
    // main.js 的处理器同步切换 class；放到下一帧读，兼容异步实现。
    requestAnimationFrame(sync);
  });
  new MutationObserver(sync).observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class'],
  });
  const sidebar = document.querySelector('.sidebar');
  if (sidebar) {
    new MutationObserver(sync).observe(sidebar, { attributes: true, attributeFilter: ['class'] });
  }
}

function initSheet() {
  const trigger = document.getElementById('doc-nav-menu-trigger');
  const sheet = document.getElementById('doc-nav-menu');
  const scrim = document.querySelector('.sheet-scrim');
  if (!trigger || !sheet || !scrim) return;
  let closing = false;

  const open = () => {
    scrim.hidden = false;
    sheet.hidden = false;
    document.documentElement.classList.add('sheet-open');
    trigger.setAttribute('aria-expanded', 'true');
    const focus = sheet.querySelector('.sheet-nav-item[aria-current]');
    if (focus) focus.focus();
  };

  const close = () => {
    if (sheet.hidden || closing) return;
    closing = true;
    scrim.dataset.closing = 'true';
    sheet.dataset.closing = 'true';
    setTimeout(() => {
      scrim.hidden = true;
      sheet.hidden = true;
      delete scrim.dataset.closing;
      delete sheet.dataset.closing;
      document.documentElement.classList.remove('sheet-open');
      trigger.setAttribute('aria-expanded', 'false');
      trigger.focus();
      closing = false;
    }, SHEET_EXIT_MS);
  };

  trigger.addEventListener('click', open);
  scrim.addEventListener('click', close);
  sheet.querySelector('.sheet-handle')?.addEventListener('click', close);
  sheet.addEventListener('click', (e) => {
    if (e.target.closest('a')) close();
  });
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && !sheet.hidden) {
      e.preventDefault();
      close();
    }
  });
}

function initBackToTop() {
  const btn = document.querySelector('[data-doc-action=top]');
  if (!btn) return;
  btn.addEventListener('click', () => {
    const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
    window.scrollTo({ top: 0, behavior: reduce ? 'auto' : 'smooth' });
    history.replaceState(null, '', location.pathname + location.search);
  });
}

export function initHeader() {
  initThemeButtons();
  initSearchTriggers();
  initSidebarToggle();
  initSheet();
  initBackToTop();
}
