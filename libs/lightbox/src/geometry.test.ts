// @vitest-environment node
import { describe, expect, it } from 'vitest';
import {
  clampPanToViewport,
  clampScale,
  DOUBLE_CLICK_SCALE,
  effectiveDims,
  fitCentered,
  matApply,
  matIdentity,
  matMultiply,
  matRotateDeg,
  matToCss,
  matTranslate,
  nextRotationDeg,
  normDeg,
  originalUrl,
  panBy,
  type Rect,
  SCALE_MAX,
  SCALE_MIN,
  transformFor,
  wheelZoomFactor,
  zoomAround,
} from './geometry';

describe('fitCentered', () => {
  // 大图受 maxW=vw*0.92 / maxH=vh*0.88 约束,scale 取较小者
  it('宽图被视口宽度卡住(横向受限)', () => {
    // 2000x1000 in 1000x800: maxW=920, maxH=704
    // scale = min(920/2000, 704/1000, 1) = min(0.46, 0.704, 1) = 0.46
    const r = fitCentered(2000, 1000, 1000, 800);
    expect(r.w).toBeCloseTo(920, 5);
    expect(r.h).toBeCloseTo(460, 5);
    expect(r.x).toBeCloseTo((1000 - 920) / 2, 5); // 40
    expect(r.y).toBeCloseTo((800 - 460) / 2, 5); // 170
  });

  it('高图被视口高度卡住(纵向受限)', () => {
    // 1000x2000 in 1000x800: maxW=920, maxH=704
    // scale = min(920/1000, 704/2000, 1) = min(0.92, 0.352, 1) = 0.352
    const r = fitCentered(1000, 2000, 1000, 800);
    expect(r.w).toBeCloseTo(352, 5);
    expect(r.h).toBeCloseTo(704, 5);
  });

  it('小图不被放大(scale 钳到 1)', () => {
    // 300x200 in 1000x800: scale = min(920/300, 704/200, 1) = 1
    const r = fitCentered(300, 200, 1000, 800);
    expect(r.w).toBe(300);
    expect(r.h).toBe(200);
    expect(r.x).toBeCloseTo((1000 - 300) / 2, 5); // 350
    expect(r.y).toBeCloseTo((800 - 200) / 2, 5); // 300
  });

  it('正方形图按 maxW/maxH 中较小者缩放', () => {
    // 1000x1000 in 1000x800: maxW=920, maxH=704
    // scale = min(0.92, 0.704, 1) = 0.704
    const r = fitCentered(1000, 1000, 1000, 800);
    expect(r.w).toBeCloseTo(704, 5);
    expect(r.h).toBeCloseTo(704, 5);
  });
});

describe('transformFor', () => {
  it('居中态(base=target):scale=1,translate 到目标左上角', () => {
    const target: Rect = { x: 40, y: 170, w: 920, h: 460 };
    const t = transformFor(target, 920, 460);
    expect(t).toBe('translate(40px,170px) scale(1,1)');
  });

  it('缩小态(rect 比 base 小):scale<1', () => {
    // 原图位置小,base=居中尺寸 920x460
    const origin: Rect = { x: 100, y: 500, w: 400, h: 200 };
    const t = transformFor(origin, 920, 460);
    // 400/920 = 0.43478260869565216（JS 浮点完整精度）
    expect(t).toBe('translate(100px,500px) scale(0.43478260869565216,0.43478260869565216)');
  });

  it('baseW=0 守卫:scale 守卫为 1(不产生 NaN/Infinity)', () => {
    const rect: Rect = { x: 0, y: 0, w: 0, h: 0 };
    const t = transformFor(rect, 0, 0);
    expect(t).toBe('translate(0px,0px) scale(1,1)');
  });

  it('字符串格式为 translate(Xpx,Ypx) scale(SX,SY)', () => {
    const rect: Rect = { x: 10, y: 20, w: 100, h: 50 };
    const t = transformFor(rect, 200, 100);
    expect(t).toMatch(
      /^translate\(\d+(\.\d+)?px,\d+(\.\d+)?px\) scale\(\d+(\.\d+)?,\d+(\.\d+)?\)$/,
    );
  });
});

describe('originalUrl', () => {
  it('本地 /uploads/ 去 query string', () => {
    expect(originalUrl('/uploads/x.webp?w=800')).toBe('/uploads/x.webp');
    expect(originalUrl('uploads/x.webp?w=800')).toBe('uploads/x.webp');
  });

  it('外链 URL 完整保留 query string', () => {
    expect(originalUrl('https://example.com/img.png?token=abc')).toBe(
      'https://example.com/img.png?token=abc',
    );
    expect(originalUrl('http://example.com/img.png?w=800&sign=123')).toBe(
      'http://example.com/img.png?w=800&sign=123',
    );
  });

  it('无 query 原样返回', () => {
    expect(originalUrl('/uploads/x.webp')).toBe('/uploads/x.webp');
  });

  it('null 输入返回空串', () => {
    expect(originalUrl(null)).toBe('');
  });

  it('空串输入返回空串', () => {
    expect(originalUrl('')).toBe('');
  });
});

