import { prefersReducedMotion } from '@yggdrasil/shared';
import {
  BUTTON_ZOOM_STEP,
  clampPanToViewport,
  clampScale,
  DOUBLE_CLICK_SCALE,
  DRAG_CLOSE_PX,
  effectiveDims,
  fitCentered,
  type Mat,
  matIdentity,
  matMultiply,
  matRotateDeg,
  matScale,
  matToCss,
  matTranslate,
  nextRotationCW,
  originalUrl,
  panBy,
  type Rect,
  transformFor,
  wheelZoomFactor,
  zoomAround,
} from './geometry';
import './style.css';

// 单指/双指手势状态机：pan（放大后拖拽平移）、pinch（双指捏合缩放）、
// dragclose（fit 态竖直拖拽关闭，对齐旧「滚动关闭」的 120px 行程）。
type GestureState =
  | { mode: 'pan'; pointerId: number; startX: number; startY: number; startUser: Mat }
  | {
      mode: 'pinch';
      idA: number;
      idB: number;
      startDist: number;
      startMid: { x: number; y: number };
      startScale: number;
      startUser: Mat;
    }
  | { mode: 'dragclose'; pointerId: number; startY: number; dy: number };

interface LightboxState {
  overlay: HTMLDivElement;
  img: HTMLImageElement;
  caption: HTMLElement;
  counter: HTMLDivElement;
  errorBox: HTMLDivElement;
  toolbar: HTMLDivElement;
  badge: HTMLDivElement;
  downloadLink: HTMLAnchorElement;
  prevBtn: HTMLButtonElement | null;
  nextBtn: HTMLButtonElement | null;
  originNode: HTMLElement;
  gallery: HTMLElement[];
  index: number | null;
  isSingle: boolean;
  origSrc: string;
  altText: string;
  closing: boolean;
  reduced: boolean;
  keyHandler: ((this: Document, ev: KeyboardEvent) => void) | null;
  resizeHandler: (() => void) | null;
  // 查看器操控：rot 顺时针 0/90/180/270；scale 相对 fit 的倍率；user 矩阵
  // 承载缩放锚点与平移，null = 恒等（飞行/图集切换路径仍写 transformFor
  // 字符串，首次操控时才经 ensureNormalized 归一到矩阵基态）。
  view: { rot: number; scale: number; user: Mat | null };
  normalized: boolean;
  pointers: Map<number, { x: number; y: number }>;
  gesture: GestureState | null;
  idleTimer: number | undefined;
  badgeTimer: number | undefined;
  toolbarHover: boolean;
  target?: Rect;
  baseW?: number;
  baseH?: number;
}

declare global {
  interface Window {
    __initLightbox: (selectors: string | string[]) => void;
    __lightboxSelectors?: string[];
  }
}

// ============ 工具函数 ============

// prefersReducedMotion 由 @yggdrasil/shared 提供（lightbox.test.ts 会 mock matchMedia）。

// 读取元素当前在视口里的 rect（用于飞行起点/终点）。
// 统一映射成 {x,y,w,h}：getBoundingClientRect 返回的 DOMRect 用
// left/top/width/height，而 fitCentered/transformFor 用 x/y/w/h，
// 这里转成一致格式，避免 .w 读到 undefined。
function rectOf(el: Element): Rect {
  const r = el.getBoundingClientRect();
  return {
    x: r.left,
    y: r.top,
    w: r.width,
    h: r.height,
  };
}

// 注意：fitCentered / transformFor / originalUrl 已抽到 ./geometry.ts。

// ============ 加载失败与错误态 ============

// 缩略图加载失败的退避重试间隔（ms）：429 限流等瞬时错误多在几秒内自愈；
// 全部耗尽才认定永久性失败（文件丢失/损坏）。img 的 error 事件不带 HTTP
// 状态码，无法区分 404 与 429，故统一先重试。
const RETRY_DELAYS_MS = [1000, 2000, 4000];

// 默认错误文案。容器可用 data-error-text 定制（素材页传「本地文件已丢失」）；
// 标 is-error 时补默认属性，卡片 CSS 经 attr() 显示。
const DEFAULT_ERROR_TEXT = '图片加载失败';

// Material Symbols broken_image（标准 960 视口，fill=currentColor 跟随灯箱文字色）。
// 源档案：public/icons/broken_image_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg
const ERROR_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="M180-120q-24 0-42-18t-18-42v-600q0-24 18-42t42-18h600q24 0 42 18t18 42v600q0 24-18 42t-42 18H180Zm43-314 172-172 170 170 171-171 44 44v-217H180v303l43 43Zm-43 254h600v-298l-44-44-171 171-170-170-172 172-43-43v212Zm0 0v-298 60-362 600Z"/></svg>';

