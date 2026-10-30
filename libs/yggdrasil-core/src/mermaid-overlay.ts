/**
 * Mermaid 流程图点击放大浮层。
 *
 * 问题：复杂流程图（多 subgraph / 宽 LR 布局）在小屏被 useMaxWidth + max-width:100%
 * 压缩到无法阅读。本模块提供全屏浮层：点击图 → 克隆 SVG 按原始尺寸渲染 →
 * 支持拖拽平移、双击/工具栏/双指捏合/Ctrl+滚轮缩放。
 *
 * 交互与图片灯箱（libs/lightbox）统一：
 * - 打开：FLIP 飞行动画——从图表里可见 SVG 的屏幕位置 250ms ease-out 连续缩放到
 *   居中适配位（double-rAF + 强制 reflow 提交首帧）。SVG 矢量，放大无损。
 * - 关闭：飞回 SVG 实时位置（打开期间页面可能滚动过）。
 * - 滚动驱动关闭：body 滚动不锁定，任何滚动按 |scrollY − openScrollY| / 120 插值
 *   「居中态 ↔ 原位态」并同步淡出，progress ≥ 1 直接移除浮层，文章停在滚动位置。
 * - Esc / 点背景 / ✕ 按钮：播 250ms 飞回动画后移除。
 * - reduced-motion：打开纯淡入，滚动/关闭立即移除，无飞行。
 *
 * 与灯箱的有意差异：保留缩放/平移能力（流程图需要细读），但滚轮不缩放——
 * 普通滚轮 = 页面滚动 = 滚动关闭；缩放走双击、工具栏按钮、双指捏合、
 * Ctrl/⌘+滚轮（触控板捏合手势上报为 ctrlKey wheel）。
 *
 * 设计要点：
 * - 克隆 SVG 剥离 max-width 约束，按 viewBox 原始尺寸渲染。
 * - pan/zoom 用 CSS transform（translate + scale），transform-origin: 0 0，
 *   数学用「屏幕坐标 ↔ 内容坐标」映射，缩放锚定光标/捏合中心点。
 *   飞行首帧/末帧与滚动插值复用同一 transform 模型（flyStateFor 纯函数）。
 * - Pointer Events 统一鼠标 + 触摸：1 指 = 平移，2 指 = 捏合缩放。
 * - 每页同时只允许一个浮层（单例 state）。
 */

import { prefersReducedMotion } from '@yggdrasil/shared';

// ── 单例状态 ──

let overlay: HTMLDivElement | null = null;
let content: HTMLDivElement | null = null;
let svgClone: SVGSVGElement | null = null;

/** 打开浮层的图表容器：关闭时飞回其内部 SVG 的实时位置，并归还焦点。 */
let originPre: HTMLElement | null = null;

/**
 * 当前主题下可见的 SVG：服务端输出浅/深两份，CSS 隐藏其一（display:none 没有
 * client rect）。都不可见（如测试环境无布局）时回退到第一份。
 */
function visibleSvg(root: HTMLElement): SVGSVGElement | null {
  const candidates = root.querySelectorAll<SVGSVGElement>(':scope > div > svg');
  for (const svg of candidates) {
    if (svg.getClientRects().length > 0) return svg;
  }
  return root.querySelector('svg');
}

/** 内容原始尺寸（viewBox 像素），用于 fit-to-screen 与飞行 scale 计算。 */
let naturalW = 0;
let naturalH = 0;

/** 内容 (0,0) 锚定在视口的基准坐标（让 SVG 初始居中）。 */
let originX = 0;
let originY = 0;

// transform 状态
let scale = 1;
let tx = 0;
let ty = 0;

/** fit 态的 scale（fit 态 tx/ty 恒为 0），滚动关闭插值的起点。 */
let fitScale = 1;

// 缩放上下限
const MIN_SCALE = 0.1;
const MAX_SCALE = 5;

/** 开/关飞行动画时长（与图片灯箱一致）。 */
const ANIM_MS = 250;
/** 滚动关闭：滚动多少 px 完成飞回（与图片灯箱一致）。 */
const SCROLL_CLOSE_PX = 120;

