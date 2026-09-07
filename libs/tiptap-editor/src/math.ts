import type { Editor } from '@tiptap/core';
import { mergeAttributes, Node, nodeInputRule } from '@tiptap/core';
import type { Node as PMNode } from '@tiptap/pm/model';
import katex from 'katex';

/**
 * 数学公式节点(inline `$...$` 与 block `$$...$$`)。
 *
 * 背景:`@tiptap/markdown` 的 MarkdownManager.encodeTextForMarkdown 对所有
 * 非代码文本节点做 escapeMarkdownSyntax + encodeHtmlEntities,会把 LaTeX 的
 * `\`→`\\`、`_`→`\_`、`&`→`&amp;`。若公式走普通文本路径,序列化后存库即损坏,
 * 服务端 KaTeX 收到 `\\begin{bmatrix}` 报错。
 *
 * 解法:把公式建模成 atom Node,LaTeX 存在 attrs.latex(不是子文本),并声明
 * markdownTokenizer/parseMarkdown/renderMarkdown 顶层 spec 字段。MarkdownManager
 * 渲染非文本节点时直接调 renderMarkdown 返回值,绕过 encodeTextForMarkdown
 * (源码 MarkdownManager.ts:1116-1182:有 handler 的节点不走 escape 路径)。
 *
 * NodeView 内用客户端 katex.renderToString 渲染预览(类 Notion/Typora)。
 * KaTeX CSS(/katex/katex.min.css)已由 Dioxus.toml 全局注入,字体就绪。
 */

// ---- tokenizer 正则 ----
// inline `$...$`:单行、非贪婪、内部不含 $ 与换行。
// 行尾 $ 锚定,避免「打 5$」被误吞——要求 $ 前至少有一个非 $ 字符且 $ 后是词界/标点/行尾。
const INLINE_MATH_INPUT_REGEX = /(?:^|\s)\$([^$\n]+)\$$/;
// block `$$...$$`:跨行、非贪婪。由 markdownTokenizer 识别(非 input rule)。

/** NodeView 构造参数。 */
interface MathNodeViewOptions {
  node: PMNode;
  editor: Editor;
  getPos?: () => number | undefined;
  displayMode: boolean;
}

/**
 * 数学公式 NodeView:渲染 KaTeX 预览,双击进入源码编辑。
 *
 * 仿 UploadImageNodeView 的 atom 范式:contentDOM=null、ignoreMutation=true、
 * update 做类型引用比较 + latex 变化时重渲染。
 */
class MathNodeView {
  private node: PMNode;
  private editor: Editor;
  private getPos?: () => number | undefined;
  private readonly displayMode: boolean;

  private container: HTMLSpanElement;
  private readonly renderEl: HTMLSpanElement;
  private editEl: HTMLTextAreaElement | null = null;

  constructor(opts: MathNodeViewOptions) {
    this.node = opts.node;
    this.editor = opts.editor;
    this.getPos = opts.getPos;
    this.displayMode = opts.displayMode;

    // inline 用 <span>,block 用 <span> 内部 katex-display(由 displayMode 控制 KaTeX 输出)。
    // 外层统一 span 便于 inline 在文本流中;block 的居中由 .math-block-display CSS 控制。
    this.container = document.createElement('span');
    this.container.classList.add('math-node');
    this.container.classList.add(this.displayMode ? 'math-node-block' : 'math-node-inline');
    this.container.setAttribute('contenteditable', 'false');
    this.container.title = '双击编辑公式源码';

    this.renderEl = document.createElement('span');
    this.renderEl.classList.add('math-node-render');
    this.container.appendChild(this.renderEl);

    this.render();
    this.attachEdit();
  }

  get dom(): HTMLElement {
    return this.container;
  }

  get contentDOM(): HTMLElement | null {
    // atom 节点无可编辑内容区域,光标无法进入。
    return null;
  }

  /** ProseMirror 调用:节点属性变化时重渲染预览。返回 false 拒绝非同类节点。 */
  update(node: PMNode): boolean {
    if (node.type !== this.node.type) return false;
    const oldLatex = this.node.attrs.latex as string;
    const newLatex = node.attrs.latex as string;
    this.node = node;
    if (oldLatex !== newLatex) {
      this.render();
    }
    return true;
  }

  /** KaTeX 输出是受信任的视觉层(无脚本),ProseMirror 不应把它的 mutation 当编辑。 */
  ignoreMutation(): boolean {
    return true;
  }

  /** textarea 自行处理输入，避免 ProseMirror 用输入字符替换选中的公式节点。 */
  stopEvent(event: Event): boolean {
    return this.editEl !== null && event.target === this.editEl;
  }

  destroy(): void {
    this.renderEl.innerHTML = '';
    this.editEl = null;
  }

  /** 用 katex.renderToString 渲染预览。坏 TeX 显示红色错误,不抛异常(对齐服务端 throw_on_error=false)。 */
  private render(): void {
    const latex = (this.node.attrs.latex as string) ?? '';
    try {
      const html = katex.renderToString(latex, {
        displayMode: this.displayMode,
        throwOnError: false,
        output: 'html',
      });
      this.renderEl.innerHTML = html;
      this.container.classList.remove('math-node-error');
    } catch {
      // throwOnError=false 下一般不抛;兜底显示转义原文。
      this.renderEl.textContent = latex;
      this.container.classList.add('math-node-error');
    }
  }

  /** 双击进入源码编辑:显示 textarea,失焦/Enter 回写 attrs。 */
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
    ta.className = 'math-node-edit';
    ta.value = (this.node.attrs.latex as string) ?? '';
    ta.rows = this.displayMode ? 4 : 1;
    ta.spellcheck = false;
    ta.placeholder = 'LaTeX 源码';

