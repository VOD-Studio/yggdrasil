import { common, createLowlight } from 'lowlight';
import { extractLang } from './highlight';

const base = createLowlight(common);

// Vue SFC 别名:lowlight 依赖的 highlight.js 11 早已移除 vue grammar,
// common/all 均不含 vue。这里把 vue 注册为 xml 的别名,使 ```vue 代码块
// 在编辑器写作时按 XML/HTML 着色(template 段正常,script/style 段不精确
// 但不抛 Unknown language)。阅读侧(读者看到的)走 syntect 自包含 Vue
// 语法(syntaxes/Vue.sublime-syntax),三段都有完整高亮,两侧独立互不影响。
base.registerAlias({ xml: ['vue'] });

/**
 * lowlight 实例，包装 highlight 方法以处理完整 info string。
 *
 * CodeBlockLowlight 取 `block.node.attrs.language`（如 `python runnable {...}`）
 * 直接调 `lowlight.highlight(language, code)`，但 lowlight 只认 `python` 这个 token。
 * wrapper 先 extractLang 提取首 token，再高亮，使 runnable 块也能按正确语言着色。
 *
 * 普通 code block（language='python'）也兼容：extractLang('python') → 'python'。
 */
export const lowlight = {
  ...base,
  highlight(language: string, value: string) {
    return base.highlight(extractLang(language), value);
  },
};

type HighlightNode = ReturnType<typeof lowlight.highlight>['children'][number];

interface HighlightSpan {
  from: number;
  to: number;
  className: string;
}

export function highlightCode(language: string, text: string): HighlightSpan[] {
  const result = base.registered(extractLang(language))
    ? lowlight.highlight(language, text)
    : lowlight.highlightAuto(text);
  const spans: HighlightSpan[] = [];
  let offset = 0;
  function collect(nodes: HighlightNode[], parentClass = '') {
    for (const node of nodes) {
      if (node.type === 'element') {
        const classes = node.properties.className;
        const className = Array.isArray(classes) ? classes.join(' ') : '';
        collect(node.children, [parentClass, className].filter(Boolean).join(' '));
      } else if (node.type === 'text') {
        const end = offset + node.value.length;
        if (parentClass && end > offset)
          spans.push({ from: offset, to: end, className: parentClass });
        offset = end;
      }
    }
  }
  collect(result.children);
  return spans;
}
