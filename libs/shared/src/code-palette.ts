/**
 * 代码配色的单一来源：文章高亮（经 tmTheme 生成 highlight.css）、Tiptap 代码块、
 * CodeMirror 与 xterm 都从这里取色。Mermaid 图表用界面 token 配色，见
 * src/api/markdown/mermaid/config.rs。
 *
 * 配色移植自 clay.nvim 的 Clay 设计规范（restrained 点缀模式）：层级靠亮度，不靠色相。
 * 除字符串的暖灰和函数定义的陶土色外，语法色都是 R = G = B 的中性灰；越是作者写的东西
 * （变量、定义）对比越强，语言结构（关键字、标点）退后，注释最弱。新增、删除同样只用
 * 亮度区分。
 *
 * 与 clay.nvim 的差异：
 * - 底色、正文、强调字、行号沿用站点 token（surface-1、fg-body、fg、fg-3），不另起一套；
 * - 站点要求所有语法色在代码块底色（浅色 surface-0/1、深色 surface-1/3）上不低于
 *   4.5:1，Clay 的注释（4.4）与标点（3.4）达不到，于是把 fg-body 到标点之间的阶梯
 *   按 Clay 的对数对比度比例压缩到 5.15:1 为止，两个变体在代码块底色上对比度一致；
 * - 函数定义用 clay-text 而不是品牌色：品牌色在浅色底上只有 3:1，不承载文字；
 * - 注释不用斜体：注释里常有中文，中文没有真斜体。
 *
 * 校验见 code-palette.test.ts。
 */

import type { ThemeName } from './index';