// ---- 工具栏图标（Material Symbols，标准 960 视口，fill=currentColor）----
// 源档案：public/icons/{zoom_in,zoom_out,rotate_90_degrees_cw,fit_screen,download,close}_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg
const ZOOM_IN_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="M796-121 533-384q-30 26-69.96 40.5Q423.08-329 378-329q-108.16 0-183.08-75Q120-479 120-585t75-181q75-75 181.5-75t181 75Q632-691 632-584.85 632-542 618-502q-14 40-42 75l264 262-44 44ZM377-389q81.25 0 138.13-57.5Q572-504 572-585t-56.87-138.5Q458.25-781 377-781q-82.08 0-139.54 57.5Q180-666 180-585t57.46 138.5Q294.92-389 377-389Zm-31-85v-82h-82v-60h82v-81h60v81h81v60h-81v82h-60Z"/></svg>';
const ZOOM_OUT_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="M796-121 533-384q-30 26-70 40.5T378-329q-108 0-183-75t-75-181q0-106 75-181t182-75q106 0 180.5 75T632-585q0 43-14 83t-42 75l264 262-44 44ZM377-389q81 0 138-57.5T572-585q0-81-57-138.5T377-781q-82 0-139.5 57.5T180-585q0 81 57.5 138.5T377-389ZM275-556v-60h201v60H275Z"/></svg>';
const ROTATE_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="M436-80q-73 0-137.5-28.5t-113-77q-48.5-48.5-77-113T80-436q0-146 105-251t251-105h42l-84-84 44-44 158 158-158 158-44-44 84-84h-42q-122 0-209 87t-87 209q0 122 87 209t209 87q54 0 101-14t89-47l42 44q-47 40-108 58.5T436-80Zm262-140L476-442l222-222 222 222-222 222Zm0-82 136-136-136-136-136 136 136 136Zm0-136Z"/></svg>';
const RESET_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="M820-610v-130H690v-60h130q24 0 42 18t18 42v130h-60Zm-740 0v-130q0-24 18-42t42-18h130v60H140v130H80Zm610 450v-60h130v-130h60v130q0 24-18 42t-42 18H690Zm-550 0q-24 0-42-18t-18-42v-130h60v130h130v60H140Zm60-120v-400h560v400H200Zm60-60h440v-280H260v280Zm0 0v-280 280Z"/></svg>';
const DOWNLOAD_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="M480-313 287-506l43-43 120 120v-371h60v371l120-120 43 43-193 193ZM220-160q-24 0-42-18t-18-42v-143h60v143h520v-143h60v143q0 24-18 42t-42 18H220Z"/></svg>';
const CLOSE_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="m256-200-56-56 224-224-224-224 56-56 224 224 224-224 56 56-224 224 224 224-56 56-224-224-224 224Z"/></svg>';

// 标记容器加载失败：CSS 隐藏双层图、改显破图图标 + 文案；灯箱入口据此
// 直接展示错误态，不再请求原图。
function markLoadError(container: Element): void {
  container.classList.add('is-error');
  if (!container.hasAttribute('data-error-text')) {
    container.setAttribute('data-error-text', DEFAULT_ERROR_TEXT);
  }
}

// 取节点错误文案：data-error-text 定制优先（素材页），缺省通用文案。
function errorTextOf(node: HTMLElement): string {
  return node.getAttribute('data-error-text') || DEFAULT_ERROR_TEXT;
}

// ============ 懒加载 ============

// 为单个 .blur-img 容器初始化高清图懒加载。
// IO 进入视口后把 data-src 写入 src，加载完成加 is-loaded 触发 CSS 淡入。
function initLazyLoad(container: Element): void {
  const raw = container.querySelector('.blur-img-full');
  if (!(raw instanceof HTMLImageElement)) return;
  const fullImg: HTMLImageElement = raw;
  if (container.getAttribute('data-blur-init')) return;
  container.setAttribute('data-blur-init', 'true');

  const fullSrc = fullImg.getAttribute('data-src');
  if (!fullSrc) return;

  const onFullLoaded = (): void => {
    // 给容器加 is-loaded，CSS 据此显式隐藏 placeholder。
    // 直接把 full 层 opacity 设为 1（清掉 transition），不依赖 CSS 的 opacity
    // 过渡：合成层重绘时机不稳定，可能导致 full 层卡在 opacity:0，直到一次
    // 强制重排才更新。
    container.classList.add('is-loaded');
    fullImg.style.transition = 'none';
    fullImg.style.opacity = '1';
  };
  fullImg.addEventListener('load', onFullLoaded);

  // 加载失败：按退避间隔重试（429 限流等瞬时错误可自愈），重试耗尽仍失败
  // 才标 is-error 永久错误态（文件丢失/损坏）。
  let retryCount = 0;
  const onFullError = (): void => {
    if (retryCount >= RETRY_DELAYS_MS.length) {
      markLoadError(container);
      return;
    }
    const delay = RETRY_DELAYS_MS[retryCount];
    retryCount += 1;
    setTimeout((): void => {
      // 重试前若已成功或已终态（理论上不会：is-error 只在本链末端设置），不再重设。
      if (container.classList.contains('is-loaded') || container.classList.contains('is-error'))
        return;
      fullImg.src = fullSrc;
    }, delay);
  };
  fullImg.addEventListener('error', onFullError);

  // 缓存兜底：若设 src 时图片已在缓存（load 几乎立即触发，可能早于监听注册），
  // 用 complete 补一次。注意无 src 的 img complete 也为 true，故先判 src。
  // complete 但 naturalWidth 为 0 = 已失败（error 同样可能早于监听注册），
  // 走同一错误路径而不是误标 is-loaded。
  if (fullImg.getAttribute('src') && fullImg.complete) {
    if (fullImg.naturalWidth > 0) {
      onFullLoaded();
    } else {
      onFullError();
    }
  }

  if ('IntersectionObserver' in window) {
    const io = new IntersectionObserver(
      (entries: IntersectionObserverEntry[]): void => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            fullImg.src = fullSrc;
            io.unobserve(container);
          }
        }
      },
      { rootMargin: '200px' },
    );
    io.observe(container);
  } else {
    fullImg.src = fullSrc;
  }
}

// ============ 图像收集 ============

// 从节点提取原图 URL 与 alt 描述（同时兼容 .blur-img 容器与原生外链 <img> 节点）。
function getImageDetails(originNode: HTMLElement): { origSrc: string; altText: string } | null {
  if (originNode instanceof HTMLImageElement) {
    const src = originNode.getAttribute('data-src') || originNode.getAttribute('src') || '';
    if (!src) return null;
    return {
      origSrc: originalUrl(src),
      altText: originNode.getAttribute('alt') || '',
    };
  }
  const fullImgEl = originNode.querySelector('.blur-img-full');
  if (fullImgEl instanceof HTMLImageElement) {
    const dataSrc = fullImgEl.getAttribute('data-src') || fullImgEl.getAttribute('src') || '';
    if (!dataSrc) return null;
    return {
      origSrc: originalUrl(dataSrc),
      altText: fullImgEl.getAttribute('alt') || '',
    };
  }
  return null;
}

