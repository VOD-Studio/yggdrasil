import { Extension, findChildren } from '@tiptap/core';
import type { Node } from '@tiptap/pm/model';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view';
import { codeHighlighter } from './render-assets';

const highlightKey = new PluginKey<DecorationSet>('yggdrasilCodeHighlight');

/** 同步保留代码块的编辑与序列化，出现代码块后再装配高亮装饰。 */
export const CodeBlockHighlight = Extension.create({
  name: 'codeBlockHighlight',
  addProseMirrorPlugins() {
    const blocks = (doc: Node) => findChildren(doc, (node) => node.type.name === 'codeBlock');
    const cache = new WeakMap<
      Node,
      ReturnType<typeof import('./code-highlighter').highlightCode>
    >();

    const decorations = (doc: Node) => {
      const renderer = codeHighlighter.current;
      if (!renderer) return DecorationSet.empty;
      const marks = blocks(doc).flatMap(({ node, pos }) => {
        let spans = cache.get(node);
        if (!spans) {
          spans = renderer.highlightCode(node.attrs.language ?? '', node.textContent);
          cache.set(node, spans);
        }
        return spans.map(({ from, to, className }) =>
          Decoration.inline(pos + 1 + from, pos + 1 + to, { class: className }),
        );
      });
      return DecorationSet.create(doc, marks);
    };

    let loading = false;
    const prepare = (view: EditorView) => {
      if (loading || codeHighlighter.current || !blocks(view.state.doc).length) return;
      loading = true;
      void codeHighlighter
        .load()
        .then(() => {
          if (!view.isDestroyed) view.dispatch(view.state.tr.setMeta(highlightKey, true));
        })
        .catch((error) => {
          console.error('编辑器语法高亮加载失败', error);
        })
        .finally(() => {
          loading = false;
        });
    };

    return [
      new Plugin({
        key: highlightKey,
        state: {
          init: (_, state) => decorations(state.doc),
          apply(tr, previous) {
            return tr.docChanged || tr.getMeta(highlightKey) ? decorations(tr.doc) : previous;
          },
        },
        props: { decorations: (state) => highlightKey.getState(state) },
        view(view) {
          prepare(view);
          return {
            update(view, previous) {
              if (view.state.doc !== previous.doc) prepare(view);
            },
          };
        },
      }),
    ];
  },
});
