(() => {
  if (window.__yggNavigationEvents) return;
  const events = {};
  window.__yggNavigationEvents = events;
  window.addEventListener('popstate', (event) => events.pop?.(event), true);
  document.addEventListener('click', (event) => events.click?.(event));
  window.addEventListener('pagereveal', (event) => {
    if (!event.viewTransition || window.matchMedia?.('(prefers-reduced-motion: reduce)').matches)
      return;
    const wrapper = document.querySelector('[data-vt-route]') ?? document.documentElement;
    wrapper.setAttribute('data-vt-enter-handled', 'true');
  });
})();
