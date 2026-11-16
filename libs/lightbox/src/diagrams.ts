const SVG_NS = 'http://www.w3.org/2000/svg';

export interface PreparedDiagram {
  element: SVGSVGElement;
  naturalWidth: number;
  naturalHeight: number;
  altText: string;
  downloadUrl: string;
}

export function visibleDiagramSvg(originNode: HTMLElement): SVGSVGElement | null {
  if (!originNode.matches('figure.mermaid-diagram')) return null;
  const svgs = [...originNode.querySelectorAll<SVGSVGElement>('svg')];
  const visible = svgs.find((svg) => {
    const rect = svg.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  });
  if (visible) return visible;
  const theme = document.documentElement.classList.contains('dark') ? 'dark' : 'light';
  return originNode.querySelector<SVGSVGElement>(`.mermaid-${theme} svg`) ?? svgs[0] ?? null;
}

const positive = (value: number): boolean => Number.isFinite(value) && value > 0;

export function prepareDiagram(originNode: HTMLElement): PreparedDiagram | null {
  const source = visibleDiagramSvg(originNode);
  if (!source) return null;
  const viewBox = source
    .getAttribute('viewBox')
    ?.trim()
    .split(/[\s,]+/)
    .map(Number);
  const validViewBox =
    viewBox?.length === 4 &&
    viewBox.every(Number.isFinite) &&
    positive(viewBox[2]) &&
    positive(viewBox[3]);
  let naturalWidth = validViewBox ? viewBox[2] : (source.width?.baseVal?.value ?? 0);
  let naturalHeight = validViewBox ? viewBox[3] : (source.height?.baseVal?.value ?? 0);
  if (!positive(naturalWidth) || !positive(naturalHeight)) {
    const rect = source.getBoundingClientRect();
    naturalWidth = rect.width;
    naturalHeight = rect.height;
  }
  if (!positive(naturalWidth) || !positive(naturalHeight)) return null;

  const element = source.cloneNode(true) as SVGSVGElement;
  element.setAttributeNS('http://www.w3.org/2000/xmlns/', 'xmlns', SVG_NS);
  if (!validViewBox) element.setAttribute('viewBox', `0 0 ${naturalWidth} ${naturalHeight}`);
  element.setAttribute('width', String(naturalWidth));
  element.setAttribute('height', String(naturalHeight));
  element.style.width = `${naturalWidth}px`;
  element.style.height = `${naturalHeight}px`;
  element.style.maxWidth = 'none';
  element.style.display = 'block';
  element.style.setProperty('text-autospace', 'no-autospace');
  element.style.setProperty('text-spacing-trim', 'space-all');
  element.style.overflowWrap = 'normal';
  element.style.lineHeight = '1.5';
  element.style.color = getComputedStyle(source).color;
  const altText =
    source.querySelector(':scope > title')?.textContent?.trim() ||
    source.getAttribute('aria-label')?.trim() ||
    '流程图';
  // 在灯箱写入定位、缩放或旋转之前保存原始矢量图；下载不依赖页面样式。
  const blob = new Blob([new XMLSerializer().serializeToString(element)], {
    type: 'image/svg+xml;charset=utf-8',
  });
  return { element, naturalWidth, naturalHeight, altText, downloadUrl: URL.createObjectURL(blob) };
}