// 收集所有 selectors 命中的 .blur-img 节点与原生外链 <img> 节点。
// gallery: 正文图（组成图集）；singles: 带 lightbox-single class 的单张图（如封面）。
function collectImages(roots: Element[]): { gallery: HTMLElement[]; singles: HTMLElement[] } {
  const gallery: HTMLElement[] = [];
  const singles: HTMLElement[] = [];
  for (const root of roots) {
    const nodes = root.querySelectorAll('.blur-img, img');
    for (const n of nodes) {
      if (!(n instanceof HTMLElement)) continue;
      if (n instanceof HTMLImageElement && n.closest('.blur-img')) continue;
      if (n.classList.contains('lightbox-single')) {
        singles.push(n);
      } else {
        gallery.push(n);
      }
    }
  }
  return { gallery, singles };
}

// ============ 灯箱状态与开/关 ============

// 当前灯箱状态（同时只允许一个灯箱）。
let state: LightboxState | null = null;

function openLightbox(originNode: HTMLElement, gallery: HTMLElement[], index: number | null): void {
  if (state) closeLightbox(true);

  const details = getImageDetails(originNode);
  if (!details) return;
  const { origSrc, altText } = details;
  const isSingle = originNode.classList.contains('lightbox-single') || gallery.length === 0;

  const vw = window.innerWidth;
  const vh = window.innerHeight;

  // 建 DOM
  const overlay = document.createElement('div');
  overlay.className = 'lightbox-overlay';
  overlay.setAttribute('role', 'dialog');
  overlay.setAttribute('aria-modal', 'true');
  overlay.setAttribute('aria-label', '图片预览');
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
  // 原图请求失败（404/429/文件丢失）：切错误态展示原因，不再「闪一下就消失」。
  // 监听常驻 img：打开中失败与图集切换换到坏图走同一路径。
  img.addEventListener('error', (): void => {
    if (state?.img === img) showErrorState(errorTextOf(state.originNode));
  });

  const caption = document.createElement('figcaption');
  caption.className = 'lightbox-caption';
  caption.textContent = altText;
  if (!altText) caption.style.display = 'none';

  const counter = document.createElement('div');
  counter.className = 'lightbox-counter';
  if (isSingle || gallery.length === 0) {
    counter.style.display = 'none';
  } else {
    counter.textContent = `${(index ?? 0) + 1} / ${gallery.length}`;
  }

  // 错误态容器：原图加载失败或点击已知坏图（is-error）时显示原因，
  // 替代过去的立即 removeOverlay。pointer-events:none（CSS）让点击穿透到
  // overlay 照常触发关闭。
  const errorBox = document.createElement('div');
  errorBox.className = 'lightbox-error';
  errorBox.style.display = 'none';
  errorBox.innerHTML = `${ERROR_ICON_SVG}<p class="lightbox-error-text"></p><p class="lightbox-error-hint">按 Esc 或点击背景关闭</p>`;

  // 图集模式（>1 张）才加左右导航箭头；单张不显示。
  let prevBtn: HTMLButtonElement | null = null;
  let nextBtn: HTMLButtonElement | null = null;
  if (!isSingle && gallery.length > 1) {
    prevBtn = document.createElement('button');
    prevBtn.className = 'lightbox-nav lightbox-prev';
    prevBtn.setAttribute('type', 'button');
    prevBtn.setAttribute('aria-label', '上一张');
    prevBtn.textContent = '\u2039';

    nextBtn = document.createElement('button');
    nextBtn.className = 'lightbox-nav lightbox-next';
    nextBtn.setAttribute('type', 'button');
    nextBtn.setAttribute('aria-label', '下一张');
    nextBtn.textContent = '\u203a';
  }

  // ---- 查看器工具栏：毛玻璃胶囊（缩小/放大/旋转/重置/下载/关闭）----
  const toolbar = document.createElement('div');
  toolbar.className = 'lightbox-toolbar';
  toolbar.setAttribute('role', 'toolbar');
  toolbar.setAttribute('aria-label', '图片操作');

  // 按钮点击 stopPropagation：不冒泡到 overlay 触发关闭
  const makeTool = (aria: string, svg: string, onClick: () => void): HTMLButtonElement => {
    const btn = document.createElement('button');
    btn.className = 'lightbox-tool';
    btn.setAttribute('type', 'button');
    btn.setAttribute('aria-label', aria);
    btn.innerHTML = svg;
    btn.addEventListener('click', (ev: MouseEvent): void => {
      ev.stopPropagation();
      onClick();
    });
    return btn;
  };
  toolbar.appendChild(
    makeTool('缩小', ZOOM_OUT_ICON_SVG, () => {
      zoomBy(1 / BUTTON_ZOOM_STEP, window.innerWidth / 2, window.innerHeight / 2, true);
    }),
  );
  toolbar.appendChild(
    makeTool('放大', ZOOM_IN_ICON_SVG, () => {
      zoomBy(BUTTON_ZOOM_STEP, window.innerWidth / 2, window.innerHeight / 2, true);
    }),
  );
  toolbar.appendChild(makeTool('顺时针旋转 90 度', ROTATE_ICON_SVG, rotateCW));
  toolbar.appendChild(makeTool('重置视图', RESET_ICON_SVG, () => resetView(true)));

  const downloadLink = document.createElement('a');
  downloadLink.className = 'lightbox-tool';
  downloadLink.setAttribute('aria-label', '下载原图');
  downloadLink.innerHTML = DOWNLOAD_ICON_SVG;
  // 不 preventDefault：保留 download/新标签打开的默认行为；仅阻止冒泡关闭
  downloadLink.addEventListener('click', (ev: MouseEvent): void => {
    ev.stopPropagation();
  });
  toolbar.appendChild(downloadLink);

  toolbar.appendChild(
    makeTool('关闭', CLOSE_ICON_SVG, () => {
      closeLightbox(false);
    }),
  );

  // 缩放倍率徽标（操控时短暂显示）
  const badge = document.createElement('div');
  badge.className = 'lightbox-zoom-badge';
  badge.setAttribute('aria-hidden', 'true');

  // 悬停工具栏时不自动隐藏
  toolbar.addEventListener('pointerenter', (): void => {
    if (!state) return;
    state.toolbarHover = true;
    pokeToolbar();
  });
  toolbar.addEventListener('pointerleave', (): void => {
    if (!state) return;
    state.toolbarHover = false;
    pokeToolbar();
  });

  overlay.appendChild(img);
  overlay.appendChild(caption);
  overlay.appendChild(counter);
  overlay.appendChild(errorBox);
  overlay.appendChild(toolbar);
  overlay.appendChild(badge);
  if (prevBtn) overlay.appendChild(prevBtn);
  if (nextBtn) overlay.appendChild(nextBtn);
  document.body.appendChild(overlay);

  // 遮罩淡入（一次，单调 0→1）：reflow 提交首帧 opacity:0，rAF 起过渡。
  // 之后 start()/加载流程不再触碰遮罩 opacity，避免任何可见回退。
  void overlay.offsetHeight;
  requestAnimationFrame((): void => {
    if (!state) return; // 首帧前可能已被关闭（immediate 路径）
    overlay.style.transition = 'opacity 250ms ease-out';
    overlay.style.opacity = '1';
  });

  state = {
    overlay,
    img,
    caption,
    counter,
    errorBox,
    toolbar,
    badge,
    downloadLink,
    prevBtn,
    nextBtn,
    originNode,
    gallery,
    index,
    isSingle,
    origSrc,
    altText,
    closing: false,
    reduced: prefersReducedMotion(),
    keyHandler: null,
    resizeHandler: null,
    view: { rot: 0, scale: 1, user: null },
    normalized: false,
    pointers: new Map(),
    gesture: null,
    idleTimer: undefined,
    badgeTimer: undefined,
    toolbarHover: false,
  };
  updateDownloadLink();
  pokeToolbar();

  // 焦点移入灯箱
  overlay.focus();
  // 立即绑定交互（不等图片加载）：加载期间 Esc/滚动/点背景也须可关闭。
  bindInteractions();

  // 图片加载后再做动画（naturalW/H 要等加载）
  const start = (): void => {
    if (!state) return; // 加载前可能已被关闭
    const naturalW = img.naturalWidth || img.clientWidth || 1;
    const naturalH = img.naturalHeight || img.clientHeight || 1;
    const originRect = rectOf(originNode);

    // 基准 = originRect（文章里图片的实际尺寸）。
    // img 的布局尺寸固定为 originRect，transform 的 scale 相对它缩放：
    // 首帧（文章位置）scale=1，居中态 scale=target.w/originRect.w。
    // 这样无论 target 比 originRect 大或小，动画都是「从文章图原样连续缩放」，
    // 视觉上是原地展开，不会像「从外面飞来」。灯箱图尺寸恒为视口最大（fitCentered）。
    const target = fitCentered(naturalW, naturalH, vw, vh);
    const baseW = originRect.w;
    const baseH = originRect.h;
    // 存基准与目标，供关闭/滚动关闭复用同一对（保证 scale 连续）。
    state.target = target;
    state.baseW = baseW;
    state.baseH = baseH;
    img.style.width = `${baseW}px`;
    img.style.height = `${baseH}px`;

    // reduced-motion：直接淡入居中（遮罩淡入已在创建时启动，这里只动图）
    if (state.reduced) {
      img.style.opacity = '0';
      img.style.transform = transformFor(target, baseW, baseH);
      img.style.left = '0';
      img.style.top = '0';
      // 下一帧淡入
      requestAnimationFrame((): void => {
        if (!state) return;
        img.style.transition = 'opacity 200ms ease-out';
        img.style.opacity = '1';
      });
      return;
    }

    // 首帧：文章位置 + 原尺寸（scale=1），透明，且关闭 transition
    img.style.transition = 'none';
    img.style.left = '0';
    img.style.top = '0';
    img.style.transform = transformFor(originRect, baseW, baseH);
    img.style.opacity = '0';
    // 强制 reflow，确保首帧的 transform 已提交到渲染层。
    // 否则单层 rAF 里浏览器可能合并首帧与目标帧，动画从错误位置起跳。
    void img.offsetHeight;

    // double-rAF：第一帧绘制首帧（无动画），第二帧才启动 transition 到居中。
    // 遮罩淡入已在创建时启动并独立进行，这里不再触碰 overlay opacity。
    requestAnimationFrame((): void => {
      if (!state) return;
      requestAnimationFrame((): void => {
        if (!state) return;
        img.style.transition = 'transform 250ms ease-out, opacity 250ms ease-out';
        img.style.transform = transformFor(target, baseW, baseH);
        img.style.opacity = '1';
      });
    });
  };

  if (originNode.classList.contains('is-error')) {
    // 缩略图阶段已确认损坏（重试耗尽）：不再请求原图，直接错误态。
    // overlay 可 Esc/点击背景关闭，图集模式仍可左右切换离开。
    showErrorState(errorTextOf(originNode));
  } else if (img.complete && img.naturalWidth) {
    start();
  } else {
    img.addEventListener('load', start, { once: true });
    img.src = origSrc;
  }
}

