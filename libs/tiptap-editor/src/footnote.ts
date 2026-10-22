import type { Editor } from '@tiptap/core';
import { Extension, mergeAttributes, Node, nodeInputRule } from '@tiptap/core';
import type { Node as PMNode } from '@tiptap/pm/model';
import { Plugin, PluginKey } from '@tiptap/pm/state';

/**
 * 脚注节点(footnoteRef 引用 + footnoteDef 定义)与实时编号扩展。
 *
 * 设计目标:让脚注像数学公式、代码块一样在富文本编辑器内所见即所得——
 * 引用 `[^1]` 渲染成上标编号节点,定义 `[^1]: 内容` 渲染成可编辑卡片块,
 * 编号按引用首次出现顺序分配,与线上文章页(服务端 render_markdown_enhanced
 * 的 GFM 模式)一致。
 *
 * 技术范式照搬 `./math.ts`:inline/block atom Node + markdown spec 三件套
 * (markdownTokenizer/parseMarkdown/renderMarkdown)。atom 节点的 renderMarkdown
 * 直接拼字面量,绕过 @tiptap/markdown 的 escapeMarkdownSyntax(它会把
 * `[`→`\[`、`]`→`\]`,这正是旧版 `unescapeFootnoteSyntax` 要修复的问题——
 * 走节点路径后该转义不再发生,unescape 后处理可移除)。
 *
 * 编号是"派生视图",不写回 attrs(否则每次重算触发 transaction 循环):
 * FootnoteNumbering 扩展在文档变化后重算 Map<label, number> 并写入
 * editor.storage.footnoteNumbering,NodeView 读取它绘制编号。
 *
 * 编号顺序与服务端 src/api/markdown/mod.rs 中 render_markdown_enhanced 的 fn_order 对齐:按
 * footnoteRef 在文档中首次出现顺序给唯一 label 分配 1, 2, 3…
 */

// ---- tokenizer 正则 ----
// inline `[^label]` 引用:label 不含 ] 与换行。负向断言 (?!:) 排除 `[label]:` 定义形式
// (定义是 block 级,理论上不会进 inline tokenize,但加断言更稳妥,也防 [^1]: 被段落内联误吞)。
const FOOTNOTE_REF_REGEX = /^\[\^([^\]\n]+)\](?!:)/;
// input rule:行内输入 `[^label]` 后空格/标点触发(仿 InlineMath 的行尾锚定)。
// 要求 `[^` 前是空白或行首,避免误吞普通文本里的 `[`。
const FOOTNOTE_REF_INPUT_REGEX = /(?:^|\s)\[\^([^\]\n\\]+)\](?:[\s。，、；：！？.,;:!?]|$)$/;
// block `[^label]: 内容` 定义首行 + 后续缩进续行(4 空格或 tab)。
// 续行收集规则与服务端 pulldown-cmark GFM 模式一致。
const FOOTNOTE_DEF_REGEX = /^\[\^([^\]\n]+)\]:[ \t]*(.*)\n?((?:[ \t]+[^\n]*(?:\n|$))*)/;

/** storage 里存放的编号表,NodeView 读取它绘制编号。 */
export interface FootnoteNumberingStorage {
  /** label → 显示编号(1 起)。 */
  numbering: Map<string, number>;
  /** 每次重算自增,NodeView 用它判断是否需要重绘。 */
  version: number;
  /** label → 定义内容(供引用节点双击预览)。 */
  definitions: Map<string, string>;
}

/** 把多行 content 重新格式化为 GFM 续行:首行跟在 `[^label]: ` 后,续行缩进 4 空格。 */
function formatDefinitionBody(content: string): string {
  const lines = content.replace(/\r\n/g, '\n').split('\n');
  if (lines.length <= 1) return content;
  // 续行每行加 4 空格缩进(空行不缩进,保持可读)。
  return lines.map((line, i) => (i === 0 || line === '' ? line : `    ${line}`)).join('\n');
}

/** 从 marked 捕获的续行 raw 还原出纯内容(去掉每行前导缩进)。 */
function unindentContinuation(raw: string): string {
  return raw
    .split('\n')
    .map((line) => line.replace(/^[ \t]{1,4}/, ''))
    .join('\n')
    .replace(/\n+$/, '');
}

// ============================================================
// NodeView:脚注引用(inline 上标)
// ============================================================
class FootnoteRefNodeView {
  private node: PMNode;
  private editor: Editor;
  private container: HTMLSpanElement;
  private linkEl: HTMLAnchorElement;