// pointer 跟踪
const pointers = new Map<number, { x: number; y: number }>();
let panStart: { x: number; y: number; tx: number; ty: number } | null = null;
let pinchStart: {
  dist: number;
  cx: number;
  cy: number;
  scale: number;
  tx: number;
  ty: number;
} | null = null;

// 开/关与滚动关闭状态
let closing = false;
let reduced = false;
let openScrollY = 0;
let scrollHandler: (() => void) | null = null;
let keyHandler: ((e: KeyboardEvent) => void) | null = null;
/** 打开动画的 transition 清理兜底定时器（关闭前必须清掉，防止误清关闭动画的 transition）。 */
let openAnimTimer: number | null = null;

// ── 工具函数 ──

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

function clamp(v: number, min: number, max: number): number {
  return Math.min(Math.max(v, min), max);
}

/** 读取元素当前在视口里的 rect（飞行起点/终点；关闭时实时读，处理期间滚动过的情况）。 */
function rectOf(el: Element): Rect {
  const r = el.getBoundingClientRect();
  return { x: r.left, y: r.top, w: r.width, h: r.height };
}

/**
 * 把 <pre> 里 SVG 的屏幕 rect 映射成浮层 transform 态（飞行动画的原位帧）。
 *
 * content 锚定在 (originX, originY)，transform-origin 0 0：
 *   屏幕位置 = origin + translate，显示宽度 = naturalW × scale。
 * 让内容左上角落在 rect.(x,y)、宽度等于 rect.w 即可。
 */
export function flyStateFor(
  rect: Rect,
  originX: number,
  originY: number,
  naturalW: number,
): { scale: number; tx: number; ty: number } {
  const s = naturalW > 0 ? rect.w / naturalW : 1;
  return { scale: s, tx: rect.x - originX, ty: rect.y - originY };
}

function applyTransform(): void {
  if (!content) return;
  content.style.transform = `translate(${tx}px, ${ty}px) scale(${scale})`;
}

/**
 * 计算让 SVG 居中且完整可见的初始 transform。
 * fit 态 tx/ty 恒为 0，scale 存进 fitScale 供滚动关闭插值。
 *
 * 居中必须乘 scale：transform-origin 0 0 让缩放向左上角收缩，显示尺寸是
 * natural × scale（宽图 fitScale<1 时差了 naturalW(1−s)/2，左侧会被裁出视口）。
 */
function fitToScreen(): void {
  const pad = 48;
  const vw = window.innerWidth - pad;
  const vh = window.innerHeight - pad;
  // 适配缩放：不放大超出原始尺寸（除非图比视口还小）
  scale = clamp(Math.min(vw / naturalW, vh / naturalH), MIN_SCALE, 1);
  fitScale = scale;
  originX = (window.innerWidth - naturalW * scale) / 2;
  originY = (window.innerHeight - naturalH * scale) / 2;
  tx = 0;
  ty = 0;
  if (content) {
    content.style.left = `${originX}px`;
    content.style.top = `${originY}px`;
  }
  applyTransform();
}

/**
 * 在屏幕坐标 (px, py) 处缩放：保持该点对应的内容点不变。
 *
 * 数学推导（transform-origin: 0 0）：
 *   屏幕坐标 = origin + translate + scale × 内容坐标
 *   内容坐标 = (屏幕坐标 - origin - translate) / scale
 *
 * 缩放后保持同一内容点在 (px, py)：
 *   px = origin + newTx + newScale × contentX
 *   newTx = px - origin - newScale × contentX
 */
function zoomAt(px: number, py: number, newScale: number): void {
  newScale = clamp(newScale, MIN_SCALE, MAX_SCALE);
  const cx = (px - originX - tx) / scale;
  const cy = (py - originY - ty) / scale;
  tx = px - originX - newScale * cx;
  ty = py - originY - newScale * cy;
  scale = newScale;
  applyTransform();
}

/** 离散缩放指令的过渡时长（工具栏按钮/双击）。 */
const ZOOM_ANIM_MS = 200;
/** 缩放过渡的 transition 清理兜底定时器（连续快速点击时复用同一个）。 */
let zoomAnimTimer: number | undefined;

