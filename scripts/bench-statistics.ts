export const round3 = (value: number) => Math.round(value * 1000) / 1000;

export function median(values: number[]): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const upper = sorted.length >> 1;
  return sorted.length % 2 ? sorted[upper] : (sorted[upper - 1] + sorted[upper]) / 2;
}

export function summarize(values: (number | null)[]) {
  const valid = values.filter((value): value is number => value !== null);
  const middle = median(valid);
  return {
    samples: valid.length,
    median: middle === null ? null : round3(middle),
    min: valid.length ? round3(Math.min(...valid)) : null,
    max: valid.length ? round3(Math.max(...valid)) : null,
  };
}

export const MIN_REQUESTS = { p50: 20, p90: 100, p99: 1_000, p999: 10_000 } as const;

type Round = { round: number; saturated?: boolean };
export interface PairedComparison {
  pairs: number;
  saturatedPairs: number;
  deltaPct: number | null;
  p: number | null;
  adjustedP: number | null;
  verdict: string;
}

// 每轮的 A/B 执行顺序独立随机化。零假设下交换该轮标签等价于翻转对数比的符号。
function pairedP(differences: number[]) {
  const observed = Math.abs(differences.reduce((sum, value) => sum + value, 0));
  let extreme = 0;
  const visit = (index: number, sum: number) => {
    if (index === differences.length) {
      if (Math.abs(sum) >= observed - 1e-12) extreme++;
      return;
    }
    visit(index + 1, sum + differences[index]);
    visit(index + 1, sum - differences[index]);
  };
  visit(0, 0);
  return extreme / 2 ** differences.length;
}

export function comparePaired<T extends Round>(
  a: T[],
  b: T[],
  value: (sample: T) => number | null,
): PairedComparison {
  const byRound = new Map(b.map((sample) => [sample.round, sample]));
  const differences: number[] = [];
  let saturatedPairs = 0;
  for (const sample of a) {
    const other = byRound.get(sample.round);
    if (!other) continue;
    if (sample.saturated || other.saturated) {
      saturatedPairs++;
      continue;
    }
    const left = value(sample);
    const right = value(other);
    if (left !== null && right !== null && left > 0 && right > 0) {
      differences.push(Math.log(right) - Math.log(left));
    }
  }
  const middle = median(differences);
  const p = saturatedPairs === 0 && differences.length >= 6 ? pairedP(differences) : null;
  return {
    pairs: differences.length,
    saturatedPairs,
    deltaPct: middle === null ? null : round3(Math.expm1(middle) * 100),
    p,
    adjustedP: null,
    verdict: saturatedPairs ? '负载未达标' : p === null ? '样本不足' : '证据不足',
  };
}

// Holm 校正计入所有预先定义的指标，样本不足的指标同样占用比较名额。
export function adjustComparisons(comparisons: PairedComparison[]) {
  const valid = comparisons
    .filter((item): item is PairedComparison & { p: number } => item.p !== null)
    .sort((a, b) => a.p - b.p);
  let previous = 0;
  for (const [index, item] of valid.entries()) {
    const adjustedP = Math.min(1, Math.max(previous, item.p * (comparisons.length - index)));
    previous = adjustedP;
    item.adjustedP = adjustedP;
    if (adjustedP < 0.05 && item.deltaPct !== null && Math.abs(item.deltaPct) >= 3) {
      item.verdict = item.deltaPct < 0 ? 'B 更快' : 'B 更慢';
    }
  }
}