describe('矩阵复合 matMultiply / matApply', () => {
  it('恒等矩阵不改变点', () => {
    const p = matApply(matIdentity(), 12, 34);
    expect(p.x).toBe(12);
    expect(p.y).toBe(34);
  });

  it('m∘n：先应用 n（T(10,0)·S(2) 把 (1,1) 映到 (12,2)）', () => {
    const m = matMultiply(matTranslate(10, 0), { a: 2, b: 0, c: 0, d: 2, e: 0, f: 0 });
    const p = matApply(m, 1, 1);
    expect(p.x).toBe(12);
    expect(p.y).toBe(2);
  });

  it('matRotateDeg(90) 吸附浮点噪声，x 轴正向顺时针转到 y 轴正向', () => {
    const r = matRotateDeg(90);
    expect(r.a).toBe(0);
    expect(r.b).toBe(1);
    expect(r.c).toBe(-1);
    expect(r.d).toBe(0);
    const p = matApply(r, 10, 0);
    expect(p.x).toBe(0);
    expect(p.y).toBe(10);
  });

  it('matToCss 输出 matrix() 且坐标保留 4 位小数', () => {
    expect(matToCss(matIdentity())).toBe('matrix(1, 0, 0, 1, 0, 0)');
    expect(matToCss(matTranslate(1.23456789, -2))).toBe('matrix(1, 0, 0, 1, 1.2346, -2)');
  });
});

describe('缩放/旋转/平移手势数学', () => {
  it('zoomAround 锚点不动：锚点下的像素缩放前后位置一致', () => {
    const m = zoomAround(matIdentity(), 2, 100, 50);
    const anchor = matApply(m, 100, 50);
    expect(anchor.x).toBeCloseTo(100, 6);
    expect(anchor.y).toBeCloseTo(50, 6);
    // 原点被拉远：相对锚点翻倍
    const origin = matApply(m, 0, 0);
    expect(origin.x).toBeCloseTo(-100, 6);
    expect(origin.y).toBeCloseTo(-50, 6);
  });

  it('panBy 在视觉空间附加位移', () => {
    const p = matApply(panBy(matIdentity(), 7, -3), 10, 10);
    expect(p.x).toBe(17);
    expect(p.y).toBe(7);
  });

  it('clampScale 钳到 [SCALE_MIN, SCALE_MAX]', () => {
    expect(clampScale(0.5)).toBe(SCALE_MIN);
    expect(clampScale(99)).toBe(SCALE_MAX);
    expect(clampScale(2)).toBe(2);
    expect(DOUBLE_CLICK_SCALE).toBeGreaterThan(SCALE_MIN);
    expect(DOUBLE_CLICK_SCALE).toBeLessThanOrEqual(SCALE_MAX);
  });

  it('nextRotationDeg 累计步进不取模，normDeg 归一到 [0,360)', () => {
    expect(nextRotationDeg(0)).toBe(90);
    expect(nextRotationDeg(270)).toBe(360);
    expect(normDeg(360)).toBe(0);
    expect(normDeg(450)).toBe(90);
  });

  it('effectiveDims 仅在 90°/270° 交换宽高', () => {
    expect(effectiveDims(800, 600, 0)).toEqual({ w: 800, h: 600 });
    expect(effectiveDims(800, 600, 180)).toEqual({ w: 800, h: 600 });
    expect(effectiveDims(800, 600, 90)).toEqual({ w: 600, h: 800 });
    expect(effectiveDims(800, 600, 270)).toEqual({ w: 600, h: 800 });
  });

  it('wheelZoomFactor：每 100px 行程 ×1.2，行模式折算 ×16，暴冲钳制', () => {
    expect(wheelZoomFactor(-100, 0)).toBeCloseTo(1.2, 6);
    expect(wheelZoomFactor(100, 0)).toBeCloseTo(1 / 1.2, 6);
    expect(wheelZoomFactor(-10, 1)).toBeCloseTo(1.2 ** 1.6, 6);
    expect(wheelZoomFactor(-100000, 0)).toBe(1.7);
    expect(wheelZoomFactor(100000, 0)).toBe(0.6);
  });
});

describe('clampPanToViewport 平移边界', () => {
  // 视口 1000x1000，图 1000x500 → fit 920x460，居中于 (40,270)。
  // M0（rot=0）= translate(40,270)，布局盒 920x460。
  const vw = 1000;
  const vh = 1000;
  const fit = fitCentered(1000, 500, vw, vh);
  const m0 = matTranslate(fit.x, fit.y);

  it('fit 居中态无需修正（返回原矩阵）', () => {
    const user = matIdentity();
    expect(clampPanToViewport(user, m0, fit.w, fit.h, vw, vh)).toBe(user);
  });

  it('图小于视口的轴：任何平移都被拉回居中', () => {
    const clamped = clampPanToViewport(matTranslate(30, -15), m0, fit.w, fit.h, vw, vh);
    const p = matApply(matMultiply(clamped, m0), 0, 0);
    expect(p.x).toBeCloseTo(fit.x, 6);
    expect(p.y).toBeCloseTo(fit.y, 6);
  });

  it('放大后图宽超过视口：往右拖露出左黑边会被钳回图边贴视口边', () => {
    // 放大 2 倍后视觉宽 1840 > 1000，静止左缘 -420；往右拖 500 后左缘 +80 → 钳回 0
    const zoomed = zoomAround(matIdentity(), 2, vw / 2, vh / 2);
    const dragged = panBy(zoomed, 500, 0);
    const clamped = clampPanToViewport(dragged, m0, fit.w, fit.h, vw, vh);
    const total = matMultiply(clamped, m0);
    expect(matApply(total, 0, 0).x).toBeCloseTo(0, 6);
    expect(matApply(total, fit.w, 0).x).toBeGreaterThanOrEqual(vw - 0.0001);
  });
});
