// @vitest-environment node
/**
 * 旋转中心与关闭飞行的连续性回归测试。
 *
 * 旋转时锁定布局盒中心，关闭时从眼前姿态沿最短角度转正并缩回正文。
 *
 * happy-dom/jsdom 不实现 CSS transition 插值，因此本文件内置一个
 * CSS Transforms Level 1 §9 的插值模拟器（浏览器算法）：
 * 1. 两端均为定长函数列表且逐位同型（translate↔translate / rotate↔rotate /
 *    scale↔scale）→ 逐函数数值插值；
 * 2. 否则（matrix()、长度/类型不匹配）→ 各自分解为 translate/rotate/scale
 *    线性插值后重组（transform-origin 为 top left，见 style.css .lightbox-img）。
 *
 * 用模拟器检查中心轨迹、对应角点和方向，避免只看包围盒而漏掉末帧翻转。
 */
import { describe, expect, it } from 'vitest';
import {
  baseViewGeometry,
  clampPanToViewport,
  closeFlightTransform,
  type FlightPose,
  flightPoseFromMatrix,
  type Mat,
  matApply,
  matIdentity,
  matMultiply,
  matRotateDeg,
  matTranslate,
  nextRotationDeg,
  normDeg,
  panBy,
  type Rect,
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

// ============ 关闭飞行：捕获眼前姿态，同时转正并缩回正文 ============

describe('关闭飞行：实际姿态与最短归位路径', () => {
  const layoutW = 920;
  const layoutH = 517.5;
  const target: Rect = { x: 114.25, y: 453.125, w: 368, h: 207 };
  const corners = [
    [0, 0],
    [layoutW, 0],
    [0, layoutH],
    [layoutW, layoutH],
  ];
  const targetCorners = [
    { x: target.x, y: target.y },
    { x: target.x + target.w, y: target.y },
    { x: target.x, y: target.y + target.h },
    { x: target.x + target.w, y: target.y + target.h },
  ];
  const rotations = [
    [90, 0],
    [270, 360],
    [450, 360],
    [900, 1080],
    [-90, 0],
    [-450, -360],
    [37, 0],
    [180, 360],
    [-180, 0],
  ];

  function capture(deg: number): { matrix: Mat; pose: FlightPose } {
    // 非正方形布局，叠加非零平移和 1.5 倍缩放；37° 覆盖旋转尚未结束的帧。
    const user = { a: 1.5, b: 0, c: 0, d: 1.5, e: 73, f: -41 };
    const matrix = fnsToMat(
      parseTransformList(viewTransformCss(user, deg, 1, layoutW, layoutH, VW, VH)),
    );
    const pose = flightPoseFromMatrix(matrix, layoutW, layoutH, deg);
    expect(pose).not.toBeNull();
    return { matrix, pose: pose! };
  }

  function expectPoint(actual: { x: number; y: number }, expected: { x: number; y: number }) {
    expect(Math.hypot(actual.x - expected.x, actual.y - expected.y)).toBeLessThan(0.1);
  }

  it.each(rotations)('%s° 起点等价且终点对应四角归位到 %s°', (deg, endDeg) => {
    const { matrix, pose } = capture(deg);
    expect(pose.deg).toBeCloseTo(deg, 8);
    expect(pose.scaleX).toBeCloseTo(1.5, 8);
    expect(pose.scaleY).toBeCloseTo(1.5, 8);
    const startCss = closeFlightTransform(pose, target, 0);
    const endCss = closeFlightTransform(pose, target, 1);
    const start = fnsToMat(parseTransformList(startCss));
    const end = fnsToMat(parseTransformList(endCss));
    expect(parseTransformList(endCss).find((f) => f.fn === 'rotate')!.args[0]).toBe(endDeg);
    corners.forEach(([x, y], i) => {
      expectPoint(matApply(start, x, y), matApply(matrix, x, y));
      expectPoint(matApply(end, x, y), targetCorners[i]);
    });
  });

  it.each(rotations)('%s° 到 %s° 全程最短转正、中心直行且不压扁', (deg, endDeg) => {
    const { pose } = capture(deg);
    const startCss = closeFlightTransform(pose, target, 0);
    const endCss = closeFlightTransform(pose, target, 1);
    const startFns = parseTransformList(startCss);
    const endFns = parseTransformList(endCss);
    // 必须走同构逐函数插值，不能因 scale 参数个数不同退回矩阵分解。
    expect(startFns.map((f) => [f.fn, f.args.length])).toEqual([
      ['translate', 2],
      ['scale', 1],
      ['translate', 2],
      ['rotate', 1],
      ['scale', 2],
      ['translate', 2],
    ]);
    expect(endFns.map((f) => [f.fn, f.args.length])).toEqual(
      startFns.map((f) => [f.fn, f.args.length]),
    );
    let previousDistance = Math.abs(endDeg - deg);
    let previousAngle = deg;
    let totalAngle = 0;
    for (const progress of T_STEPS) {
      const css = closeFlightTransform(pose, target, progress);
      const fns = parseTransformList(css);
      const angle = fns.find((f) => f.fn === 'rotate')!.args[0];
      const distance = Math.abs(endDeg - angle);
      expect(distance).toBeLessThanOrEqual(previousDistance + 1e-8);
      expect(angle).toBeCloseTo(lerp(deg, endDeg, progress), 4);
      totalAngle += Math.abs(angle - previousAngle);
      previousAngle = angle;
      previousDistance = distance;

      const direct = fnsToMat(fns);
      const interpolated = interpolateTransform(startCss, endCss, progress);
      for (const [x, y] of corners) {
        expectPoint(matApply(direct, x, y), matApply(interpolated, x, y));
      }
      expectPoint(matApply(interpolated, layoutW / 2, layoutH / 2), {
        x: lerp(pose.centerX, target.x + target.w / 2, progress),
        y: lerp(pose.centerY, target.y + target.h / 2, progress),
      });
      // 两轴始终正向等比；对应边长度比不变，既不压扁也不镜像。
      const sx = Math.hypot(interpolated.a, interpolated.b);
      const sy = Math.hypot(interpolated.c, interpolated.d);
      expect(sx).toBeGreaterThan(0);
      expect(sy).toBeGreaterThan(0);
      expect(sx / sy).toBeCloseTo(1, 8);
      expect((layoutW * sx) / (layoutH * sy)).toBeCloseTo(layoutW / layoutH, 8);
      expect(interpolated.a * interpolated.d - interpolated.b * interpolated.c).toBeGreaterThan(0);
      expect(interpolated.a * interpolated.c + interpolated.b * interpolated.d).toBeCloseTo(0, 8);
      const drawnAngle = (Math.atan2(interpolated.b, interpolated.a) * 180) / Math.PI;
      const unwrappedAngle = drawnAngle + 360 * Math.round((angle - drawnAngle) / 360);
      expect(unwrappedAngle).toBeCloseTo(angle, 6);
    }
    expect(previousDistance).toBe(0);
    expect(totalAngle).toBeLessThanOrEqual(180);
  });

  it('未操控的非均匀缩放保持实际首帧，允许裁切缩略图的非等比路径', () => {
    const matrix = { a: 0.625, b: 0, c: 0, d: 0.875, e: 29.25, f: -17.5 };
    const pose = flightPoseFromMatrix(matrix, layoutW, layoutH, 0)!;
    expect(pose).not.toBeNull();
    expect(pose.scaleX).toBe(0.625);
    expect(pose.scaleY).toBe(0.875);
    const crop: Rect = { x: 81, y: 207, w: 320, h: 240 };
    const startCss = closeFlightTransform(pose, crop, 0);
    const endCss = closeFlightTransform(pose, crop, 1);
    const start = fnsToMat(parseTransformList(startCss));
    corners.forEach(([x, y]) => {
      expectPoint(matApply(start, x, y), matApply(matrix, x, y));
    });
    for (const progress of T_STEPS) {
      const interpolated = interpolateTransform(startCss, endCss, progress);
      expectPoint(matApply(interpolated, 0, 0), {
        x: lerp(matrix.e, crop.x, progress),
        y: lerp(matrix.f, crop.y, progress),
      });
      expectPoint(matApply(interpolated, layoutW, layoutH), {
        x: lerp(matrix.e + matrix.a * layoutW, crop.x + crop.w, progress),
        y: lerp(matrix.f + matrix.d * layoutH, crop.y + crop.h, progress),
      });
      expect(interpolated.a).toBeGreaterThan(0);
      expect(interpolated.d).toBeGreaterThan(0);
    }
    const end = fnsToMat(parseTransformList(endCss));
    const expected = [
      { x: crop.x, y: crop.y },
      { x: crop.x + crop.w, y: crop.y },
      { x: crop.x, y: crop.y + crop.h },
      { x: crop.x + crop.w, y: crop.y + crop.h },
    ];
    corners.forEach(([x, y], i) => {
      expectPoint(matApply(end, x, y), expected[i]);
    });
  });

  it('进度钳在首尾，不越过正文目标', () => {
    const { pose } = capture(450);
    for (const progress of [-Infinity, -2]) {
      expect(closeFlightTransform(pose, target, progress)).toBe(
        closeFlightTransform(pose, target, 0),
      );
    }
    for (const progress of [2, Infinity]) {
      expect(closeFlightTransform(pose, target, progress)).toBe(
        closeFlightTransform(pose, target, 1),
      );
    }
  });

  it.each([0, -1, NaN, Infinity, -Infinity])('拒绝无效布局尺寸 %s', (size) => {
    expect(flightPoseFromMatrix(matIdentity(), size, layoutH, 0)).toBeNull();
    expect(flightPoseFromMatrix(matIdentity(), layoutW, size, 0)).toBeNull();
  });

  it.each([NaN, Infinity, -Infinity])('拒绝无效矩阵分量与参考角 %s', (value) => {
    for (const key of ['a', 'b', 'c', 'd', 'e', 'f'] as const) {
      expect(
        flightPoseFromMatrix({ ...matIdentity(), [key]: value }, layoutW, layoutH, 0),
      ).toBeNull();
    }
    expect(flightPoseFromMatrix(matIdentity(), layoutW, layoutH, value)).toBeNull();
  });

  it('拒绝零缩放、镜像与计算后溢出的几何，不制造假布局', () => {
    const invalid: Mat[] = [
      { a: 0, b: 0, c: 0, d: 1, e: 0, f: 0 },
      { a: 1, b: 0, c: 0, d: 0, e: 0, f: 0 },
      { a: 1, b: 0, c: 0, d: -1, e: 0, f: 0 },
      { a: 1, b: 1, c: 1, d: 1, e: 0, f: 0 },
      { a: Number.MAX_VALUE, b: Number.MAX_VALUE, c: 0, d: 1, e: 0, f: 0 },
      { a: Number.MAX_VALUE, b: 0, c: 0, d: Number.MAX_VALUE, e: 0, f: 0 },
    ];
    for (const matrix of invalid) {
      expect(flightPoseFromMatrix(matrix, layoutW, layoutH, 0)).toBeNull();
    }
    expect(
      flightPoseFromMatrix({ ...matIdentity(), e: Number.MAX_VALUE }, Number.MAX_VALUE, layoutH, 0),
    ).toBeNull();
  });
});
