import { Mark, mergeAttributes, Node } from '@tiptap/core';

/**
 * 行内 HTML 与下标/上标的 Markdown 往返。
 *
 * 服务端 pulldown-cmark（`Options::all()`）把 `~x~` / `^x^` 渲染成 `<sub>` / `<sup>`，
 * 并把行内原始 HTML 原样放行（经 sanitizer 白名单）。@tiptap/markdown 对 `html` token
 * 硬编码走 generateJSON，会剥掉 schema 不认识的标签，所以这里注册 inline tokenizer
 * 抢在 marked 内置 `tag` 之前处理：
 *
 * - `<kbd> <mark> <abbr> <sub> <sup>` 成对标签 → 对应 Mark（记住原文开标签，写回时原样输出）；
 * - `~x~` / `^x^` → Subscript / Superscript Mark（`syntax: 'delimiter'`，写回仍是定界符写法）；
 * - 其余行内原始 HTML（未闭合标签、`<time …>`、注释等）→ RawInlineHtml 原子节点，源码逐字保留。
 *
 * 定界符开闭规则与 pulldown-cmark 一致（见 `matchDelimited`）：词内 `H~2~O`、`x^2^` 不成立，
 * 保持字面量；序列化侧的转义由 @tiptap/markdown 补丁（escapeMarkdownSyntax）保证往返稳定。
 */

const PUNCT = /[\p{P}\p{S}]/u;
const WS = /\s/u;

/** HTML 开标签的属性串（CommonMark 原始 HTML 的 attribute 语法）。 */
const ATTRS =
  '(?:\\s+[A-Za-z_:][A-Za-z0-9_.:-]*(?:\\s*=\\s*(?:[^\\s"\'=<>`]+|\'[^\']*\'|"[^"]*"))?)*';

/** 行内可识别为格式的成对标签。 */
const PAIRED_TAGS = ['kbd', 'mark', 'abbr', 'sub', 'sup'] as const;

/** CommonMark 行内原始 HTML：开标签、闭标签、注释、处理指令、声明、CDATA。 */
const RAW_HTML_REGEX = new RegExp(
  `^(?:<[A-Za-z][A-Za-z0-9-]*${ATTRS}\\s*/?>|</[A-Za-z][A-Za-z0-9-]*\\s*>|<!-->|<!--->|<!--[\\s\\S]*?-->|<\\?[\\s\\S]*?\\?>|<![A-Za-z][^>]*>|<!\\[CDATA\\[[\\s\\S]*?\\]\\]>)`,
);

const TITLE_REGEX = /\btitle\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+))/i;

/** 已产出 token 的最后一个码点；没有则返回 ''。 */
function prevChar(tokens: readonly { raw?: string }[]): string {
  const raw = tokens[tokens.length - 1]?.raw;
  if (!raw) return '';
  const chars = Array.from(raw);
  return chars[chars.length - 1];
}

interface DelimitedMatch {
  raw: string;
  inner: string;
}

/**
 * 匹配以 `d` 开头的单字符定界符 run（`~x~` / `^x^`）。
 * 能开 = 后一字符非空白，且前一字符不存在/空白/标点；
 * 能闭 = 前一字符非空白，且后一字符不存在/空白/标点；开闭 run 长度都必须是 1。
 */
function matchDelimited(src: string, d: '~' | '^', prev: string): DelimitedMatch | null {
  const chars = Array.from(src);
  if (chars[0] !== d || chars[1] === d) return null;
  const first = chars[1];
  if (first === undefined || WS.test(first)) return null;
  if (prev !== '' && !WS.test(prev) && !PUNCT.test(prev)) return null;

  let i = 1;
  while (i < chars.length) {
    const c = chars[i];
    if (c === '\\') {
      i += 2;
      continue;
    }
    if (c === '`') {
      // 行内代码内的定界符不参与匹配：找同长度的闭合 run，找不到则只跳过开头 run。
      let n = 1;
      while (chars[i + n] === '`') n += 1;
      let k = i + n;
      let end = -1;
      while (k < chars.length) {
        if (chars[k] !== '`') {
          k += 1;
          continue;
        }
        let m = 1;
        while (chars[k + m] === '`') m += 1;
        if (m === n) {
          end = k + m;
          break;
        }
        k += m;
      }
      i = end === -1 ? i + n : end;
      continue;
    }
    if (c === d) {
      let n = 1;
      while (chars[i + n] === d) n += 1;
      const before = chars[i - 1];
      const after = chars[i + n];
      const closes =
        n === 1 &&
        i > 1 &&
        !WS.test(before) &&
        (after === undefined || WS.test(after) || PUNCT.test(after));
      if (closes) {
        return { raw: chars.slice(0, i + 1).join(''), inner: chars.slice(1, i).join('') };
      }
      i += n;
      continue;
    }
    i += 1;
  }
  return null;
}

interface PairedTagMatch {
  tag: string;
  openTag: string;
  inner: string;
  raw: string;
}

/** 匹配以成对标签开头的片段：`<tag attrs>inner</tag>`；嵌套同名标签、无闭合、空内容都不算。 */
function matchPairedTag(src: string, tags: readonly string[]): PairedTagMatch | null {
  if (src[0] !== '<') return null;
  const open = new RegExp(`^<(${tags.join('|')})(${ATTRS})\\s*>`, 'i').exec(src);
  if (!open) return null;
  const tag = open[1].toLowerCase();
  const rest = src.slice(open[0].length);
  const close = new RegExp(`</${tag}\\s*>`, 'i').exec(rest);
  if (!close) return null;
  const inner = rest.slice(0, close.index);
  if (inner === '' || new RegExp(`<${tag}[\\s>]`, 'i').test(inner)) return null;
  return {
    tag,
    openTag: open[0],
    inner,
    raw: open[0] + inner + close[0],
  };
}