// 切换灯箱到错误态：隐藏图、显示原因；overlay 保持可交互（Esc/点击背景关闭、
// 图集仍可切换）。counter/caption 不受影响。
function showErrorState(message: string): void {
  if (!state) return;
  state.img.style.display = 'none';
  const text = state.errorBox.querySelector('.lightbox-error-text');
  if (text) text.textContent = message;
  state.errorBox.style.display = '';
  // 错误态下图不可见，操控无意义：藏起工具栏与徽标
  state.toolbar.style.display = 'none';
  state.badge.classList.remove('is-visible');
}

// 从错误态恢复（图集切回可加载的图）：显示图、隐藏错误提示。
function hideErrorState(): void {
  if (!state) return;
  state.errorBox.style.display = 'none';
  state.img.style.display = '';
  state.toolbar.style.display = '';
}

function closeLightbox(immediate: boolean): void {
  if (!state || state.closing) return;
  state.closing = true;
  cleanupInteractions();

  const s = state;

  // 基准 = originRect 尺寸（与打开时一致），scale 相对它缩放。
  const baseW = s.baseW || (s.target ? s.target.w : 1);
  const baseH = s.baseH || (s.target ? s.target.h : 1);
  const originRect = rectOf(s.originNode); // 实时读，处理期间滚动过的情况

  if (s.reduced || immediate) {
    removeOverlay();
    return;
  }

  // 飞回 originRect：scale 从 1 缩到 originRect.w/baseW
  s.img.style.transition = 'transform 250ms ease-out, opacity 250ms ease-out';
  s.overlay.style.transition = 'opacity 250ms ease-out';
  s.img.style.transform = transformFor(originRect, baseW, baseH);
  s.img.style.opacity = '0';
  s.overlay.style.opacity = '0';

  const done = (): void => {
    removeOverlay();
  };
  // 250ms 兜底，避免 transitionend 不触发
  const timer = setTimeout(done, 280);
  s.img.addEventListener(
    'transitionend',
    (): void => {
      clearTimeout(timer);
      done();
    },
    { once: true },
  );
}

