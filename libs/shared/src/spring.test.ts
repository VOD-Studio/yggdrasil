import { describe, expect, it } from 'vitest';
import {
  dampingRatio,
  motionCss,
  overshoot,
  SPRING_PRESETS,
  sampleSpring,
  settleTime,
  springPosition,
} from './spring';

const preset = (name: string) => {
  const found = SPRING_PRESETS.find((p) => p.name === name);
  if (!found) throw new Error(`missing preset ${name}`);
  return found;
};

describe('springPosition', () => {
  it('starts at rest and converges to the target', () => {
    for (const p of SPRING_PRESETS) {
      const x = springPosition(p);
      expect(x(0)).toBeCloseTo(0, 6);
      expect(x(5)).toBeCloseTo(1, 4);
    }
  });

  it('handles critical damping', () => {
    const x = springPosition({ stiffness: 100, damping: 20, mass: 1 });
    expect(dampingRatio({ stiffness: 100, damping: 20, mass: 1 })).toBe(1);
    expect(x(0)).toBeCloseTo(0, 6);
    expect(x(2)).toBeCloseTo(1, 4);
  });
});

describe('presets', () => {
  it('keeps the overshoot each use case expects', () => {
    expect(overshoot(preset('bouncy'))).toBeGreaterThan(0.05);
    expect(overshoot(preset('bouncy'))).toBeLessThan(0.12);
    expect(overshoot(preset('snappy'))).toBeLessThan(0.03);
    expect(overshoot(preset('gentle'))).toBeGreaterThan(0.15);
    expect(overshoot(preset('heavy'))).toBe(0);
  });

  it('settles within a perceptible budget', () => {
    for (const p of SPRING_PRESETS) {
      const t = settleTime(p);
      expect(t).toBeGreaterThan(0.2);
      expect(t).toBeLessThan(0.7);
    }
  });

  it('never moves backwards when overdamped', () => {
    const x = springPosition(preset('heavy'));
    let prev = 0;
    for (let t = 0; t <= 0.5; t += 0.001) {
      const v = x(t);
      expect(v).toBeGreaterThanOrEqual(prev);
      prev = v;
    }
  });
});

describe('sampleSpring', () => {
  it('emits a linear() easing pinned to 0 and 1', () => {
    const { easing, durationMs } = sampleSpring(preset('snappy'), 20);
    expect(easing.startsWith('linear(0, ')).toBe(true);
    expect(easing.endsWith(', 1)')).toBe(true);
    expect(easing.split(',')).toHaveLength(21);
    expect(durationMs % 10).toBe(0);
  });
});

describe('motionCss', () => {
  it('defines an easing and a duration for every preset', () => {
    const css = motionCss();
    for (const p of SPRING_PRESETS) {
      expect(css).toContain(`--ease-spring-${p.name}: linear(`);
      expect(css).toMatch(new RegExp(`--spring-${p.name}-duration: \\d+ms;`));
    }
  });
});
