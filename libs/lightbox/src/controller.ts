import { prefersReducedMotion } from '@yggdrasil/shared';
import { prepareDiagram } from './diagrams';
import { createLightboxElements } from './dom';
import {
  closeFlightTransform,
  fitCentered,
  matchesAspect,
  SCROLL_FLIGHT_MIN_MS,
  scrollFlightEase,
  transformFor,
} from './geometry';
import { errorTextOf, getImageDetails } from './images';
import { bindInteractions } from './interactions';
import { ResourceScope } from './resources';
import type { LightboxActions, LightboxState } from './types';
import {
  captureCloseFlight,
  FADE_TRANSITION,
  FLIGHT_MS,
  FLIGHT_TRANSITION,
  measureInset,
  originRectOf,
  pokeToolbar,
  resetScrollClose,
  resetView,
  rotate,
  setFade,
  setOriginHidden,
  stepZoom,
  syncZoomControls,
  updateDownloadLink,
  writeScrollFlight,
  zoomBy,
} from './view';

export interface LightboxController {
  readonly originNode: HTMLElement;
  close: (immediate: boolean) => void;
}

/** All callbacks, listeners and animations belong to this single open session. */
export function openLightbox(
  originNode: HTMLElement,
  gallery: HTMLElement[],
  index: number | null,
  onRemoved: () => void,
): LightboxController | null {
  const isDiagram = originNode.matches('figure.mermaid-diagram');
  const diagram = isDiagram ? prepareDiagram(originNode) : null;
  const details = isDiagram
    ? diagram && { origSrc: diagram.downloadUrl, altText: diagram.altText }
    : getImageDetails(originNode);
  if (!details) return null;
  const { origSrc, altText } = details;
  if (isDiagram) {
    gallery = [];
    index = null;
  }
  const media = diagram?.element ?? document.createElement('img');
  if (media instanceof HTMLImageElement) media.alt = altText;
  const isSingle = originNode.classList.contains('lightbox-single') || gallery.length === 0;
  const actions: LightboxActions = {
    zoomBy: (factor, x, y, animate) => zoomBy(s, factor, x, y, animate),
    stepZoom: (dir) => stepZoom(s, dir),
    rotate: (dir) => rotate(s, dir),
    resetView: (animate) => resetView(s, animate),
    close,
    gotoIndex,
    stepIndex: (delta) => gotoIndex((pendingIndex ?? s.index ?? 0) + delta),
  };
  const elements = createLightboxElements(media, altText, gallery, index, isSingle, actions);
  const { overlay } = elements;
  const s: LightboxState = {
    ...elements,
    scope: new ResourceScope(),
    originNode,
    gallery,
    index,
    isSingle,
    openScrollY: window.scrollY,
    scrollClose: null,
    origSrc,
    altText,
    naturalWidth: diagram?.naturalWidth ?? 0,
    naturalHeight: diagram?.naturalHeight ?? 0,
    downloadName: diagram ? 'diagram.svg' : null,
    closing: false,
    solidFlight: false,
    reduced: prefersReducedMotion(),
    view: { deg: 0, scale: 1, user: null },
    normalized: false,
    pointers: new Map(),
    gesture: null,
    idleTimer: undefined,
    chromeHover: false,
    inset: measureInset(elements),
  };
  let imageScope = new ResourceScope();
  let galleryTimer: number | undefined;
  // 换图淡出期间（150ms）已接受、尚未落地的目标；连按方向键以它为基准，不会丢步。
  let pendingIndex: number | null = null;
  function loadImage(src: string, onReady: () => void): void {
    if (!(s.media instanceof HTMLImageElement)) return;
    const img = s.media;
    imageScope.dispose();
    imageScope = new ResourceScope();
    const request = imageScope;
    const ready = (): void => {
      if (request.disposed || s.closing || s.scope.disposed) return;
      s.naturalWidth = img.naturalWidth || img.clientWidth || 1;
      s.naturalHeight = img.naturalHeight || img.clientHeight || 1;
      onReady();
    };
    request.listen(img, 'load', ready, { once: true });
    img.src = src;
    if (img.complete && img.naturalWidth) {
      request.dispose();
      imageScope = new ResourceScope();
      s.naturalWidth = img.naturalWidth;
      s.naturalHeight = img.naturalHeight || img.clientHeight || 1;
      onReady();
    }
  }
  // 背景与界面层淡入（一次，单调 0→1）：reflow 提交首帧 opacity:0，rAF 起过渡。
  // 之后 start()/加载流程不再触碰这两层的 opacity，避免任何可见回退。
  void overlay.offsetHeight;
  s.scope.frame((): void => {
    if (s.closing || s.scrollClose) return;
    setFade(s, 1, FADE_TRANSITION);
  });

  if (media instanceof HTMLImageElement) {
    s.scope.listen(media, 'error', () => {
      if (!s.closing) showErrorState(errorTextOf(s.originNode));
    });
  }
  updateDownloadLink(s);
  syncZoomControls(s);
  syncThumbs();
  pokeToolbar(s);

  // 焦点移入灯箱
  overlay.focus({ preventScroll: true });
  // 立即绑定交互（不等图片加载）：加载期间 Esc/滚动/点背景也须可关闭。
  const unbindInteractions = bindInteractions(s, actions);

  // 图片加载后再做动画（naturalW/H 要等加载）
  const start = (): void => {
    if (s.closing || s.scope.disposed || s.scrollClose) return;
    const naturalW = s.naturalWidth;
    const naturalH = s.naturalHeight;
    const originRect = originRectOf(originNode);
    s.inset = measureInset(s);

    // 基准 = originRect（文章里图片的实际尺寸）。
    // img 的布局尺寸固定为 originRect，transform 的 scale 相对它缩放：
    // 首帧（文章位置）scale=1，居中态 scale=target.w/originRect.w。
    // 这样无论 target 比 originRect 大或小，动画都是「从文章图原样连续缩放」，
    // 视觉上是原地展开，不会像「从外面飞来」。灯箱图尺寸恒为视口最大（fitCentered）。
    const target = fitCentered(naturalW, naturalH, window.innerWidth, window.innerHeight, s.inset);
    const baseW = originRect.w;
    const baseH = originRect.h;
    // 存基准与目标，供关闭/滚动关闭复用同一对（保证 scale 连续）。
    s.target = target;
    s.baseW = baseW;
    s.baseH = baseH;
    media.style.width = `${baseW}px`;
    media.style.height = `${baseH}px`;

    // reduced-motion：直接淡入居中（遮罩淡入已在创建时启动，这里只动图）
    if (s.reduced) {
      media.style.opacity = '0';
      media.style.transform = transformFor(target, baseW, baseH);
      media.style.left = '0';
      media.style.top = '0';
      // 下一帧淡入
      imageScope.frame((): void => {
        if (s.closing || s.scope.disposed || s.scrollClose) return;
        media.style.transition = 'opacity 200ms ease-out';
        media.style.opacity = '1';
      });
      return;
    }

    // 首帧：文章位置 + 原尺寸（scale=1）并关闭 transition。比例一致时灯箱图不透明地盖在
    // 原位、原图同帧隐藏；比例不一致（object-fit 裁切的缩略图）时首帧透明，靠透明度过渡遮住。
    s.solidFlight = matchesAspect(originRect, naturalW, naturalH);
    media.style.transition = 'none';
    media.style.left = '0';
    media.style.top = '0';
    media.style.transform = transformFor(originRect, baseW, baseH);
    media.style.opacity = s.solidFlight ? '1' : '0';
    setOriginHidden(s, s.solidFlight);
    // 强制 reflow，确保首帧的 transform 已提交到渲染层。
    // 否则单层 rAF 里浏览器可能合并首帧与目标帧，动画从错误位置起跳。
    void media.getBoundingClientRect();

    // double-rAF：第一帧绘制首帧（无动画），第二帧才启动 transition 到居中。
    // 背景与界面层的淡入已在创建时启动并独立进行，这里不再触碰它们的 opacity。
    imageScope.frame((): void => {
      if (s.closing || s.scope.disposed || s.scrollClose) return;
      imageScope.frame((): void => {
        if (s.closing || s.scope.disposed || s.scrollClose) return;
        media.style.transition = FLIGHT_TRANSITION;
        media.style.transform = transformFor(target, baseW, baseH);
        media.style.opacity = '1';
      });
    });
  };

  if (diagram) {
    start();
  } else if (originNode.classList.contains('is-error')) {
    // 缩略图阶段已确认损坏（重试耗尽）：不再请求原图，直接错误态。
    // overlay 可 Esc/点击背景关闭，图集模式仍可左右切换离开。
    showErrorState(errorTextOf(originNode));
  } else {
    loadImage(origSrc, start);
  }
  function showErrorState(message: string): void {
    s.media.style.display = 'none';
    const text = s.errorBox.querySelector('.lightbox-error-text');
    if (text) text.textContent = message;
    s.errorBox.style.display = '';
    // 错误态下图不可见，操控无意义：藏起缩放旋转工具组（计数、关闭与缩略条保留）
    s.toolbar.style.display = 'none';
  }

  // 从错误态恢复（图集切回可加载的图）：显示图、隐藏错误提示。
  function hideErrorState(): void {
    s.errorBox.style.display = 'none';
    s.media.style.display = '';
    s.toolbar.style.display = '';
  }

  function close(immediate: boolean): void {
    if (s.scope.disposed) return;
    if (immediate) {
      removeOverlay();
      return;
    }
    if (s.closing) return;
    s.closing = true;
    unbindInteractions();
    imageScope.dispose();
    s.scope.clearTimeout(galleryTimer);

    const originRect = originRectOf(s.originNode); // 实时读，处理期间滚动过的情况

    if (s.reduced) {
      removeOverlay();
      return;
    }

    // 先让精确终点完成一次绘制，再恢复原图；300ms 兜底不再抢在最后一帧前删除副本。
    const finishFlight = (): void => {
      s.scope.frame(() => {
        if (s.scrollClose) writeScrollFlight(s, s.scrollClose, 1);
        s.scope.timeout(() => {
          if (s.scrollClose) writeScrollFlight(s, s.scrollClose, 1);
          removeOverlay();
        }, 0);
      });
    };

    if (s.scrollClose) {
      // 逐帧插值到页面原图的实时 rect：滚轮惯性仍在移动页面时，终点每帧跟随；
      // 终点至少绘制一帧，再删除副本；最后一次绘制与清理仍跟随原图实时位置。
      const flight = s.scrollClose;
      s.scope.clearTimeout(flight.settleTimer);
      const from = flight.progress;
      const duration = Math.max(SCROLL_FLIGHT_MIN_MS, FLIGHT_MS * (1 - from));
      const t0 = performance.now();
      const step = (): void => {
        const t = Math.min((performance.now() - t0) / duration, 1);
        writeScrollFlight(s, flight, from + (1 - from) * scrollFlightEase(t));
        if (t < 1) s.scope.frame(step);
        else finishFlight();
      };
      step();
      return;
    }
    const snapshot = captureCloseFlight(s);
    if (snapshot) {
      // 冻结实际绘制帧，再用同构函数列表同步转正、收回；拖拽位移已在矩阵内。
      s.media.style.transition = 'none';
      s.media.style.opacity = String(snapshot.mediaOpacity);
      setFade(s, snapshot.fadeOpacity, 'none');
      s.media.style.transform = closeFlightTransform(snapshot.pose, originRect, 0);
      void s.media.getBoundingClientRect();
      s.media.style.transition = FLIGHT_TRANSITION;
      s.media.style.transform = closeFlightTransform(snapshot.pose, originRect, 1);
    } else {
      // 未加载或错误态只淡出背景，不伪造布局尺寸或飞行路径。
      s.media.style.transition = FLIGHT_TRANSITION;
    }
    setFade(s, 0, FADE_TRANSITION);
    s.media.style.opacity = s.solidFlight ? '1' : '0';

    const done = (): void => {
      if (snapshot) {
        s.media.style.transition = 'none';
        s.media.style.transform = closeFlightTransform(snapshot.pose, originRect, 1);
        finishFlight();
      } else {
        removeOverlay();
      }
    };
    // FLIGHT_MS 兜底，避免 transitionend 不触发
    const timer = s.scope.timeout(done, FLIGHT_MS);
    s.scope.listen(s.media, 'transitionend', (event: TransitionEvent): void => {
      if (event.target !== s.media || event.propertyName !== 'transform') return;
      s.scope.clearTimeout(timer);
      done();
    });
  }

  function removeOverlay(): void {
    if (s.scope.disposed) return;
    s.closing = true;
    unbindInteractions();
    imageScope.dispose();
    s.scope.dispose();
    if (diagram) URL.revokeObjectURL(diagram.downloadUrl);
    // 先恢复原图可见：随后的 focus() 落在 visibility:hidden 的元素上会失败。
    setOriginHidden(s, false);
    const prev = s.originNode;
    if (s.overlay?.parentNode) {
      s.overlay.parentNode.removeChild(s.overlay);
    }
    onRemoved();
    // 焦点归还：.blur-img 是 span 不可聚焦，让其内部 full img 获得焦点。
    // 用 preventScroll 抑制 focus() 默认的 scrollIntoView 行为——否则关闭灯箱后
    // 页面会自动滚动把原图完整纳入视口（用户只点了一半露出的图时尤其明显）。
    if (prev) {
      const targetEl =
        prev.matches('figure.mermaid-diagram') || prev instanceof HTMLImageElement
          ? prev
          : prev.querySelector('.blur-img-full');
      if (targetEl instanceof HTMLElement) {
        targetEl.setAttribute('tabindex', '-1');
        targetEl.focus({ preventScroll: true });
      }
    }
  }

  function gotoIndex(rawIndex: number): void {
    if (s.scope.disposed || s.closing || s.isSingle || !(s.media instanceof HTMLImageElement))
      return;
    const img = s.media;
    if (!s.gallery || s.gallery.length === 0) return;
    let newIndex = rawIndex;
    if (newIndex < 0) newIndex = s.gallery.length - 1;
    if (newIndex >= s.gallery.length) newIndex = 0;
    if (newIndex === s.index && pendingIndex === null) return;

    const newNode = s.gallery[newIndex];
    const details = getImageDetails(newNode);
    if (!details) return;
    const { origSrc, altText } = details;
    imageScope.dispose();
    s.scope.clearTimeout(galleryTimer);
    resetScrollClose(s);

    // 淡出当前图
    s.media.style.transition = 'opacity 150ms ease-out';
    s.media.style.opacity = '0';

    // 150ms 后换图淡入
    pendingIndex = newIndex;
    const swap = (): void => {
      pendingIndex = null;
      if (s.scope.disposed || s.closing || s.scrollClose) return;
      // 换图后必须按新图真实尺寸重算几何，否则新图沿用第一张的
      // target/scale，宽高比不同的图会被压扁/拉伸。
      // 布局盒直接设为 target 尺寸（宽高比 = 新图），transform 归位 scale(1,1)；
      // baseW/H 同步为 target 尺寸，关闭/滚动关闭的飞回动画以它为 scale 基准。
      const applyGeometry = (): void => {
        if (s.scope.disposed || s.closing || s.scrollClose) return;
        const naturalW = s.naturalWidth;
        const naturalH = s.naturalHeight;
        s.inset = measureInset(s);
        const target = fitCentered(
          naturalW,
          naturalH,
          window.innerWidth,
          window.innerHeight,
          s.inset,
        );
        s.target = target;
        s.baseW = target.w;
        s.baseH = target.h;
        // 几何跳变不播动画（transition 只作用于随后的 opacity 淡入）。
        s.media.style.transition = 'none';
        s.media.style.width = `${target.w}px`;
        s.media.style.height = `${target.h}px`;
        s.media.style.transform = transformFor(target, target.w, target.h);
      };
      const fade = (): void => {
        if (s.scope.disposed || s.closing || s.scrollClose) return;
        s.media.style.transition = 'opacity 150ms ease-out';
        s.media.style.opacity = '1';
      };
      const onReady = (): void => {
        applyGeometry();
        if (s.scope.disposed || s.closing || s.scrollClose) return;
        s.solidFlight =
          !s.reduced && matchesAspect(originRectOf(newNode), s.naturalWidth, s.naturalHeight);
        setOriginHidden(s, s.solidFlight);
        fade();
      };
      s.origSrc = origSrc;
      s.altText = altText;
      s.downloadName = null;
      s.naturalWidth = 0;
      s.naturalHeight = 0;
      img.alt = altText;
      s.caption.textContent = altText;
      s.caption.style.display = altText ? '' : 'none';
      s.counter.textContent = `${newIndex + 1} / ${s.gallery.length}`;
      // 更新 originNode 为新图，使后续关闭/拖拽关闭飞回新图位置
      setOriginHidden(s, false);
      s.solidFlight = false;
      s.originNode = newNode;
      s.index = newIndex;
      s.openScrollY = window.scrollY;
      // 换图归零查看器操控（旋转/缩放/平移不带到下一张）；applyGeometry 已把
      // 布局盒设为新 target 尺寸，normalized 回退到 transformFor 路径。
      s.view = { deg: 0, scale: 1, user: null };
      s.normalized = false;
      s.media.classList.remove('is-zoomed');
      syncZoomControls(s);
      syncThumbs();
      updateDownloadLink(s);
      s.target = undefined;
      if (newNode.classList.contains('is-error')) {
        // 目标是已知坏图：不请求，直接错误态（counter/caption/originNode 照常更新）。
        showErrorState(errorTextOf(newNode));
      } else {
        hideErrorState();
        // 先换 src 再判 complete：换之前判的是旧图（必命中），会按旧图尺寸算几何。
        loadImage(origSrc, onReady);
      }
    };
    galleryTimer = s.scope.timeout(swap, s.reduced ? 0 : 150);
  }

  // 缩略条：标出当前张并把它滚到条的中间。直接写 scrollLeft，不用
  // scrollIntoView —— 后者可能连带滚动页面，触发滚动关闭。
  function syncThumbs(): void {
    if (!s.thumbStrip) return;
    s.thumbs.forEach((btn, k) => {
      if (k === s.index) btn.setAttribute('aria-current', 'true');
      else btn.removeAttribute('aria-current');
    });
    const active = s.index === null ? undefined : s.thumbs[s.index];
    if (!active) return;
    const strip = s.thumbStrip;
    strip.scrollLeft = active.offsetLeft - (strip.clientWidth - active.offsetWidth) / 2;
  }

  return {
    get originNode() {
      return s.originNode;
    },
    close,
  };
}
