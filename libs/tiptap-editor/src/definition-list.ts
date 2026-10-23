import { mergeAttributes, Node } from '@tiptap/core';

/**
 * 定义列表节点：`术语\n: 定义`（pulldown-cmark ENABLE_DEFINITION_LIST 语法）。
 *
 * 结构：definitionList > (definitionTerm definitionDescription+)+，对应 dl/dt/dd。
 * 不加输入规则和快捷键：ProseMirror 默认 splitBlock 在 dt 末尾回车生成 dd、
 * 在 dd 中回车追加 dd，均是合法结构。
 *
 * markdown tokenizer 必须显式提供 `start`（见 gfm-alert.ts 注释）。start 命中点只会落在
 * 块起点（文首或空行之后），所以不会切断段落：段落后面紧跟「术语 + 定义」时，
 * 前台把它们并进同一段，这里保持为普通段落。
 */

const DEF_LINE = /^ {0,3}:[ \t]+(.*)$/;
const NOT_TERM =
  /^ {0,3}(?:#{1,6}(?:\s|$)|>|[-*+](?:\s|$)|\d{1,9}[.)](?:\s|$)|`{3}|~{3}|\||<|\[\^|\$\$|(?:[-*_][ \t]*){3,}$)/;
const START_REGEX = /(^|\n[ \t]*\n)[^\n]*\S[^\n]*\n {0,3}:[ \t]/;

function isTerm(line: string | undefined, next: string | undefined): line is string {
  return (
    line !== undefined &&
    line.trim() !== '' &&
    !DEF_LINE.test(line) &&
    !NOT_TERM.test(line) &&
    next !== undefined &&
    DEF_LINE.test(next)
  );
}

export const DefinitionTerm = Node.create({
  name: 'definitionTerm',
  content: 'inline*',
  defining: true,
  parseHTML: () => [{ tag: 'dt' }],
  renderHTML: ({ HTMLAttributes }) => ['dt', mergeAttributes(HTMLAttributes), 0],
});

export const DefinitionDescription = Node.create({
  name: 'definitionDescription',
  content: 'inline*',
  defining: true,
  parseHTML: () => [{ tag: 'dd' }],
  renderHTML: ({ HTMLAttributes }) => ['dd', mergeAttributes(HTMLAttributes), 0],
});

export const DefinitionList = Node.create({
  name: 'definitionList',
  group: 'block',
  content: '(definitionTerm definitionDescription+)+',
  parseHTML: () => [{ tag: 'dl' }],
  renderHTML: ({ HTMLAttributes }) => ['dl', mergeAttributes(HTMLAttributes), 0],

  markdownTokenName: 'definitionList',
  markdownTokenizer: {
    name: 'definitionList',
    level: 'block',
    start: (src: string) => {
      const m = START_REGEX.exec(src);
      return m ? m.index + m[1].length : -1;
    },
    tokenize: (src, _tokens, lexer) => {
      const lines = src.split('\n');
      const groups: Array<{ term: string; defs: string[] }> = [];
      let i = 0;
      let end = 0;
      while (isTerm(lines[i], lines[i + 1])) {
        const defs: string[] = [];
        let j = i + 1;
        for (let m = DEF_LINE.exec(lines[j] ?? ''); m; m = DEF_LINE.exec(lines[j] ?? '')) {
          defs.push(m[1]);
          j += 1;
        }
        groups.push({ term: lines[i], defs });
        end = j;
        if (lines[end] === '' && isTerm(lines[end + 1], lines[end + 2])) {
          i = end + 1;
        } else {
          break;
        }
      }
      if (groups.length === 0) return undefined;
      const raw = lines.slice(0, end).join('\n') + (end < lines.length ? '\n' : '');
      return {
        type: 'definitionList',
        raw,
        items: groups.map((g) => ({
          term: lexer.inlineTokens(g.term.trim()),
          defs: g.defs.map((d) => lexer.inlineTokens(d)),
        })),
      };
    },
  },
  parseMarkdown: (token, h) => {
    const content: ReturnType<typeof h.createNode>[] = [];
    for (const item of token.items ?? []) {
      content.push(h.createNode('definitionTerm', undefined, h.parseInline(item.term)));
      for (const def of item.defs) {
        content.push(h.createNode('definitionDescription', undefined, h.parseInline(def)));
      }
    }
    return h.createNode('definitionList', undefined, content);
  },
  renderMarkdown: (node, h) => {
    const groups: string[][] = [];
    node.content?.forEach((child) => {
      const text = h.renderChildren(child.content ?? []).replace(/\s*\n\s*/g, ' ');
      if (child.type === 'definitionTerm') {
        groups.push([text]);
      } else {
        groups[groups.length - 1]?.push(`: ${text}`);
      }
    });
    return groups.map((g) => g.join('\n')).join('\n\n');
  },
});