/**
 * 离散缩放指令（工具栏/双击）播 200ms 过渡，结束后清回 'none'
 * 交还直接操作的即时响应。快速连点：前一段过渡被取消（transitioncancel，
 * 不触发 end），后一段从当前渲染位置续接，不会跳变。
 * 直接操作（拖拽/滚轮/捏合）不走这里——逐帧事件流必须即时。
 */
function animateZoomTo(apply: () => void): void {
  if (!content) return;
  if (reduced) {
    apply();
    return;
  }
  content.style.transition = `transform ${ZOOM_ANIM_MS}ms ease-out`;
  apply();
  const clear = (): void => {
    if (content) content.style.transition = 'none';
  };
  content.addEventListener('transitionend', clear, { once: true });
  clearTimeout(zoomAnimTimer);
  zoomAnimTimer = setTimeout(() => {
    zoomAnimTimer = undefined;
    clear();
  }, ZOOM_ANIM_MS + 30);
}

// ── 浮层 DOM 构建 ──

function buildOverlay(svg: SVGElement, pre: HTMLElement): void {
  reduced = prefersReducedMotion();
  originPre = pre;
  openScrollY = window.scrollY;
  closing = false;

  overlay = document.createElement('div');
  overlay.className = 'mermaid-overlay';
  overlay.setAttribute('role', 'dialog');
  overlay.setAttribute('aria-modal', 'true');
  overlay.setAttribute('aria-label', '流程图放大查看');
  overlay.setAttribute('tabindex', '-1');

  // 关闭按钮
  const closeBtn = document.createElement('button');
  closeBtn.className = 'mermaid-overlay-close';
  closeBtn.type = 'button';
  closeBtn.setAttribute('aria-label', '关闭');
  closeBtn.textContent = '✕';

  // SVG 容器（应用 transform）
  content = document.createElement('div');
  content.className = 'mermaid-overlay-content';

  // 克隆 SVG，剥离 max-width 约束，按 viewBox 原始尺寸渲染
  svgClone = svg.cloneNode(true) as SVGSVGElement;
  const vb = svgClone.viewBox?.baseVal;
  if (vb && vb.width > 0 && vb.height > 0) {
    naturalW = vb.width;
    naturalH = vb.height;
  } else {
    // 退化：读取 width/height 属性
    naturalW = svgClone.width.baseVal.value || 800;
    naturalH = svgClone.height.baseVal.value || 600;
  }
  svgClone.style.maxWidth = 'none';
  svgClone.style.width = `${naturalW}px`;
  svgClone.style.height = `${naturalH}px`;
  svgClone.style.display = 'block';
  content.appendChild(svgClone);

  // 底部工具栏
  const toolbar = document.createElement('div');
  toolbar.className = 'mermaid-overlay-toolbar';

  const zoomOut = document.createElement('button');
  zoomOut.className = 'mermaid-overlay-btn';
  zoomOut.type = 'button';
  zoomOut.setAttribute('aria-label', '缩小');
  zoomOut.textContent = '−';

  const zoomReset = document.createElement('button');
  zoomReset.className = 'mermaid-overlay-btn';
  zoomReset.type = 'button';
  zoomReset.setAttribute('aria-label', '重置');
  zoomReset.textContent = '⤢';

  const zoomIn = document.createElement('button');
  zoomIn.className = 'mermaid-overlay-btn';
  zoomIn.type = 'button';
  zoomIn.setAttribute('aria-label', '放大');
  zoomIn.textContent = '+';

  zoomIn.addEventListener('click', (e) => {
    e.stopPropagation();
    animateZoomTo(() => zoomAt(window.innerWidth / 2, window.innerHeight / 2, scale * 1.3));
  });
  zoomOut.addEventListener('click', (e) => {
    e.stopPropagation();
    animateZoomTo(() => zoomAt(window.innerWidth / 2, window.innerHeight / 2, scale / 1.3));
  });
  zoomReset.addEventListener('click', (e) => {
    e.stopPropagation();
    animateZoomTo(() => fitToScreen());
  });

  toolbar.append(zoomOut, zoomReset, zoomIn);

  // 底部提示
  const hint = document.createElement('div');
  hint.className = 'mermaid-overlay-hint';
  hint.textContent = '拖拽移动 · 双击缩放 · 滚动或 ESC 关闭';

  overlay.append(closeBtn, content, toolbar, hint);
  document.body.appendChild(overlay);

  // 初始适配（fitScale / originX / originY 在此确定，飞行数学依赖它们）
  fitToScreen();

  // ── 打开动画（与图片灯箱一致的 FLIP 飞行）──
  if (reduced) {
    // reduced-motion：内容直接 fit 态，整体纯淡入。
    // overlay 的 CSS 关键帧动画被 media query 关掉，用 inline transition 淡入。
    content.style.opacity = '0';
    overlay.style.opacity = '0';
    requestAnimationFrame(() => {
      if (!overlay || !content) return;
      overlay.style.transition = 'opacity 200ms ease-out';
      content.style.transition = 'opacity 200ms ease-out';
      overlay.style.opacity = '1';
      content.style.opacity = '1';
    });
  } else {
    // 首帧：<pre> 里 SVG 的屏幕位置（无 transition），透明
    const fly = flyStateFor(rectOf(svg), originX, originY, naturalW);
    content.style.transition = 'none';
    content.style.opacity = '0';
    scale = fly.scale;
    tx = fly.tx;
    ty = fly.ty;
    applyTransform();
    // 强制 reflow，确保首帧 transform 已提交到渲染层，
    // 否则浏览器可能合并首帧与目标帧，动画从错误位置起跳。
    void content.offsetHeight;

    // double-rAF：第一帧绘制首帧（无动画），第二帧才启动 transition 飞到居中。
    requestAnimationFrame(() => {
      if (!content) return;
      requestAnimationFrame(() => {
        if (!content) return;
        content.style.transition = `transform ${ANIM_MS}ms ease-out, opacity ${ANIM_MS}ms ease-out`;
        content.style.opacity = '1';
        scale = fitScale;
        tx = 0;
        ty = 0;
        applyTransform();
      });
    });

    // 动画结束后清掉 transition，交还 pan/zoom 的即时响应；300ms 兜底防
    // transitionend 不触发（如动画被滚动关闭打断）。关闭动画开始前必须清掉
    // 这个兜底定时器，否则会误清关闭动画的 transition。
    const clearTransition = (): void => {
      if (content) content.style.transition = 'none';
    };
    content.addEventListener('transitionend', clearTransition, { once: true });
    openAnimTimer = setTimeout(() => {
      openAnimTimer = null;
      if (!closing) clearTransition();
    }, ANIM_MS + 50);
  }

  // 焦点移入浮层
  overlay.focus();

  // 绑定交互
  bindInteractions(closeBtn);
}