/** 语法角色按 Clay 色板命名：键名是层级，不是语言元素。 */
export interface SyntaxPalette {
  /** 代码块底色，等于 --color-surface-1。 */
  background: string;
  /** 变量、模块等普通标识符，等于 --color-fg-body。 */
  foreground: string;
  /** 最强强调：标题、新增行、无效代码，等于 --color-fg。 */
  bright: string;
  /** 类型、常量、数字、布尔、成员、标签名、转义、修改行。 */
  secondary: string;
  /** 字符串；唯一带色相的文字灰，色相取自 Clay。 */
  string: string;
  /** 关键字、函数调用、参数、内置变量、注解、宏、删除行。 */
  muted: string;
  /** 注释、文档注释、引用。 */
  comment: string;
  /** 标点、运算符、内置函数。 */
  subtle: string;
  /** 函数定义，等于 --color-clay-text。 */
  clay: string;
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

const LIGHT_SYNTAX: SyntaxPalette = {
  background: '#fafaf8',
  foreground: '#2b2b29',
  bright: '#151515',
  secondary: '#3e3e3e',
  string: '#56433c',
  muted: '#4f4f4f',
  comment: '#5c5c5c',
  subtle: '#6a6a6a',
  clay: '#b4532f',
  gutter: '#6b6b69',
  lineHighlight: 'rgba(0, 0, 0, 0.045)',
  selection: 'rgba(217, 119, 87, 0.22)',
  cursor: '#d97757',
};

const DARK_SYNTAX: SyntaxPalette = {
  background: '#161615',
  foreground: '#dad8d2',
  bright: '#f2f1ee',
  secondary: '#bebebe',
  string: '#c2afa8',
  muted: '#a7a7a7',
  comment: '#989898',
  subtle: '#898989',
  clay: '#e08264',
  gutter: '#8b8984',
  lineHighlight: 'rgba(255, 255, 255, 0.05)',
  selection: 'rgba(217, 119, 87, 0.3)',
  cursor: '#d97757',
};

/**
 * Clay 的 ANSI 16 色表：彩色槽位映射回灰阶，红色是 Clay。颜色名是绝对的，两个变体中
 * black 都比 white 深：深色里 white 是正文、black 取页面底 surface-0；浅色里 black 是
 * 正文，white 取标点灰，bright white 取 Clay 浅色的行号灰。
 */
function clayAnsi(s: SyntaxPalette, ends: { black: string; white: string; brightWhite: string }) {
  return {
    black: ends.black,
    red: s.clay,
    green: s.bright,
    yellow: s.secondary,
    blue: s.secondary,
    magenta: s.muted,
    cyan: s.secondary,
    white: ends.white,
    brightBlack: s.comment,
    brightRed: s.clay,
    brightGreen: s.bright,
    brightYellow: s.foreground,
    brightBlue: s.foreground,
    brightMagenta: s.secondary,
    brightCyan: s.foreground,
    brightWhite: ends.brightWhite,
  } satisfies AnsiPalette;
}

export const CODE_PALETTES: Record<ThemeName, CodePalette> = {
  light: {
    syntax: LIGHT_SYNTAX,
    ansi: clayAnsi(LIGHT_SYNTAX, {
      black: LIGHT_SYNTAX.foreground,
      white: LIGHT_SYNTAX.subtle,
      brightWhite: '#9d9d9d',
    }),
  },
  dark: {
    syntax: DARK_SYNTAX,
    ansi: clayAnsi(DARK_SYNTAX, {
      black: '#0f0f0e',
      white: DARK_SYNTAX.foreground,
      brightWhite: DARK_SYNTAX.bright,
    }),
  },
};

/**
 * 每个语法角色在 tmTheme 中对应的 TextMate scope。syntect 以类名输出 scope 的每一段，
 * 生成的 CSS 按类数决定优先级，所以更长的 scope（keyword.operator.word）会压过更短的
 * （keyword.operator）。不要映射 meta.function-call 这类外壳 scope：它包着整个调用，
 * 参数里的变量会跟着变色。
 */
const TM_SCOPES: [keyof SyntaxPalette, string, string?][] = [
  ['comment', 'comment, punctuation.definition.comment, markup.quote'],
  [
    'muted',
    'keyword, storage, variable.parameter, variable.language, support.variable, variable.annotation, punctuation.definition.annotation',
  ],
  ['subtle', 'punctuation, keyword.operator, meta.brace'],
  ['muted', 'keyword.operator.word'],
  ['string', 'string, punctuation.definition.string, markup.raw'],
  [
    'secondary',
    'constant, support.constant, variable.other.constant, entity.name.constant, variable.other.member, variable.other.property, entity.name.label, entity.name.tag, entity.other.attribute-name',
  ],
  [
    'secondary',
    'entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, entity.name.trait, entity.name.impl, entity.name.interface, entity.name.union, entity.other.inherited-class, support.type, support.class, storage.type.primitive',
  ],
  // JSON、TOML 的键是成员，不是字符串。
  [
    'secondary',
    'meta.structure.dictionary.key string, meta.structure.dictionary.key punctuation.definition.string, meta.mapping.key string, meta.mapping.key punctuation.definition.string',
  ],
  ['clay', 'entity.name.function'],
  ['muted', 'variable.function, support.function, support.macro, entity.name.macro'],
  ['subtle', 'support.function.builtin'],
  ['bright', 'markup.heading, entity.name.section', 'bold'],
  ['foreground', 'markup.bold', 'bold'],
  ['foreground', 'meta.link.inline.description, string.other.link', 'underline'],
  ['secondary', 'markup.underline.link', 'underline'],
  ['bright', 'markup.inserted, invalid'],
  ['muted', 'markup.deleted, meta.diff.range'],
  ['secondary', 'markup.changed, meta.diff.header'],
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
 * 与上面 TM_SCOPES 的归类一致。highlight.js 的 built_in 混有内置类型、常量和函数，
 * 归入内置变量（muted），不按内置函数退到标点灰。
 * 未列出的类（变量、属性访问、doctag）沿用外层颜色。
 */
const HLJS_CLASSES: [keyof SyntaxPalette, string[]][] = [
  ['comment', ['comment', 'quote']],
  [
    'muted',
    [
      'keyword',
      'meta',
      'meta .hljs-keyword',
      'params',
      'variable.language_',
      'built_in',
      'title.function_.invoke__',
      'deletion',
    ],
  ],
  ['string', ['string', 'regexp', 'code', 'meta .hljs-string']],
  [
    'secondary',
    [
      'number',
      'literal',
      'symbol',
      'variable.constant_',
      'char.escape_',
      'type',
      'title.class_',
      'title.class_.inherited__',
      'name',
      'selector-tag',
      'selector-class',
      'selector-id',
      'attr',
      'attribute',
      'property',
      'link',
    ],
  ],
  ['clay', ['title', 'title.function_']],
  ['subtle', ['punctuation', 'operator', 'tag', 'bullet']],
  ['foreground', ['subst']],
  ['bright', ['section', 'addition']],
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
${scope} .hljs-link {
  text-decoration: underline;
}
`;
}
