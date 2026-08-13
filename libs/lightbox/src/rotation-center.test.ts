// @vitest-environment node
/**
 * 旋转中心回归测试 + 现状复现。
 *
 * 用户症状：灯箱里旋转照片时，照片不是绕自身中心原地旋转，而是甩出去转。
 *
 * happy-dom/jsdom 不实现 CSS transition 插值，因此本文件内置一个
 * CSS Transforms Level 1 §9 的插值模拟器（浏览器算法）：
 * 1. 两端均为定长函数列表且逐位同型（translate↔translate / rotate↔rotate /
 *    scale↔scale）→ 逐函数数值插值；
 * 2. 否则（matrix()、长度/类型不匹配）→ 各自分解为 translate/rotate/scale
 *    线性插值后重组（transform-origin 为 top left，见 style.css .lightbox-img）。
 *
 * 用模拟器计算过渡中点的「布局盒中心的视觉位置」，断言它恒等于视口中心。
 */
import { describe, expect, it } from 'vitest';
import {
  baseViewGeometry,
  clampPanToViewport,
  type Mat,
  matApply,
  matIdentity,
  matMultiply,
  matRotateDeg,
  matTranslate,
  nextRotationDeg,
  normDeg,
  panBy,
  viewTransformCss,
  zoomAround,
} from './geometry';

// ============ CSS 插值模拟器（测试专用） ============

interface TransformFn {
  fn: string;
  args: number[];
}

/** 解析 transform 字符串为函数列表：translate(1px, 2px) rotate(90deg) ... */
function parseTransformList(css: string): TransformFn[] {
  const out: TransformFn[] = [];
  const re = /(matrix|translate|rotate|scale)\(([^)]*)\)/g;
  for (;;) {
    const m = re.exec(css);
    if (m === null) break;
    const args = m[2].split(',').map((s) => Number.parseFloat(s.trim()));
    out.push({ fn: m[1], args });
  }
  if (out.length === 0) throw new Error(`无法解析 transform: ${css}`);
  return out;
}

/** 函数列表 → 2D 矩阵（左→右依次复合，与 CSS 一致）。 */
function fnsToMat(fns: TransformFn[]): Mat {
  let acc = matIdentity();
  for (const { fn, args } of fns) {
    let m: Mat;
    if (fn === 'matrix') {
      m = { a: args[0], b: args[1], c: args[2], d: args[3], e: args[4], f: args[5] };
    } else if (fn === 'translate') {
      m = matTranslate(args[0], args[1] ?? 0);
    } else if (fn === 'rotate') {
      m = matRotateDeg(args[0]);
    } else if (fn === 'scale') {
      const sy = args.length > 1 ? args[1] : args[0];
      m = { a: args[0], b: 0, c: 0, d: sy, e: 0, f: 0 };
    } else {
      throw new Error(`未支持的函数: ${fn}`);
    }
    acc = matMultiply(acc, m);
  }
  return acc;
}

/** CSS2D 分解（相似矩阵：translate + rotate + scale，skew=0）。 */
function decompose(m: Mat): { tx: number; ty: number; rot: number; sx: number; sy: number } {
  const sx = Math.hypot(m.a, m.b);
  const rot = (Math.atan2(m.b, m.a) * 180) / Math.PI;
  const det = m.a * m.d - m.b * m.c;
  return { tx: m.e, ty: m.f, rot, sx, sy: det / sx };
}

const lerp = (a: number, b: number, t: number): number => a + (b - a) * t;

/** 浏览器 transition 插值：同构列表逐函数插值，否则分解-插值-重组。 */
function interpolateTransform(fromCss: string, toCss: string, t: number): Mat {
  const from = parseTransformList(fromCss);
  const to = parseTransformList(toCss);
  const pairwise =
    from.length === to.length &&
    from.every((f, i) => f.fn === to[i].fn && f.args.length === to[i].args.length);
  if (pairwise && from[0].fn !== 'matrix') {
    const lerped = from.map((f, i) => ({
      fn: f.fn,
      args: f.args.map((a, j) => lerp(a, to[i].args[j], t)),
    }));
    return fnsToMat(lerped);
  }
  const a = decompose(fnsToMat(from));
  const b = decompose(fnsToMat(to));
  const m = matMultiply(
    matTranslate(lerp(a.tx, b.tx, t), lerp(a.ty, b.ty, t)),
    matMultiply(matRotateDeg(lerp(a.rot, b.rot, t)), {
      a: lerp(a.sx, b.sx, t),
      b: 0,
      c: 0,
      d: lerp(a.sy, b.sy, t),
      e: 0,
      f: 0,
    }),
  );
  return m;
}

/** 过渡 t 时刻布局盒中心的视觉位置（transform-origin: top left）。 */
function centerAt(fromCss: string, toCss: string, t: number, layoutW: number, layoutH: number) {
  return matApply(interpolateTransform(fromCss, toCss, t), layoutW / 2, layoutH / 2);
}

// ============ 场景常量 ============

const VW = 1000;
const VH = 800;
const NAT_W = 1600; // 横向照片
const NAT_H = 900;
const CX = VW / 2;
const CY = VH / 2;
const T_STEPS = [0, 0.25, 0.5, 0.75, 1];

// ============ 回归锁：旋转必须绕视口中心原地进行 ============