  constructor(opts: { node: PMNode; editor: Editor }) {
    this.node = opts.node;
    this.editor = opts.editor;

    this.container = document.createElement('span');
    this.container.setAttribute('contenteditable', 'false');
    this.container.classList.add('fn-ref-node');

    this.linkEl = document.createElement('a');
    this.linkEl.classList.add('fn-ref-link');
    this.linkEl.setAttribute('role', 'doc-noteref');
    this.container.appendChild(this.linkEl);

    this.applyAttrs();
    // 初始编号文本先占位,plugin 的 view.update 会立即覆盖为真实编号。
    this.linkEl.textContent = `^${(this.node.attrs.label as string) ?? ''}`;
    this.attachPreview();
  }

  get dom(): HTMLElement {
    return this.container;
  }

  get contentDOM(): HTMLElement | null {
    return null;
  }

  update(node: PMNode): boolean {
    if (node.type !== this.node.type) return false;
    const labelChanged = (node.attrs.label as string) !== (this.node.attrs.label as string);
    this.node = node;
    // label 变了才需更新 data-label/title;编号文本由 plugin 维护,这里不动。
    if (labelChanged) {
      this.applyAttrs();
    }
    return true;
  }

  ignoreMutation(): boolean {
    return true;
  }

  stopEvent(): boolean {
    return false;
  }

  destroy(): void {
    this.container.innerHTML = '';
  }

  private getStorage(): FootnoteNumberingStorage | null {
    const s = (this.editor.storage as unknown as Record<string, unknown>).footnoteNumbering;
    return (s as FootnoteNumberingStorage) ?? null;
  }

  private applyAttrs(): void {
    const label = (this.node.attrs.label as string) ?? '';
    this.container.setAttribute('data-label', label);
    this.linkEl.title = `脚注：${label}`;
  }

  /** 双击弹出定义内容的只读预览气泡(定义节点里编辑)。 */
  private attachPreview(): void {
    this.container.addEventListener('dblclick', (e) => {
      e.preventDefault();
      e.stopPropagation();
      const label = (this.node.attrs.label as string) ?? '';
      const def = this.getStorage()?.definitions.get(label);
      // 简易浮层:聚焦后失焦消失。避免引入额外 UI 依赖。
      const tip = document.createElement('span');
      tip.className = 'fn-ref-preview';
      tip.textContent = def ?? '（未定义的脚注）';
      tip.setAttribute('contenteditable', 'false');
      this.container.appendChild(tip);
      const dismiss = () => {
        tip.remove();
        document.removeEventListener('click', onOutside, true);
      };
      const onOutside = (ev: MouseEvent) => {
        if (!tip.contains(ev.target as globalThis.Node)) dismiss();
      };
      setTimeout(() => document.addEventListener('click', onOutside, true), 0);
      tip.addEventListener('click', (ev) => ev.stopPropagation());
    });
  }
}

// ============================================================
// NodeView:脚注定义(block 卡片,可双击编辑)
// ============================================================
class FootnoteDefNodeView {
  private node: PMNode;
  private editor: Editor;
  private getPos?: () => number | undefined;

  private container: HTMLElement;
  private readonly labelEl: HTMLSpanElement;
  private readonly contentEl: HTMLSpanElement;
  private editEl: HTMLTextAreaElement | null = null;

  constructor(opts: { node: PMNode; editor: Editor; getPos?: () => number | undefined }) {
    this.node = opts.node;
    this.editor = opts.editor;
    this.getPos = opts.getPos;

    this.container = document.createElement('aside');
    this.container.classList.add('footnote-definition');
    this.container.setAttribute('contenteditable', 'false');
    this.container.setAttribute('role', 'doc-footnote');
    this.container.title = '双击编辑脚注内容';

    this.labelEl = document.createElement('sup');
    this.labelEl.classList.add('footnote-definition-label');
    this.container.appendChild(this.labelEl);

    this.contentEl = document.createElement('span');
    this.contentEl.classList.add('footnote-definition-content');
    this.container.appendChild(this.contentEl);

    this.applyAttrs();
    // 编号文本由 plugin 维护;此处先占位。
    this.labelEl.textContent = `^${(this.node.attrs.label as string) ?? ''}`;
    this.attachEdit();
  }

  get dom(): HTMLElement {
    return this.container;
  }

  get contentDOM(): HTMLElement | null {
    return null;
  }

  update(node: PMNode): boolean {
    if (node.type !== this.node.type) return false;
    const oldLabel = this.node.attrs.label as string;
    const oldContent = this.node.attrs.content as string;
    this.node = node;
    // label/content 变了才重绘 data 属性与内容文本;编号由 plugin 维护。
    if (
      oldLabel !== (node.attrs.label as string) ||
      oldContent !== (node.attrs.content as string)
    ) {
      this.applyAttrs();
    }
    return true;
  }

  ignoreMutation(): boolean {
    return true;
  }

  stopEvent(): boolean {
    // 编辑态下 textarea 事件交给浏览器;非编辑态双击由 attachEdit 处理。
    return this.editEl !== null;
  }

  destroy(): void {
    this.container.innerHTML = '';
    this.editEl = null;
  }

