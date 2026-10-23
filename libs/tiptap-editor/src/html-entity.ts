import { mergeAttributes, Node } from '@tiptap/core';

/**
 * HTML 实体原子节点（`&copy;` `&amp;` `&#169;` …）。
 *
 * @tiptap/markdown 解析时会把实体解码成普通字符，序列化再按 `&`→`&amp;` 编码，
 * 源码里的写法（`&copy;`）就丢了。这里把合法实体建模成原子节点：Markdown 源码逐字保留，
 * 编辑器里显示解码后的字符，悬停看原文。
 */

let decoder: HTMLTextAreaElement | null = null;

/** 浏览器解码实体（惰性创建 textarea）；不认识的实体原样返回。 */
function decodeEntity(raw: string): string {
  decoder ??= document.createElement('textarea');
  decoder.innerHTML = raw;
  return decoder.value;
}

const ENTITY_START = /&(?:#[0-9]|#[xX][0-9a-fA-F]|[A-Za-z])/;
const ENTITY_REGEX = /^&(?:#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[A-Za-z][A-Za-z0-9]{1,31});/;

export const HtmlEntity = Node.create({
  name: 'htmlEntity',
  inline: true,
  group: 'inline',
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      raw: {
        default: '',
        parseHTML: (el: HTMLElement) => el.getAttribute('data-entity') ?? '',
        renderHTML: () => ({}),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-entity]' }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const raw = String(node.attrs.raw ?? '');
    return [
      'span',
      mergeAttributes(HTMLAttributes, {
        class: 'tiptap-entity',
        'data-entity': raw,
        title: raw,
      }),
      decodeEntity(raw),
    ];
  },

  markdownTokenName: 'htmlEntity',
  markdownTokenizer: {
    name: 'htmlEntity',
    level: 'inline',
    start: (src: string) => src.search(ENTITY_START),
    tokenize: (src: string) => {
      const m = ENTITY_REGEX.exec(src);
      // 浏览器认得（解码结果与原文不同）才算实体；`&foo;` 仍是字面文本，
      // 序列化为 `&amp;foo;`，与服务端渲染一致。
      if (!m || decodeEntity(m[0]) === m[0]) return undefined;
      return { type: 'htmlEntity', raw: m[0] };
    },
  },
  parseMarkdown: (token, h) => h.createNode('htmlEntity', { raw: token.raw }),
  renderMarkdown: (node) => node.attrs?.raw ?? '',
});
