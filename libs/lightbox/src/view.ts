import {
  type BaseViewGeometry,
  baseViewGeometry,
  clampPanToViewport,
  clampScale,
  type Inset,
  matIdentity,
  nextRotationDeg,
  type Rect,
  SCALE_MAX,
  SCALE_MIN,
  stepZoomLevel,
  viewTransformCss,
  zoomAround,
} from './geometry';
import type { LightboxState } from './types';

export function rectOf(el: Element): Rect {
  const r = el.getBoundingClientRect();
  return {
    x: r.left,
    y: r.top,
    w: r.width,
    h: r.height,
  };
}

export function resetScrollClose(s: LightboxState): void {
  s.openScrollY = window.scrollY;
  if (!s.scrollClose) return;
  s.scrollClose = null;
  s.img.style.opacity = '1';
  s.overlay.style.transition = 'opacity 180ms ease-out';
  s.overlay.style.opacity = '1';
}

export function isErrorShown(s: LightboxState): boolean {
  return s.errorBox.style.display !== 'none';
}

// 基态几何：读视口/位图尺寸，委托给纯函数 baseViewGeometry。
// 布局盒恒为未旋转 fit（旋转不变），90°/270° 的适配折进 k，消灭 width/height
// 瞬时交换带来的跳变。
export function baseGeometry(s: LightboxState): BaseViewGeometry {
  const natW = s.img.naturalWidth || 1;
  const natH = s.img.naturalHeight || 1;
  return baseViewGeometry(natW, natH, s.view.deg, window.innerWidth, window.innerHeight, s.inset);
}

// 适配区域边距：上下取顶栏底边与底栏高度中较大的一侧（对称，图片仍绕视口中心
// 适配），左右在宽屏让出导航箭头；都不低于缺省的 4% / 6%。窄屏箭头叠在图上。
const CHROME_GAP = 16;
export function measureInset(s: Pick<LightboxState, 'topbar' | 'bottom' | 'prevBtn'>): Inset {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const top = s.topbar.getBoundingClientRect().bottom;
  const bottomRect = s.bottom.getBoundingClientRect();
  const bottom = bottomRect.height > 0 ? vh - bottomRect.top : 0;
  const nav = s.prevBtn && vw >= 640 ? s.prevBtn.getBoundingClientRect().right : 0;
  return {
    x: Math.max(vw * 0.04, nav > 0 ? nav + CHROME_GAP : 0),
    y: Math.max(vh * 0.06, top > 0 ? top + CHROME_GAP : 0, bottom > 0 ? bottom + CHROME_GAP : 0),
  };
}

// 把 view 写到 DOM。animate=true 走 180ms ease-out（按钮/旋转/重置/回弹）；
// false 直出（滚轮/拖拽/捏合需 <200ms 响应）。每次写入都重算 fit 并钳制平移。
// 变换写定长同构函数列表（viewTransformCss）：CSS 逐函数插值，rotate 槽绕
// 「布局盒中心→视口中心」不动轴进行，旋转全程中心锁定。
export function applyView(s: LightboxState, animate: boolean): void {
  resetScrollClose(s);
  const { k, layoutW, layoutH, fit, m0 } = baseGeometry(s);
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
  s.img.style.transform = viewTransformCss(
    user,
    s.view.deg,
    k,
    layoutW,
    layoutH,
    window.innerWidth,
    window.innerHeight,
  );
  // 关闭飞回/图集切换的基准同步：baseW/H 恒为当前布局盒尺寸
  s.target = fit;
  s.baseW = layoutW;
  s.baseH = layoutH;
  s.img.classList.toggle('is-zoomed', s.view.scale > 1);
  syncZoomControls(s);
}

