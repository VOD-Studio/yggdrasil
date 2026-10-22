/**
 * 代码配色的单一来源：文章高亮（经 tmTheme 生成 highlight.css）、CodeMirror、
 * xterm 与 Mermaid 都从这里取色。
 *
 * 暖调、低饱和，与陶土橙品牌色同一温度。所有语法色在代码块底色（浅色
 * surface-0/1、深色 surface-1/3）上的对比度不低于 4.5:1，见 code-palette.test.ts。
 * 注释不用斜体：注释里常有中文，中文没有真斜体。
 */

import type { ThemeName } from './index';

export interface SyntaxPalette {
  /** 代码块底色，等于 --color-surface-1。 */
  background: string;
  /** 普通文字，等于 --color-fg-body。 */
  foreground: string;
  comment: string;
  keyword: string;
  string: string;
  number: string;
  function: string;
  type: string;
  punctuation: string;
  regex: string;
  inserted: string;
  deleted: string;
  /** 行号、折叠图标，等于 --color-fg-3。 */
  gutter: string;
  /** 当前行、悬停，等于 --color-ink-1。 */
  lineHighlight: string;
  selection: string;
  cursor: string;
}

export interface AnsiPalette {
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

export interface CodePalette {
  syntax: SyntaxPalette;
  ansi: AnsiPalette;
}

export const CODE_PALETTES: Record<ThemeName, CodePalette> = {
  light: {
    syntax: {
      background: '#fafaf8',
      foreground: '#2b2b29',
      comment: '#6b6b69',
      keyword: '#a3471c',
      string: '#7d6214',
      number: '#8c3f7d',
      function: '#2d5b86',
      type: '#356660',
      punctuation: '#5f5e5b',
      regex: '#9a4520',
      inserted: '#2f6f4f',
      deleted: '#b42318',
      gutter: '#6b6b69',
      lineHighlight: 'rgba(0, 0, 0, 0.045)',
      selection: 'rgba(217, 119, 87, 0.22)',
      cursor: '#d97757',
    },
    ansi: {
      black: '#2b2b29',
      red: '#b42318',
      green: '#2f6f4f',
      yellow: '#7d6214',
      blue: '#2d5b86',
      magenta: '#8c3f7d',
      cyan: '#356660',
      white: '#6b6b69',
      brightBlack: '#525252',
      brightRed: '#c93a3a',
      brightGreen: '#3d7d5c',
      brightYellow: '#8a6a10',
      brightBlue: '#3a6a96',
      brightMagenta: '#9c4a8c',
      brightCyan: '#3f7670',
      brightWhite: '#8b8984',
    },
  },
  dark: {
    syntax: {
      background: '#161615',
      foreground: '#dad8d2',
      comment: '#8b8984',
      keyword: '#ec9a78',
      string: '#dcbc78',
      number: '#d6a2cb',
      function: '#94b8dc',
      type: '#8fc5bd',
      punctuation: '#a9a7a1',
      regex: '#e8a87c',
      inserted: '#8fcfae',
      deleted: '#f0a59c',
      gutter: '#8b8984',
      lineHighlight: 'rgba(255, 255, 255, 0.05)',
      selection: 'rgba(217, 119, 87, 0.3)',
      cursor: '#d97757',
    },
    ansi: {
      black: '#3a3936',
      red: '#f0a59c',
      green: '#8fcfae',
      yellow: '#dcbc78',
      blue: '#94b8dc',
      magenta: '#d6a2cb',
      cyan: '#8fc5bd',
      white: '#dad8d2',
      brightBlack: '#6b6a66',
      brightRed: '#f6bdb6',
      brightGreen: '#aedfc4',
      brightYellow: '#e8cf98',
      brightBlue: '#b2cde6',
      brightMagenta: '#e3bddb',
      brightCyan: '#addad3',
      brightWhite: '#f2f1ee',
    },
  },
};

/** 每个语法角色在 tmTheme 中对应的 TextMate scope。 */
const TM_SCOPES: [keyof SyntaxPalette, string, string?][] = [
  ['comment', 'comment, punctuation.definition.comment'],
  ['keyword', 'keyword, storage.type, storage.modifier, keyword.control, keyword.operator.word'],
  ['string', 'string, string.quoted, punctuation.definition.string'],
  [
    'number',
    'constant.numeric, constant.language, constant.character, variable.language, support.constant',
  ],
  [
    'function',
    'entity.name.function, support.function, meta.function-call, variable.function, support.macro, entity.name.macro',
  ],
  [
    'type',
    'entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, entity.name.trait, support.type, support.class, storage.type.primitive, entity.other.inherited-class, entity.other.attribute-name',
  ],
  ['punctuation', 'punctuation, keyword.operator, meta.brace'],
  ['regex', 'string.regexp, constant.character.escape'],
  ['keyword', 'entity.name.tag'],
  ['inserted', 'markup.inserted'],
  ['deleted', 'markup.deleted, invalid'],
  ['keyword', 'markup.heading, entity.name.section', 'bold'],
  ['function', 'markup.underline.link, string.other.link'],
  ['type', 'variable.parameter'],
];

function escapeXml(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/** 生成供 syntect 读取的 .tmTheme（Sublime Text plist）。 */
export function tmTheme(name: string, palette: SyntaxPalette): string {
  const setting = (entries: [string, string][]) =>
    entries
      .map(([k, v]) => `\t\t\t\t<key>${k}</key>\n\t\t\t\t<string>${escapeXml(v)}</string>`)
      .join('\n');
  const items = [
    `\t\t<dict>\n\t\t\t<key>settings</key>\n\t\t\t<dict>\n${setting([
      ['background', palette.background],
      ['foreground', palette.foreground],
      ['caret', palette.cursor],
      ['lineHighlight', palette.background],
      ['selection', palette.background],
    ])}\n\t\t\t</dict>\n\t\t</dict>`,
    ...TM_SCOPES.map(([role, scope, fontStyle]) => {
      const entries: [string, string][] = [['foreground', palette[role]]];
      entries.push(['fontStyle', fontStyle ?? '']);
      return `\t\t<dict>\n\t\t\t<key>name</key>\n\t\t\t<string>${role}</string>\n\t\t\t<key>scope</key>\n\t\t\t<string>${escapeXml(scope)}</string>\n\t\t\t<key>settings</key>\n\t\t\t<dict>\n${setting(entries)}\n\t\t\t</dict>\n\t\t</dict>`;
    }),
  ];
  return `<?xml version="1.0" encoding="UTF-8"?>
<!-- 由 libs/shared/scripts/generate-code-themes.ts 生成，勿手改；取色见 libs/shared/src/code-palette.ts。 -->
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
\t<key>name</key>
\t<string>${escapeXml(name)}</string>
\t<key>settings</key>
\t<array>
${items.join('\n')}
\t</array>
</dict>
</plist>
`;
}

/** 浅色 / 深色 tmTheme 的文件名与内容。 */
export function tmThemeFiles(): { file: string; content: string }[] {
  return [
    {
      file: 'Yggdrasil Light.tmTheme',
      content: tmTheme('Yggdrasil Light', CODE_PALETTES.light.syntax),
    },
    {
      file: 'Yggdrasil Dark.tmTheme',
      content: tmTheme('Yggdrasil Dark', CODE_PALETTES.dark.syntax),
    },
  ];
}

/**
 * highlight.js（Tiptap 编辑器代码块经 lowlight 输出）的类名与语法角色对照，
 * 与上面 TM_SCOPES 的归类一致：标签名算关键字、属性名与参数算类型、内建函数算函数。
 * 未列出的类（变量、属性访问、meta 外壳）沿用代码块正文色。
 */
const HLJS_CLASSES: [keyof SyntaxPalette, string[]][] = [
  ['comment', ['comment', 'quote']],
  ['keyword', ['keyword', 'selector-tag', 'name', 'section', 'meta .hljs-keyword']],
  ['string', ['string', 'meta .hljs-string']],
  ['number', ['number', 'literal', 'symbol', 'variable.language_']],
  ['function', ['title', 'title.function_', 'built_in', 'link']],
  [
    'type',
    [
      'type',
      'title.class_',
      'title.class_.inherited__',
      'attr',
      'attribute',
      'params',
      'selector-class',
      'selector-id',
    ],
  ],
  ['punctuation', ['punctuation', 'operator']],
  ['regex', ['regexp', 'char.escape_']],
  ['inserted', ['addition']],
  ['deleted', ['deletion']],
];

/** 由 CODE_PALETTES 生成 Tiptap 代码块的 highlight.js 配色（浅色在前，深色挂 .dark）。 */
export function hljsThemeCss(scope = '.tiptap-editor pre code'): string {
  const block = (prefix: string, palette: SyntaxPalette): string =>
    HLJS_CLASSES.map(([role, classes]) => {
      const selectors = classes.map((c) => `${prefix}${scope} .hljs-${c}`).join(',\n');
      return `${selectors} {\n  color: ${palette[role]};\n}`;
    }).join('\n');
  return `/* 由 libs/shared/scripts/generate-code-themes.ts 生成，勿手改；取色见 libs/shared/src/code-palette.ts。 */
${block('', CODE_PALETTES.light.syntax)}
${block('.dark ', CODE_PALETTES.dark.syntax)}
${scope} .hljs-section,
${scope} .hljs-strong {
  font-weight: 600;
}
`;
}
