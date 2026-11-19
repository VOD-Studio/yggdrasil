import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
} from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
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

/** 传给 CodeMirrorEditor.create 的配置：Rust 端用空对象加 setter 填充。字段全部可选。 */
export interface EditorOptions {
  language?: string;
  theme?: ThemeName;
  vim?: boolean;
  schema?: SqlSchema;
  value?: string;
  onChange?: (value: string) => void;
  onReady?: () => void;
  /** Ctrl/Cmd + Enter 快捷键回调（SQL 控制台、可运行代码块触发执行）。 */
  onRunShortcut?: () => void;
  /** Vim 模式变化回调：normal / insert / visual / replace；开启 Vim 时先报一次 normal。 */
  onVimModeChange?: (mode: string) => void;
}

type Language = 'go' | 'python' | 'rust' | 'javascript' | 'typescript' | 'sql';
type LanguageExtension = (schema: SqlSchema) => Extension;

function normalizeLanguage(lang: string): Language {
  switch (lang.toLowerCase()) {
    case 'go':
    case 'golang':
      return 'go';
    case 'python':
      return 'python';
    case 'rust':
      return 'rust';
    case 'node':
    case 'javascript':
    case 'js':
      return 'javascript';
    case 'bun':
    case 'typescript':
    case 'ts':
      return 'typescript';
    default:
      return 'sql';
  }
}

async function loadLanguage(language: Language): Promise<LanguageExtension> {
  switch (language) {
    case 'go':
      return (await import('./languages/go')).go;
    case 'python':
      return (await import('./languages/python')).python;
    case 'rust':
      return (await import('./languages/rust')).rust;
    case 'javascript':
    case 'typescript': {
      const { javascript } = await import('./languages/javascript');
      return () => javascript({ typescript: language === 'typescript' });
    }
    case 'sql':
      return (await import('./languages/sql')).sqlExtension;
  }
}

/** 通过 Compartment 热切换扩展，异步加载期间保留编辑状态。 */
export class CodeMirrorInstance {
  private destroyed = false;
  private view: EditorView;
  private themeCompartment = new Compartment();
  private languageCompartment = new Compartment();
  private vimCompartment = new Compartment();
  private vimEnabled = false;
  private vimRequest = 0;
  private languageRequest = 0;
  private language: Language = 'sql';
  private sqlExtension?: LanguageExtension;
  readonly ready: Promise<void>;
  private schema: SqlSchema;

  constructor(
    container: HTMLElement,
    options: EditorOptions,
    private readonly onDestroy?: () => void,
  ) {
    const theme: ThemeName = options.theme ?? 'light';
    this.schema = options.schema ?? { tables: [] };

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
          this.vimCompartment.of([]),
          this.themeCompartment.of(themeExtension(theme)),
          this.languageCompartment.of([]),
          EditorView.updateListener.of((v) => {
            if (v.docChanged) {
              options.onChange?.(this.view.state.doc.toString());
            }
          }),
        ],
      }),
      parent: container,
    });

    this.onVimModeChange = options.onVimModeChange;
    this.ready = Promise.all([
      this.setLanguage(options.language ?? 'sql'),
      this.setVim(options.vim ?? false),
    ]).then(() => {
      if (!this.destroyed) options.onReady?.();
    });
  }

  private onVimModeChange?: (mode: string) => void;

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

  /** 热切换 Vim 键位，跳过已经生效或正在加载的相同设置。 */
  async setVim(enabled: boolean): Promise<void> {
    if (enabled === this.vimEnabled) return;
    this.vimEnabled = enabled;
    const request = ++this.vimRequest;
    if (!enabled) {
      this.view.dispatch({ effects: this.vimCompartment.reconfigure([]) });
      return;
    }

    try {
      const { getCM, vim } = await import('./vim');
      if (this.destroyed || request !== this.vimRequest) return;
      this.view.dispatch({ effects: this.vimCompartment.reconfigure(vim()) });
      const notify = this.onVimModeChange;
      if (notify) {
        getCM(this.view)?.on('vim-mode-change', (event: { mode: string }) => notify(event.mode));
        notify('normal');
      }
    } catch (error) {
      if (this.destroyed || request !== this.vimRequest) return;
      this.vimEnabled = false;
      console.error('CodeMirror Vim 加载失败', error);
    }
  }

  /** 保留最新一次语言选择，忽略因切换或卸载而过期的加载结果。 */
  async setLanguage(lang: string): Promise<void> {
    this.language = normalizeLanguage(lang);
    const language = this.language;
    const request = ++this.languageRequest;
    try {
      const extension = await loadLanguage(language);
      if (this.destroyed || request !== this.languageRequest) return;
      this.view.dispatch({ effects: this.languageCompartment.reconfigure(extension(this.schema)) });
      if (language === 'sql') this.sqlExtension = extension;
    } catch (error) {
      if (!this.destroyed && request === this.languageRequest) {
        console.error('CodeMirror 语法加载失败', error);
      }
    }
  }

  /** schema 可能早于 SQL 分包返回，语言加载时使用最新值。 */
  setSchema(schema: SqlSchema): void {
    this.schema = schema;
    if (this.language === 'sql' && this.sqlExtension) {
      this.view.dispatch({
        effects: this.languageCompartment.reconfigure(this.sqlExtension(schema)),
      });
    }
  }

  focus(): void {
    this.view.focus();
  }

  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    this.view.destroy();
    this.onDestroy?.();
  }
}
