export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

// 计算图片在视口居中、contain 适配后的目标 rect。
// naturalW/H: 图片真实像素尺寸；vw/vh: 视口尺寸。
export function fitCentered(naturalW: number, naturalH: number, vw: number, vh: number): Rect {
  var maxW = vw * 0.92;
  var maxH = vh * 0.88;
  var scale = Math.min(maxW / naturalW, maxH / naturalH, 1);
  var w = naturalW * scale;
  var h = naturalH * scale;
  return {
    x: (vw - w) / 2,
    y: (vh - h) / 2,
    w: w,
    h: h,
  };
}

// 把目标 rect 转成 transform 字符串。
// baseW/baseH 是 img 元素的布局尺寸（=居中目标尺寸），scale 相对它缩放。
// transform-origin 为 top left（见 CSS），translate 到 rect 左上角后 scale。
// - 居中态：scale=1（base 就是居中尺寸）
// - originRect 态：scale = originRect.w / base.w（缩小）
export function transformFor(rect: Rect, baseW: number, baseH: number): string {
  var sx = baseW > 0 ? rect.w / baseW : 1;
  var sy = baseH > 0 ? rect.h / baseH : 1;
  return 'translate(' + rect.x + 'px,' + rect.y + 'px) scale(' + sx + ',' + sy + ')';
}

// 原图 URL = data-src 去 query。data-src 形如 "/uploads/x.webp?w=800"。
// 外链 URL（如 https://... 或 http://...）完整保留 query 参数，防止剥离签名/Token 导致 403/404。
export function originalUrl(dataSrc: string | null): string {
  if (!dataSrc) return '';
  if (dataSrc.startsWith('/uploads/') || dataSrc.startsWith('uploads/')) {
    return dataSrc.split('?')[0];
  }
  return dataSrc;
}

// ============ 查看器操控：2D 矩阵与手势数学 ============
//
// 灯箱打开/关闭的飞行动画仍走 transformFor（translate+scale 字符串，支持非均匀
// 缩放）；用户一旦缩放/旋转/平移，静止态变换统一由 viewTransformCss 的
// 「定长同构函数列表」表达：
//   translate(u.e,u.f) scale(u.s) translate(C) rotate(deg) scale(k) translate(−c)
// 其中 C=视口中心、c=布局盒中心（布局盒恒为未旋转 fit 尺寸，不随旋转交换），
// k=rotationFitScale（90°/270° 时把旋转后的视觉尺寸收回 fitCentered）。
//
// 为什么不用 matrix()：CSS Transitions 对 matrix()↔matrix() 走「分解为
// translate/rotate/scale 各自线性插值再重组」，绕中心旋转所需的 pivot 补偿
// 混在 translate 槽里被线性化，过渡中点中心会甩出视口中心（曾偏离 ~124px）。
// 定长同构列表触发逐函数插值：translate 槽两端恒定，rotate 绕不动点
// 「布局盒中心→视口中心」进行，全程中心锁定。transform-origin 恒为 top left。
//
// M0（基态矩阵，仍用于 dragclose/clampPan 的几何计算）= T(C)·R·S(k)·T(−c)；
// M_user（用户矩阵）承载缩放锚点与平移，恒为无旋转相似（b=c=0, a=d）。
// 按钮/旋转/重置的动画用 transition，滚轮/拖拽直出（<200ms 响应）。

// 2D 仿射矩阵（等价 CSS matrix(a,b,c,d,e,f)）：x' = a·x + c·y + e，y' = b·x + d·y + f。
export interface Mat {
  a: number;
  b: number;
  c: number;
  d: number;
  e: number;
  f: number;
}

export function matIdentity(): Mat {
  return { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 };
}

export function matTranslate(x: number, y: number): Mat {
  return { a: 1, b: 0, c: 0, d: 1, e: x, f: y };
}

export function matScale(k: number): Mat {
  return { a: k, b: 0, c: 0, d: k, e: 0, f: 0 };
}

// 旋转（角度制，顺时针为正，与 CSS rotate 一致）。
// 90° 步进的 cos/sin 会产生 6e-17 级浮点噪声，吸附到 0，保证输出矩阵干净。
export function matRotateDeg(deg: number): Mat {
  const rad = (deg * Math.PI) / 180;
  const snap = (v: number): number => (Math.abs(v) < 1e-10 ? 0 : v);
  const cos = snap(Math.cos(rad));
  const sin = snap(Math.sin(rad));
  return { a: cos, b: sin, c: -sin, d: cos, e: 0, f: 0 };
}

