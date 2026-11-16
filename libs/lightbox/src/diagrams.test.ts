import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { transformFor } from './geometry';
import './index';

const ns = 'http://www.w3.org/2000/svg';
const diagram = (theme: string) =>
  `<svg xmlns="${ns}" xmlns:xlink="http://www.w3.org/1999/xlink" id="chart-${theme}" viewBox="10 20 480 160" overflow="visible" style="color:${theme === 'dark' ? 'rgb(240, 240, 240)' : 'rgb(20, 20, 20)'}"><title>${theme} 流程</title><style>#chart-${theme} p{margin:0}</style><defs><marker id="arrow-${theme}"><path d="M0 0L5 5"/></marker></defs><path marker-end="url(#arrow-${theme})" stroke="currentColor"/><foreignObject x="20" y="30" width="300" height="80"><div xmlns="http://www.w3.org/1999/xhtml"><p>${theme} 中文 LongEnglishLabel</p><img src="/label.png"/></div></foreignObject><use xlink:href="#arrow-${theme}"/></svg>`;
function mount(): HTMLElement[] {
  document.body.innerHTML = `<main class="post-content" data-showcase-lightbox-owner="diagrams"><img src="/one.webp"><img src="/two.webp"><figure class="mermaid-diagram"><div class="mermaid-light"></div><div class="mermaid-dark"></div></figure><figure class="mermaid-diagram"></figure></main>`;
  const containers = document.querySelectorAll('.mermaid-light, .mermaid-dark, figure:last-child');
  ['light', 'dark', 'second'].forEach((theme, index) => {
    const svg = new DOMParser().parseFromString(diagram(theme), 'image/svg+xml').documentElement;
    containers[index].append(document.importNode(svg, true));
  });
  const figures = [...document.querySelectorAll<HTMLElement>('figure')];
  for (const figure of figures) {
    vi.spyOn(figure, 'getBoundingClientRect').mockReturnValue(new DOMRect(80, 90, 520, 200));
    for (const svg of figure.querySelectorAll('svg')) {
      vi.spyOn(svg, 'getBoundingClientRect').mockImplementation(() => {
        const dark = document.documentElement.classList.contains('dark');
        const hidden = svg.parentElement?.classList.contains(
          dark ? 'mermaid-light' : 'mermaid-dark',
        );
        return hidden ? new DOMRect() : new DOMRect(100, 110 - window.scrollY, 480, 160);
      });
    }
  }
  window.__initLightbox('.post-content');
  return figures;
}
function key(value: string): void {
  document.dispatchEvent(new KeyboardEvent('keydown', { key: value }));
}
function media(): SVGSVGElement {
  return document.querySelector<SVGSVGElement>('.lightbox-diagram')!;
}
function settle(): void {
  vi.advanceTimersByTime(400);
}
function close(): void {
  key('Escape');
  settle();
}

