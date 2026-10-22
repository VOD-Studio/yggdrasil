import type { Editor, Range } from '@tiptap/core';
import { icon } from '../icons';
import { openRunnableModal } from './runnable-modal';

export interface CommandItem {
  title: string;
  description: string;
  icon: string;
  command: (props: { editor: Editor; range: Range }) => void;
  /**
   * 搜索别名（空格分隔），让中英文都能命中。
   * 例：「代码块」keywords='code codeblock' → /code 与 /代码 都匹配。
   * title/description 已含的字词不必重复写（过滤逻辑会一并匹配）。
   */
  keywords?: string;
}

/**
 * 判断命令是否匹配搜索词（不区分大小写）。
 *
 * 命中 title / description / keywords 任一即算匹配。keywords 是空格分隔的别名
 * （含英文/常见词），让中英文互通：`/code` 能命中「代码块」，`/代码` 也能。
 * 抽成纯函数便于单元测试。
 */
export function matchCommand(item: CommandItem, query: string): boolean {
  const q = query.toLowerCase();
  return (
    item.title.toLowerCase().includes(q) ||
    item.description.toLowerCase().includes(q) ||
    (item.keywords?.toLowerCase().includes(q) ?? false)
  );
}

/**
 * 斜杠命令扩展的选项。
 *
 * `onImageUpload` 由宿主注入（参见 index.ts），用于把用户选择的图片文件
 * 上传到服务端并返回可访问的 URL。未提供时"上传图片"命令会被隐藏，
 * 只保留"图片链接"（手动填 URL）。
 *
 * `onPickFromLibrary` 由宿主注入（write.rs），触发打开素材库弹窗；
 * 未提供时"素材库"命令会被隐藏。
 */
export interface SlashCommandOptions {
  onImageUpload?: (file: File) => Promise<string>;
  /** 由 index.ts 注入：直接调 coordinator.insertUploading（走占位符 + 上传）。 */
  onInsertUploading?: (file: File) => void;
  /** 由宿主注入：打开素材库选择器；确认后宿主调 insertImagesFromLibrary 回填。 */
  onPickFromLibrary?: () => void;
}

/**
 * 构造斜杠命令列表（抽成独立导出函数，便于单元测试命令 gating 与命令体）。
 *
 * 命令集由注入回调决定：无 `onImageUpload` 隐藏「上传图片」，
 * 无 `onPickFromLibrary` 隐藏「素材库」；「图片链接」「链接」始终可用。
 */