  private applyAttrs(): void {
    const label = (this.node.attrs.label as string) ?? '';
    const content = (this.node.attrs.content as string) ?? '';
    this.container.setAttribute('data-label', label);
    // 编辑态不覆盖 textarea,避免打断输入。
    if (!this.editEl) {
      this.contentEl.textContent = content;
    }
  }

  /** 仿 MathNodeView.enterEdit:textarea + Ctrl/Cmd+Enter 或失焦提交、Esc 放弃。 */
  private attachEdit(): void {
    this.container.addEventListener('dblclick', (e) => {
      e.preventDefault();
      e.stopPropagation();
      this.enterEdit();
    });
  }

  private enterEdit(): void {
    if (this.editEl) return;
    const ta = document.createElement('textarea');
    ta.className = 'footnote-definition-edit';
    ta.value = (this.node.attrs.content as string) ?? '';
    ta.rows = 3;
    ta.spellcheck = false;
    ta.placeholder = '脚注内容(Markdown)';

    this.contentEl.classList.add('footnote-definition-content-hidden');
    this.container.appendChild(ta);
    this.editEl = ta;
    ta.focus();
    ta.setSelectionRange(ta.value.length, ta.value.length);

    const commit = () => {
      if (!this.editEl) return;
      const next = this.editEl.value;
      this.editEl = null;
      ta.remove();
      this.contentEl.classList.remove('footnote-definition-content-hidden');
      const pos = this.getPos?.();
      if (pos !== undefined) {
        this.editor
          .chain()
          .focus()
          .command(({ tr, state }) => {
            const target = state.doc.nodeAt(pos);
            if (!target || target.type !== this.node.type) return false;
            tr.setNodeMarkup(pos, undefined, { ...target.attrs, content: next });
            return true;
          })
          .run();
      } else {
        (this.node.attrs as { content: string }).content = next;
        this.applyAttrs();
      }
    };

    ta.addEventListener('keydown', (ev) => {
      if (ev.key === 'Enter' && (ev.metaKey || ev.ctrlKey)) {
        ev.preventDefault();
        commit();
      } else if (ev.key === 'Escape') {
        ev.preventDefault();
        this.editEl = null;
        ta.remove();
        this.contentEl.classList.remove('footnote-definition-content-hidden');
      }
    });
    ta.addEventListener('blur', commit);
  }
}

// ============================================================
// footnoteRef:inline atom 引用节点
// ============================================================
export const FootnoteRef = Node.create({
  name: 'footnoteRef',
  inline: true,
  group: 'inline',
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      label: {
        default: '',
        parseHTML: (el) => (el as HTMLElement).getAttribute('data-label') ?? '',
        renderHTML: (attrs) => {
          const v = attrs.label;
          return v == null || v === '' ? {} : { 'data-label': v };
        },
      },
    };
  },

  parseHTML() {
    return [{ tag: 'sup[data-footnote-ref]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['sup', mergeAttributes({ 'data-footnote-ref': 'true' }, HTMLAttributes)];
  },

  markdownTokenName: 'footnoteRef',
  markdownTokenizer: {
    name: 'footnoteRef',
    level: 'inline',
    start: (src: string) => src.indexOf('[^'),
    tokenize: (src: string) => {
      const m = FOOTNOTE_REF_REGEX.exec(src);
      if (!m) return undefined;
      return { type: 'footnoteRef', raw: m[0], text: m[1] };
    },
  },
  parseMarkdown: (token, helpers) => helpers.createNode('footnoteRef', { label: token.text || '' }),
  renderMarkdown: (node) => `[^${node.attrs?.label ?? ''}]`,

  addNodeView() {
    return ({ node, editor }) => new FootnoteRefNodeView({ node, editor });
  },

  addInputRules() {
    return [
      nodeInputRule({
        find: FOOTNOTE_REF_INPUT_REGEX,
        type: this.type,
        getAttributes: (match) => ({ label: match[1] ?? '' }),
      }),
    ];
  },
});

