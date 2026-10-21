/**
 * 代码块 copy 按钮。
 *
 * 为 pre>code 注入一个 .copy-code 按钮,点击复制代码文本并回显 copied!。
 * 2 秒后还原文案。样式见 input.css 的 .copy-code 规则(全局 Tailwind 构建)。
 */

function initCopyButtons(root: Element): void {
  const blocks = root.querySelectorAll('pre > code');
  blocks.forEach((code) => {
    const pre = code.parentElement;
    if (!pre) return;
    if (pre.querySelector('.copy-code')) return;
    // mermaid 代码块由 mermaid.ts 渲染成 SVG，不注入 copy 按钮（图无需复制源码）。
    if (code.classList.contains('language-mermaid')) return;

    const btn = document.createElement('button');
    btn.className = 'copy-code';
    btn.textContent = 'copy';
    btn.setAttribute('aria-label', 'Copy code');

    const codeText = code.textContent || '';
    btn.addEventListener('click', (event) => {
      // 阻止冒泡：复制是叶子动作。可运行块的静态展示容器在 Dioxus 侧监听
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
      btn.textContent = 'copied!';
      setTimeout(() => {
        btn.textContent = 'copy';
      }, 2000);
    });

    pre.appendChild(btn);
  });
}

export function initPostContent(selector: string): void {
  const root = document.querySelector(selector);
  if (!root) return;
  initCopyButtons(root);
}