// ── 交互绑定 ──

function bindInteractions(closeBtn: HTMLButtonElement): void {
  if (!overlay || !content) return;

  // 关闭：✕ 按钮（stopPropagation 防止穿透到背景）
  closeBtn.addEventListener('click', closeOverlay);

  // 关闭：点击背景（非内容区）
  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) closeOverlay();
  });

  // ESC 关闭。持久监听、destroy 时移除——不能用 { once: true }，
  // 否则用户按任意键一次后 Esc 永久失效。
  keyHandler = (e: KeyboardEvent): void => {
    if (e.key === 'Escape' && overlay) {
      e.preventDefault();
      closeOverlay();
    }
  };
  document.addEventListener('keydown', keyHandler);

  // 滚动驱动关闭（与图片灯箱一致）：body 滚动不锁定，任何滚动按进度插值
  // 「居中态 ↔ 原位态」，逐帧读 SVG 实时 rect——文章滚多少图就回多少。
  scrollHandler = (): void => {
    if (!overlay || !content || closing) return;
    if (reduced) {
      // reduced-motion：立即关
      closing = true;
      destroyOverlay();
      return;
    }
    if (!originPre) return;
    const liveSvg = visibleSvg(originPre);
    const originRect = liveSvg ? rectOf(liveSvg) : rectOf(originPre);
    const fly = flyStateFor(originRect, originX, originY, naturalW);
    const dy = Math.abs(window.scrollY - openScrollY);
    const progress = Math.min(dy / SCROLL_CLOSE_PX, 1);
    // 在 fit 态与原位态之间按 progress 线性插值（fit 态 tx/ty 恒为 0）
    content.style.transition = 'none';
    scale = fitScale + (fly.scale - fitScale) * progress;
    tx = fly.tx * progress;
    ty = fly.ty * progress;
    applyTransform();
    content.style.opacity = String(1 - progress);
    overlay.style.opacity = String(1 - progress);
    if (progress >= 1) {
      // 已飞回原位：文章停在当前滚动位置，移除浮层
      closing = true;
      destroyOverlay();
    }
  };
  window.addEventListener('scroll', scrollHandler, { passive: true });

  // 双击：切换适配 / 原始大小
  content.addEventListener('dblclick', (e) => {
    const px = e.clientX;
    const py = e.clientY;
    if (scale < 0.99) {
      // 当前是适配模式 → 放大到 100%（原始尺寸），锚定双击点
      animateZoomTo(() => zoomAt(px, py, 1));
    } else {
      animateZoomTo(() => fitToScreen());
    }
  });

  // Pointer Events：统一鼠标 + 触摸
  content.addEventListener('pointerdown', onPointerDown);
  content.addEventListener('pointermove', onPointerMove);
  content.addEventListener('pointerup', onPointerUp);
  content.addEventListener('pointercancel', onPointerUp);
  // 阻止浏览器默认触摸手势（滚动 / 捏合缩放页面）
  content.style.touchAction = 'none';

  // 滚轮：只响应 Ctrl/⌘+滚轮（触控板捏合手势上报为 ctrlKey wheel）做缩放。
  // 普通滚轮不拦截——留给页面滚动，由滚动关闭接管（与图片灯箱统一）。
  content.addEventListener('wheel', onWheel, { passive: false });

  // 窗口尺寸变化时重新适配
  window.addEventListener('resize', onResize);
}