// 工具组的档位文字与两端禁用态跟随当前倍率。
export function syncZoomControls(s: LightboxState): void {
  const pct = `${Math.round(s.view.scale * 100)}%`;
  if (s.zoomLevel.textContent !== pct) {
    s.zoomLevel.textContent = pct;
    s.zoomLevel.setAttribute('aria-label', `${pct}，重置视图`);
  }
  s.zoomOutBtn.disabled = s.view.scale <= SCALE_MIN + 1e-3;
  s.zoomInBtn.disabled = s.view.scale >= SCALE_MAX - 1e-3;
}

// 首次操控前把布局盒从「飞行基准」（originRect 尺寸 + 非均匀 scale）归一到
// 「fit 布局盒 + 定长函数列表」的基态——两者视觉完全一致，跳变不可见。
export function ensureNormalized(s: LightboxState): void {
  if (s.normalized) return;
  s.normalized = true;
  applyView(s, false);
  // 强制 reflow：归一化基态（fit 布局盒 + 列表字符串）必须在此提交绘制。
  // 否则它与随后的 applyView(true) 合并在同一帧，180ms 过渡的起算值仍是飞行
  // 动画遗留的 translate+scale 字符串 —— 首个缩放/旋转动画出现非均匀 scale
  // 回弹与起始帧跳变（素材页正方形缩略图打开竖图时最明显）。
  void s.img.offsetHeight;
}

// 顶栏、底栏与箭头空闲淡出：fit 态它们在图片之外，常驻；放大后图会铺到它们
// 下面，2.5s 无指针活动且不在栏上、不在手势中时淡出，任何指针活动唤醒。
export function pokeToolbar(s: LightboxState): void {
  if (s.closing) return;
  s.overlay.classList.remove('is-idle');
  s.scope.clearTimeout(s.idleTimer);
  s.idleTimer = s.scope.timeout((): void => {
    if (s.scope.disposed || s.chromeHover || s.gesture || s.view.scale <= 1) return;
    s.overlay.classList.add('is-idle');
  }, 2500);
}

export function zoomBy(
  s: LightboxState,
  k: number,
  px: number,
  py: number,
  animate: boolean,
): void {
  if (!s?.target || s.closing || isErrorShown(s)) return;
  const next = clampScale(s.view.scale * k);
  if (next === s.view.scale) return; // 已到顶/到底
  ensureNormalized(s);
  s.view.user = zoomAround(s.view.user ?? matIdentity(), next / s.view.scale, px, py);
  s.view.scale = next;
  applyView(s, animate);
  pokeToolbar(s);
}

// 按钮与 +/- 键：走到相邻档位，锚定视口中心。
export function stepZoom(s: LightboxState, dir: 1 | -1): void {
  const goal = stepZoomLevel(s.view.scale, dir);
  zoomBy(s, goal / s.view.scale, window.innerWidth / 2, window.innerHeight / 2, true);
}

export function rotate(s: LightboxState, dir: 1 | -1): void {
  if (!s?.target || s.closing || isErrorShown(s)) return;
  ensureNormalized(s);
  s.view.deg = nextRotationDeg(s.view.deg, dir);
  // 旋转后保持倍率、绕视口中心重锚定、平移清零（避免旋转把图甩出视口）
  s.view.user = zoomAround(
    matIdentity(),
    s.view.scale,
    window.innerWidth / 2,
    window.innerHeight / 2,
  );
  applyView(s, true);
  pokeToolbar(s);
}

export function resetView(s: LightboxState, animate: boolean): void {
  if (!s?.target || s.closing || isErrorShown(s)) return;
  ensureNormalized(s);
  // 归位取最近整圈：rotate 槽按累计角插值，450→360 只回摆 90°（而非 450°）。
  s.view = { deg: Math.round(s.view.deg / 360) * 360, scale: 1, user: null };
  applyView(s, animate);
  pokeToolbar(s);
}

// 下载链接跟随当前图：同源 /uploads 用 download 属性存原图；
// 外链 download 跨域无效，改新标签页打开。
export function updateDownloadLink(s: LightboxState): void {
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