describe('流程图复用单张灯箱', () => {
  const blobs: Blob[] = [];
  const revoked = vi.fn();
  beforeEach(() => {
    vi.useFakeTimers();
    blobs.length = 0;
    revoked.mockClear();
    vi.stubGlobal('innerWidth', 1000);
    vi.stubGlobal('innerHeight', 800);
    Object.defineProperty(window, 'scrollY', { value: 0, configurable: true });
    vi.spyOn(URL, 'createObjectURL').mockImplementation((blob) => {
      blobs.push(blob as Blob);
      return `blob:diagram-${blobs.length}`;
    });
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(revoked);
  });
  afterEach(() => {
    window.__closeLightboxFor('diagrams');
    vi.runOnlyPendingTimers();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    vi.useRealTimers();
    document.documentElement.classList.remove('dark');
    document.body.innerHTML = '';
  });

  it.each(['light', 'dark'])('按打开时 %s 主题保留完整 SVG，下载不含灯箱变换', async (theme) => {
    document.documentElement.classList.toggle('dark', theme === 'dark');
    const [figure] = mount();
    const source = figure.innerHTML;
    figure.click();
    settle();
    expect(media().textContent).toContain(`${theme} 中文`);
    expect(media().querySelector('foreignObject div')?.namespaceURI).toBe(
      'http://www.w3.org/1999/xhtml',
    );
    expect(media().getAttribute('viewBox')).toBe('10 20 480 160');
    expect(media().getAttribute('overflow')).toBe('visible');
    expect(media().querySelector('style')?.textContent).toContain(`#chart-${theme}`);
    expect(media().style.color).toBe(theme === 'dark' ? 'rgb(240, 240, 240)' : 'rgb(20, 20, 20)');
    key('+');
    key('r');
    settle();
    const link = document.querySelector<HTMLAnchorElement>('[aria-label="下载 SVG"]')!;
    expect(link.download).toBe('diagram.svg');
    expect(link.getAttribute('href')).toBe('blob:diagram-1');
    expect(link.hasAttribute('target')).toBe(false);
    const xml = await blobs[0].text();
    const root = new DOMParser().parseFromString(xml, 'image/svg+xml').documentElement;
    expect(root.namespaceURI).toBe(ns);
    expect(root.getAttribute('width')).toBe('480');
    expect(root.getAttribute('height')).toBe('160');
    expect(root.querySelector('path[stroke]')?.getAttribute('stroke')).toBe('currentColor');
    expect(root.getAttribute('style')).toContain(
      theme === 'dark' ? 'rgb(240, 240, 240)' : 'rgb(20, 20, 20)',
    );
    expect(root.getAttribute('viewBox')).toBe('10 20 480 160');
    expect(root.getAttribute('style')).toContain('overflow-wrap: normal');
    expect(root.getAttribute('style')).not.toContain('transform');
    expect(root.querySelector('use')?.getAttributeNS('http://www.w3.org/1999/xlink', 'href')).toBe(
      `#arrow-${theme}`,
    );
    expect(root.querySelector('foreignObject p')?.textContent).toContain('中文 LongEnglishLabel');
    expect(figure.innerHTML).toBe(source);
    close();
    expect(revoked).toHaveBeenCalledExactlyOnceWith('blob:diagram-1');
    expect(figure.style.visibility).not.toBe('hidden');
    expect(document.activeElement).toBe(figure);
  });

  it('流程图独立打开，标签图片不进入正文图片图集', () => {
    const [first, second] = mount();
    first.click();
    settle();
    expect(document.querySelector('.lightbox-prev, .lightbox-next, .lightbox-thumb')).toBeNull();
    expect(document.querySelector<HTMLElement>('.lightbox-counter')?.style.display).toBe('none');
    const current = media();
    key('ArrowRight');
    key('ArrowLeft');
    settle();
    expect(media()).toBe(current);
    close();
    second.click();
    settle();
    expect(media().textContent).toContain('second 中文');
    close();
    document.querySelector<HTMLImageElement>('.post-content > img')!.click();
    expect(document.querySelector('.lightbox-counter')?.textContent).toBe('1 / 2');
    key('ArrowRight');
    settle();
    expect(document.querySelector('.lightbox-img')?.getAttribute('src')).toBe('/two.webp');
  });

  it('飞行以内部 SVG 而不是 figure 边框为目标，支持缩放旋转重置和滚动关闭', () => {
    const [figure] = mount();
    figure.click();
    expect(figure.classList.contains('lightbox-origin-hidden')).toBe(true);
    expect(media().style.transform).toBe(
      transformFor(
        { x: 100, y: 110, w: 480, h: 160 },
        Number.parseFloat(media().style.width),
        Number.parseFloat(media().style.height),
      ),
    );
    settle();
    key('+');
    settle();
    expect(media().classList.contains('is-zoomed')).toBe(true);
    key('r');
    settle();
    expect(media().style.transform).toContain('rotate(90deg)');
    document.querySelector<HTMLElement>('.lightbox-zoom-level')!.click();
    settle();
    expect(media().classList.contains('is-zoomed')).toBe(false);
    Object.defineProperty(window, 'scrollY', { value: 150, configurable: true });
    window.dispatchEvent(new Event('scroll'));
    settle();
    expect(document.querySelector('.lightbox-overlay')).toBeNull();
    expect(figure.style.visibility).not.toBe('hidden');
  });

  it('关闭按内部 SVG 的实时位置飞回，恢复焦点不滚动', () => {
    const [figure] = mount();
    figure.click();
    settle();
    const svg = figure.querySelector('svg')!;
    vi.mocked(svg.getBoundingClientRect).mockReturnValue(new DOMRect(120, 140, 480, 160));
    const focus = vi.spyOn(figure, 'focus');
    key('Escape');
    const matrix = new DOMMatrixReadOnly(media().style.transform);
    const w = Number.parseFloat(media().style.width);
    const h = Number.parseFloat(media().style.height);
    expect(matrix.e).toBeCloseTo(120, 2);
    expect(matrix.f).toBeCloseTo(140, 2);
    expect(matrix.a * w + matrix.e).toBeCloseTo(600, 1);
    expect(matrix.b * w + matrix.f).toBeCloseTo(140, 1);
    expect(matrix.c * h + matrix.e).toBeCloseTo(120, 1);
    expect(matrix.d * h + matrix.f).toBeCloseTo(300, 1);
    settle();
    expect(figure.classList.contains('lightbox-origin-hidden')).toBe(false);
    expect(focus).toHaveBeenCalledWith({ preventScroll: true });
  });

  it('替换会话和 owner 卸载释放 URL、恢复原图并解除监听', () => {
    const [first, second] = mount();
    first.click();
    settle();
    const old = media();
    key('Escape');
    second.click();
    settle();
    expect(revoked).toHaveBeenCalledWith('blob:diagram-1');
    expect(document.querySelectorAll('.lightbox-overlay')).toHaveLength(1);
    expect(first.style.visibility).not.toBe('hidden');
    const focus = vi.spyOn(second, 'focus');
    window.__closeLightboxFor('diagrams');
    expect(revoked).toHaveBeenCalledWith('blob:diagram-2');
    expect(second.style.visibility).not.toBe('hidden');
    const style = old.style.cssText;
    key('r');
    window.dispatchEvent(new Event('resize'));
    settle();
    expect(old.style.cssText).toBe(style);
    expect(document.querySelector('.lightbox-overlay')).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
    // owner 清理不应依赖触发页面滚动的焦点恢复。
    for (const call of focus.mock.calls) expect(call[0]).toEqual({ preventScroll: true });
  });

  it('reduced-motion 沿用立即就绪和滚动关闭', () => {
    vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: true } as MediaQueryList);
    const [figure] = mount();
    figure.click();
    settle();
    expect(media()).not.toBeNull();
    key('+');
    key('r');
    settle();
    expect(media().style.transform).toContain('rotate(90deg)');
    Object.defineProperty(window, 'scrollY', { value: 60, configurable: true });
    window.dispatchEvent(new Event('scroll'));
    settle();
    expect(document.querySelector('.lightbox-overlay')).toBeNull();
  });

  it.each([
    ['viewBox="0 0 NaN 160" width="320" height="120"', '0 0 320 120', '320', '120'],
    ['width="0" height="0"', '0 0 480 160', '480', '160'],
  ])('无有效 viewBox 时按有效属性或实际 SVG 尺寸补坐标系', (attributes, viewBox, width, height) => {
    const [figure] = mount();
    figure.innerHTML = `<svg xmlns="${ns}" ${attributes}><title>备用尺寸</title></svg>`;
    vi.spyOn(figure.querySelector('svg')!, 'getBoundingClientRect').mockReturnValue(
      new DOMRect(100, 110, 480, 160),
    );
    figure.click();
    settle();
    expect(media().getAttribute('viewBox')).toBe(viewBox);
    expect(media().getAttribute('width')).toBe(width);
    expect(media().getAttribute('height')).toBe(height);
  });

  it.each([
    '<figure class="mermaid-diagram"></figure>',
    `<figure class="mermaid-diagram"><svg xmlns="${ns}" viewBox="0 0 0 0" width="0" height="0"></svg></figure>`,
  ])('无可用尺寸或 SVG 不创建空浮层', (html) => {
    document.body.innerHTML = `<main class="post-content" data-showcase-lightbox-owner="diagrams">${html}</main>`;
    window.__initLightbox('.post-content');
    document.querySelector<HTMLElement>('figure')!.click();
    settle();
    expect(document.querySelector('.lightbox-overlay')).toBeNull();
    expect(blobs).toHaveLength(0);
  });
});