export function buildSlashCommands(options: SlashCommandOptions): CommandItem[] {
  const uploadFn = options.onImageUpload;
  const COMMANDS: CommandItem[] = [
    {
      title: '标题 1',
      description: '大标题',
      icon: icon('heading-1'),
      keywords: 'h1 heading 标题',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).setHeading({ level: 1 }).run();
      },
    },
    {
      title: '标题 2',
      description: '中标题',
      icon: icon('heading-2'),
      keywords: 'h2 heading 标题',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).setHeading({ level: 2 }).run();
      },
    },
    {
      title: '标题 3',
      description: '小标题',
      icon: icon('heading-3'),
      keywords: 'h3 heading 标题',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).setHeading({ level: 3 }).run();
      },
    },
    {
      title: '无序列表',
      description: '创建无序列表',
      icon: icon('list'),
      keywords: 'bullet list ul 列表',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).toggleBulletList().run();
      },
    },
    {
      title: '有序列表',
      description: '创建有序列表',
      icon: icon('list-ordered'),
      keywords: 'ordered ol number list 列表',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).toggleOrderedList().run();
      },
    },
    {
      title: '任务列表',
      description: '创建任务列表',
      icon: icon('list-checks'),
      keywords: 'task todo checklist 列表',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).toggleTaskList().run();
      },
    },
    {
      title: '引用',
      description: '插入引用块',
      icon: icon('text-quote'),
      keywords: 'quote blockquote 引用',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).toggleBlockquote().run();
      },
    },
    {
      title: '代码块',
      description: '插入代码块',
      icon: icon('code-xml'),
      keywords: 'code codeblock pre 代码',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).toggleCodeBlock().run();
      },
    },
    {
      title: '可运行代码块',
      description: '插入可被读者执行的代码块',
      icon: icon('square-terminal'),
      keywords: 'code run runnable execute 代码 运行',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).run();
        openRunnableModal(editor);
      },
    },
    {
      title: '分割线',
      description: '插入水平分割线',
      icon: icon('minus'),
      keywords: 'hr rule divider 分割',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).setHorizontalRule().run();
      },
    },
    {
      title: '表格',
      description: '插入 3×3 表格',
      icon: icon('table'),
      keywords: 'table 表格',
      command: ({ editor, range }) => {
        editor
          .chain()
          .focus()
          .deleteRange(range)
          .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
          .run();
      },
    },
  ];

  // 图片相关命令：上传命令仅在上传回调可用时才出现。
  if (uploadFn) {
    COMMANDS.push({
      title: '上传图片',
      description: '从本地选择并上传图片',
      icon: icon('image-up'),
      keywords: 'image upload 图片',
      command: ({ editor, range }) => {
        // 必须先删掉 /命令 文本，文件选择对话框会阻塞，关闭后 range 可能失效。
        editor.chain().focus().deleteRange(range).run();
        const input = document.createElement('input');
        input.type = 'file';
        input.accept = 'image/jpeg,image/png,image/gif,image/webp';
        input.addEventListener('change', () => {
          const file = input.files?.[0];
          if (!file) return;
          // 优先走 coordinator（占位符 + 上传），否则退回直接上传（无占位符）
          if (options.onInsertUploading) {
            options.onInsertUploading(file);
          } else if (uploadFn) {
            uploadFn(file)
              .then((url) => {
                editor.chain().focus().setImage({ src: url }).run();
              })
              .catch((err) => {
                const msg = err instanceof Error ? err.message : String(err);
                console.error('[SlashCommand] Upload failed:', msg);
              });
          }
        });
        // click() 会立即触发原生文件选择器；回调在用户选择文件后异步执行。
        input.click();
      },
    });
  }

  // 素材库命令：仅在宿主注入素材库回调时出现（write.rs 总是注入）。
  if (options.onPickFromLibrary) {
    const pickFromLibrary = options.onPickFromLibrary;
    COMMANDS.push({
      title: '素材库',
      description: '从素材库选择图片插入',
      icon: icon('images'),
      keywords: 'image library assets 素材 图片',
      command: ({ editor, range }) => {
        // 与「上传图片」同一约定：先删掉 /命令 文本（弹窗异步，关闭后 range 可能失效）。
        // 光标停在删除位置；ProseMirror selection 不随弹窗焦点变化，
        // 宿主确认后调 insertImagesFromLibrary，focus() 即在原位置插入。
        editor.chain().focus().deleteRange(range).run();
        pickFromLibrary();
      },
    });
  }

  COMMANDS.push(
    {
      title: '图片链接',
      description: '通过 URL 插入图片',
      icon: icon('image'),
      keywords: 'image url 图片',
      command: ({ editor, range }) => {
        const url = window.prompt('输入图片 URL');
        if (url && isValidUrl(url)) {
          editor.chain().focus().deleteRange(range).setImage({ src: url }).run();
        }
      },
    },
    {
      title: '链接',
      description: '插入链接',
      icon: icon('link'),
      keywords: 'link url a href 链接',
      command: ({ editor, range }) => {
        const url = window.prompt('输入链接 URL');
        if (!url || !isValidUrl(url)) return;
        // deleteRange 后光标停在 range.to；先插入 URL 文本，再选中刚插入的范围设 link
        // （setLink 需要非空选区才生效，原顺序 setLink 在空选区无效）。
        const insertFrom = range.to;
        editor
          .chain()
          .focus()
          .deleteRange(range)
          .insertContent(url)
          .setTextSelection({ from: insertFrom, to: insertFrom + url.length })
          .setLink({ href: url })
          .run();
      },
    },
  );

  return COMMANDS;
}

/** 校验图片/链接 URL:只允许 http(s) 和 data:image。拒绝 javascript: 等。 */
export function isValidUrl(url: string): boolean {
  return /^https?:\/\//i.test(url) || /^data:image\//i.test(url);
}
