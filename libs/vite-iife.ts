import { resolve } from 'node:path';
import { defineConfig, type UserConfig } from 'vite';

export interface IifeLib {
  /** 包目录，传 import.meta.dirname。 */
  dir: string;
  /** public/ 下的输出子目录。 */
  out: string;
  /** IIFE 挂在 window 上的全局名。 */
  name: string;
  /** 产物基名：<file>.js、<file>.[ext]。 */
  file: string;
  /** 入口，相对 dir，默认 src/index.ts。 */
  entry?: string;
  /** true 时 output.exports = 'default'：默认导出成为 window 全局本身。 */
  defaultExport?: boolean;
  /** 构建前是否清空输出目录，默认 true。 */
  emptyOutDir?: boolean;
}

/** 各包共用的 Vite IIFE 库构建：输出直写 public/<out>/，由 cargo-leptos 的 assets-dir 拷进站点根。 */
export function iifeLib(o: IifeLib): UserConfig {
  return defineConfig({
    build: {
      outDir: resolve(o.dir, '../../public', o.out),
      emptyOutDir: o.emptyOutDir ?? true,
      lib: {
        entry: resolve(o.dir, o.entry ?? 'src/index.ts'),
        name: o.name,
        fileName: () => `${o.file}.js`,
        formats: ['iife'],
      },
      rolldownOptions: {
        output: {
          assetFileNames: `${o.file}.[ext]`,
          ...(o.defaultExport ? { exports: 'default' as const } : {}),
        },
      },
      cssCodeSplit: false,
      minify: true,
      sourcemap: true,
    },
  });
}
