import type { JSONContent, MarkdownRendererHelpers, RenderContext } from '@tiptap/core';
import { BulletList, ListItem, OrderedList } from '@tiptap/extension-list';

/**
 * 列表的 Markdown 往返：松散度与编号列表内容。
 *
 * - 松散列表（项之间有空行）写回仍是松散；`loose` 记在列表节点上，不影响编辑行为。
 * - 编号列表交给 marked 内置 list（`token.loose`/`token.start` 可读，项内代码块内容正确），
 *   不再用 @tiptap/extension-list 自带的 orderedList tokenizer（它会让代码块多缩进空格，
 *   且支持服务端不认的字母/罗马编号）。
 * - 列表项续行缩进跟随标记宽度（`- ` 两格，`10. ` 四格），首个子节点是代码块时续行同样缩进。
 */

const LOOSE_ATTR = { loose: { default: false, rendered: false } };

function renderList(node: JSONContent, h: MarkdownRendererHelpers): string {
  if (!node.content) return '';
  return h.renderChildren(node.content, node.attrs?.loose ? '\n\n' : '\n');
}

export const LooseBulletList = BulletList.extend({
  addAttributes() {
    return { ...this.parent?.(), ...LOOSE_ATTR };
  },
  parseMarkdown: (token, h) =>
    token.type !== 'list' || token.ordered
      ? []
      : {
          type: 'bulletList',
          attrs: { loose: !!token.loose },
          content: h.parseChildren(token.items ?? []),
        },
  renderMarkdown: (node, h) => renderList(node, h),
});

export const LooseOrderedList = OrderedList.extend({
  addAttributes() {
    return { ...this.parent?.(), ...LOOSE_ATTR };
  },
  // 覆盖父级自带 tokenizer：永不匹配，交给 marked 内置 list 接管。
  // 字段设 undefined 会回落父配置，所以只能用一个永不匹配的 tokenizer 覆盖。
  markdownTokenizer: {
    name: 'orderedList',
    level: 'block',
    start: () => -1,
    tokenize: () => undefined,
  },
  parseMarkdown: (token, h) =>
    token.type !== 'list' || !token.ordered
      ? []
      : {
          type: 'orderedList',
          attrs: { start: typeof token.start === 'number' ? token.start : 1, loose: !!token.loose },
          content: h.parseChildren(token.items ?? []),
        },
  renderMarkdown: (node, h) => renderList(node, h),
});

export const LooseListItem = ListItem.extend({
  renderMarkdown: (node, h, ctx?: RenderContext) => {
    const parentAttrs: Record<string, unknown> = ctx?.meta?.parentAttrs ?? {};
    const prefix =
      ctx?.parentType === 'orderedList'
        ? `${(Number(parentAttrs.start) || 1) + (ctx?.index ?? 0)}. `
        : '- ';
    const pad = ' '.repeat(prefix.length);
    // 非空行加缩进，空行保持空。
    const indentAll = (text: string) =>
      text
        .split('\n')
        .map((line) => (line === '' ? line : pad + line))
        .join('\n');

    const [first, ...rest] = node.content ?? [];
    if (!first) return prefix.trimEnd();
    // 首个子节点：首行跟在标记后，续行缩进。
    const firstText = h.renderChildren([first]).split('\n');
    const head = [
      prefix + firstText[0],
      ...firstText.slice(1).map((l) => (l === '' ? l : pad + l)),
    ].join('\n');

    const parts = [head];
    rest.forEach((child, i) => {
      const text = h.renderChild?.(child, i + 1) ?? h.renderChildren([child]);
      const sep = parentAttrs.loose === true || child.type === 'paragraph' ? '\n\n' : '\n';
      parts.push(sep + indentAll(text));
    });
    return parts.join('');
  },
});
