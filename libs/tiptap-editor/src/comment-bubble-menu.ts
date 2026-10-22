/**
 * 评论编辑器气泡菜单（comment variant 专属）。
 *
 * 选中文字时浮出的轻量格式条：粗体 / 斜体 / 删除线 / 行内代码 / 链接。
 * 这是评论模式**唯一**的格式 UI——没有常驻工具栏（progressive disclosure，
 * 对齐 GitHub/Linear 评论框的最佳实践）。块级格式（引用/列表/代码块/数学）
 * 依赖 StarterKit 输入规则（`> `、`- `、``` 等）与粘贴，不占 UI。
 *
 * 链接按钮的两态：
 * - 选区不在链接上：菜单内容切换为 URL 输入行（Enter 确认 / Esc 取消）；
 * - 选区在链接上：点击即 unlink（按钮变 link_off 态）。
 *
 * 实现要点：
 * - 用底层 BubbleMenuPlugin（而非 BubbleMenu 扩展）以便在同一文件里完成
 *   按钮 DOM 构建、active 态同步与链接输入行切换；
 * - appendTo document.body + strategy 'fixed'：评论卡片有 overflow-hidden
 *   与页面入场 transform 动画，挂在编辑器父级会被裁剪/错位（与 MCP modal
 *   的 fixed-under-transform 教训一致）；
 * - 图标为 Lucide 内联 SVG（stroke: currentColor 随主题），见 icons.ts。
 */

import type { Editor } from '@tiptap/core';
import { Extension } from '@tiptap/core';
import { BubbleMenuPlugin } from '@tiptap/extension-bubble-menu';
import { icon } from './icons';

const BOLD_ICON = icon('bold');
const ITALIC_ICON = icon('italic');
const STRIKE_ICON = icon('strikethrough');
const CODE_ICON = icon('code');
const LINK_ICON = icon('link');

/** 行内格式按钮定义：mark 名 → toggle 命令与图标。 */
const MARK_BUTTONS: ReadonlyArray<{
  mark: string;
  title: string;
  icon: string;
  run: (editor: Editor) => void;
}> = [
  {
    mark: 'bold',
    title: '粗体 (Ctrl+B)',
    icon: BOLD_ICON,
    run: (e) => void e.chain().focus().toggleBold().run(),
  },
  {
    mark: 'italic',
    title: '斜体 (Ctrl+I)',
    icon: ITALIC_ICON,
    run: (e) => void e.chain().focus().toggleItalic().run(),
  },
  {
    mark: 'strike',
    title: '删除线',
    icon: STRIKE_ICON,
    run: (e) => void e.chain().focus().toggleStrike().run(),
  },
  {
    mark: 'code',
    title: '行内代码',
    icon: CODE_ICON,
    run: (e) => void e.chain().focus().toggleCode().run(),
  },
];

/**
 * 评论气泡菜单扩展：挂 BubbleMenuPlugin 并在 onCreate 时构建按钮 DOM。
 *
 * shouldShow 仅在「非空文本选区」显示；代码块内与图片节点选区隐藏
 * （行内格式对它们无意义）。
 */
export const CommentBubbleMenu = Extension.create({
  name: 'commentBubbleMenu',

  onCreate() {
    buildMenuDom(this.editor);
  },

  addProseMirrorPlugins() {
    const element = document.createElement('div');
    element.className = 'comment-bubble-menu';
    // 供 onCreate 的 buildMenuDom 找到本实例的容器（多编辑器并存时按 editor
    // 实例隔离，pluginKey 每个编辑器独立注册互不冲突）。
    (this.editor.storage as unknown as Record<string, unknown>).commentBubbleMenuElement = element;

    return [
      BubbleMenuPlugin({
        pluginKey: 'commentBubbleMenu',
        editor: this.editor,
        element,
        updateDelay: 100,
        appendTo: () => document.body,
        shouldShow: ({ editor, state }) => {
          const { from, to, empty } = state.selection;
          if (empty || from === to) return false;
          if (!editor.isEditable) return false;
          // 代码块内与图片节点选区：行内格式无意义，不弹。
          if (editor.isActive('codeBlock')) return false;
          if (editor.isActive('image')) return false;
          return true;
        },
        options: {
          placement: 'top',
          offset: 8,
          strategy: 'fixed',
        },
      }),
    ];
  },
});

