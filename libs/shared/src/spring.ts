/**
 * 弹簧动效：解阻尼谐振子 m·x'' + c·x' + k·(x − 1) = 0（x(0) = 0，x'(0) = 0），
 * 采样成 CSS `linear()` 缓动，并给出收敛时长。
 *
 * 生成结果写进 `styles/motion.css`（见 `scripts/generate-motion-css.ts`），
 * CSS 与 TS 两侧共用同一组参数。
 */

export interface SpringParams {
  /** 刚度 k */
  stiffness: number;
  /** 阻尼 c */
  damping: number;
  /** 质量 m */
  mass: number;
}

export interface SpringPreset extends SpringParams {
  name: string;
  /** 用途说明，写进生成的 CSS 注释。 */
  use: string;
}

/** 四档弹簧，取值与效果稿「设计基础」一致。 */
export const SPRING_PRESETS: readonly SpringPreset[] = [
  { name: 'bouncy', use: '按压、开关、箭头', stiffness: 600, damping: 30, mass: 1 },
  { name: 'snappy', use: '进场、面板、Popover', stiffness: 320, damping: 28, mass: 1 },
  { name: 'gentle', use: '复制成功、点赞', stiffness: 380, damping: 18, mass: 1 },
  { name: 'heavy', use: '抽屉、分割条吸附', stiffness: 1200, damping: 80, mass: 0.1 },
];

/** 与目标的偏差小于该值即视为静止。 */
export const SETTLE_TOLERANCE = 0.005;

/** 阻尼比 ζ = c / (2√(mk))。小于 1 为欠阻尼（会过冲）。 */
export function dampingRatio({ stiffness, damping, mass }: SpringParams): number {
  return damping / (2 * Math.sqrt(mass * stiffness));
}

/** 返回位移函数 x(t)，t 以秒计，从 0 走向 1。 */
export function springPosition(params: SpringParams): (t: number) => number {
  const { stiffness, mass } = params;
  const w0 = Math.sqrt(stiffness / mass);
  const zeta = dampingRatio(params);
  if (zeta < 1) {
    const wd = w0 * Math.sqrt(1 - zeta * zeta);
    return (t) =>
      1 - Math.exp(-zeta * w0 * t) * (Math.cos(wd * t) + ((zeta * w0) / wd) * Math.sin(wd * t));
  }
  if (zeta === 1) {
    return (t) => 1 - Math.exp(-w0 * t) * (1 + w0 * t);
  }
  const s = Math.sqrt(zeta * zeta - 1);
  const r1 = -w0 * (zeta - s);
  const r2 = -w0 * (zeta + s);
  return (t) => 1 + (r2 * Math.exp(r1 * t) - r1 * Math.exp(r2 * t)) / (r1 - r2);
}

const STEP = 0.0005;
const HORIZON = 5;

/** 最后一次偏离目标超过容差的时刻（秒）。 */
export function settleTime(params: SpringParams, tolerance = SETTLE_TOLERANCE): number {
  const x = springPosition(params);
  let last = 0;
  for (let t = 0; t <= HORIZON; t += STEP) {
    if (Math.abs(x(t) - 1) > tolerance) last = t;
  }
  return last;
}

/** 过冲量（超过目标的最大比例，0 表示不过冲）。 */
export function overshoot(params: SpringParams): number {
  const x = springPosition(params);
  const end = settleTime(params);
  let peak = 0;
  for (let t = 0; t <= end; t += STEP) peak = Math.max(peak, x(t));
  return Math.max(0, peak - 1);
}

export interface SampledSpring {
  /** CSS `linear()` 缓动函数。 */
  easing: string;
  /** 收敛时长，取整到 10ms。 */
  durationMs: number;
}

function formatStop(value: number): string {
  const fixed = value.toFixed(4).replace(/0+$/, '').replace(/\.$/, '');
  return fixed === '-0' ? '0' : fixed;
}

/** 在收敛时长内等距采样，首尾固定为 0 与 1。 */
export function sampleSpring(params: SpringParams, samples = 40): SampledSpring {
  const x = springPosition(params);
  const durationMs = Math.ceil((settleTime(params) * 1000) / 10) * 10;
  const seconds = durationMs / 1000;
  const stops = ['0'];
  for (let i = 1; i < samples; i++) stops.push(formatStop(x((i / samples) * seconds)));
  stops.push('1');
  return { easing: `linear(${stops.join(', ')})`, durationMs };
}

/** 生成 `styles/motion.css` 的完整内容。 */
export function motionCss(presets: readonly SpringPreset[] = SPRING_PRESETS): string {
  const lines = [
    '/* 由 libs/shared/scripts/generate-motion-css.ts 生成，勿手改。',
    '   改参数请编辑 libs/shared/src/spring.ts 后运行 `bun run motion-css`（libs/shared 下）。 */',
    '@theme static {',
  ];
  for (const preset of presets) {
    const { easing, durationMs } = sampleSpring(preset);
    const zeta = dampingRatio(preset);
    const over = (overshoot(preset) * 100).toFixed(1);
    lines.push(
      `  /* ${preset.use}：k ${preset.stiffness} · c ${preset.damping} · m ${preset.mass}，ζ ${zeta.toFixed(3)}，过冲 ${over}% */`,
      `  --ease-spring-${preset.name}: ${easing};`,
      `  --spring-${preset.name}-duration: ${durationMs}ms;`,
    );
  }
  lines.push('}', '');
  return lines.join('\n');
}