// 矩阵复合 m ∘ n：点先被 n 变换，再被 m 变换（与 CSS transform 列表左→右一致）。
export function matMultiply(m: Mat, n: Mat): Mat {
  return {
    a: m.a * n.a + m.c * n.b,
    b: m.b * n.a + m.d * n.b,
    c: m.a * n.c + m.c * n.d,
    d: m.b * n.c + m.d * n.d,
    e: m.a * n.e + m.c * n.f + m.e,
    f: m.b * n.e + m.d * n.f + m.f,
  };
}

export function matApply(m: Mat, x: number, y: number): { x: number; y: number } {
  return { x: m.a * x + m.c * y + m.e, y: m.b * x + m.d * y + m.f };
}

// 输出 CSS matrix()。坐标保留 4 位小数（约 0.0001px 精度），避免浮点尾数噪声。
export function matToCss(m: Mat): string {
  const r = (v: number): number => Math.round(v * 1e4) / 1e4;
  return `matrix(${r(m.a)}, ${r(m.b)}, ${r(m.c)}, ${r(m.d)}, ${r(m.e)}, ${r(m.f)})`;
}

// 以视口点 p 为锚点缩放：p 下的图像素在缩放前后保持不动。
export function zoomAround(user: Mat, k: number, px: number, py: number): Mat {
  return matMultiply(
    matTranslate(px, py),
    matMultiply(matScale(k), matMultiply(matTranslate(-px, -py), user)),
  );
}

// 平移（拖拽平移）：在视觉空间直接附加位移。
export function panBy(user: Mat, dx: number, dy: number): Mat {
  return matMultiply(matTranslate(dx, dy), user);
}

// ---- 缩放档位 ----

export const SCALE_MIN = 1; // 最小 = 适应窗口
export const SCALE_MAX = 8; // 相对 fit 的最大倍率
export const BUTTON_ZOOM_STEP = 1.5; // 工具栏 ± 按钮步进
export const DOUBLE_CLICK_SCALE = 2.5; // 双击/双敲击放大的目标倍率
// 竖直拖拽关闭的行程阈值（px）：与旧「滚动关闭」的 120px 行程对齐。
export const DRAG_CLOSE_PX = 120;

export function clampScale(s: number): number {
  return Math.min(SCALE_MAX, Math.max(SCALE_MIN, s));
}

// 归一化到 [0, 360)。几何计算（fit/宽高互换）只看等效角。
export function normDeg(deg: number): number {
  return ((deg % 360) + 360) % 360;
}

// 累计旋转角步进（不取模）：CSS rotate() 按数值插值，270→360 才是顺时针 90°；
// 若取模成 270→0，过渡会反向空转 270°。
export function nextRotationDeg(deg: number): number {
  return deg + 90;
}

// 旋转后的适配缩放：布局盒恒为未旋转 fit 尺寸（不随旋转交换），
// 90°/270° 时视觉宽高互换，需缩小 k 使旋转后仍恰好 fitCentered。
// 布局盒与旋转后 fit 的宽高比互为倒数，两个比值相等，取 min 仅为浮点稳妥。
export function rotationFitScale(
  naturalW: number,
  naturalH: number,
  rot: number,
  vw: number,
  vh: number,
): number {
  if (normDeg(rot) % 180 === 0) return 1;
  const base = fitCentered(naturalW, naturalH, vw, vh);
  const fit = fitCentered(naturalH, naturalW, vw, vh);
  return Math.min(fit.w / base.h, fit.h / base.w);
}

export interface BaseViewGeometry {
  // 基态矩阵 M0 = T(视口中心)·R(rot)·S(k)·T(−布局盒中心)，origin top left。
  m0: Mat;
  // 旋转适配缩放（viewTransformCss 的第 5 槽）。
  k: number;
  // 布局盒尺寸 = 未旋转 fitCentered（旋转不变，消灭 width/height 瞬时跳变）。
  layoutW: number;
  layoutH: number;
  // 旋转后的视觉 fit rect（close/resize 的基准）。
  fit: Rect;
}

// 查看器基态几何：deg 可为累计角（≥360），内部归一。
export function baseViewGeometry(
  naturalW: number,
  naturalH: number,
  deg: number,
  vw: number,
  vh: number,
): BaseViewGeometry {
  const rot = normDeg(deg);
  const layout = fitCentered(naturalW, naturalH, vw, vh);
  const eff = effectiveDims(naturalW, naturalH, rot);
  const fit = fitCentered(eff.w, eff.h, vw, vh);
  const k = rotationFitScale(naturalW, naturalH, rot, vw, vh);
  const m0 = matMultiply(
    matTranslate(vw / 2, vh / 2),
    matMultiply(
      matRotateDeg(rot),
      matMultiply(matScale(k), matTranslate(-layout.w / 2, -layout.h / 2)),
    ),
  );
  return { m0, k, layoutW: layout.w, layoutH: layout.h, fit };
}