function removeOverlay(): void {
  if (!state) return;
  const prev = state.originNode;
  if (state.overlay?.parentNode) {
    state.overlay.parentNode.removeChild(state.overlay);
  }
  state = null;
  // 焦点归还：.blur-img 是 span 不可聚焦，让其内部 full img 获得焦点。
  // 用 preventScroll 抑制 focus() 默认的 scrollIntoView 行为——否则关闭灯箱后
  // 页面会自动滚动把原图完整纳入视口（用户只点了一半露出的图时尤其明显）。
  if (prev) {
    const targetEl = prev instanceof HTMLImageElement ? prev : prev.querySelector('.blur-img-full');
    if (targetEl instanceof HTMLImageElement) {
      targetEl.setAttribute('tabindex', '-1');
      targetEl.focus({ preventScroll: true });
    }
  }
}

// ============ 图集切换 ============

// 图集切换：淡入淡出，不飞行。newIndex 循环（首尾衔接）。
function gotoIndex(rawIndex: number): void {
  if (!state || state.isSingle) return;
  const s = state;
  if (!s.gallery || s.gallery.length === 0) return;
  let newIndex = rawIndex;
  if (newIndex < 0) newIndex = s.gallery.length - 1;
  if (newIndex >= s.gallery.length) newIndex = 0;
  if (newIndex === s.index) return;

  const newNode = s.gallery[newIndex];
  const details = getImageDetails(newNode);
  if (!details) return;
  const { origSrc, altText } = details;

  // 淡出当前图
  s.img.style.transition = 'opacity 150ms ease-out';
  s.img.style.opacity = '0';

  // 150ms 后换图淡入
  const swap = (): void => {
    if (!state) return; // 切换中可能已关闭
    // 换图后必须按新图真实尺寸重算几何，否则新图沿用第一张的
    // target/scale，宽高比不同的图会被压扁/拉伸。
    // 布局盒直接设为 target 尺寸（宽高比 = 新图），transform 归位 scale(1,1)；
    // baseW/H 同步为 target 尺寸，关闭/滚动关闭的飞回动画以它为 scale 基准。
    const applyGeometry = (): void => {
      if (!state) return;
      const naturalW = s.img.naturalWidth || 1;
      const naturalH = s.img.naturalHeight || 1;
      const target = fitCentered(naturalW, naturalH, window.innerWidth, window.innerHeight);
      s.target = target;
      s.baseW = target.w;
      s.baseH = target.h;
      // 几何跳变不播动画（transition 只作用于随后的 opacity 淡入）。
      s.img.style.transition = 'none';
      s.img.style.width = `${target.w}px`;
      s.img.style.height = `${target.h}px`;
      s.img.style.transform = transformFor(target, target.w, target.h);
    };
    const fade = (): void => {
      if (!state) return;
      s.img.style.transition = 'opacity 150ms ease-out';
      s.img.style.opacity = '1';
    };
    const onReady = (): void => {
      applyGeometry();
      fade();
    };
    if (newNode.classList.contains('is-error')) {
      // 目标是已知坏图：不请求，直接错误态（counter/caption/originNode 照常更新）。
      showErrorState(errorTextOf(newNode));
    } else {
      hideErrorState();
      // 先换 src 再判 complete：换之前判的是旧图（必命中），会按旧图尺寸算几何。
      s.img.src = origSrc;
      if (s.img.complete && s.img.naturalWidth) {
        onReady(); // 缓存命中，新图尺寸同步可用
      } else {
        s.img.addEventListener('load', onReady, { once: true });
      }
    }
    s.caption.textContent = altText;
    s.caption.style.display = altText ? '' : 'none';
    s.counter.textContent = `${newIndex + 1} / ${s.gallery.length}`;
    // 更新 originNode 为新图，使后续关闭/拖拽关闭飞回新图位置
    s.originNode = newNode;
    s.index = newIndex;
    // 换图归零查看器操控（旋转/缩放/平移不带到下一张）；applyGeometry 已把
    // 布局盒设为新 target 尺寸，normalized 回退到 transformFor 路径。
    s.view = { rot: 0, scale: 1, user: null };
    s.normalized = false;
    s.img.classList.remove('is-zoomed');
    updateDownloadLink();
  };
  setTimeout(swap, 150);
}

// ============ 查看器操控（缩放/旋转/平移/下载） ============

// 错误态（图隐藏）下操控无意义。
function isErrorShown(s: LightboxState): boolean {
  return s.errorBox.style.display !== 'none';
}

// 基态矩阵 M0：布局盒 →「旋转 rot 后 fitCentered 居中」的视觉位置。
// 布局盒尺寸按旋转后的有效 fit 反推（90°/270° 宽高互换），位图宽高比不变。
function baseGeometry(s: LightboxState): { m0: Mat; layoutW: number; layoutH: number; fit: Rect } {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const natW = s.img.naturalWidth || 1;
  const natH = s.img.naturalHeight || 1;
  const eff = effectiveDims(natW, natH, s.view.rot);
  const fit = fitCentered(eff.w, eff.h, vw, vh);
  const layoutW = s.view.rot % 180 === 0 ? fit.w : fit.h;
  const layoutH = s.view.rot % 180 === 0 ? fit.h : fit.w;
  const m0 = matMultiply(
    matTranslate(vw / 2, vh / 2),
    matMultiply(matRotateDeg(s.view.rot), matTranslate(-layoutW / 2, -layoutH / 2)),
  );
  return { m0, layoutW, layoutH, fit };
}

