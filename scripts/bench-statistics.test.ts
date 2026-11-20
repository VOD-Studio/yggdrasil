import { expect, test } from 'bun:test';
import { adjustComparisons, comparePaired, summarize } from './bench-statistics';

const runs = (values: number[]) => values.map((value, index) => ({ round: index + 1, value }));
const value = (sample: { value: number }) => sample.value;

test('按轮配对并消除共同波动，保留各轮的相对变化', () => {
  const a = runs([1, 10, 2, 20, 3, 30]);
  const b = a.map((sample) => ({ ...sample, value: sample.value * 0.8 })).reverse();
  const result = comparePaired(a, b, value);
  expect(result.pairs).toBe(6);
  expect(result.deltaPct).toBe(-20);
  expect(result.p).toBe(2 / 64);
  adjustComparisons([result]);
  expect(result.verdict).toBe('B 更快');
});

test('任何负载未达标的配对都阻止推断，但仍显示有效样本的差值', () => {
  const a = runs([1, 10, 2, 20, 3, 30, 4]);
  const b = a.map((sample) => ({
    ...sample,
    value: sample.value * 1.1,
    saturated: sample.round === 1,
  }));
  const result = comparePaired(a, b, value);
  expect(result).toMatchObject({
    pairs: 6,
    saturatedPairs: 1,
    deltaPct: 10,
    p: null,
    verdict: '负载未达标',
  });
});

test('多重比较校正计入样本不足的指标', () => {
  const a = runs([1, 10, 2, 20, 3, 30]);
  const b = a.map((sample) => ({ ...sample, value: sample.value / 2 }));
  const complete = comparePaired(a, b, value);
  const missing = comparePaired(a, b, (sample) => (sample.round > 5 ? null : value(sample)));
  adjustComparisons([complete, missing]);
  expect(complete.adjustedP).toBe(4 / 64);
  expect(complete.verdict).toBe('证据不足');
  expect(missing.verdict).toBe('样本不足');
  expect(summarize([null, null]).median).toBeNull();
});