    // 隐藏预览,挂上编辑框。
    this.renderEl.classList.add('math-node-render-hidden');
    this.container.appendChild(ta);
    this.editEl = ta;
    ta.focus();
    // 光标置尾。
    ta.setSelectionRange(ta.value.length, ta.value.length);

    const commit = () => {
      if (!this.editEl) return;
      const next = this.editEl.value;
      this.editEl = null;
      ta.remove();
      this.renderEl.classList.remove('math-node-render-hidden');
      // 回写 attrs;getPos 可能 undefined(只读模式/异常),此时仅本地刷新预览。
      const pos = this.getPos?.();
      if (pos !== undefined) {
        this.editor
          .chain()
          .focus()
          .command(({ tr, state }) => {
            const target = state.doc.nodeAt(pos);
            if (!target || target.type !== this.node.type) return false;
            tr.setNodeMarkup(pos, undefined, { ...target.attrs, latex: next });
            return true;
          })
          .run();
      } else {
        // 只读回退:直接改本地 node 引用的 attrs 影响后续 update 比较。
        (this.node.attrs as { latex: string }).latex = next;
        this.render();
      }
    };

    // Ctrl/Cmd+Enter 或失焦提交;Esc 放弃。
    ta.addEventListener('keydown', (ev) => {
      if (ev.key === 'Enter' && (ev.metaKey || ev.ctrlKey)) {
        ev.preventDefault();
        commit();
      } else if (ev.key === 'Escape') {
        ev.preventDefault();
        this.editEl = null;
        ta.remove();
        this.renderEl.classList.remove('math-node-render-hidden');
      }
    });
    ta.addEventListener('blur', commit);
  }
}

/**
 * inline 数学公式节点 `$...$`。
 *
 * atom:不可拆分,整体选中/删除。latex 存 attrs,序列化时 renderMarkdown
 * 直接拼 `$${latex}$`,绕过文本转义路径。
 */
export const InlineMath = Node.create({
  name: 'math',
  inline: true,
  group: 'inline',
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      latex: {
        default: '',
        parseHTML: (el) => (el as HTMLElement).getAttribute('data-latex') ?? '',
        renderHTML: (attrs) => {
          const v = attrs.latex;
          return v == null || v === '' ? {} : { 'data-latex': v };
        },
      },
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-math]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['span', mergeAttributes({ 'data-math': 'inline' }, HTMLAttributes)];
  },

  // ---- markdown spec(顶层字段,见 @tiptap/extension-image dist/index.js:50)----
  markdownTokenName: 'mathInline',
  markdownTokenizer: {
    name: 'mathInline',
    level: 'inline',
    start: (src: string) => src.indexOf('$'),
    tokenize: (src: string) => {
      // 非贪婪,单行,内部不含 $ 与换行。避免误吞 $$(块级先由 block tokenizer 吃掉)。
      const m = /^\$([^$\n]+?)\$/.exec(src);
      if (!m) return undefined;
      return { type: 'mathInline', raw: m[0], text: m[1] };
    },
  },
  parseMarkdown: (token, helpers) => helpers.createNode('math', { latex: token.text || '' }),
  renderMarkdown: (node) => `$${node.attrs?.latex ?? ''}$`,

  addNodeView() {
    return ({ node, editor, getPos }) =>
      new MathNodeView({ node, editor, getPos, displayMode: false });
  },

  addInputRules() {
    // 选中文字后打 `$...$ ` 形式:行尾 $ 触发,把前导 $ 与结尾 $ 之间的文本转成 math 节点。
    // 正则要求 $ 前有空白/行首,避免「价格 5$」误触。
    return [
      nodeInputRule({
        find: INLINE_MATH_INPUT_REGEX,
        type: this.type,
        getAttributes: (match) => ({ latex: match[1] ?? '' }),
      }),
    ];
  },
});

/**
 * 块级数学公式节点 `$$...$$`。独占一行,居中显示。
 *
 * 无 input rule:块级 `$$...$$` 跨行,input rule 不便处理;用户通过源码模式或
 * markdown 解析进入。富文本内可由 InlineMath 的编辑流程沿用(双击改源码)。
 */
export const DisplayMath = Node.create({
  name: 'mathBlock',
  group: 'block',
  atom: true,
  defining: true,

  addAttributes() {
    return {
      latex: {
        default: '',
        parseHTML: (el) => (el as HTMLElement).getAttribute('data-latex') ?? '',
        renderHTML: (attrs) => {
          const v = attrs.latex;
          return v == null || v === '' ? {} : { 'data-latex': v };
        },
      },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-math-block]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['div', mergeAttributes({ 'data-math-block': 'true' }, HTMLAttributes)];
  },

  markdownTokenName: 'mathBlock',
  markdownTokenizer: {
    name: 'mathBlock',
    level: 'block',
    start: (src: string) => src.indexOf('$$'),
    tokenize: (src: string) => {
      // 跨行非贪婪;闭合 $$ 后允许换行。
      const m = /^\$\$([\s\S]+?)\$\$(?:\n|$)/.exec(src);
      if (!m) return undefined;
      return { type: 'mathBlock', raw: m[0], text: m[1].trim() };
    },
  },
  parseMarkdown: (token, helpers) => helpers.createNode('mathBlock', { latex: token.text || '' }),
  renderMarkdown: (node) => `$$${node.attrs?.latex ?? ''}$$`,

  addNodeView() {
    return ({ node, editor, getPos }) =>
      new MathNodeView({ node, editor, getPos, displayMode: true });
  },
});
