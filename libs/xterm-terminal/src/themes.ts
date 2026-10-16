// 主题名由 @yggdrasil/shared 统一定义，re-export 保持本模块 API 不变。
import { CODE_PALETTES, type CodePalette } from '@yggdrasil/shared';

export type { ThemeName } from '@yggdrasil/shared';

// 终端配色取自 CODE_PALETTES，与文章代码高亮、CodeMirror 同源。
// background 等于 --color-surface-1，与外层代码块容器无缝衔接；
// xterm 只接受具体颜色值，不能用 CSS 变量。
function terminalTheme({ syntax, ansi }: CodePalette) {
  return {
    background: syntax.background,
    foreground: syntax.foreground,
    cursor: syntax.cursor,
    cursorAccent: syntax.background,
    selectionBackground: syntax.selection,
    selectionInactiveBackground: syntax.selection,
    ...ansi,
  };
}

export const LIGHT_THEME = terminalTheme(CODE_PALETTES.light);

export const DARK_THEME = terminalTheme(CODE_PALETTES.dark);
