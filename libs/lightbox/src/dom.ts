import { thumbnailSrc } from './images';
import type { LightboxActions, LightboxElements } from './types';

// ---- 图标（Lucide，ISC；24 视口、1.6 线宽描边，与站点 Icon 组件同源）----
const lucide = (body: string, size = 18): string =>
  `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

const ERROR_ICON_SVG = lucide(
  '<line x1="2" x2="22" y1="2" y2="22"/><path d="M10.41 10.41a2 2 0 1 1-2.83-2.83"/><line x1="13.5" x2="6" y1="13.5" y2="21"/><line x1="18" x2="21" y1="12" y2="15"/><path d="M3.59 3.59A1.99 1.99 0 0 0 3 5v14a2 2 0 0 0 2 2h14c.55 0 1.052-.22 1.41-.59"/><path d="M21 15V5a2 2 0 0 0-2-2H9"/>',
  40,
);
const ZOOM_IN_ICON_SVG = lucide(
  '<circle cx="11" cy="11" r="8"/><line x1="21" x2="16.65" y1="21" y2="16.65"/><line x1="11" x2="11" y1="8" y2="14"/><line x1="8" x2="14" y1="11" y2="11"/>',
);
const ZOOM_OUT_ICON_SVG = lucide(
  '<circle cx="11" cy="11" r="8"/><line x1="21" x2="16.65" y1="21" y2="16.65"/><line x1="8" x2="14" y1="11" y2="11"/>',
);
const ROTATE_CCW_ICON_SVG = lucide(
  '<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/>',
);
const ROTATE_CW_ICON_SVG = lucide(
  '<path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/>',
);
const DOWNLOAD_ICON_SVG = lucide(
  '<path d="M12 15V3"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/>',
);
const CLOSE_ICON_SVG = lucide('<path d="M18 6 6 18"/><path d="m6 6 12 12"/>', 20);
const PREV_ICON_SVG = lucide('<path d="m15 18-6-6 6-6"/>', 20);
const NEXT_ICON_SVG = lucide('<path d="m9 18 6-6-6-6"/>', 20);

// 按钮点击 stopPropagation：不冒泡到 overlay 触发关闭
function makeButton(
  className: string,
  label: string,
  html: string,
  onClick: (() => void) | null,
  shortcut?: string,
): HTMLButtonElement {
  const btn = document.createElement('button');
  btn.className = className;
  btn.setAttribute('type', 'button');
  btn.setAttribute('aria-label', label);
  if (shortcut) btn.title = `${label}（${shortcut}）`;
  btn.innerHTML = html;
  if (onClick) {
    btn.addEventListener('click', (ev: MouseEvent): void => {
      ev.stopPropagation();
      onClick();
    });
  }
  return btn;
}

function divider(): HTMLSpanElement {
  const el = document.createElement('span');
  el.className = 'lightbox-divider';
  el.setAttribute('aria-hidden', 'true');
  return el;
}

export function createLightboxElements(
  altText: string,
  gallery: HTMLElement[],
  index: number | null,
  isSingle: boolean,
  actions: LightboxActions,
): LightboxElements {
  const galleryLength = gallery.length;
  const isGallery = !isSingle && galleryLength > 1;

  // 建 DOM
  const overlay = document.createElement('div');
  overlay.className = 'lightbox-overlay';
  overlay.setAttribute('role', 'dialog');
  overlay.setAttribute('aria-modal', 'true');
  overlay.setAttribute('aria-label', '图片查看器');
  overlay.setAttribute('tabindex', '-1');
  // 遮罩从 0 淡入：创建即隐藏，append 后立即启动淡入（见下方 rAF），
  // 与原图加载解耦。若加载期就保持 opacity:1，start() 为首帧硬切回 0 再
  // 淡入，用户会看到遮罩在打开瞬间闪一下；原图慢/失败时遮罩照常淡入可见，
  // 不留 opacity:0 的全屏层拦截页面交互。
  overlay.style.opacity = '0';

  const img = document.createElement('img');
  img.className = 'lightbox-img';
  img.setAttribute('alt', altText);
  // 加载前先占 0 尺寸，避免原图（可能数千 px）在加载期间撑大文档
  // 可滚动区、触发非预期的 scroll 事件。start() 拿到 natural 尺寸后再设真实值。
  img.style.width = '0px';
  img.style.height = '0px';

  // 错误态容器：原图加载失败或点击已知坏图（is-error）时显示原因，
  // 替代过去的立即 removeOverlay。pointer-events:none（CSS）让点击穿透到
  // overlay 照常触发关闭。
  const errorBox = document.createElement('div');
  errorBox.className = 'lightbox-error';
  errorBox.style.display = 'none';
  errorBox.innerHTML = `${ERROR_ICON_SVG}<p class="lightbox-error-text"></p><p class="lightbox-error-hint">按 Esc 或点击背景关闭</p>`;

  // ---- 顶栏：计数 / 工具组 / 关闭 ----
  // 顶栏与底栏自身 pointer-events:none，空白处的点击落到 overlay 照常关闭。
  const topbar = document.createElement('div');
  topbar.className = 'lightbox-topbar';

  const counter = document.createElement('span');
  counter.className = 'lightbox-counter';
  if (isGallery) {
    counter.textContent = `${(index ?? 0) + 1} / ${galleryLength}`;
  } else {
    counter.style.display = 'none';
  }

  const toolbar = document.createElement('div');
  toolbar.className = 'lightbox-toolbar';
  toolbar.setAttribute('role', 'toolbar');
  toolbar.setAttribute('aria-label', '图片操作');

  const zoomOutBtn = makeButton(
    'lightbox-tool',
    '缩小',
    ZOOM_OUT_ICON_SVG,
    () => actions.stepZoom(-1),
    '-',
  );
  // 档位文字兼作重置：点一下回到 100% 并转正。名称随倍率更新（view.ts syncZoomControls）。
  const zoomLevel = makeButton(
    'lightbox-zoom-level',
    '100%，重置视图',
    '100%',
    () => actions.resetView(true),
    '0',
  );
  const zoomInBtn = makeButton(
    'lightbox-tool',
    '放大',
    ZOOM_IN_ICON_SVG,
    () => actions.stepZoom(1),
    '+',
  );

  const downloadLink = document.createElement('a');
  downloadLink.className = 'lightbox-tool';
  downloadLink.setAttribute('aria-label', '下载原图');
  downloadLink.innerHTML = DOWNLOAD_ICON_SVG;
  // 不 preventDefault：保留 download/新标签打开的默认行为；仅阻止冒泡关闭
  downloadLink.addEventListener('click', (ev: MouseEvent): void => {
    ev.stopPropagation();
  });

  toolbar.append(
    zoomOutBtn,
    zoomLevel,
    zoomInBtn,
    divider(),
    makeButton(
      'lightbox-tool',
      '向左旋转',
      ROTATE_CCW_ICON_SVG,
      () => actions.rotate(-1),
      'Shift+R',
    ),
    makeButton('lightbox-tool', '向右旋转', ROTATE_CW_ICON_SVG, () => actions.rotate(1), 'R'),
    divider(),
    downloadLink,
  );

  const closeBtn = makeButton(
    'lightbox-tool lightbox-close',
    '关闭',
    CLOSE_ICON_SVG,
    () => actions.close(false),
    'Esc',
  );
  topbar.append(counter, toolbar, closeBtn);

  // 图集模式（>1 张）才加左右导航箭头；单张不显示。点击按当前序号切换，
  // 在 interactions.ts 绑定。
  let prevBtn: HTMLButtonElement | null = null;
  let nextBtn: HTMLButtonElement | null = null;
  if (isGallery) {
    prevBtn = makeButton('lightbox-nav lightbox-prev', '上一张', PREV_ICON_SVG, null, '←');
    nextBtn = makeButton('lightbox-nav lightbox-next', '下一张', NEXT_ICON_SVG, null, '→');
  }

  // ---- 底栏：图注 / 缩略条 / 键盘提示 ----
  const bottom = document.createElement('div');
  bottom.className = 'lightbox-bottom';

  const caption = document.createElement('p');
  caption.className = 'lightbox-caption';
  caption.textContent = altText;
  if (!altText) caption.style.display = 'none';
  bottom.appendChild(caption);

  let thumbStrip: HTMLDivElement | null = null;
  const thumbs: HTMLButtonElement[] = [];
  if (isGallery) {
    thumbStrip = document.createElement('div');
    thumbStrip.className = 'lightbox-thumbs';
    thumbStrip.setAttribute('role', 'group');
    thumbStrip.setAttribute('aria-label', '图片列表');
    gallery.forEach((node, k) => {
      const btn = makeButton('lightbox-thumb', `第 ${k + 1} 张`, '', () => actions.gotoIndex(k));
      // 复用正文已请求过的地址（多数已在缓存）；懒加载，长图集只取可见的几张。
      const src = thumbnailSrc(node);
      if (src) {
        const t = document.createElement('img');
        t.src = src;
        t.alt = '';
        t.loading = 'lazy';
        t.decoding = 'async';
        t.draggable = false;
        btn.appendChild(t);
      }
      if (k === index) btn.setAttribute('aria-current', 'true');
      thumbs.push(btn);
      thumbStrip?.appendChild(btn);
    });
    bottom.appendChild(thumbStrip);
  }

  const hint = document.createElement('p');
  hint.className = 'lightbox-hint';
  hint.setAttribute('aria-hidden', 'true');
  hint.textContent = [
    isGallery ? '← → 切换' : '',
    'Ctrl / ⌘ + 滚轮或双击缩放',
    '拖动平移',
    'R 旋转',
    'Esc 关闭',
  ]
    .filter(Boolean)
    .join(' · ');
  bottom.appendChild(hint);

  overlay.append(img, errorBox, topbar, bottom);
  if (prevBtn) overlay.appendChild(prevBtn);
  if (nextBtn) overlay.appendChild(nextBtn);
  document.body.appendChild(overlay);

  return {
    overlay,
    img,
    caption,
    counter,
    errorBox,
    topbar,
    bottom,
    toolbar,
    zoomOutBtn,
    zoomInBtn,
    zoomLevel,
    downloadLink,
    thumbStrip,
    thumbs,
    prevBtn,
    nextBtn,
  };
}
