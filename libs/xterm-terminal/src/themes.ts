// 主题名由 @yggdrasil/shared 统一定义，re-export 保持本模块 API 不变。
export type { ThemeName } from '@yggdrasil/shared';

// Catppuccin Latte（浅色）终端配色，与 highlight.css 浅色主题视觉一致。
// background 用项目 --color-paper-code-block 的色值（Catppuccin Latte Surface0），
// 保证终端背景与外层容器无缝衔接。
// 色值取自 https://catppuccin.com/palette/ Latte 调色板。
export const LIGHT_THEME = {
  background: '#dce0e8',
  foreground: '#4c4f69',
  cursor: '#dc8a78',
  cursorAccent: '#eff1f5',
  // xterm 接收具体颜色值，与全站主题绿色 30% 不透明度保持一致。
  selectionBackground: 'rgba(64, 160, 43, 0.3)',
  selectionInactiveBackground: 'rgba(64, 160, 43, 0.3)',
  black: '#5c5f77',
  red: '#d20f39',
  green: '#40a02b',
  yellow: '#df8e1d',
  blue: '#1e66f5',
  magenta: '#ea76cb',
  cyan: '#179299',
  white: '#acb0be',
  brightBlack: '#6c6f85',
  brightRed: '#d20f39',
  brightGreen: '#40a02b',
  brightYellow: '#df8e1d',
  brightBlue: '#1e66f5',
  brightMagenta: '#ea76cb',
  brightCyan: '#179299',
  brightWhite: '#bcc0cc',
};

// Catppuccin Mocha（深色）终端配色，与 highlight.css 深色主题视觉一致。
// background 用项目 --color-paper-code-block 的色值（Catppuccin Mocha Surface0），
// 保证终端背景与外层容器无缝衔接。
// 色值取自 https://catppuccin.com/palette/ Mocha 调色板。
export const DARK_THEME = {
  background: '#313244',
  foreground: '#cdd6f4',
  cursor: '#f5e0dc',
  cursorAccent: '#1e1e2e',
  selectionBackground: 'rgba(166, 227, 161, 0.3)',
  selectionInactiveBackground: 'rgba(166, 227, 161, 0.3)',
  black: '#45475a',
  red: '#f38ba8',
  green: '#a6e3a1',
  yellow: '#f9e2af',
  blue: '#89b4fa',
  magenta: '#f5c2e7',
  cyan: '#94e2d5',
  white: '#bac2de',
  brightBlack: '#585b70',
  brightRed: '#f38ba8',
  brightGreen: '#a6e3a1',
  brightYellow: '#f9e2af',
  brightBlue: '#89b4fa',
  brightMagenta: '#f5c2e7',
  brightCyan: '#94e2d5',
  brightWhite: '#a6adc8',
};
