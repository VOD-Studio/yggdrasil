import { prefersReducedMotion } from '@yggdrasil/shared';
import { createLightboxElements } from './dom';
import {
  closeFlightTransform,
  fitCentered,
  matIdentity,
  panBy,
  scrollFlightTransform,
  transformFor,
  viewTransformCss,
} from './geometry';
import { errorTextOf, getImageDetails } from './images';
import { bindInteractions } from './interactions';
import { ResourceScope } from './resources';
import type { LightboxActions, LightboxState } from './types';
import {
  baseGeometry,
  pokeToolbar,
  rectOf,
  resetScrollClose,
  resetView,
  rotateCW,
  updateDownloadLink,
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
  const details = getImageDetails(originNode);
  if (!details) return null;
  const { origSrc, altText } = details;
  const isSingle = originNode.classList.contains('lightbox-single') || gallery.length === 0;
  const actions: LightboxActions = {
    zoomBy: (factor, x, y, animate) => zoomBy(s, factor, x, y, animate),
    rotateCW: () => rotateCW(s),
    resetView: (animate) => resetView(s, animate),
    close,
    gotoIndex,
  };
  const elements = createLightboxElements(altText, gallery.length, index, isSingle, actions);
  const { overlay, img } = elements;
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
    closing: false,
    reduced: prefersReducedMotion(),
    view: { deg: 0, scale: 1, user: null },
    normalized: false,
    pointers: new Map(),
    gesture: null,
    idleTimer: undefined,
    badgeTimer: undefined,
    toolbarHover: false,
  };
  let imageScope = new ResourceScope();
  let galleryTimer: number | undefined;
  function loadImage(src: string, onReady: () => void): void {
    imageScope.dispose();
    imageScope = new ResourceScope();
    const request = imageScope;
    const ready = (): void => {
      if (request.disposed || s.closing || s.scope.disposed) return;
      onReady();
    };
    request.listen(img, 'load', ready, { once: true });
    img.src = src;
    if (img.complete && img.naturalWidth) {
      request.dispose();
      imageScope = new ResourceScope();
      onReady();
    }
  }
  // 遮罩淡入（一次，单调 0→1）：reflow 提交首帧 opacity:0，rAF 起过渡。
  // 之后 start()/加载流程不再触碰遮罩 opacity，避免任何可见回退。
  void overlay.offsetHeight;
  s.scope.frame((): void => {
    if (s.closing || s.scrollClose) return;
    overlay.style.transition = 'opacity 250ms ease-out';
    overlay.style.opacity = '1';
  });

  s.scope.listen(img, 'error', () => {
    if (!s.closing) showErrorState(errorTextOf(s.originNode));
  });
  updateDownloadLink(s);
  pokeToolbar(s);

  // 焦点移入灯箱
  overlay.focus({ preventScroll: true });
  // 立即绑定交互（不等图片加载）：加载期间 Esc/滚动/点背景也须可关闭。
  const unbindInteractions = bindInteractions(s, actions);

  // 图片加载后再做动画（naturalW/H 要等加载）
  const start = (): void => {
    if (s.closing || s.scope.disposed || s.scrollClose) return;
    const naturalW = img.naturalWidth || img.clientWidth || 1;
    const naturalH = img.naturalHeight || img.clientHeight || 1;
    const originRect = rectOf(originNode);

    // 基准 = originRect（文章里图片的实际尺寸）。
    // img 的布局尺寸固定为 originRect，transform 的 scale 相对它缩放：
    // 首帧（文章位置）scale=1，居中态 scale=target.w/originRect.w。
    // 这样无论 target 比 originRect 大或小，动画都是「从文章图原样连续缩放」，
    // 视觉上是原地展开，不会像「从外面飞来」。灯箱图尺寸恒为视口最大（fitCentered）。
    const target = fitCentered(naturalW, naturalH, window.innerWidth, window.innerHeight);
    const baseW = originRect.w;
    const baseH = originRect.h;
    // 存基准与目标，供关闭/滚动关闭复用同一对（保证 scale 连续）。
    s.target = target;
    s.baseW = baseW;
    s.baseH = baseH;
    img.style.width = `${baseW}px`;
    img.style.height = `${baseH}px`;

    // reduced-motion：直接淡入居中（遮罩淡入已在创建时启动，这里只动图）
    if (s.reduced) {
      img.style.opacity = '0';
      img.style.transform = transformFor(target, baseW, baseH);
      img.style.left = '0';
      img.style.top = '0';
      // 下一帧淡入
      imageScope.frame((): void => {
        if (s.closing || s.scope.disposed || s.scrollClose) return;
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
    imageScope.frame((): void => {
      if (s.closing || s.scope.disposed || s.scrollClose) return;
      imageScope.frame((): void => {
        if (s.closing || s.scope.disposed || s.scrollClose) return;
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
  } else {
    loadImage(origSrc, start);
  }
  function showErrorState(message: string): void {
    s.img.style.display = 'none';
    const text = s.errorBox.querySelector('.lightbox-error-text');
    if (text) text.textContent = message;
    s.errorBox.style.display = '';
    // 错误态下图不可见，操控无意义：藏起工具栏与徽标
    s.toolbar.style.display = 'none';
    s.badge.classList.remove('is-visible');
  }

  // 从错误态恢复（图集切回可加载的图）：显示图、隐藏错误提示。
  function hideErrorState(): void {
    s.errorBox.style.display = 'none';
    s.img.style.display = '';
    s.toolbar.style.display = '';
  }

  function close(immediate: boolean): void {
    if (s.scope.disposed) return;
    if (immediate) {
      removeOverlay();
      return;
    }
    if (s.closing) return;
    // Keep the drag offset before releasing the gesture so closing starts at its painted position.
    // 让下面重写的规范首帧与当前视觉逐像素一致（飞行从拖拽位置起算）。
    const dragDy = s.gesture?.mode === 'dragclose' ? s.gesture.dy : 0;
    s.closing = true;
    unbindInteractions();
    imageScope.dispose();
    s.scope.clearTimeout(galleryTimer);

    const originRect = rectOf(s.originNode); // 实时读，处理期间滚动过的情况

    if (s.reduced) {
      removeOverlay();
      return;
    }

    let followScroll: (() => void) | null = null;
    if (s.scrollClose) {
      // 与滚动中的外层变换同构，Esc/大步滚轮都从当前动画帧继续飞回。
      const flight = s.scrollClose;
      s.img.style.transition = 'transform 250ms ease-out, opacity 250ms ease-out';
      followScroll = (): void => {
        if (s.scope.disposed) return;
        s.img.style.transform = scrollFlightTransform(
          flight.rect,
          rectOf(s.originNode),
          1,
          flight.transform,
        );
      };
      followScroll();
      // 收尾的 250ms 内滚轮惯性仍可能移动页面，终点继续跟随，结束后解除。
      s.scope.listen(window, 'scroll', followScroll, { passive: true });
    } else if (s.normalized) {
      // 操控态（缩放/旋转/平移过）：当前 transform 是定长 6 函数列表，或
      // dragclose 的 matrix() 串。飞回目标必须写成同构 6 函数列表
      // （closeFlightTransform）：rotate/k 槽两端相等恒不动，只有外层
      // translate/scale 插值 —— 累计旋转角再大，关闭时也不会反向空转。
      const { k, layoutW, layoutH } = baseGeometry(s);
      const user = panBy(s.view.user ?? matIdentity(), 0, dragDy);
      // 先无过渡写回规范首帧并强制 reflow 提交（起算值必须是 6 函数列表，
      // 否则 dragclose 的 matrix 起算值与目标列表不匹配，仍走矩阵插值）。
      s.img.style.transition = 'none';
      s.img.style.transform = viewTransformCss(
        user,
        s.view.deg,
        k,
        layoutW,
        layoutH,
        window.innerWidth,
        window.innerHeight,
      );
      void s.img.offsetHeight;
      s.img.style.transition = 'transform 250ms ease-out, opacity 250ms ease-out';
      s.img.style.transform = closeFlightTransform(originRect, layoutW, layoutH, s.view.deg, k);
    } else {
      // 未操控：当前是 transformFor 的 translate+scale，同构插值直接飞回。
      // 基准 = originRect 尺寸（与打开时一致），scale 相对它缩放。
      const baseW = s.baseW || (s.target ? s.target.w : 1);
      const baseH = s.baseH || (s.target ? s.target.h : 1);
      s.img.style.transition = 'transform 250ms ease-out, opacity 250ms ease-out';
      s.img.style.transform = transformFor(originRect, baseW, baseH);
    }
    s.overlay.style.transition = 'opacity 250ms ease-out';
    s.img.style.opacity = '0';
    s.overlay.style.opacity = '0';

    const done = (): void => {
      if (followScroll) window.removeEventListener('scroll', followScroll);
      removeOverlay();
    };
    // 250ms 兜底，避免 transitionend 不触发
    const timer = s.scope.timeout(done, 280);
    s.scope.listen(
      s.img,
      'transitionend',
      (): void => {
        s.scope.clearTimeout(timer);
        done();
      },
      { once: true },
    );
  }

  function removeOverlay(): void {
    if (s.scope.disposed) return;
    s.closing = true;
    unbindInteractions();
    imageScope.dispose();
    s.scope.dispose();
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
        prev instanceof HTMLImageElement ? prev : prev.querySelector('.blur-img-full');
      if (targetEl instanceof HTMLImageElement) {
        targetEl.setAttribute('tabindex', '-1');
        targetEl.focus({ preventScroll: true });
      }
    }
  }

  function gotoIndex(rawIndex: number): void {
    if (s.scope.disposed || s.closing || s.isSingle) return;
    if (!s.gallery || s.gallery.length === 0) return;
    let newIndex = rawIndex;
    if (newIndex < 0) newIndex = s.gallery.length - 1;
    if (newIndex >= s.gallery.length) newIndex = 0;
    if (newIndex === s.index) return;

    const newNode = s.gallery[newIndex];
    const details = getImageDetails(newNode);
    if (!details) return;
    const { origSrc, altText } = details;
    imageScope.dispose();
    s.scope.clearTimeout(galleryTimer);
    resetScrollClose(s);

    // 淡出当前图
    s.img.style.transition = 'opacity 150ms ease-out';
    s.img.style.opacity = '0';

    // 150ms 后换图淡入
    const swap = (): void => {
      if (s.scope.disposed || s.closing || s.scrollClose) return;
      // 换图后必须按新图真实尺寸重算几何，否则新图沿用第一张的
      // target/scale，宽高比不同的图会被压扁/拉伸。
      // 布局盒直接设为 target 尺寸（宽高比 = 新图），transform 归位 scale(1,1)；
      // baseW/H 同步为 target 尺寸，关闭/滚动关闭的飞回动画以它为 scale 基准。
      const applyGeometry = (): void => {
        if (s.scope.disposed || s.closing || s.scrollClose) return;
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
        if (s.scope.disposed || s.closing || s.scrollClose) return;
        s.img.style.transition = 'opacity 150ms ease-out';
        s.img.style.opacity = '1';
      };
      const onReady = (): void => {
        applyGeometry();
        fade();
      };
      s.origSrc = origSrc;
      s.altText = altText;
      s.img.alt = altText;
      s.caption.textContent = altText;
      s.caption.style.display = altText ? '' : 'none';
      s.counter.textContent = `${newIndex + 1} / ${s.gallery.length}`;
      // 更新 originNode 为新图，使后续关闭/拖拽关闭飞回新图位置
      s.originNode = newNode;
      s.index = newIndex;
      s.openScrollY = window.scrollY;
      // 换图归零查看器操控（旋转/缩放/平移不带到下一张）；applyGeometry 已把
      // 布局盒设为新 target 尺寸，normalized 回退到 transformFor 路径。
      s.view = { deg: 0, scale: 1, user: null };
      s.normalized = false;
      s.img.classList.remove('is-zoomed');
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

  return {
    get originNode() {
      return s.originNode;
    },
    close,
  };
}