// 查看器变换的规范字符串：定长同构 6 函数列表（见模块头注释）。
// deg 用累计角（插值方向正确）；user 恒为无旋转相似（zoomAround/panBy/
// clampPanToViewport 保持 {a=s, b=0, c=0, d=s} 形）。坐标保留 4 位小数。
export function viewTransformCss(
  user: Mat,
  deg: number,
  k: number,
  layoutW: number,
  layoutH: number,
  vw: number,
  vh: number,
): string {
  const r = (v: number): number => Math.round(v * 1e4) / 1e4;
  return (
    `translate(${r(user.e)}px, ${r(user.f)}px) scale(${r(user.a)}) ` +
    `translate(${r(vw / 2)}px, ${r(vh / 2)}px) rotate(${r(deg)}deg) scale(${r(k)}) ` +
    `translate(${r(-layoutW / 2)}px, ${r(-layoutH / 2)}px)`
  );
}

// 关闭飞回目标：与 viewTransformCss 同构的定长 6 函数列表。
// 外层 translate/scale 把布局盒映射到缩略图 rect（支持非均匀，对齐 transformFor）；
// 内层 rotate(deg)/scale(k) 两端取值与当前操控态完全相等 —— 逐函数插值时
// 旋转槽恒不动，累计角（如 900°）不会在关闭动画里反向空转。
// 若目标仍用 transformFor 的 translate+scale 两函数串，函数列表结构不匹配，
// 浏览器回退矩阵插值：rotate(900°) 在矩阵上 ≡ 180°，关闭全程可见回 spin。
// 第 3 槽 translate 从「视口中心」插值到「布局盒中心」，旋转轴随图平滑滑向缩略图。
export function closeFlightTransform(
  rect: Rect,
  layoutW: number,
  layoutH: number,
  deg: number,
  k: number,
): string {
  const r = (v: number): number => Math.round(v * 1e4) / 1e4;
  const sx = layoutW > 0 ? rect.w / layoutW : 1;
  const sy = layoutH > 0 ? rect.h / layoutH : 1;
  return (
    `translate(${r(rect.x)}px, ${r(rect.y)}px) scale(${r(sx)}, ${r(sy)}) ` +
    `translate(${r(layoutW / 2)}px, ${r(layoutH / 2)}px) rotate(${r(deg)}deg) scale(${r(k)}) ` +
    `translate(${r(-layoutW / 2)}px, ${r(-layoutH / 2)}px)`
  );
}

// 旋转后的有效视觉尺寸：90°/270° 时宽高互换（fit 计算用）。
export function effectiveDims(
  naturalW: number,
  naturalH: number,
  rot: number,
): { w: number; h: number } {
  return rot % 180 === 0 ? { w: naturalW, h: naturalH } : { w: naturalH, h: naturalW };
}

// 滚轮缩放缓冲：deltaMode 1 = 行滚动（×16 折算成像素），2 = 页滚动（×100）。
// 每 100px 滚轮行程对应 ×1.2 倍率；单次钳到 [0.6, 1.7] 防止触控板暴冲。
export function wheelZoomFactor(deltaY: number, deltaMode: number): number {
  const px = deltaMode === 1 ? deltaY * 16 : deltaMode === 2 ? deltaY * 100 : deltaY;
  const k = 1.2 ** (-px / 100);
  return Math.min(1.7, Math.max(0.6, k));
}

// 平移边界钳制：图大于视口的轴不允许露出黑边（图边贴视口边）；
// 图小于视口的轴强制居中（缩回 fit 时自动归位）。返回修正后的 user 矩阵。
export function clampPanToViewport(
  user: Mat,
  base: Mat,
  layoutW: number,
  layoutH: number,
  vw: number,
  vh: number,
): Mat {
  const total = matMultiply(user, base);
  const p1 = matApply(total, 0, 0);
  const p2 = matApply(total, layoutW, 0);
  const p3 = matApply(total, 0, layoutH);
  const p4 = matApply(total, layoutW, layoutH);
  const minX = Math.min(p1.x, p2.x, p3.x, p4.x);
  const maxX = Math.max(p1.x, p2.x, p3.x, p4.x);
  const minY = Math.min(p1.y, p2.y, p3.y, p4.y);
  const maxY = Math.max(p1.y, p2.y, p3.y, p4.y);

  let dx = 0;
  if (maxX - minX > vw) {
    if (minX > 0) dx = -minX;
    else if (maxX < vw) dx = vw - maxX;
  } else {
    dx = vw / 2 - (minX + maxX) / 2;
  }
  let dy = 0;
  if (maxY - minY > vh) {
    if (minY > 0) dy = -minY;
    else if (maxY < vh) dy = vh - maxY;
  } else {
    dy = vh / 2 - (minY + maxY) / 2;
  }
  if (dx === 0 && dy === 0) return user;
  return matMultiply(matTranslate(dx, dy), user);
}