describe('回归锁：定长函数列表 + 布局盒旋转不变 ⇒ 绕中心旋转', () => {
  it('布局盒尺寸不随旋转交换（消灭 width/height 瞬时跳变）', () => {
    const g0 = baseViewGeometry(NAT_W, NAT_H, 0, VW, VH);
    const g90 = baseViewGeometry(NAT_W, NAT_H, 90, VW, VH);
    const g180 = baseViewGeometry(NAT_W, NAT_H, 180, VW, VH);
    const g270 = baseViewGeometry(NAT_W, NAT_H, 270, VW, VH);
    for (const g of [g90, g180, g270]) {
      expect(g.layoutW).toBeCloseTo(g0.layoutW, 6);
      expect(g.layoutH).toBeCloseTo(g0.layoutH, 6);
    }
  });

  it('rotationFitScale：90°/270° 缩小到旋转后 fit，0°/180° 为 1', () => {
    expect(baseViewGeometry(NAT_W, NAT_H, 0, VW, VH).k).toBe(1);
    expect(baseViewGeometry(NAT_W, NAT_H, 180, VW, VH).k).toBe(1);
    // 1600×900 @1000×800：fit=920×517.5；旋转后 fit(900×1600)=396×704
    // k = 704/920 = 396/517.5 ≈ 0.7652
    expect(baseViewGeometry(NAT_W, NAT_H, 90, VW, VH).k).toBeCloseTo(0.7652, 4);
    expect(baseViewGeometry(NAT_W, NAT_H, 270, VW, VH).k).toBeCloseTo(0.7652, 4);
  });

  it('viewTransformCss 与 m0 矩阵视觉等价（dragclose/clamp 仍用 m0）', () => {
    for (const deg of [0, 90, 180, 270, 360]) {
      const g = baseViewGeometry(NAT_W, NAT_H, deg, VW, VH);
      const css = viewTransformCss(matIdentity(), deg, g.k, g.layoutW, g.layoutH, VW, VH);
      const m = fnsToMat(parseTransformList(css));
      // 字符串槽位保留 4 位小数，复合后误差 <0.01px（亚像素，视觉不可见）
      for (const key of ['a', 'b', 'c', 'd', 'e', 'f'] as const) {
        expect(Math.abs(m[key] - g.m0[key])).toBeLessThan(0.01);
      }
    }
  });

  it('累计旋转角：270° 的下一步是 360° 而非 0°（CSS 数值插值方向）', () => {
    expect(nextRotationDeg(0)).toBe(90);
    expect(nextRotationDeg(270)).toBe(360);
    expect(normDeg(360)).toBe(0);
    expect(normDeg(450)).toBe(90);
  });

  it('user 矩阵恒为无旋转相似（zoomAround/panBy/clampPan 保持 b=c=0, a=d）', () => {
    const g = baseViewGeometry(NAT_W, NAT_H, 0, VW, VH);
    let u = zoomAround(matIdentity(), 2.5, CX, CY);
    u = panBy(u, 120, -40);
    u = clampPanToViewport(u, g.m0, g.layoutW, g.layoutH, VW, VH);
    expect(u.b).toBe(0);
    expect(u.c).toBe(0);
    expect(u.a).toBe(u.d);
  });

  it('四个旋转步进全程中心锁定视口中心（scale=1，无平移）', () => {
    let deg = 0;
    for (let step = 0; step < 4; step++) {
      const next = nextRotationDeg(deg);
      const gFrom = baseViewGeometry(NAT_W, NAT_H, deg, VW, VH);
      const gTo = baseViewGeometry(NAT_W, NAT_H, next, VW, VH);
      const fromCss = viewTransformCss(
        matIdentity(),
        deg,
        gFrom.k,
        gFrom.layoutW,
        gFrom.layoutH,
        VW,
        VH,
      );
      const toCss = viewTransformCss(matIdentity(), next, gTo.k, gTo.layoutW, gTo.layoutH, VW, VH);
      for (const t of T_STEPS) {
        const c = centerAt(fromCss, toCss, t, gTo.layoutW, gTo.layoutH);
        expect(c.x).toBeCloseTo(CX, 2);
        expect(c.y).toBeCloseTo(CY, 2);
      }
      deg = next;
    }
  });

  it('缩放 2.5× 后旋转：中心仍锁定（rotateCW 绕视口中心重锚定）', () => {
    const user = zoomAround(matIdentity(), 2.5, CX, CY);
    const g0 = baseViewGeometry(NAT_W, NAT_H, 0, VW, VH);
    const g90 = baseViewGeometry(NAT_W, NAT_H, 90, VW, VH);
    const fromCss = viewTransformCss(user, 0, g0.k, g0.layoutW, g0.layoutH, VW, VH);
    const toCss = viewTransformCss(user, 90, g90.k, g90.layoutW, g90.layoutH, VW, VH);
    for (const t of T_STEPS) {
      const c = centerAt(fromCss, toCss, t, g90.layoutW, g90.layoutH);
      expect(c.x).toBeCloseTo(CX, 2);
      expect(c.y).toBeCloseTo(CY, 2);
    }
  });

  it('列表结构恒定：逐函数插值生效（仅 rotate/适配 scale 两槽变化）', () => {
    const g0 = baseViewGeometry(NAT_W, NAT_H, 0, VW, VH);
    const g90 = baseViewGeometry(NAT_W, NAT_H, 90, VW, VH);
    const from = parseTransformList(
      viewTransformCss(matIdentity(), 0, g0.k, g0.layoutW, g0.layoutH, VW, VH),
    );
    const to = parseTransformList(
      viewTransformCss(matIdentity(), 90, g90.k, g90.layoutW, g90.layoutH, VW, VH),
    );
    expect(from.map((f) => f.fn)).toEqual([
      'translate',
      'scale',
      'translate',
      'rotate',
      'scale',
      'translate',
    ]);
    expect(to.map((f) => f.fn)).toEqual(from.map((f) => f.fn));
    // 除 rotate(槽3) 与适配 scale(槽4) 外，所有槽数值两端相等
    for (const slot of [0, 1, 2, 5]) {
      expect(to[slot].args).toEqual(from[slot].args);
    }
    expect(to[3].args[0] - from[3].args[0]).toBe(90);
  });
});