// 把 view 写到 DOM。animate=true 走 180ms ease-out（按钮/旋转/重置/回弹）；
// false 直出（滚轮/拖拽/捏合需 <200ms 响应）。每次写入都重算 fit 并钳制平移。
function applyView(animate: boolean): void {
  const s = state;
  if (!s) return;
  const { m0, layoutW, layoutH, fit } = baseGeometry(s);
  const wPx = `${layoutW}px`;
  const hPx = `${layoutH}px`;
  if (s.img.style.width !== wPx) s.img.style.width = wPx;
  if (s.img.style.height !== hPx) s.img.style.height = hPx;
  const user = clampPanToViewport(
    s.view.user ?? matIdentity(),
    m0,
    layoutW,
    layoutH,
    window.innerWidth,
    window.innerHeight,
  );
  s.view.user = user;
  s.img.style.transition = animate && !s.reduced ? 'transform 180ms ease-out' : 'none';
  s.img.style.transform = matToCss(matMultiply(user, m0));
  // 关闭飞回/图集切换的基准同步：baseW/H 恒为当前布局盒尺寸
  s.target = fit;
  s.baseW = layoutW;
  s.baseH = layoutH;
  s.img.classList.toggle('is-zoomed', s.view.scale > 1);
}

// 首次操控前把布局盒从「飞行基准」（originRect 尺寸 + 非均匀 scale）归一到
// 「fit 尺寸 + scale(1)」的矩阵基态——两者视觉完全一致，跳变不可见。
function ensureNormalized(): void {
  const s = state;
  if (!s || s.normalized) return;
  s.normalized = true;
  applyView(false);
  // 强制 reflow：归一化基态（fit 布局盒 + matrix 字符串）必须在此提交绘制。
  // 否则它与随后的 applyView(true) 合并在同一帧，180ms 过渡的起算值仍是飞行
  // 动画遗留的 translate+scale 字符串 —— 首个缩放/旋转动画出现非均匀 scale
  // 回弹与起始帧跳变（素材页正方形缩略图打开竖图时最明显）。
  void s.img.offsetHeight;
}

// 缩放倍率徽标：操控时短暂显示，600ms 后淡出。
function showBadge(): void {
  const s = state;
  if (!s) return;
  s.badge.textContent = `${Math.round(s.view.scale * 100)}%`;
  s.badge.classList.add('is-visible');
  clearTimeout(s.badgeTimer);
  s.badgeTimer = setTimeout((): void => {
    state?.badge.classList.remove('is-visible');
  }, 600);
}

// 工具栏空闲自动隐藏：任何指针活动唤醒，2.5s 无活动且不在工具栏上/手势中则淡出。
function pokeToolbar(): void {
  const s = state;
  if (!s) return;
  s.toolbar.classList.remove('is-hidden');
  clearTimeout(s.idleTimer);
  s.idleTimer = setTimeout((): void => {
    if (!state || state.toolbarHover || state.gesture) return;
    state.toolbar.classList.add('is-hidden');
  }, 2500);
}

function zoomBy(k: number, px: number, py: number, animate: boolean): void {
  const s = state;
  if (!s?.target || isErrorShown(s)) return;
  const next = clampScale(s.view.scale * k);
  if (next === s.view.scale) return; // 已到顶/到底
  ensureNormalized();
  s.view.user = zoomAround(s.view.user ?? matIdentity(), next / s.view.scale, px, py);
  s.view.scale = next;
  applyView(animate);
  showBadge();
  pokeToolbar();
}

function rotateCW(): void {
  const s = state;
  if (!s?.target || isErrorShown(s)) return;
  ensureNormalized();
  s.view.rot = nextRotationCW(s.view.rot);
  // 旋转后保持倍率、绕视口中心重锚定、平移清零（避免旋转把图甩出视口）
  s.view.user = zoomAround(
    matIdentity(),
    s.view.scale,
    window.innerWidth / 2,
    window.innerHeight / 2,
  );
  applyView(true);
  pokeToolbar();
}

function resetView(animate: boolean): void {
  const s = state;
  if (!s?.target || isErrorShown(s)) return;
  ensureNormalized();
  s.view = { rot: 0, scale: 1, user: null };
  applyView(animate);
  showBadge();
  pokeToolbar();
}

// 下载链接跟随当前图：同源 /uploads 用 download 属性存原图；
// 外链 download 跨域无效，改新标签页打开。
function updateDownloadLink(): void {
  const s = state;
  if (!s) return;
  s.downloadLink.setAttribute('href', s.origSrc);
  if (s.origSrc.startsWith('/')) {
    s.downloadLink.setAttribute('download', s.origSrc.split('/').pop() ?? '');
    s.downloadLink.removeAttribute('target');
    s.downloadLink.removeAttribute('rel');
  } else {
    s.downloadLink.removeAttribute('download');
    s.downloadLink.setAttribute('target', '_blank');
    s.downloadLink.setAttribute('rel', 'noopener');
  }
}

// ============ 交互绑定 ============

