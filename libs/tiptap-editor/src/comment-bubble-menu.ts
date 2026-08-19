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
 * - 图标为 Material Symbols inline SVG（fill: currentColor 随主题），
 *   源文件存档于 public/icons/。
 */

import type { Editor } from '@tiptap/core';
import { Extension } from '@tiptap/core';
import { BubbleMenuPlugin } from '@tiptap/extension-bubble-menu';

/** Material Symbols format_bold。 */
const BOLD_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor" width="18" height="18"><path d="M275-200v-560h228q66 0 114.5 42T666-612q0 38-21 70t-56 49v6q43 14 69.5 50t26.5 81q0 68-52.5 112T510-200H275Zm86-76h144q38 0 66-25t28-63q0-37-28-62t-66-25H361v175Zm0-247h136q35 0 60.5-23t25.5-58q0-35-25.5-58.5T497-686H361v163Z"/></svg>';
/** Material Symbols format_italic。 */
const ITALIC_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor" width="18" height="18"><path d="M224-199v-80h134l139-409H338v-80h380v80H584L445-279h159v80H224Z"/></svg>';
/** Material Symbols strikethrough_s。 */
const STRIKE_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor" width="18" height="18"><path d="M504-160q-78 0-142-41.5T269-313l69-29q20 48 65 77t101 29q52 0 83-27t31-73q0-23-9.5-48.5T582-430h84q14 23 21 46t7 48q0 78-53 127t-137 49ZM80-490v-60h800v60H80Zm394-316q66 0 117 31t75 86l-69 31q-14-34-46.5-53T474-730q-49 0-79 24t-30 66q0 8 1 15t3 15h-74q-2-8-3-16t-1-16q0-73 51-118.5T474-806Z"/></svg>';
/** Material Symbols code。 */
const CODE_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor" width="18" height="18"><path d="M320-242 80-482l242-242 43 43-199 199 197 197-43 43Zm318 2-43-43 199-199-197-197 43-43 240 240-242 242Z"/></svg>';
/** Material Symbols link。 */
const LINK_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor" width="18" height="18"><path d="M450-280H280q-83 0-141.5-58.5T80-480q0-83 58.5-141.5T280-680h170v60H280q-58.33 0-99.17 40.76-40.83 40.77-40.83 99Q140-422 180.83-381q40.84 41 99.17 41h170v60ZM325-450v-60h310v60H325Zm185 170v-60h170q58.33 0 99.17-40.76 40.83-40.77 40.83-99Q820-538 779.17-579q-40.84-41-99.17-41H510v-60h170q83 0 141.5 58.5T880-480q0 83-58.5 141.5T680-280H510Z"/></svg>';

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
  confirm.textContent = '↵';
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