/** 移除 document/window 级监听（浮层 DOM 上的监听随元素移除自然失效）。 */
function cleanupInteractions(): void {
  if (scrollHandler) {
    window.removeEventListener('scroll', scrollHandler);
    scrollHandler = null;
  }
  if (keyHandler) {
    document.removeEventListener('keydown', keyHandler);
    keyHandler = null;
  }
  window.removeEventListener('resize', onResize);
}

function onPointerDown(e: PointerEvent): void {
  if (!content) return;
  content.setPointerCapture(e.pointerId);
  pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });

  if (pointers.size === 1) {
    panStart = { x: e.clientX, y: e.clientY, tx, ty };
  } else if (pointers.size === 2) {
    panStart = null; // 切到 pinch 模式
    const pts = [...pointers.values()];
    pinchStart = {
      dist: Math.hypot(pts[1].x - pts[0].x, pts[1].y - pts[0].y),
      cx: (pts[0].x + pts[1].x) / 2,
      cy: (pts[0].y + pts[1].y) / 2,
      scale,
      tx,
      ty,
    };
  }
}

function onPointerMove(e: PointerEvent): void {
  if (!pointers.has(e.pointerId)) return;
  pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });

  if (pointers.size === 1 && panStart) {
    // 单指拖拽 = 平移
    tx = panStart.tx + (e.clientX - panStart.x);
    ty = panStart.ty + (e.clientY - panStart.y);
    applyTransform();
  } else if (pointers.size >= 2 && pinchStart) {
    // 双指 = 捏合缩放（以两指中心为锚点）
    const pts = [...pointers.values()];
    const curDist = Math.hypot(pts[1].x - pts[0].x, pts[1].y - pts[0].y);
    const newScale = (pinchStart.scale * curDist) / pinchStart.dist;
    // 锚定初始捏合中心
    const cx = (pinchStart.cx - originX - pinchStart.tx) / pinchStart.scale;
    const cy = (pinchStart.cy - originY - pinchStart.ty) / pinchStart.scale;
    const clamped = clamp(newScale, MIN_SCALE, MAX_SCALE);
    tx = pinchStart.cx - originX - clamped * cx;
    ty = pinchStart.cy - originY - clamped * cy;
    scale = clamped;
    applyTransform();
  }
}

function onPointerUp(e: PointerEvent): void {
  pointers.delete(e.pointerId);
  if (pointers.size < 2) pinchStart = null;
  if (pointers.size === 1) {
    // 从 pinch 回落到单指，重新设 pan 起点
    const pt = [...pointers.values()][0];
    panStart = { x: pt.x, y: pt.y, tx, ty };
  }
  if (pointers.size === 0) panStart = null;
}