function bindInteractions(): void {
  const s = state;
  if (!s) return;

  // 点背景关闭（点图片本身不关，因箭头在图上、避免误关）
  s.overlay.addEventListener('click', (ev: MouseEvent): void => {
    if (state && ev.target === state.overlay) closeLightbox(false);
  });

  // 阻断触屏滚动串联到 body（旧「滚动关闭」已由 img 竖直拖拽手势取代；
  // 固定遮罩下背景滚动只会不可见地挪动页面）。
  s.overlay.addEventListener(
    'touchmove',
    (ev: TouchEvent): void => {
      ev.preventDefault();
    },
    { passive: false },
  );

  // 滚轮缩放（桌面）：preventDefault 阻断页面滚动，缩放锚定光标。
  // 工具栏上滚轮不缩放（防误触）。
  s.overlay.addEventListener(
    'wheel',
    (ev: WheelEvent): void => {
      if (!state) return;
      ev.preventDefault();
      if (ev.target instanceof Element && ev.target.closest('.lightbox-toolbar')) return;
      zoomBy(wheelZoomFactor(ev.deltaY, ev.deltaMode), ev.clientX, ev.clientY, false);
    },
    { passive: false },
  );

  // 双击：fit ↔ 2.5× 切换，锚定点击点
  s.img.addEventListener('dblclick', (ev: MouseEvent): void => {
    if (!state?.target || isErrorShown(state)) return;
    ev.preventDefault();
    const goal = state.view.scale > 1 ? 1 : DOUBLE_CLICK_SCALE;
    zoomBy(goal / state.view.scale, ev.clientX, ev.clientY, true);
  });

  // 键盘：Esc 关；图集 ←→ 切换；+/- 缩放；R 旋转；0 重置
  s.keyHandler = (ev: KeyboardEvent): void => {
    if (!state) return;
    if (ev.key === 'Escape') {
      closeLightbox(false);
      return;
    }
    if (!state.isSingle && state.gallery.length > 1) {
      if (ev.key === 'ArrowLeft') {
        ev.preventDefault();
        gotoIndex((state.index ?? 0) - 1);
        return;
      }
      if (ev.key === 'ArrowRight') {
        ev.preventDefault();
        gotoIndex((state.index ?? 0) + 1);
        return;
      }
    }
    const cx = window.innerWidth / 2;
    const cy = window.innerHeight / 2;
    if (ev.key === '+' || ev.key === '=') {
      ev.preventDefault();
      zoomBy(BUTTON_ZOOM_STEP, cx, cy, true);
    } else if (ev.key === '-' || ev.key === '_') {
      ev.preventDefault();
      zoomBy(1 / BUTTON_ZOOM_STEP, cx, cy, true);
    } else if (ev.key === 'r' || ev.key === 'R') {
      ev.preventDefault();
      rotateCW();
    } else if (ev.key === '0') {
      ev.preventDefault();
      resetView(true);
    }
  };
  document.addEventListener('keydown', s.keyHandler);

  // 视口尺寸变化：重算 fit。已操控态保持 rot/scale 由 applyView 重锚定；
  // 未操控态（transformFor 路径）按原逻辑重算居中目标。
  s.resizeHandler = (): void => {
    const st = state;
    if (!st?.target || st.closing) return;
    if (st.normalized) {
      applyView(false);
      return;
    }
    const natW = st.img.naturalWidth || 1;
    const natH = st.img.naturalHeight || 1;
    const target = fitCentered(natW, natH, window.innerWidth, window.innerHeight);
    st.target = target;
    st.img.style.transition = 'none';
    st.img.style.transform = transformFor(target, st.baseW ?? target.w, st.baseH ?? target.h);
  };
  window.addEventListener('resize', s.resizeHandler);

  // ---- 指针手势（鼠标/触控统一 Pointer Events；img CSS 为 touch-action:none）----
  s.img.addEventListener('dragstart', (ev: Event): void => {
    ev.preventDefault();
  });
  s.img.addEventListener('pointerdown', onPointerDown);
  s.img.addEventListener('pointermove', onPointerMove);
  s.img.addEventListener('pointerup', onPointerUp);
  s.img.addEventListener('pointercancel', onPointerUp);

  // 工具栏空闲自动隐藏：overlay 上任何指针活动唤醒
  s.overlay.addEventListener('pointermove', (): void => {
    pokeToolbar();
  });
  s.overlay.addEventListener('pointerdown', (): void => {
    pokeToolbar();
  });

  // 图集导航箭头点击（stopPropagation 防止冒泡到 overlay 触发关闭）
  s.prevBtn?.addEventListener('click', (ev: MouseEvent): void => {
    ev.stopPropagation();
    if (state) gotoIndex((state.index ?? 0) - 1);
  });
  s.nextBtn?.addEventListener('click', (ev: MouseEvent): void => {
    ev.stopPropagation();
    if (state) gotoIndex((state.index ?? 0) + 1);
  });
}

// ---- 指针手势处理 ----

function onPointerDown(ev: PointerEvent): void {
  const s = state;
  if (!s?.target || isErrorShown(s)) return;
  if (ev.pointerType === 'mouse' && ev.button !== 0) return; // 仅主键
  s.img.setPointerCapture(ev.pointerId);
  s.pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
  pokeToolbar();

  if (s.pointers.size === 2) {
    // 第二指落下 → 捏合：以当前 user/scale 为起点，单指手势无缝接管
    const [idA, idB] = [...s.pointers.keys()];
    const p1 = s.pointers.get(idA) ?? { x: ev.clientX, y: ev.clientY };
    const p2 = s.pointers.get(idB) ?? { x: ev.clientX, y: ev.clientY };
    ensureNormalized();
    s.gesture = {
      mode: 'pinch',
      idA,
      idB,
      startDist: Math.hypot(p2.x - p1.x, p2.y - p1.y) || 1,
      startMid: { x: (p1.x + p2.x) / 2, y: (p1.y + p2.y) / 2 },
      startScale: s.view.scale,
      startUser: s.view.user ?? matIdentity(),
    };
    return;
  }
  if (s.pointers.size > 2) return;

  ensureNormalized();
  if (s.view.scale > 1) {
    s.gesture = {
      mode: 'pan',
      pointerId: ev.pointerId,
      startX: ev.clientX,
      startY: ev.clientY,
      startUser: s.view.user ?? matIdentity(),
    };
  } else {
    s.gesture = { mode: 'dragclose', pointerId: ev.pointerId, startY: ev.clientY, dy: 0 };
  }
}

