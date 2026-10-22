// rustdoc 主题引导：把博客顶栏三档（浅色/深色/跟随系统）映射到 rustdoc 的存储键与 html 属性。
// 在 storage.js 之后同步执行，首屏前完成，不闪烁。每次加载都强制按博客选择覆盖，
// 所以在别处选过的 ayu 会被覆盖。所有 storage 访问都包在 try/catch 里。
(function () {
  var html = document.documentElement;
  var KEY = 'yggdrasil-theme';
  var HIDE_CLASSES = ['hide-sidebar', 'hide-toc', 'hide-modnav', 'sans-serif-fonts'];

  function get(key) {
    try {
      return localStorage.getItem(key);
    } catch (e) {
      return null;
    }
  }

  function set(key, value) {
    try {
      localStorage.setItem(key, value);
    } catch (e) {}
  }

  function remove(key) {
    try {
      localStorage.removeItem(key);
    } catch (e) {}
  }

  function mode() {
    var v = get(KEY);
    return v === 'light' || v === 'dark' ? v : 'system';
  }

  function syncDark() {
    html.classList.toggle('dark', html.getAttribute('data-theme') === 'dark');
  }

  function apply(m) {
    if (m === 'system') {
      remove(KEY);
      set('rustdoc-use-system-theme', 'true');
      set('rustdoc-preferred-light-theme', 'light');
      set('rustdoc-preferred-dark-theme', 'dark');
    } else {
      set(KEY, m);
      set('rustdoc-use-system-theme', 'false');
      set('rustdoc-theme', m);
    }
    set('rustdoc-hide-sidebar', 'false');
    set('rustdoc-hide-toc', 'false');
    set('rustdoc-hide-modnav', 'false');
    set('rustdoc-sans-serif-fonts', 'false');
    for (var i = 0; i < HIDE_CLASSES.length; i++) html.classList.remove(HIDE_CLASSES[i]);
    html.setAttribute('data-theme-mode', m);
    try {
      if (typeof updateTheme === 'function') updateTheme();
    } catch (e) {}
    syncDark();
  }

  try {
    new MutationObserver(syncDark).observe(html, {
      attributes: true,
      attributeFilter: ['data-theme'],
    });
  } catch (e) {}

  window.yggdrasilDocTheme = { mode: mode, apply: apply };
  apply(mode());
})();
