const PLACEHOLDER = '搜索条目、类型或函数（S、/ 或 ⌘K）';

const TABS = {
  'In Names': '名称',
  'In Parameters': '参数',
  'In Return Types': '返回类型',
};

const TYPENAMES = {
  keyword: '关键字',
  'primitive type': '原生类型',
  module: '模块',
  'extern crate': '外部 crate',
  're-export': '重导出',
  struct: '结构体',
  enum: '枚举',
  function: '函数',
  'type alias': '类型别名',
  static: '静态变量',
  trait: 'Trait',
  'trait method': 'Trait 方法',
  method: '方法',
  'struct field': '字段',
  'enum variant': '变体',
  macro: '宏',
  'assoc type': '关联类型',
  constant: '常量',
  'assoc const': '关联常量',
  union: '联合体',
  'foreign type': '外部类型',
  'existential type': '存在类型',
  'attribute macro': '属性宏',
  'derive macro': '派生宏',
  'trait alias': 'Trait 别名',
  attribute: '属性',
};

function firstText(el) {
  for (const n of el.childNodes) if (n.nodeType === Node.TEXT_NODE) return n;
  return null;
}

/** 第一个文本节点按表替换，保留前后空白；文字相同则不写，避免触发 observer。 */
function replaceFirstText(el, table) {
  const node = firstText(el);
  if (!node) return;
  const key = node.nodeValue.trim();
  const next = table[key];
  if (next === undefined) return;
  const value = node.nodeValue.replace(key, next);
  if (node.nodeValue !== value) node.nodeValue = value;
}

function localizeSearch(scope) {
  for (const b of scope.querySelectorAll('#search-tabs > button')) replaceFirstText(b, TABS);
  for (const t of scope.querySelectorAll('.typename')) {
    const key = t.textContent.trim();
    const next = TYPENAMES[key];
    if (next !== undefined && t.textContent !== next) t.textContent = next;
  }
  for (const s of scope.querySelectorAll('.search-switcher')) {
    replaceFirstText(s, { 'Search results': '搜索范围' });
  }
}

function setPlaceholder() {
  const input = document.querySelector('.search-input');
  if (input && input.getAttribute('placeholder') !== PLACEHOLDER) {
    input.setAttribute('placeholder', PLACEHOLDER);
  }
}

export function initSearch() {
  // rustdoc 打开搜索时会整块替换 #alternative-display 的内容（甚至节点本身），
  // 所以观察 body。回调在微任务里同步改写（先于绘制，不闪英文）；改写只在文字不同时
  // 才发生，并用 takeRecords 丢掉自己产生的记录，避免自我触发。
  const run = () => {
    setPlaceholder();
    localizeSearch(document);
  };
  run();
  const observer = new MutationObserver(() => {
    run();
    observer.takeRecords();
  });
  observer.observe(document.body, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ['placeholder'],
  });
}