function onPointerMove(ev: PointerEvent): void {
  const s = state;
  if (!s?.gesture || !s.pointers.has(ev.pointerId)) return;
  s.pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
  const g = s.gesture;

  if (g.mode === 'pinch') {
    const p1 = s.pointers.get(g.idA);
    const p2 = s.pointers.get(g.idB);
    if (!p1 || !p2) return;
    const dist = Math.hypot(p2.x - p1.x, p2.y - p1.y) || 1;
    const mid = { x: (p1.x + p2.x) / 2, y: (p1.y + p2.y) / 2 };
    const next = clampScale(g.startScale * (dist / g.startDist));
    // 双指锚点：startMid 下的像素跟手移到当前 mid，同时按倍率缩放
    s.view.user = matMultiply(
      matTranslate(mid.x, mid.y),
      matMultiply(
        matScale(next / g.startScale),
        matMultiply(matTranslate(-g.startMid.x, -g.startMid.y), g.startUser),
      ),
    );
    s.view.scale = next;
    applyView(false);
    showBadge();
    return;
  }
  if (g.pointerId !== ev.pointerId) return;

  if (g.mode === 'pan') {
    s.view.user = panBy(g.startUser, ev.clientX - g.startX, ev.clientY - g.startY);
    applyView(false);
    return;
  }

  // dragclose：图跟手指竖直移动，遮罩按行程淡出（120px 到位）
  const dy = ev.clientY - g.startY;
  g.dy = dy;
  const { m0 } = baseGeometry(s);
  s.img.style.transition = 'none';
  s.img.style.transform = matToCss(matMultiply(matTranslate(0, dy), m0));
  const progress = Math.min(Math.abs(dy) / DRAG_CLOSE_PX, 1);
  s.overlay.style.opacity = String(1 - progress);
}

function onPointerUp(ev: PointerEvent): void {
  const s = state;
  if (!s) return;
  s.pointers.delete(ev.pointerId);
  const g = s.gesture;
  if (!g) return;

  if (g.mode === 'pinch') {
    if (ev.pointerId !== g.idA && ev.pointerId !== g.idB) return;
    // 剩余一指 → 切换为 pan（重新取起点，避免跳变）；无剩余则结束
    const remaining = [...s.pointers.entries()][0];
    s.gesture = remaining
      ? {
          mode: 'pan',
          pointerId: remaining[0],
          startX: remaining[1].x,
          startY: remaining[1].y,
          startUser: s.view.user ?? matIdentity(),
        }
      : null;
    applyView(true); // 钳制回弹
    return;
  }
  if (g.pointerId !== ev.pointerId) return;
  s.gesture = null;

  if (g.mode === 'dragclose') {
    if (Math.abs(g.dy) >= DRAG_CLOSE_PX) {
      closeLightbox(false); // 行程到位 → 走飞回关闭
    } else {
      // 未到位 → 回弹：图归位、遮罩恢复不透明
      applyView(true);
      s.overlay.style.transition = 'opacity 180ms ease-out';
      s.overlay.style.opacity = '1';
    }
    return;
  }
  applyView(true); // pan 结束：边界钳制回弹
}

function cleanupInteractions(): void {
  if (!state) return;
  if (state.keyHandler) {
    document.removeEventListener('keydown', state.keyHandler);
    state.keyHandler = null;
  }
  if (state.resizeHandler) {
    window.removeEventListener('resize', state.resizeHandler);
    state.resizeHandler = null;
  }
  clearTimeout(state.idleTimer);
  state.idleTimer = undefined;
  clearTimeout(state.badgeTimer);
  state.badgeTimer = undefined;
  state.pointers.clear();
  state.gesture = null;
}

// ============ 初始化入口 ============

window.__initLightbox = (selectors: string | string[]): void => {
  // selectors 可以是字符串、字符串数组
  const sels = Array.isArray(selectors) ? selectors : [selectors];
  const roots: Element[] = [];
  for (const sel of sels) {
    const found = document.querySelectorAll(sel);
    for (const el of found) roots.push(el);
  }

  // 先对所有图片做懒加载（图集与单张都做）
  const collected = collectImages(roots);
  for (const node of collected.gallery.concat(collected.singles)) {
    initLazyLoad(node);
  }

  // 幂等守卫：SPA 页面（如 /admin/assets）数据刷新后会重复调用 __initLightbox，
  // 而 Dioxus keyed diff 可能复用同一批 DOM 节点——无守卫会叠加 click 监听，
  // 点一次连续触发多次 openLightbox（飞行动画重启/闪烁）。
  // initLazyLoad 已有 data-blur-init 守卫，这里给 click 绑定补同款。
  const bindClick = (node: HTMLElement, handler: (e: MouseEvent) => void): void => {
    if (node.getAttribute('data-lb-bound')) return;
    node.setAttribute('data-lb-bound', 'true');
    node.addEventListener('click', handler);
  };

  // 正文图：带 index。for..of + const 天然捕获每次迭代的 idx，
  // 无需旧 IIFE 包装（旧 var 循环闭包必须立即执行函数固定变量）。
  const gallery = collected.gallery;
  gallery.forEach((node, idx) => {
    bindClick(node, (e: MouseEvent) => {
      e.preventDefault();
      openLightbox(node, gallery, idx);
    });
  });
  // 单张图（封面）：index = null，gallery 传空数组表示单张
  for (const node of collected.singles) {
    bindClick(node, (e: MouseEvent) => {
      e.preventDefault();
      openLightbox(node, [], null);
    });
  }
};

// ============ 自启动 ============
// 方案 iii：双保险契约，无需轮询。
// 1) Rust 内联 eval 先跑（常态）：设 __lightboxSelectors，此时 __initLightbox 可能未定义 → 只设配置；
//    lightbox.js 后加载完 → 读到配置 → 这里自启动。
// 2) lightbox.js 先加载完：__initLightbox 就绪但无配置 → 不自启动；
//    Rust eval 后跑 → 设配置 + 兜底 if(__initLightbox) 显式调用 → 初始化。
if (Array.isArray(window.__lightboxSelectors)) {
  window.__initLightbox(window.__lightboxSelectors);
}
