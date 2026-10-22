// 让 rustdoc 跟随博客主题（localStorage 'yggdrasil-theme'：'light' | 'dark' | 其余视为跟随系统）。
// 只在博客选择变化后同步一次：之后在 rustdoc 设置里另选的主题（如 ayu）会保留，
// 直到博客那边再次切换。在 storage.js 之后同步执行，首屏前完成，不闪烁。
(function () {
  try {
    var site = localStorage.getItem("yggdrasil-theme");
    if (site !== "light" && site !== "dark") site = "system";
    var mark = "yggdrasil-doc-theme-synced";
    if (localStorage.getItem(mark) === site) return;
    if (site === "system") {
      localStorage.setItem("rustdoc-use-system-theme", "true");
      localStorage.setItem("rustdoc-preferred-light-theme", "light");
      localStorage.setItem("rustdoc-preferred-dark-theme", "dark");
    } else {
      localStorage.setItem("rustdoc-use-system-theme", "false");
      localStorage.setItem("rustdoc-theme", site);
    }
    localStorage.setItem(mark, site);
    if (typeof updateTheme === "function") updateTheme();
  } catch (e) {}
})();
