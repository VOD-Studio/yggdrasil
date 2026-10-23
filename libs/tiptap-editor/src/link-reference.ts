import { mergeAttributes, Node } from '@tiptap/core';
import { Link } from '@tiptap/extension-link';

/**
 * 参考式链接 `[text][label]` / `[label][]` / `[label]` 与链接定义行 `[label]: url "title"` 的往返。
 *
 * marked 解析参考式链接时直接解引用成普通 link token，写回就变成 `[text](url)`，定义行也会消失。
 * 这里：
 * - ReferenceLink 在 link mark 上记住标签与写法（refLabel / refForm / refHref），
 *   链接地址没被改过时按原写法输出；
 * - LinkDefinition 把定义行建模成原子块节点，原文逐字保留（marked 对 `def` token 不产出内容，
 *   引用解析不受影响）。
 */

type RefForm = 'full' | 'collapsed' | 'shortcut';

interface ReferenceInfo {
  refLabel: string | null;
  refForm: RefForm | null;
  refHref: string | null;
}

const NO_REFERENCE: ReferenceInfo = { refLabel: null, refForm: null, refHref: null };

/** 从 link token 的 raw 推断是否参考式写法；内联链接与自动链接返回全 null。 */
function parseReference(raw: string, href: string): ReferenceInfo {
  if (!raw.startsWith('[') || !raw.endsWith(']')) return NO_REFERENCE;
  const m = /\[((?:\\.|[^\\[\]])*)\]$/.exec(raw);
  if (!m) return NO_REFERENCE;
  if (m.index === 0) return { refLabel: m[1], refForm: 'shortcut', refHref: href };
  if (m[1] === '') {
    return { refLabel: raw.slice(1, m.index - 1), refForm: 'collapsed', refHref: href };
  }
  return { refLabel: m[1], refForm: 'full', refHref: href };
}

export const ReferenceLink = Link.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      refLabel: { default: null, rendered: false },
      refForm: { default: null, rendered: false },
      refHref: { default: null, rendered: false },
    };
  },

  parseMarkdown: (token, h) =>
    h.applyMark('link', h.parseInline(token.tokens || []), {
      href: token.href,
      title: token.title || null,
      ...parseReference(String(token.raw ?? ''), String(token.href ?? '')),
    }),

  renderMarkdown: (node, h, ctx) => {
    const attrs = node.attrs ?? {};
    const href: string = attrs.href ?? '';
    const title: string = attrs.title ?? '';
    const text = h.renderChildren(node);
    if (typeof attrs.refLabel === 'string' && href === attrs.refHref) {
      // collapsed / shortcut 只在链接文字仍等于标签时成立，否则退回完整写法。
      if (attrs.refForm === 'full' || ctx?.meta?.markText !== attrs.refLabel) {
        return `[${text}][${attrs.refLabel}]`;
      }
      return attrs.refForm === 'collapsed' ? `[${text}][]` : `[${text}]`;
    }
    return title ? `[${text}](${href} "${title}")` : `[${text}](${href})`;
  },
});

export const LinkDefinition = Node.create({
  name: 'linkDefinition',
  group: 'block',
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      raw: {
        default: '',
        parseHTML: (el: HTMLElement) => el.getAttribute('data-link-def') ?? '',
        renderHTML: () => ({}),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-link-def]' }];
  },

  renderHTML({ node, HTMLAttributes }) {
    return [
      'div',
      mergeAttributes(HTMLAttributes, {
        class: 'tiptap-link-def',
        'data-link-def': node.attrs.raw,
      }),
      node.attrs.raw,
    ];
  },

  markdownTokenName: 'def',
  parseMarkdown: (token, h) =>
    h.createNode('linkDefinition', { raw: String(token.raw ?? '').replace(/\s+$/, '') }),
  renderMarkdown: (node) => node.attrs?.raw ?? '',
});