// ============================================================
// footnoteDef:block atom 定义节点
// ============================================================
export const FootnoteDef = Node.create({
  name: 'footnoteDef',
  group: 'block',
  atom: true,
  defining: true,

  addAttributes() {
    return {
      label: {
        default: '',
        parseHTML: (el) => (el as HTMLElement).getAttribute('data-label') ?? '',
        renderHTML: (attrs) => {
          const v = attrs.label;
          return v == null || v === '' ? {} : { 'data-label': v };
        },
      },
      content: {
        default: '',
        parseHTML: (el) => (el as HTMLElement).getAttribute('data-content') ?? '',
        renderHTML: (attrs) => {
          const v = attrs.content;
          return v == null || v === '' ? {} : { 'data-content': v };
        },
      },
    };
  },

  parseHTML() {
    return [{ tag: 'aside[data-footnote-def]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['aside', mergeAttributes({ 'data-footnote-def': 'true' }, HTMLAttributes)];
  },

  markdownTokenName: 'footnoteDef',
  markdownTokenizer: {
    name: 'footnoteDef',
    level: 'block',
    start: (src: string) => src.indexOf('[^'),
    tokenize: (src: string) => {
      const m = FOOTNOTE_DEF_REGEX.exec(src);
      if (!m) return undefined;
      const label = m[1];
      const firstLine = m[2] ?? '';
      const continuation = unindentContinuation(m[3] ?? '');
      const content = continuation ? `${firstLine}\n${continuation}` : firstLine;
      return { type: 'footnoteDef', raw: m[0], label, text: content };
    },
  },
  parseMarkdown: (token, helpers) =>
    helpers.createNode('footnoteDef', { label: token.label || '', content: token.text || '' }),
  renderMarkdown: (node) => {
    const label = node.attrs?.label ?? '';
    const content = node.attrs?.content ?? '';
    return `[^${label}]: ${formatDefinitionBody(content)}`;
  },

  addNodeView() {
    return ({ node, editor, getPos }) => new FootnoteDefNodeView({ node, editor, getPos });
  },
});

// ============================================================
// FootnoteNumbering:实时编号扩展
// ============================================================
const FOOTNOTE_NUMBERING_KEY = new PluginKey('footnoteNumbering');

export const FootnoteNumbering = Extension.create({
  name: 'footnoteNumbering',

  addStorage() {
    return {
      numbering: new Map<string, number>(),
      definitions: new Map<string, string>(),
      version: 0,
    } satisfies FootnoteNumberingStorage;
  },

  addProseMirrorPlugins() {
    const editor = this.editor;
    return [
      new Plugin({
        key: FOOTNOTE_NUMBERING_KEY,
        // view.update 在每次 transaction 应用后触发;在此重算编号表、写 storage,
        // 并直接遍历 DOM 重绘脚注节点的编号文本。
        //
        // 为什么不 dispatch transaction 强制 NodeView.update:那会形成
        // update→dispatch→update 链(虽 changed 守卫能止住,但空 transaction
        // 会让光标/选区异常)。直接改 DOM 文本是 NodeView 外部副作用,但脚注
        // 编号是纯派生视图(不进文档模型),ProseMirror 的 ignoreMutation 已让
        // 它对编辑无感,符合 atom + ignoreMutation 范式。
        view() {
          return {
            update(view) {
              const numbering = new Map<string, number>();
              const definitions = new Map<string, string>();
              let order = 0;
              // 先扫定义(label → content),供引用节点双击预览。
              view.state.doc.descendants((node) => {
                if (node.type.name === 'footnoteDef') {
                  definitions.set(node.attrs.label as string, node.attrs.content as string);
                }
                return true;
              });
              // 再按文档顺序扫引用,首次出现的 label 分配递增编号。
              view.state.doc.descendants((node) => {
                if (node.type.name === 'footnoteRef') {
                  const label = node.attrs.label as string;
                  if (!numbering.has(label)) {
                    order += 1;
                    numbering.set(label, order);
                  }
                }
                return true;
              });
              const fnStorage = (
                editor.storage as unknown as { footnoteNumbering: FootnoteNumberingStorage }
              ).footnoteNumbering;
              const numberingChanged = !mapsEqual(
                fnStorage.numbering as Map<string, unknown>,
                numbering as Map<string, unknown>,
              );
              const defsChanged = !mapsEqual(
                fnStorage.definitions as Map<string, unknown>,
                definitions as Map<string, unknown>,
              );
              fnStorage.numbering = numbering;
              fnStorage.definitions = definitions;
              if (numberingChanged || defsChanged || fnStorage.version === 0) {
                fnStorage.version += 1;
                redrawFootnoteLabels(view.dom, numbering);
              }
            },
          };
        },
      }),
    ];
  },
});

/** 遍历 DOM,把所有脚注节点的编号文本刷成最新值。 */
function redrawFootnoteLabels(dom: HTMLElement, numbering: Map<string, number>): void {
  const nodes = dom.querySelectorAll('.fn-ref-node, .footnote-definition');
  for (const el of nodes) {
    const label = el.getAttribute('data-label') ?? '';
    const num = numbering.get(label);
    const display = num !== undefined ? String(num) : `^${label}`;
    const target = el.classList.contains('fn-ref-node')
      ? el.querySelector('.fn-ref-link')
      : el.querySelector('.footnote-definition-label');
    if (target) target.textContent = display;
  }
}

function mapsEqual(a: Map<string, unknown>, b: Map<string, unknown>): boolean {
  if (a.size !== b.size) return false;
  for (const [k, v] of a) {
    if (!b.has(k) || b.get(k) !== v) return false;
  }
  return true;
}