function onWheel(e: WheelEvent): void {
  // 触控板捏合 / Ctrl+滚轮 = 缩放；普通滚轮不拦截，留给页面滚动触发滚动关闭。
  if (!e.ctrlKey && !e.metaKey) return;
  e.preventDefault();
  const factor = e.deltaY > 0 ? 1 / 1.15 : 1.15;
  zoomAt(e.clientX, e.clientY, scale * factor);
}

function onResize(): void {
  if (overlay) fitToScreen();
}

// ── 开/关浮层 ──

/** 同步关闭浮层并清理全部状态（无动画延迟）。 */
function destroyOverlay(): void {
  cleanupInteractions();
  if (openAnimTimer) {
    clearTimeout(openAnimTimer);
    openAnimTimer = null;
  }
  clearTimeout(zoomAnimTimer);
  zoomAnimTimer = undefined;
  if (overlay) overlay.remove();
  overlay = null;
  content = null;
  svgClone = null;
  pointers.clear();
  panStart = null;
  pinchStart = null;
  closing = false;

  // 焦点归还：pre 默认不可聚焦，补 tabindex 后用 preventScroll 抑制
  // focus() 默认的 scrollIntoView（否则关闭后页面自动滚动把图纳入视口）。
  const pre = originPre;
  originPre = null;
  if (pre?.isConnected) {
    pre.setAttribute('tabindex', '-1');
    pre.focus({ preventScroll: true });
  }
}

/**
 * 关闭浮层（带飞回动画，与图片灯箱一致）。
 * 内容 250ms 飞回 <pre> 里 SVG 的实时位置并淡出，overlay 同步 CSS 淡出；
 * 状态在动画结束后统一清理。
 */
function closeOverlay(): void {
  if (!overlay || closing) return;
  closing = true;
  // 停止滚动监听，防止飞回动画与滚动插值互相打架
  cleanupInteractions();
  if (openAnimTimer) {
    clearTimeout(openAnimTimer);
    openAnimTimer = null;
  }
  // 缩放过渡的清理兜底也不能留，否则会误清下面关闭动画的 transition
  clearTimeout(zoomAnimTimer);
  zoomAnimTimer = undefined;

  const liveSvg = originPre ? visibleSvg(originPre) : null;
  if (reduced || !originPre || !originPre.isConnected) {
    // reduced-motion 或原始节点已离开 DOM：直接移除
    destroyOverlay();
    return;
  }

  const originRect = liveSvg ? rectOf(liveSvg) : rectOf(originPre);
  const fly = flyStateFor(originRect, originX, originY, naturalW);
  if (content) {
    content.style.transition = `transform ${ANIM_MS}ms ease-out, opacity ${ANIM_MS}ms ease-out`;
    content.style.opacity = '0';
    scale = fly.scale;
    tx = fly.tx;
    ty = fly.ty;
    applyTransform();
  }
  overlay.classList.add('mermaid-overlay-closing');

  const el = overlay;
  const done = (): void => {
    if (overlay === el) destroyOverlay();
  };
  // 兜底定时器防 transitionend 不触发
  const timer = setTimeout(done, ANIM_MS + 30);
  content?.addEventListener(
    'transitionend',
    (): void => {
      clearTimeout(timer);
      done();
    },
    { once: true },
  );
}

// ── 对外接口 ──

/**
 * 为服务端渲染的 mermaid 图表容器绑定点击放大。
 *
 * 仅绑定一次（dataset 守卫）；主题切换由 CSS 切换两份 SVG，无需重新绑定。
 */
export function attachOverlayTrigger(pre: HTMLElement): void {
  if (pre.dataset.mermaidOverlayReady) return;
  pre.dataset.mermaidOverlayReady = 'true';

  pre.addEventListener('click', () => {
    if (overlay) return; // 已有浮层打开，忽略
    const svg = visibleSvg(pre);
    if (!svg) return;
    buildOverlay(svg, pre);
  });
}

/** 测试用：同步重置浮层状态（移除 DOM + 清空单例变量）。 */
export function _resetOverlay(): void {
  destroyOverlay();
}
