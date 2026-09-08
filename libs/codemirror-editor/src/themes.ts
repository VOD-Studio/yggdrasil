// @catppuccin/codemirror 提供现成的 Catppuccin 主题 Extension，
// 与项目 themes/ 下的 Catppuccin Latte/Mocha .tmTheme 视觉一致。
import { catppuccinLatte, catppuccinMocha } from '@catppuccin/codemirror';
import { type Extension, Prec } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import type { ThemeName } from '@yggdrasil/shared';

export type { ThemeName };

// 高于 Catppuccin 的选区规则；只改背景，保留 drawSelection 对原生选区的隐藏，
// 避免原生选区与自绘选区叠加，也不覆盖语法高亮的文字颜色。
const selectionBackgroundOverride: Extension = Prec.high(
  EditorView.theme({
    '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection':
      {
        backgroundColor: 'var(--color-paper-selection)',
      },
  }),
);

/**
 * 覆盖 CodeMirror core 内置 base theme 的两处问题：
 *
 * 1. `.cm-gutters` 默认背景：core 的 `&light .cm-gutters`（`#f5f5f5`）/ `&dark`
 *    （`#333338`）特异性高于 catppuccin 的 `.cm-gutters`，catppuccin 的 base 背景被
 *    压制，行号列与代码区背景不一致。用 `!important` 强制透明，继承 editor base 色。
 *
 * 2. `.cm-editor` 默认不撑满父容器：core 的 `&` 没设 height/flex，编辑器只占内容
 *    高度。`height: 100%` 在父容器只有 min-height 时会塌缩（CSS：百分比高度需要
 *    父元素有明确 height）。改用 `flex: 1`，配合父容器 `display: flex`，编辑器才能
 *    真正填满，避免「有内容的上半部分」与「空白下半部分」背景割裂。
 */
const gutterBackgroundOverride: Extension = EditorView.theme({
  '&': {
    flex: '1 1 0',
    minHeight: '0',
  },
  '.cm-gutters': {
    backgroundColor: 'transparent !important',
  },
});

/**
 * 修复 CodeMirror 折叠图标（fold gutter 的 ⌄ / ›）垂直不居中。
 *
 * core 的 baseTheme 对 `.cm-gutterElement` 只设了 `box-sizing`，对折叠列
 * `.cm-foldGutter .cm-gutterElement` 完全没设样式。`.cm-gutter` 是
 * `display:flex; flex-direction:column`，格子高度由内容决定——折叠列格子高度
 * ≈ font-size（14px），比正文行（line-height ≈19.6px）矮一截，图标被挤在矮格子
 * 顶部/底部，视觉上偏离每行中央。
 *
 * 把折叠列每个格子变成 flex 容器并居中，图标稳稳落在格子中央；同时格子必须和正文
 * 行等高，图标才会落在正文行的视觉中央。正文 .cm-line 的 line-height 由 CodeMirror
 * 按「字号 × 1.4」算出（实测 14px → 19.6px），这里用同样倍数复刻，避免硬编码像素
 * 值随字号漂移。
 *
 * 注意：折叠后正文行被撑高的根因（文章页 47.5px vs 试运行页 19.6px）不在
 * placeholder——而在 `<img class="cm-widgetBuffer">` 被 `.md-content img` 的
 * `margin: 1rem 0` 命中（上下各 16px = 32px）。修复见 input.css 的
 * `.md-content img:where(:not(.cm-widgetBuffer))`。
 */
const foldGutterCenterOverride: Extension = EditorView.theme({
  '.cm-foldGutter .cm-gutterElement': {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    lineHeight: '1.4',
  },
});

/** 根据主题名返回对应的 CodeMirror 主题 Extension。 */
export function themeExtension(name: ThemeName): Extension {
  const catppuccin = name === 'light' ? catppuccinLatte : catppuccinMocha;
  return [
    catppuccin,
    selectionBackgroundOverride,
    gutterBackgroundOverride,
    foldGutterCenterOverride,
  ];
}
