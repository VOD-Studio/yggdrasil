// 配色取自 @yggdrasil/shared 的 CODE_PALETTES，与文章代码高亮（highlight.css）、
// 终端同源。编辑器外观（面板、提示框）用全站 token 变量，随 .dark 自动切换。
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { type Extension, Prec } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { tags as t } from '@lezer/highlight';
import { CODE_PALETTES, type SyntaxPalette, type ThemeName } from '@yggdrasil/shared';

export type { ThemeName };

function editorTheme(p: SyntaxPalette, dark: boolean): Extension {
  return EditorView.theme(
    {
      '&': { color: p.foreground, backgroundColor: p.background },
      '.cm-content': { caretColor: p.cursor },
      '.cm-cursor, .cm-dropCursor': { borderLeftColor: p.cursor, borderLeftWidth: '2px' },
      // Vim 块状光标：插件以 Prec.highest 写死粉红，这里多一层 .cm-vimCursorLayer 提高特异性。
      '& .cm-vimCursorLayer .cm-fat-cursor': { background: p.cursor, color: p.background },
      '&:not(.cm-focused) .cm-vimCursorLayer .cm-fat-cursor': {
        background: 'none',
        outline: `solid 1px ${p.cursor}`,
      },
      '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection':
        { backgroundColor: p.selection },
      '.cm-activeLine': { backgroundColor: p.lineHighlight },
      '.cm-gutters': { color: p.gutter, border: 'none' },
      '.cm-activeLineGutter': { backgroundColor: p.lineHighlight, color: p.foreground },
      '.cm-selectionMatch': { backgroundColor: 'rgba(217, 119, 87, 0.12)' },
      '.cm-searchMatch': {
        backgroundColor: 'rgba(217, 119, 87, 0.18)',
        outline: '1px solid rgba(217, 119, 87, 0.4)',
      },
      '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: 'rgba(217, 119, 87, 0.32)' },
      '&.cm-focused .cm-matchingBracket, &.cm-focused .cm-nonmatchingBracket': {
        backgroundColor: 'rgba(217, 119, 87, 0.18)',
        outline: 'none',
      },
      '.cm-foldPlaceholder': {
        backgroundColor: 'var(--color-ink-2)',
        border: 'none',
        color: p.gutter,
        borderRadius: '4px',
      },
      '.cm-placeholder': { color: p.gutter },
      '.cm-panels': { backgroundColor: 'var(--color-surface-2)', color: 'var(--color-fg)' },
      '.cm-panels.cm-panels-top': { borderBottom: 'var(--hairline) solid var(--color-hairline)' },
      '.cm-panels.cm-panels-bottom': { borderTop: 'var(--hairline) solid var(--color-hairline)' },
      '.cm-tooltip': {
        backgroundColor: 'var(--color-popover)',
        color: 'var(--color-fg)',
        border: 'none',
        borderRadius: '8px',
        boxShadow: 'var(--shadow-popover)',
      },
      '.cm-tooltip .cm-tooltip-arrow:before': {
        borderTopColor: 'transparent',
        borderBottomColor: 'transparent',
      },
      '.cm-tooltip .cm-tooltip-arrow:after': {
        borderTopColor: 'var(--color-popover)',
        borderBottomColor: 'var(--color-popover)',
      },
      '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
        backgroundColor: 'var(--color-clay-soft)',
        color: 'var(--color-fg)',
      },
    },
    { dark },
  );
}

// 归类与 code-palette.ts 的 TM_SCOPES 一致：函数定义用 clay，调用与关键字同为 muted；
// 带 definition 修饰的标签比不带的更具体，会压过调用的颜色。
function highlightStyle(p: SyntaxPalette): Extension {
  return syntaxHighlighting(
    HighlightStyle.define([
      {
        tag: [t.comment, t.lineComment, t.blockComment, t.docComment, t.docString, t.quote],
        color: p.comment,
      },
      {
        tag: [
          t.keyword,
          t.modifier,
          t.controlKeyword,
          t.operatorKeyword,
          t.definitionKeyword,
          t.moduleKeyword,
          t.self,
          t.standard(t.name),
          t.annotation,
          t.meta,
          t.macroName,
          t.labelName,
          t.function(t.variableName),
          t.function(t.propertyName),
        ],
        color: p.muted,
      },
      {
        tag: [t.function(t.definition(t.variableName)), t.function(t.definition(t.propertyName))],
        color: p.clay,
      },
      { tag: [t.string, t.special(t.string), t.character, t.regexp, t.monospace], color: p.string },
      {
        tag: [
          t.number,
          t.bool,
          t.null,
          t.atom,
          t.constant(t.name),
          t.unit,
          t.escape,
          t.typeName,
          t.className,
          t.tagName,
          t.attributeName,
          t.propertyName,
          t.changed,
        ],
        color: p.secondary,
      },
      {
        tag: [
          t.operator,
          t.punctuation,
          t.separator,
          t.bracket,
          t.derefOperator,
          t.processingInstruction,
        ],
        color: p.subtle,
      },
      { tag: [t.variableName, t.name, t.namespace], color: p.foreground },
      { tag: [t.inserted, t.invalid], color: p.bright },
      { tag: t.deleted, color: p.muted },
      { tag: t.heading, color: p.bright, fontWeight: '600' },
      { tag: t.link, color: p.foreground, textDecoration: 'underline' },
      { tag: t.url, color: p.secondary, textDecoration: 'underline' },
      { tag: t.strong, fontWeight: '600' },
      { tag: t.emphasis, fontStyle: 'normal', textDecoration: 'underline dotted' },
      { tag: t.strikethrough, textDecoration: 'line-through' },
    ]),
  );
}

const THEMES: Record<ThemeName, Extension> = {
  light: [
    editorTheme(CODE_PALETTES.light.syntax, false),
    highlightStyle(CODE_PALETTES.light.syntax),
  ],
  dark: [editorTheme(CODE_PALETTES.dark.syntax, true), highlightStyle(CODE_PALETTES.dark.syntax)],
};

/**
 * 覆盖 CodeMirror core 内置 base theme 的两处问题：
 *
 * 1. `.cm-gutters` 默认背景：core 的 `&light .cm-gutters`（`#f5f5f5`）/ `&dark`
 *    （`#333338`）特异性高于主题的 `.cm-gutters`，行号列与代码区背景不一致。
 *    用 `!important` 强制透明，继承 editor base 色。
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
  return [Prec.high(THEMES[name]), gutterBackgroundOverride, foldGutterCenterOverride];
}
