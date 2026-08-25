import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
} from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { go } from '@codemirror/lang-go';
import { javascript } from '@codemirror/lang-javascript';
import { python } from '@codemirror/lang-python';
import { PostgreSQL, sql } from '@codemirror/lang-sql';
import {
  bracketMatching,
  codeFolding,
  defaultHighlightStyle,
  foldGutter,
  foldKeymap,
  indentOnInput,
  syntaxHighlighting,
} from '@codemirror/language';
import { lintKeymap } from '@codemirror/lint';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
import { Compartment, EditorState, type Extension, Prec } from '@codemirror/state';
import {
  crosshairCursor,
  drawSelection,
  dropCursor,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from '@codemirror/view';
import { vim } from '@replit/codemirror-vim';
import { type ThemeName, themeExtension } from './themes';

/**
 * 本地版 basicSetup：与 `codemirror` 包同名的便利数组成员完全一致，唯一差别是
 * 折叠图标换成几何三角形字符。
 *
 * 原因：`codemirror` 的 basicSetup 调 `foldGutter()` 用默认 `openText: "⌄"`
 * (U+2304 DOWNWARDS HARPOON WITH BARB… 实为「下花括号」)，这是个基线/字形表现
 * 不稳定的符号字符——在多数 mono 字体里墨迹落在字符框底部，导致折叠三角在行框里
 * 明显偏低。改成 `▾` (U+25BE BLACK DOWN-POINTING SMALL TRIANGLE) / `▸`
 * (U+25B8 BLACK RIGHT-POINTING SMALL TRIANGLE) 两个「几何形状」块字符，字形稳定
 * 居中，配合 themes.ts 的 flex 居中后，三角精准落在每行行框中央。
 *
 * 按 CodeMirror 官方建议：basicSetup 「does not allow customization」，需要定制时
 * 「copy this package's source ... and adjust as desired」，此处即采用此法。
 */
const basicSetup: Extension[] = [
  lineNumbers(),
  highlightActiveLineGutter(),
  highlightSpecialChars(),
  history(),
  foldGutter({
    openText: '▾',
    closedText: '▸',
  }),
  drawSelection(),
  dropCursor(),
  EditorState.allowMultipleSelections.of(true),
  indentOnInput(),
  syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
  bracketMatching(),
  closeBrackets(),
  autocompletion(),
  rectangularSelection(),
  crosshairCursor(),
  highlightActiveLine(),
  highlightSelectionMatches(),
  keymap.of([
    ...closeBracketsKeymap,
    ...defaultKeymap,
    ...searchKeymap,
    ...historyKeymap,
    ...foldKeymap,
    ...completionKeymap,
    ...lintKeymap,
  ]),
  codeFolding(),
];

/** SQL 补全用 schema 数据（由 Rust 侧从实时库拉取注入）。 */
export interface SqlSchema {
  tables: { name: string; columns: string[] }[];
}

/**
 * 传给 CodeMirrorEditor.create 的配置。
 * 必须是 class（非 interface），以便 TS 擦除后存活，
 * wasm 侧能用 `new EditorOptions()` 构造，并通过 setter 填充字段。
 */
export class EditorOptions {
  language?: string;
  theme?: ThemeName;
  vim?: boolean;
  schema?: SqlSchema;
  value?: string;
  onChange?: (value: string) => void;
  onReady?: () => void;
  /** Ctrl/Cmd + Enter 快捷键回调（SQL 控制台触发执行）。 */
  onRunShortcut?: () => void;
}

/**
 * 把语言标识映射成 CodeMirror 语言 Extension。
 * - `python` → @codemirror/lang-python
 * - `go` / `golang` → @codemirror/lang-go
 * - `node` / `javascript` / `js` → @codemirror/lang-javascript（JS 模式）
 * - `bun` / `typescript` / `ts` → @codemirror/lang-javascript（TypeScript 模式）
 * - `sql` / 缺省 → @codemirror/lang-sql（PostgreSQL 方言 + schema 补全）
 *
 * 归一化后（src/api/code_runner/languages.rs::normalize_lang）CodeRunner 会传来
 * canonical key（node/bun/python/go/rust），但这里仍保留别名分支作防御——
 * CodeMirror 也用在 SQL 控制台等场景，调用方可能直接传别名。
 *
 * SQL 分支使用 schema 补全；非 SQL 语言忽略 schema（补全仅对 SQL 有意义）。
 */
function buildLanguageExtension(lang: string, schema: SqlSchema): Extension {
  const normalized = (lang ?? '').toLowerCase();
  if (normalized === 'go' || normalized === 'golang') {
    return go();
  }
  if (normalized === 'python') {
    return python();
  }
  if (normalized === 'node' || normalized === 'javascript' || normalized === 'js') {
    return javascript();
  }
  if (normalized === 'bun' || normalized === 'typescript' || normalized === 'ts') {
    // bun 跑 TypeScript，编辑器用 TS 模式获得类型感知高亮（接口/类型注解等）。
    return javascript({ typescript: true });
  }
  // sql / 缺省：保留原有 PostgreSQL + schema 补全行为
  return sql({
    dialect: PostgreSQL,
    schema: schemaToCompletions(schema),
    upperCaseKeywords: true,
  });
}

/**
 * CodeMirror 实例封装。
 * create() 时用四个 Compartment 注入 theme/language/schema/vim，
 * 支持后续热切换（reconfigure）而不重建实例——保留 Vim 状态、光标、撤销栈。
 */
export class CodeMirrorInstance {
  private view: EditorView;
  private themeCompartment = new Compartment();
  private languageCompartment = new Compartment();
  private vimCompartment = new Compartment();
  private language: string;
  private schema: SqlSchema;

  constructor(container: HTMLElement, options: EditorOptions) {
    const theme: ThemeName = options.theme ?? 'light';
    const schema = options.schema ?? { tables: [] };
    this.schema = schema;
    this.language = options.language ?? 'sql';

    this.view = new EditorView({
      state: EditorState.create({
        doc: options.value ?? '',
        extensions: [
          basicSetup,
          // Ctrl/Cmd + Enter 运行快捷键：用 Prec.highest 包裹，保证在 vim 的
          // ViewPlugin 按键拦截之前命中（vim 自己也用 Prec.highest，跟随此惯例）。
          ...(options.onRunShortcut
            ? [
                Prec.highest(
                  keymap.of([
                    {
                      key: 'Mod-Enter',
                      preventDefault: true,
                      run: () => {
                        options.onRunShortcut?.();
                        return true;
                      },
                    },
                  ]),
                ),
              ]
            : []),
          // vim 必须在 keymap 最前（@replit/codemirror-vim 仓库要求）
          this.vimCompartment.of(options.vim ? [vim()] : []),
          this.themeCompartment.of(themeExtension(theme)),
          this.languageCompartment.of(buildLanguageExtension(this.language, schema)),
          EditorView.updateListener.of((v) => {
            if (v.docChanged) {
              options.onChange?.(this.view.state.doc.toString());
            }
          }),
        ],
      }),
      parent: container,
    });

    options.onReady?.();
  }

  getValue(): string {
    return this.view.state.doc.toString();
  }

  setValue(s: string): void {
    this.view.dispatch({
      changes: { from: 0, to: this.view.state.doc.length, insert: s },
    });
  }

  /** 热切换主题，不重建实例（Compartment.reconfigure）。 */
  setTheme(theme: ThemeName): void {
    this.view.dispatch({
      effects: this.themeCompartment.reconfigure(themeExtension(theme)),
    });
  }

  /** 热切换 Vim 模式，不重建实例（Compartment.reconfigure）。 */
  setVim(v: boolean): void {
    this.view.dispatch({
      effects: this.vimCompartment.reconfigure(v ? [vim()] : []),
    });
  }

  /** 热切换语言（go/python/node/javascript/sql），不重建实例（Compartment.reconfigure）。 */
  setLanguage(lang: string): void {
    this.language = lang;
    this.view.dispatch({
      effects: this.languageCompartment.reconfigure(buildLanguageExtension(lang, this.schema)),
    });
  }

  /** 更新 SQL 补全 schema（Compartment.reconfigure）。
   *  当前语言为 SQL 时立即生效；非 SQL 语言会缓存 schema，切回 SQL 时生效。 */
  setSchema(schema: SqlSchema): void {
    this.schema = schema;
    // 仅当当前是 SQL 语言时才重配 extension（其它语言不消费 schema）。
    if ((this.language ?? '').toLowerCase() === 'sql' || !this.language) {
      this.view.dispatch({
        effects: this.languageCompartment.reconfigure(buildLanguageExtension('sql', schema)),
      });
    }
  }

  focus(): void {
    this.view.focus();
  }

  destroy(): void {
    this.view.destroy();
  }
}

/** 把 SqlSchema 转成 @codemirror/lang-sql 期望的补全结构。 */
function schemaToCompletions(schema: SqlSchema) {
  return schema.tables.map((t) => ({
    label: t.name,
    type: 'table',
    detail: 'table',
    columns: t.columns.map((c) => ({ label: c, type: 'column' })),
  }));
}

// 暴露 EditorOptions 到 window，供 wasm-bindgen 用 new EditorOptions()。
// IIFE 的 name 只能挂一个全局（CodeMirrorEditor），故手动 hoist EditorOptions。
declare global {
  interface Window {
    EditorOptions: typeof EditorOptions;
  }
}
window.EditorOptions = EditorOptions;
