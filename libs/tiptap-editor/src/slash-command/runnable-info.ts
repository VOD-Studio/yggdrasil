/** buildRunnableInfo 的输入配置。 */
export interface RunnableInfoOpts {
  /** 语言名（python / node）。 */
  lang: string;
  /** 超时秒数。 */
  timeoutSecs: number;
  /** 内存上限（MB）。 */
  memoryMb: number;
  /** 是否允许网络。 */
  allowNetwork: boolean;
  /** 作者是否改动过任一 overrides 字段；false 则省略 JSON。 */
  dirty: boolean;
}

/**
 * 把弹框收集的配置转成 markdown fence 的 info string。
 *
 * - dirty=false → `${lang} runnable`（省略 JSON，最小形态）
 * - dirty=true  → `${lang} runnable {"timeout_secs":N,"memory_mb":M,"allow_network":B}`
 *
 * JSON 字段顺序固定（timeout → memory → network），由显式构造保证（不依赖对象插入顺序）。
 * 到达此函数时值必然合法（弹框「插入」按钮在非法值时 disabled）。
 */
export function buildRunnableInfo(opts: RunnableInfoOpts): string {
  const prefix = `${opts.lang} runnable`;
  if (!opts.dirty) return prefix;
  // 显式拼字符串，保证字段顺序固定（timeout → memory → network），不依赖对象键序。
  const json = `{"timeout_secs":${opts.timeoutSecs},"memory_mb":${opts.memoryMb},"allow_network":${opts.allowNetwork}}`;
  return `${prefix} ${json}`;
}

/**
 * 受支持的语言（canonical key + 展示标签，与 src/pages/admin/runner.rs 的
 * SUPPORTED_LANGS 对齐；后者镜像后端 LANGUAGES 注册表，见
 * src/api/code_runner/languages.rs）。编辑器是纯 JS lib，不调 server function，故写死。
 */
export const RUNNABLE_LANGS = [
  { value: 'python', label: 'Python' },
  { value: 'node', label: 'Node.js' },
  { value: 'go', label: 'Go' },
  { value: 'rust', label: 'Rust' },
  { value: 'bun', label: 'Bun (TS)' },
] as const;

/** 模态框默认值（与后端 ResourceLimits 默认对齐：见 languages.rs）。 */
export const RUNNABLE_DEFAULTS = { timeoutSecs: 5, memoryMb: 256, allowNetwork: false };

/** timeout_secs 取值范围（与 CODE_RUNNER_MAX_TIMEOUT_SECS 对齐）。 */
export const TIMEOUT_RANGE = { min: 1, max: 30 } as const;
/** memory_mb 取值范围（与 CODE_RUNNER_MAX_MEMORY_MB 对齐）。 */
export const MEMORY_RANGE = { min: 16, max: 1024 } as const;
