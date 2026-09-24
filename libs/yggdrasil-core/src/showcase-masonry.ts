let disposeActive: (() => void) | undefined;

/** Keep catalog order while packing cards to their natural heights. */
export function initShowcaseMasonry(): void {
  disposeActive?.();
  const element = document.querySelector<HTMLElement>('.showcase-grid');
  if (!element || typeof ResizeObserver === 'undefined') return;
  const grid = element;

  let frame = 0;
  const cards = () => [...grid.querySelectorAll<HTMLElement>(':scope > .showcase-card')];
  const resize = new ResizeObserver(schedule);
  const mutations = new MutationObserver(() => {
    observeCards();
    schedule();
  });

  function layout(): void {
    frame = 0;
    const items = cards();
    const masonry = items.length > 0 && window.matchMedia('(min-width: 901px)').matches;
    grid.classList.toggle('is-masonry', masonry);
    if (!masonry) {
      for (const card of items) card.style.gridRowEnd = '';
      return;
    }

    const row = parseFloat(getComputedStyle(grid).gridAutoRows);
    const spans = items.map((card) =>
      Math.ceil(
        (card.getBoundingClientRect().height + parseFloat(getComputedStyle(card).marginBottom)) /
          row,
      ),
    );
    items.forEach((card, index) => {
      card.style.gridRowEnd = `span ${spans[index]}`;
    });
  }

  function schedule(): void {
    if (!frame) frame = requestAnimationFrame(layout);
  }

  function observeCards(): void {
    resize.disconnect();
    for (const card of cards()) {
      for (const child of card.children) resize.observe(child);
    }
  }

  layout();
  observeCards();
  mutations.observe(grid, { childList: true });
  window.addEventListener('resize', schedule);
  disposeActive = () => {
    cancelAnimationFrame(frame);
    resize.disconnect();
    mutations.disconnect();
    window.removeEventListener('resize', schedule);
    grid.classList.remove('is-masonry');
    for (const card of cards()) card.style.gridRowEnd = '';
    disposeActive = undefined;
  };
}

export function disposeShowcaseMasonry(): void {
  disposeActive?.();
}
