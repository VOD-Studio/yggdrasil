import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';

/**
 * 修复 CodeBlock 内 Backspace 在 lowlight decoration 重建后失效的问题。
 *
 * 背景：CodeBlockLowlight 的 lowlight 插件在 codeBlock 内容变化后重新高亮，重建
 * `<code>` 内的 text node（包成 `<span class="hljs-...">`）。重建过程中 DOM selection
 * 会错位到 `<pre>`（contentDOM 的父元素）而非 `<code>` 内文本节点，导致后续 Backspace
 * 无效——删不掉剩余字符，块删空后也无法删除整个 codeBlock。
 *
 * 文档层 ProseMirror 的 selection（$anchor.parent / parentOffset）始终正确，只是 DOM
 * 层与文档层不同步。这里用一个高优先级 keymap 插件接管 codeBlock 内的 Backspace：
 * 当光标在 codeBlock 内（文档层判断）时，直接基于文档 pos 删除前一个字符，或在块空时
 * clearNodes 转段落，绕过 DOM selection 错位。
 *
 * 必须用独立 Extension + addProseMirrorPlugins（而非在 CodeBlockLowlight.extend 里
 * addKeyboardShortcuts）：后者无法覆盖父类已注册的同名 keymap（Tiptap 优先级限制）。
 */
const pluginKey = new PluginKey('codeBlockBackspaceFix');

// tsconfig 的 lib 不含 Intl.Segmenter 类型，按最小形状声明；运行时缺失则走码点回退。
interface GraphemeSegmenter {
  segment(text: string): Iterable<{ segment: string }>;
}
interface SegmenterIntl {
  Segmenter?: new (locale?: string, opts?: { granularity: 'grapheme' }) => GraphemeSegmenter;
}
const intl: SegmenterIntl = Intl as unknown as SegmenterIntl;
const graphemes = intl.Segmenter
  ? new intl.Segmenter(undefined, { granularity: 'grapheme' })
  : null;

/** 文本末尾最后一个字形簇的 UTF-16 长度；无 Segmenter 时退化为按码点（代理对算一个）。 */
function lastGraphemeLength(text: string): number {
  if (graphemes) {
    let last = 0;
    for (const { segment } of graphemes.segment(text)) last = segment.length;
    return last || 1;
  }
  const n = text.length;
  if (n >= 2) {
    const lo = text.charCodeAt(n - 1);
    const hi = text.charCodeAt(n - 2);
    if (lo >= 0xdc00 && lo <= 0xdfff && hi >= 0xd800 && hi <= 0xdbff) return 2;
  }
  return 1;
}

export const CodeBlockBackspaceFix = Extension.create({
  name: 'codeBlockBackspaceFix',

  addProseMirrorPlugins() {
    const codeBlockName = 'codeBlock';
    return [
      // 高优先级（数字越小越先），抢在父类 CodeBlock keymap 之前
      new Plugin({
        key: pluginKey,
        props: {
          handleKeyDown(view, event) {
            if (event.key !== 'Backspace') return false;
            const { empty, $anchor } = view.state.selection;
            if (!empty) return false;
            // 仅处理光标在 codeBlock 内的情况
            if ($anchor.parent.type.name !== codeBlockName) return false;

            // 在块起始位置（parentOffset 0）
            if ($anchor.parentOffset === 0) {
              // 块为空 → 转段落（等价 clearNodes）
              if ($anchor.parent.textContent.length === 0) {
                const tr = view.state.tr;
                const defaultType = view.state.schema.nodes.paragraph;
                tr.setNodeMarkup($anchor.before(), defaultType);
                view.dispatch(tr);
                return true;
              }
              return false; // 块非空但在起始，不处理（让光标留在块内）
            }
            // 删除光标前一个字符（基于文档 pos，不依赖 DOM selection）。
            // 按 UTF-16 码元 -1 会把 emoji 等代理对切成孤立代理，须按字形簇回退。
            const before = $anchor.parent.textBetween(0, $anchor.parentOffset);
            const tr = view.state.tr;
            tr.delete($anchor.pos - lastGraphemeLength(before), $anchor.pos);
            view.dispatch(tr);
            return true;
          },
        },
      }),
    ];
  },
});