function escapeAttr(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/"/g, '&quot;');
}

interface HtmlTagMarkOptions {
  name: string;
  tag: (typeof PAIRED_TAGS)[number];
  /** 等价的 Markdown 定界符（仅下标/上标）。 */
  delimiter?: '~' | '^';
}

function createHtmlTagMark({ name, tag, delimiter }: HtmlTagMarkOptions) {
  const isAbbr = tag === 'abbr';
  const tagStart = new RegExp(`<${tag}[\\s>]`, 'i');

  return Mark.create({
    name,

    addAttributes() {
      return {
        // 'html' = 原文是成对标签；'delimiter' = 原文是 ~x~ / ^x^，写回保持定界符写法。
        syntax: { default: 'html', rendered: false },
        // 原文开标签（含属性），写回时原样输出；新建的 mark 为 null，输出 `<tag>`。
        openTag: { default: null, rendered: false },
        ...(isAbbr
          ? {
              title: {
                default: null,
                parseHTML: (el: HTMLElement) => el.getAttribute('title'),
              },
            }
          : {}),
      };
    },

    parseHTML() {
      return [{ tag }];
    },

    renderHTML({ HTMLAttributes }) {
      return [tag, mergeAttributes(HTMLAttributes), 0];
    },

    markdownTokenName: name,
    markdownTokenizer: {
      name,
      level: 'inline',
      start: (src: string) => {
        const candidates = [delimiter ? src.indexOf(delimiter) : -1, src.search(tagStart)].filter(
          (n) => n >= 0,
        );
        return candidates.length > 0 ? Math.min(...candidates) : -1;
      },
      tokenize: (src, tokens, lexer) => {
        if (src[0] === '<') {
          const m = matchPairedTag(src, [tag]);
          if (!m) return undefined;
          const title = isAbbr ? TITLE_REGEX.exec(m.openTag) : null;
          return {
            type: name,
            raw: m.raw,
            syntax: 'html',
            openTag: m.openTag,
            title: title ? (title[1] ?? title[2] ?? title[3] ?? null) : null,
            tokens: lexer.inlineTokens(m.inner),
          };
        }
        if (delimiter && src[0] === delimiter) {
          const m = matchDelimited(src, delimiter, prevChar(tokens));
          if (!m) return undefined;
          return {
            type: name,
            raw: m.raw,
            syntax: 'delimiter',
            tokens: lexer.inlineTokens(m.inner),
          };
        }
        return undefined;
      },
    },
    parseMarkdown: (token, h) =>
      h.applyMark(name, h.parseInline(token.tokens ?? []), {
        syntax: token.syntax,
        openTag: token.openTag ?? null,
        ...(isAbbr ? { title: token.title ?? null } : {}),
      }),
    renderMarkdown: (node, h) => {
      const attrs = node.attrs ?? {};
      if (delimiter && attrs.syntax === 'delimiter') {
        return `${delimiter}${h.renderChildren(node)}${delimiter}`;
      }
      const openTag: string =
        attrs.openTag ??
        (isAbbr && attrs.title ? `<abbr title="${escapeAttr(String(attrs.title))}">` : `<${tag}>`);
      return `${openTag}${h.renderChildren(node)}</${tag}>`;
    },
  });
}

export const Subscript = createHtmlTagMark({ name: 'subscript', tag: 'sub', delimiter: '~' });
export const Superscript = createHtmlTagMark({ name: 'superscript', tag: 'sup', delimiter: '^' });
export const Kbd = createHtmlTagMark({ name: 'kbd', tag: 'kbd' });
/** 名字避开 tiptap Highlight（同样渲染 `<mark>`）。 */
export const HtmlMark = createHtmlTagMark({ name: 'htmlMark', tag: 'mark' });
export const Abbr = createHtmlTagMark({ name: 'abbr', tag: 'abbr' });

/**
 * 行内原始 HTML 原子节点：成对格式标签之外的所有行内 HTML（未闭合的 `<abbr title="…">`、
 * `<time …>`、注释等）。编辑器里显示为等宽字面量，Markdown 源码逐字保留。
 * 自动链接 `<https://…>` / `<mailto:…>` 不匹配 RAW_HTML_REGEX，仍走 marked autolink。
 */
export const RawInlineHtml = Node.create({
  name: 'rawInlineHtml',
  inline: true,
  group: 'inline',
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      html: {
        default: '',
        parseHTML: (el: HTMLElement) => el.getAttribute('data-raw-html') ?? '',
        renderHTML: () => ({}),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-raw-html]' }];
  },

  renderHTML({ node, HTMLAttributes }) {
    return [
      'span',
      mergeAttributes(HTMLAttributes, {
        class: 'tiptap-raw-html',
        'data-raw-html': node.attrs.html,
      }),
      node.attrs.html,
    ];
  },

  markdownTokenName: 'rawInlineHtml',
  markdownTokenizer: {
    name: 'rawInlineHtml',
    level: 'inline',
    start: (src: string) => src.indexOf('<'),
    tokenize: (src: string) => {
      // 成对格式标签交给对应 Mark（与扩展注册顺序无关）。
      if (matchPairedTag(src, PAIRED_TAGS)) return undefined;
      const m = RAW_HTML_REGEX.exec(src);
      if (!m) return undefined;
      return { type: 'rawInlineHtml', raw: m[0], html: m[0] };
    },
  },
  parseMarkdown: (token, h) => h.createNode('rawInlineHtml', { html: token.html }),
  renderMarkdown: (node) => node.attrs?.html ?? '',
});