/** 构建菜单内部 DOM：格式按钮行 + 链接 URL 输入行（默认隐藏）。 */
function buildMenuDom(editor: Editor): void {
  const element = (editor.storage as unknown as Record<string, unknown>).commentBubbleMenuElement as
    | HTMLElement
    | undefined;
  if (!element) return;

  // —— 按钮行 ——
  const row = document.createElement('div');
  row.className = 'comment-bubble-menu-row';

  const syncActive = () => {
    for (const btn of Array.from(row.querySelectorAll<HTMLButtonElement>('button[data-mark]'))) {
      const mark = btn.dataset.mark!;
      btn.classList.toggle('is-active', editor.isActive(mark));
    }
    const linkBtn = row.querySelector<HTMLButtonElement>('button[data-link]');
    linkBtn?.classList.toggle('is-active', editor.isActive('link'));
  };

  for (const def of MARK_BUTTONS) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'comment-bubble-btn';
    btn.dataset.mark = def.mark;
    btn.title = def.title;
    btn.innerHTML = def.icon;
    btn.addEventListener('click', () => {
      // focus() 恢复编辑器选区后再应用格式（按钮点击已抢走焦点）。
      def.run(editor);
      syncActive();
    });
    row.appendChild(btn);
  }

  // 链接按钮：非链接选区 → 切到 URL 输入行；链接选区 → unlink。
  const linkBtn = document.createElement('button');
  linkBtn.type = 'button';
  linkBtn.className = 'comment-bubble-btn';
  linkBtn.dataset.link = 'true';
  linkBtn.title = '链接 (Ctrl+K)';
  linkBtn.innerHTML = LINK_ICON;
  row.appendChild(linkBtn);

  // —— 链接 URL 输入行 ——
  const linkRow = document.createElement('div');
  linkRow.className = 'comment-bubble-menu-linkrow';
  linkRow.hidden = true;
  const input = document.createElement('input');
  input.type = 'url';
  input.className = 'comment-bubble-link-input';
  input.placeholder = 'https://';
  input.spellcheck = false;
  const confirm = document.createElement('button');
  confirm.type = 'button';
  confirm.className = 'comment-bubble-btn';
  confirm.innerHTML = icon('corner-down-left');
  confirm.title = '确认';
  linkRow.appendChild(input);
  linkRow.appendChild(confirm);

  const applyLink = () => {
    const url = input.value.trim();
    if (url) {
      editor.chain().focus().extendMarkRange('link').setLink({ href: url }).run();
    } else {
      editor.chain().focus().run();
    }
    linkRow.hidden = true;
    row.hidden = false;
    syncActive();
  };
  const cancelLink = () => {
    linkRow.hidden = true;
    row.hidden = false;
    editor.chain().focus().run();
  };

  confirm.addEventListener('click', applyLink);
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      applyLink();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      cancelLink();
    }
  });

  linkBtn.addEventListener('click', () => {
    if (editor.isActive('link')) {
      editor.chain().focus().extendMarkRange('link').unsetLink().run();
      syncActive();
      return;
    }
    // 预填选区文本若是 URL（GitHub 式便利），否则留空。
    const { from, to } = editor.state.selection;
    const selected = editor.state.doc.textBetween(from, to, ' ', ' ');
    input.value = /^https?:\/\/\S+$/.test(selected) ? selected : '';
    row.hidden = true;
    linkRow.hidden = false;
    input.focus();
    input.select();
  });

  element.appendChild(row);
  element.appendChild(linkRow);

  // active 态跟随选区/事务变化。
  editor.on('transaction', syncActive);
  editor.on('destroy', () => {
    editor.off('transaction', syncActive);
  });
}
