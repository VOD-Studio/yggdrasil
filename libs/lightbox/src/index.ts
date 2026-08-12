import { prefersReducedMotion } from '@yggdrasil/shared';
import { fitCentered, originalUrl, type Rect, transformFor } from './geometry';
import './style.css';

interface LightboxState {
  overlay: HTMLDivElement;
  img: HTMLImageElement;
  caption: HTMLElement;
  counter: HTMLDivElement;
  errorBox: HTMLDivElement;
  prevBtn: HTMLButtonElement | null;
  nextBtn: HTMLButtonElement | null;
  originNode: HTMLElement;
  gallery: HTMLElement[];
  index: number | null;
  isSingle: boolean;
  openScrollY: number;
  origSrc: string;
  altText: string;
  closing: boolean;
  reduced: boolean;
  scrollHandler: ((this: Window, ev: Event) => void) | null;
  keyHandler: ((this: Document, ev: KeyboardEvent) => void) | null;
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
  // 遮罩必须在原图加载前可见：原图慢/失败时仍要让用户知道灯箱已打开，
  // 而不是留下 opacity:0 的全屏层拦截页面交互。
  overlay.style.opacity = '1';

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

  overlay.appendChild(img);
  overlay.appendChild(caption);
  overlay.appendChild(counter);
  overlay.appendChild(errorBox);
  if (prevBtn) overlay.appendChild(prevBtn);
  if (nextBtn) overlay.appendChild(nextBtn);
  document.body.appendChild(overlay);

  state = {
    overlay,
    img,
    caption,
    counter,
    errorBox,
    prevBtn,
    nextBtn,
    originNode,
    gallery,
    index,
    isSingle,
    openScrollY: window.scrollY,
    origSrc,
    altText,
    closing: false,
    reduced: prefersReducedMotion(),
    scrollHandler: null,
    keyHandler: null,
  };

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

    // reduced-motion：直接淡入居中
    if (state.reduced) {
      img.style.opacity = '0';
      img.style.transform = transformFor(target, baseW, baseH);
      img.style.left = '0';
      img.style.top = '0';
      overlay.style.opacity = '0';
      // 下一帧淡入
      requestAnimationFrame((): void => {
        if (!state) return;
        overlay.style.transition = 'opacity 200ms ease-out';
        img.style.transition = 'opacity 200ms ease-out';
        overlay.style.opacity = '1';
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
    overlay.style.opacity = '0';
    // 强制 reflow，确保首帧的 transform 已提交到渲染层。
    // 否则单层 rAF 里浏览器可能合并首帧与目标帧，动画从错误位置起跳。
    void img.offsetHeight;

    // double-rAF：第一帧绘制首帧（无动画），第二帧才启动 transition 到居中。
    requestAnimationFrame((): void => {
      if (!state) return;
      requestAnimationFrame((): void => {
        if (!state) return;
        img.style.transition = 'transform 250ms ease-out, opacity 250ms ease-out';
        overlay.style.transition = 'opacity 250ms ease-out';
        img.style.transform = transformFor(target, baseW, baseH);
        img.style.opacity = '1';
        overlay.style.opacity = '1';
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
}

// 从错误态恢复（图集切回可加载的图）：显示图、隐藏错误提示。
function hideErrorState(): void {
  if (!state) return;
  state.errorBox.style.display = 'none';
  state.img.style.display = '';
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
    // 更新 originNode 为新图，使后续关闭/滚动关闭飞回新图位置
    s.originNode = newNode;
    s.index = newIndex;
    s.openScrollY = window.scrollY; // 重置滚动关闭基线
  };
  setTimeout(swap, 150);
}

// ============ 交互绑定 ============

function bindInteractions(): void {
  const s = state;
  if (!s) return;

  // 点背景关闭（点图片本身不关，因箭头在图上、避免误关）
  s.overlay.addEventListener('click', (ev: MouseEvent): void => {
    if (state && ev.target === state.overlay) closeLightbox(false);
  });

  // 滚动驱动关闭：任何 scroll 都触发，用 scrollY 偏移算进度。
  // 关键：逐帧读 originNode 实时 rect，文章滚多少图就回多少。
  s.scrollHandler = (): void => {
    if (!state) return;
    const st = state;
    if (st.closing) return;
    const dy = Math.abs(window.scrollY - st.openScrollY);
    if (st.reduced) {
      // reduced-motion：立即关
      closeLightbox(true);
      return;
    }
    const target = st.target;
    const baseW = st.baseW || (target ? target.w : 1);
    const baseH = st.baseH || (target ? target.h : 1);
    const originRect = rectOf(st.originNode);
    if (!target) return; // 无 target 时不插值（忠实原 JS 的兜底语义）
    // 在 originRect 与居中 target 之间按 progress 线性插值
    const progress = Math.min(dy / 120, 1);
    const cur: Rect = {
      x: target.x + (originRect.x - target.x) * progress,
      y: target.y + (originRect.y - target.y) * progress,
      w: target.w + (originRect.w - target.w) * progress,
      h: target.h + (originRect.h - target.h) * progress,
    };
    st.img.style.transition = 'none';
    st.img.style.transform = transformFor(cur, baseW, baseH);
    st.img.style.opacity = String(1 - progress);
    st.overlay.style.opacity = String(1 - progress);
    if (progress >= 1) {
      // 已飞回原位：文章停在当前滚动位置，移除灯箱
      st.closing = true;
      cleanupInteractions();
      removeOverlay();
    }
  };
  window.addEventListener('scroll', s.scrollHandler, { passive: true });

  // 键盘：Esc 关；图集模式 ←→ 切换
  s.keyHandler = (ev: KeyboardEvent): void => {
    if (!state) return;
    if (ev.key === 'Escape') {
      closeLightbox(false);
    } else if (!state.isSingle && state.gallery.length > 1) {
      if (ev.key === 'ArrowLeft') {
        ev.preventDefault();
        gotoIndex((state.index ?? 0) - 1);
      } else if (ev.key === 'ArrowRight') {
        ev.preventDefault();
        gotoIndex((state.index ?? 0) + 1);
      }
    }
  };
  document.addEventListener('keydown', s.keyHandler);

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

function cleanupInteractions(): void {
  if (!state) return;
  if (state.scrollHandler) {
    window.removeEventListener('scroll', state.scrollHandler);
    state.scrollHandler = null;
  }
  if (state.keyHandler) {
    document.removeEventListener('keydown', state.keyHandler);
    state.keyHandler = null;
  }
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
