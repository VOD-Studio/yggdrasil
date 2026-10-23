import { mergeAttributes, Node } from '@tiptap/core';

/**
 * GFM 提示块节点：`> [!NOTE]` / `[!TIP]` / `[!IMPORTANT]` / `[!WARNING]` / `[!CAUTION]`。
 *
 * 前台 pulldown-cmark 输出 `<blockquote class="markdown-alert-<kind>">`，这里的节点
 * 解析/输出同一结构，编辑器与前台共用 article.css 的提示块样式。
 *
 * markdown 往返：自定义 block tokenizer 必须显式提供 `start`，且只认行首——
 * 缺省时 @tiptap/markdown 会合成每次 new Lexer 的 startCb，破坏共享 lexer 的行内队列。
 * 引用块本来就能打断段落，所以在这里切断段落是正确的。
 *
 * 不支持懒续行（续行缺少 `>` 前缀）：这类写法交给服务端往返守卫兜底。
 */

const ALERT_KINDS = ['note', 'tip', 'important', 'warning', 'caution'] as const;
type AlertKind = (typeof ALERT_KINDS)[number];

const ALERT_HEAD_REGEX = /^ {0,3}> ?\[!(note|tip|important|warning|caution)\][ \t]*$/i;
const ALERT_START_REGEX =
  /(^|\n) {0,3}> ?\[!(?:note|tip|important|warning|caution)\][ \t]*(?:\n|$)/i;
const QUOTE_LINE_REGEX = /^ {0,3}>/;
const QUOTE_PREFIX_REGEX = /^ {0,3}> ?/;

function isAlertKind(value: string): value is AlertKind {
  return (ALERT_KINDS as readonly string[]).includes(value);
}

export const GfmAlert = Node.create({
  name: 'gfmAlert',
  group: 'block',
  content: 'block+',
  defining: true,

  addAttributes() {
    return {
      kind: {
        default: 'note',
        parseHTML: (el) => {
          const m = /(?:^|\s)markdown-alert-(\w+)/.exec(el.getAttribute('class') ?? '');
          const kind = m?.[1] ?? '';
          return isAlertKind(kind) ? kind : 'note';
        },
        // class 由节点 renderHTML 统一输出。
        renderHTML: () => ({}),
      },
    };
  },

  parseHTML() {
    // 优先级压过 Blockquote 默认的 50。
    return [{ tag: 'blockquote[class^="markdown-alert-"]', priority: 60 }];
  },

  renderHTML({ node, HTMLAttributes }) {
    return [
      'blockquote',
      mergeAttributes(HTMLAttributes, { class: `markdown-alert-${node.attrs.kind}` }),
      0,
    ];
  },

  markdownTokenName: 'gfmAlert',
  markdownTokenizer: {
    name: 'gfmAlert',
    level: 'block',
    start: (src: string) => {
      const m = ALERT_START_REGEX.exec(src);
      return m ? m.index + m[1].length : -1;
    },
    tokenize: (src, _tokens, lexer) => {
      const lines = src.split('\n');
      const head = ALERT_HEAD_REGEX.exec(lines[0]);
      if (!head) return undefined;
      let end = 1;
      while (end < lines.length && QUOTE_LINE_REGEX.test(lines[end])) end += 1;
      const body = lines
        .slice(1, end)
        .map((line) => line.replace(QUOTE_PREFIX_REGEX, ''))
        .join('\n');
      const raw = lines.slice(0, end).join('\n') + (end < lines.length ? '\n' : '');
      return {
        type: 'gfmAlert',
        raw,
        kind: head[1].toLowerCase(),
        tokens: lexer.blockTokens(body),
      };
    },
  },
  parseMarkdown: (token, h) => {
    const kids = (h.parseBlockChildren ?? h.parseChildren)(token.tokens ?? []);
    return h.createNode(
      'gfmAlert',
      { kind: token.kind },
      kids.length ? kids : [{ type: 'paragraph' }],
    );
  },
  renderMarkdown: (node, h) => {
    const head = `> [!${String(node.attrs?.kind ?? 'note').toUpperCase()}]`;
    const parts: string[] = [];
    node.content?.forEach((child, index) => {
      const rendered = h.renderChild?.(child, index) ?? h.renderChildren([child]);
      parts.push(
        rendered
          .split('\n')
          .map((line) => (line.trim() === '' ? '>' : `> ${line}`))
          .join('\n'),
      );
    });
    return [head, parts.join('\n>\n')].filter(Boolean).join('\n');
  },
});
