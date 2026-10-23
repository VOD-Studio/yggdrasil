import { mergeAttributes, Node } from '@tiptap/core';

/**
 * `<details>/<summary>` 折叠块节点。
 *
 * 只支持规范形态（其他写法维持现状，由服务端往返守卫兜底）：
 *
 * ```
 * <details>            ← 独占一行（可带属性，如 open）
 * <summary>标题</summary>
 *                      ← 空行
 * 正文（任意块级 Markdown）
 *                      ← 空行
 * </details>
 * ```
 *
 * 正文以 Markdown 解析（空行让服务端把 `<details>` 内部当 Markdown 而不是 HTML 块）。
 * summary 行在服务端是原始 HTML 块，所以以原文显示/写回，不解析其中的实体或行内 HTML。
 * block tokenizer 的 `start` 只认行首，原因同 gfm-alert.ts：返回行中间位置会让 marked 错切段落。
 */

const DETAILS_START_REGEX = /(^|\n) {0,3}<details(?:\s[^>]*)?>[ \t]*(?=\n|$)/i;
const DETAILS_OPEN_REGEX = /^ {0,3}<details(\s[^>]*)?>[ \t]*$/i;
const DETAILS_CLOSE_REGEX = /^ {0,3}<\/details>[ \t]*$/i;
const SUMMARY_REGEX = /^ {0,3}<summary>(.*)<\/summary>[ \t]*$/i;

export const Details = Node.create({
  name: 'details',
  group: 'block',
  content: 'detailsSummary block+',
  defining: true,

  addAttributes() {
    return {
      // 原文开标签里 `details` 之后的属性串（如 ' open'），写回时原样输出。
      openAttrs: { default: '', rendered: false },
    };
  },

  // 不认 `details` 标签，避免非规范写法走 generateJSON 时被部分识别。
  parseHTML() {
    return [{ tag: 'div[data-ygg-details]' }];
  },

  // 编辑器内常显正文，不用原生 <details>，以免点击摘要折叠。
  renderHTML({ HTMLAttributes }) {
    return [
      'div',
      mergeAttributes(HTMLAttributes, { 'data-ygg-details': '', class: 'tiptap-details' }),
      0,
    ];
  },

  markdownTokenName: 'details',
  markdownTokenizer: {
    name: 'details',
    level: 'block',
    start: (src: string) => {
      const m = DETAILS_START_REGEX.exec(src);
      return m ? m.index + m[1].length : -1;
    },
    tokenize: (src, _tokens, lexer) => {
      const lines = src.split('\n');
      const head = DETAILS_OPEN_REGEX.exec(lines[0]);
      const summary = lines[1] === undefined ? null : SUMMARY_REGEX.exec(lines[1]);
      if (!head || !summary || lines[2]?.trim() !== '') return undefined;

      // 从第 3 行起数嵌套深度，归零处为闭合行。
      let depth = 1;
      let end = -1;
      for (let i = 3; i < lines.length; i += 1) {
        if (DETAILS_OPEN_REGEX.test(lines[i])) depth += 1;
        else if (DETAILS_CLOSE_REGEX.test(lines[i])) depth -= 1;
        if (depth === 0) {
          end = i;
          break;
        }
      }
      if (end === -1 || lines[end - 1].trim() !== '') return undefined;

      const body = lines.slice(3, end).join('\n');
      if (body.trim() === '') return undefined;
      return {
        type: 'details',
        raw: lines.slice(0, end + 1).join('\n') + (end + 1 < lines.length ? '\n' : ''),
        openAttrs: head[1] ?? '',
        summary: summary[1],
        tokens: lexer.blockTokens(body),
      };
    },
  },
  parseMarkdown: (token, h) => {
    const body = (h.parseBlockChildren ?? h.parseChildren)(token.tokens ?? []);
    return h.createNode('details', { openAttrs: token.openAttrs ?? '' }, [
      h.createNode(
        'detailsSummary',
        {},
        token.summary ? [{ type: 'text', text: token.summary }] : [],
      ),
      ...(body.length ? body : [{ type: 'paragraph' }]),
    ]);
  },
  renderMarkdown: (node, h) => {
    const [summary, ...body] = node.content ?? [];
    const summaryText = (summary?.content ?? []).map((c) => c.text ?? '').join('');
    const bodyText = body
      .map((child, i) => h.renderChild?.(child, i + 1) ?? h.renderChildren([child]))
      .join('\n\n');
    return `<details${node.attrs?.openAttrs ?? ''}>\n<summary>${summaryText}</summary>\n\n${bodyText}\n\n</details>`;
  },
});

export const DetailsSummary = Node.create({
  name: 'detailsSummary',
  content: 'text*',
  marks: '',
  defining: true,

  parseHTML() {
    return [{ tag: 'div[data-ygg-details-summary]' }];
  },

  renderHTML() {
    return ['div', { 'data-ygg-details-summary': '', class: 'tiptap-details-summary' }, 0];
  },

  renderMarkdown: (node) => (node.content ?? []).map((c) => c.text ?? '').join(''),

  addKeyboardShortcuts() {
    // 摘要单行：回车吞掉。
    return {
      Enter: ({ editor }) => editor.state.selection.$from.parent.type.name === this.name,
    };
  },
});
