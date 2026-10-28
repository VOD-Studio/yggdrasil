/**
 * 代码块复制按钮。
 *
 * 为 pre>code 注入一个 .copy-code 图标按钮，点击复制代码文本；成功后换成对勾、
 * 读屏名称改为「已复制」，2 秒后还原。样式见 styles/components/article.css。
 */

const COPY_ICON =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/></svg>';
const CHECK_ICON =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>';

function setCopied(btn: HTMLButtonElement, copied: boolean): void {
  btn.innerHTML = copied ? CHECK_ICON : COPY_ICON;
  btn.setAttribute('aria-label', copied ? '已复制' : '复制代码');
  btn.title = copied ? '已复制' : '复制';
  if (copied) btn.dataset.copied = 'true';
  else delete btn.dataset.copied;
}

function initCopyButtons(root: Element): void {
  const blocks = root.querySelectorAll('pre > code');
  blocks.forEach((code) => {
    const pre = code.parentElement;
    if (!pre) return;
    if (pre.querySelector('.copy-code')) return;
    // mermaid 代码块由 mermaid.ts 渲染成 SVG，不注入 copy 按钮（图无需复制源码）。
    if (code.classList.contains('language-mermaid')) return;

    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'copy-code';
    setCopied(btn, false);

    const codeText = code.textContent || '';
    btn.addEventListener('click', (event) => {
      // 阻止冒泡：复制是叶子动作。可运行块的静态展示容器在 Rust 侧监听
      // click 以激活编辑器，复制不应连带触发 CodeMirror 下载（复制 ≠ 编辑意图）。
      event.stopPropagation();
      if (navigator.clipboard?.writeText) {
        navigator.clipboard.writeText(codeText);
      } else {
        const ta = document.createElement('textarea');
        ta.value = codeText;
        ta.style.position = 'fixed';
        ta.style.opacity = '0';
        document.body.appendChild(ta);
        ta.select();
        document.execCommand('copy');
        document.body.removeChild(ta);
      }
      setCopied(btn, true);
      setTimeout(() => setCopied(btn, false), 2000);
    });

    pre.appendChild(btn);
  });
}

export function initPostContent(selector: string): void {
  const root = document.querySelector(selector);
  if (!root) return;
  initCopyButtons(root);
}
