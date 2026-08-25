import { type Editor, Extension, type Range } from '@tiptap/core';
import { PluginKey } from '@tiptap/pm/state';
import { Suggestion, type SuggestionKeyDownProps, type SuggestionProps } from '@tiptap/suggestion';
import { extractLang, extractOverridesJson } from './highlight';

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
      icon: 'H1',
      keywords: 'h1 heading 标题',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).setHeading({ level: 1 }).run();
      },
    },
    {
      title: '标题 2',
      description: '中标题',
      icon: 'H2',
      keywords: 'h2 heading 标题',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).setHeading({ level: 2 }).run();
      },
    },
    {
      title: '标题 3',
      description: '小标题',
      icon: 'H3',
      keywords: 'h3 heading 标题',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).setHeading({ level: 3 }).run();
      },
    },
    {
      title: '无序列表',
      description: '创建无序列表',
      icon: '•',
      keywords: 'bullet list ul 列表',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).toggleBulletList().run();
      },
    },
    {
      title: '有序列表',
      description: '创建有序列表',
      icon: '1.',
      keywords: 'ordered ol number list 列表',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).toggleOrderedList().run();
      },
    },
    {
      title: '任务列表',
      description: '创建任务列表',
      icon: '☑',
      keywords: 'task todo checklist 列表',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).toggleTaskList().run();
      },
    },
    {
      title: '引用',
      description: '插入引用块',
      icon: '❝',
      keywords: 'quote blockquote 引用',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).toggleBlockquote().run();
      },
    },
    {
      title: '代码块',
      description: '插入代码块',
      icon: '<>',
      keywords: 'code codeblock pre 代码',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).toggleCodeBlock().run();
      },
    },
    {
      title: '可运行代码块',
      description: '插入可被读者执行的代码块',
      icon: '▶',
      keywords: 'code run runnable execute 代码 运行',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).run();
        openRunnableModal(editor);
      },
    },
    {
      title: '分割线',
      description: '插入水平分割线',
      icon: '—',
      keywords: 'hr rule divider 分割',
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).setHorizontalRule().run();
      },
    },
    {
      title: '表格',
      description: '插入 3×3 表格',
      icon: '▦',
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
      icon: '📤',
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
      icon: '🗂',
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
      icon: '🖼',
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
      icon: '🔗',
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

const SlashCommandPluginKey = new PluginKey('slashCommand');

/**
 * 斜杠命令扩展。
 *
 * `onImageUpload` / `onPickFromLibrary` 通过 `addOptions` 注入，
 * "上传图片"/"素材库"命令据此决定是否出现（见 buildSlashCommands）。
 */
export const SlashCommand = Extension.create<SlashCommandOptions>({
  name: 'slashCommand',

  addOptions() {
    return {
      onImageUpload: undefined,
      onInsertUploading: undefined,
      onPickFromLibrary: undefined,
    };
  },

  addProseMirrorPlugins() {
    // 命令集由注入回调决定（gating 逻辑见 buildSlashCommands，抽出以便单元测试）。
    const COMMANDS = buildSlashCommands(this.options);

    return [
      Suggestion<CommandItem>({
        pluginKey: SlashCommandPluginKey,
        editor: this.editor,
        char: '/',
        items: ({ query }) => {
          return COMMANDS.filter((item) => matchCommand(item, query));
        },
        render() {
          let popup: SlashPopup | null = null;

          return {
            onStart(props) {
              popup = createPopup(props);
            },
            onUpdate(props) {
              if (!popup) return;
              popup.updateItems(props.items);
              popup.updatePosition();
            },
            onKeyDown(props) {
              if (!popup) return false;
              return popup.onKeyDown(props);
            },
            onExit() {
              if (popup) {
                popup.destroy();
                popup = null;
              }
            },
          };
        },
        command: ({ editor, range, props: item }) => {
          // 防御性重算 range：Suggestion 的 range 在某些输入路径下会过时
          //（停在首次输入 '/' 的位置，不随后续字符更新），导致 deleteRange 只删 '/'，
          // 命令文本（如 'code'）残留进新节点。基于当前 selection 重算：
          // 从光标往前在同一段落内找触发字符 '/'，删到光标。
          const { from } = editor.state.selection;
          const $from = editor.state.doc.resolve(from);
          const text = $from.parent.textBetween(0, $from.parentOffset, '', '');
          const slashIdx = text.lastIndexOf('/');
          const effectiveRange =
            slashIdx >= 0 ? { from: $from.start() + slashIdx, to: from } : range;
          item.command({ editor, range: effectiveRange });
        },
      }),
    ];
  },
});

/** 斜杠命令浮层实例:供 Suggestion render 生命周期驱动。 */
interface SlashPopup {
  component: HTMLElement;
  updateItems(items: CommandItem[]): void;
  updatePosition(): void;
  onKeyDown(props: SuggestionKeyDownProps): boolean;
  destroy(): void;
}

/** 校验图片/链接 URL:只允许 http(s) 和 data:image。拒绝 javascript: 等。 */
export function isValidUrl(url: string): boolean {
  return /^https?:\/\//i.test(url) || /^data:image\//i.test(url);
}

export function createPopup(props: SuggestionProps<CommandItem>): SlashPopup {
  const component = document.createElement('div');
  component.classList.add('slash-command');

  const list = document.createElement('div');
  list.classList.add('slash-command-list');
  component.appendChild(list);

  let selectedIndex = 0;
  let currentItems: CommandItem[] = [];

  function renderItems(items: CommandItem[]) {
    currentItems = items;
    list.innerHTML = '';
    selectedIndex = 0;

    // 空状态：显示提示，不渲染列表项。
    if (items.length === 0) {
      const empty = document.createElement('div');
      empty.classList.add('slash-command-empty');
      empty.textContent = '无匹配命令';
      list.appendChild(empty);
      return;
    }

    items.forEach((item, index) => {
      const el = document.createElement('div');
      el.classList.add('slash-command-item');
      if (index === 0) el.classList.add('is-selected');

      el.innerHTML = `
        <div class="slash-command-item-icon">${item.icon}</div>
        <div class="slash-command-item-text">
          <div class="slash-command-item-title">${item.title}</div>
          <div class="slash-command-item-desc">${item.description}</div>
        </div>
      `;

      el.addEventListener('click', () => {
        props.command(item);
      });

      el.addEventListener('mouseenter', () => {
        selectedIndex = index;
        updateSelection();
      });

      list.appendChild(el);
    });
  }

  function updateSelection() {
    const children = list.children;
    for (let i = 0; i < children.length; i++) {
      if (i === selectedIndex) {
        children[i].classList.add('is-selected');
      } else {
        children[i].classList.remove('is-selected');
      }
    }
    children[selectedIndex]?.scrollIntoView({ block: 'nearest' });
  }

  function selectItem() {
    if (currentItems[selectedIndex]) {
      props.command(currentItems[selectedIndex]);
    }
  }

  function updatePosition() {
    const rect = props.clientRect?.();
    if (!rect) return;
    component.style.left = `${rect.left}px`;
    component.style.top = `${rect.bottom + 4}px`;
  }

  renderItems(props.items);
  document.body.appendChild(component);
  updatePosition();

  return {
    component,
    updateItems(items: CommandItem[]) {
      renderItems(items);
    },
    updatePosition,
    onKeyDown({ event }: SuggestionKeyDownProps): boolean {
      // 空列表时不拦截键盘：避免 % 0 产生 NaN，也避免吞掉 Enter（让用户正常输入）。
      // Escape 仍拦截（关闭浮层）。
      if (event.key === 'Escape') {
        event.preventDefault();
        return true;
      }
      if (currentItems.length === 0) {
        return false;
      }
      if (event.key === 'ArrowUp') {
        event.preventDefault();
        selectedIndex = (selectedIndex - 1 + currentItems.length) % currentItems.length;
        updateSelection();
        return true;
      }
      if (event.key === 'ArrowDown') {
        event.preventDefault();
        selectedIndex = (selectedIndex + 1) % currentItems.length;
        updateSelection();
        return true;
      }
      if (event.key === 'Enter') {
        event.preventDefault();
        selectItem();
        return true;
      }
      return false;
    },
    destroy() {
      component.remove();
    },
  };
}

/** buildRunnableInfo 的输入配置。 */
export interface RunnableInfoOpts {
  /** 语言名（python / node）。 */
  lang: string;
  /** 超时秒数。 */
  timeoutSecs: number;
  /** 内存上限（MB）。 */
  memoryMb: number;
  /** 是否允许网络。 */
  allowNetwork: boolean;
  /** 作者是否改动过任一 overrides 字段；false 则省略 JSON。 */
  dirty: boolean;
}

/**
 * 把弹框收集的配置转成 markdown fence 的 info string。
 *
 * - dirty=false → `${lang} runnable`（省略 JSON，最小形态）
 * - dirty=true  → `${lang} runnable {"timeout_secs":N,"memory_mb":M,"allow_network":B}`
 *
 * JSON 字段顺序固定（timeout → memory → network），由显式构造保证（不依赖对象插入顺序）。
 * 到达此函数时值必然合法（弹框「插入」按钮在非法值时 disabled）。
 */
export function buildRunnableInfo(opts: RunnableInfoOpts): string {
  const prefix = `${opts.lang} runnable`;
  if (!opts.dirty) return prefix;
  // 显式拼字符串，保证字段顺序固定（timeout → memory → network），不依赖对象键序。
  const json = `{"timeout_secs":${opts.timeoutSecs},"memory_mb":${opts.memoryMb},"allow_network":${opts.allowNetwork}}`;
  return `${prefix} ${json}`;
}

/**
 * 受支持的语言（canonical key + 展示标签，与 src/pages/admin/runner.rs 的
 * SUPPORTED_LANGS 对齐；后者镜像后端 LANGUAGES 注册表，见
 * src/api/code_runner/languages.rs）。编辑器是纯 JS lib，不调 server function，故写死。
 */
const RUNNABLE_LANGS = [
  { value: 'python', label: 'Python' },
  { value: 'node', label: 'Node.js' },
  { value: 'go', label: 'Go' },
  { value: 'rust', label: 'Rust' },
  { value: 'bun', label: 'Bun (TS)' },
] as const;

/** 模态框默认值（与后端 ResourceLimits 默认对齐：见 languages.rs）。 */
const RUNNABLE_DEFAULTS = { timeoutSecs: 5, memoryMb: 256, allowNetwork: false };

/** timeout_secs 取值范围（与 CODE_RUNNER_MAX_TIMEOUT_SECS 对齐）。 */
const TIMEOUT_RANGE = { min: 1, max: 30 } as const;
/** memory_mb 取值范围（与 CODE_RUNNER_MAX_MEMORY_MB 对齐）。 */
const MEMORY_RANGE = { min: 16, max: 1024 } as const;

const SVG_NS = 'http://www.w3.org/2000/svg';

/** 描边图标（与 Dioxus 侧 FormSelect 的 chevron / 选中对勾同源）。 */
function makeSvgIcon(pathD: string, className: string): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('class', className);
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '2');
  const path = document.createElementNS(SVG_NS, 'path');
  path.setAttribute('stroke-linecap', 'round');
  path.setAttribute('stroke-linejoin', 'round');
  path.setAttribute('d', pathD);
  svg.appendChild(path);
  return svg;
}

/** createLangSelect 返回的控件句柄。 */
interface LangSelectWidget {
  /** 根元素（relative 容器，挂进表单行）。 */
  el: HTMLElement;
  /** 面板当前是否展开（模态框 Enter 提交据此避让）。 */
  isOpen(): boolean;
  /** 聚焦触发器（模态框打开时的初始焦点）。 */
  focus(): void;
}

/**
 * 自定义语言下拉：对齐 Dioxus 侧 FormSelect（src/components/forms.rs）的视觉与
 * 交互契约——原生 <select> 的弹出列表由 OS 渲染、无法跟随主题，故重写为
 * `button[aria-haspopup=listbox]` + 绝对定位面板：
 * - 打开时透明遮罩拦截外部点击关闭；选项 mousedown 阻止默认行为，焦点始终留在触发器；
 * - 键盘：↑↓ 循环高亮、Enter/Space 选中、Esc 关闭（不冒泡到模态框）、
 *   Home/End 跳首尾、Tab 关闭并自然流转焦点；
 * - 视口下方空间不足且上方更宽余时向上展开（FormSelect should_flip 同款判定）；
 * - 面板进出场复用全站 animate-select-enter（input.css）。
 */
function createLangSelect(
  options: readonly { value: string; label: string }[],
  initial: string,
  onChange: (value: string) => void,
): LangSelectWidget {
  const root = document.createElement('div');
  root.className = 'tiptap-select';

  const trigger = document.createElement('button');
  trigger.type = 'button';
  trigger.className = 'tiptap-select-trigger';
  trigger.setAttribute('aria-haspopup', 'listbox');
  trigger.setAttribute('aria-expanded', 'false');
  const label = document.createElement('span');
  trigger.appendChild(label);
  trigger.appendChild(makeSvgIcon('M6 9l6 6 6-6', 'tiptap-select-chevron'));
  root.appendChild(trigger);

  let open = false;
  let selected = Math.max(
    0,
    options.findIndex((o) => o.value === initial),
  );
  let active = selected;
  let overlay: HTMLElement | null = null;
  let panel: HTMLElement | null = null;

  label.textContent = options[selected].label;

  /** 视口下方空间不足且上方更宽余时向上展开。 */
  function shouldFlip(): boolean {
    const rect = trigger.getBoundingClientRect();
    // 行高 40px（py-2.5+行盒）×选项数 + 面板 chrome（边框+padding），封顶 254px。
    const panelHeight = Math.min(options.length * 40 + 14, 254);
    const below = window.innerHeight - rect.bottom;
    const above = rect.top;
    return below < panelHeight + 14 && above > below;
  }

  /** 同步 active/selected 的展示态（不重排 DOM，避免 hover 时选项抖动）。 */
  function paintOptions(): void {
    if (!panel) return;
    const items = panel.querySelectorAll<HTMLElement>('.tiptap-select-option');
    items.forEach((li, i) => {
      li.classList.toggle('active', i === active);
      li.classList.toggle('selected', i === selected);
      li.setAttribute('aria-selected', String(i === selected));
    });
  }

  function scrollActiveIntoView(): void {
    (panel?.children[active] as HTMLElement | undefined)?.scrollIntoView({ block: 'nearest' });
  }

  function closePanel(): void {
    if (!open) return;
    open = false;
    trigger.setAttribute('aria-expanded', 'false');
    root.classList.remove('open');
    overlay?.remove();
    panel?.remove();
    overlay = null;
    panel = null;
  }

  function select(i: number): void {
    selected = i;
    label.textContent = options[i].label;
    onChange(options[i].value);
    closePanel();
  }

  function openPanel(): void {
    if (open) return;
    open = true;
    active = selected;
    trigger.setAttribute('aria-expanded', 'true');
    root.classList.add('open');

    overlay = document.createElement('div');
    overlay.className = 'tiptap-select-overlay';
    overlay.addEventListener('click', closePanel);

    panel = document.createElement('ul');
    panel.className = 'tiptap-select-panel animate-select-enter';
    panel.setAttribute('role', 'listbox');
    if (shouldFlip()) panel.classList.add('flip');
    options.forEach((opt, i) => {
      const li = document.createElement('li');
      li.className = 'tiptap-select-option';
      li.setAttribute('role', 'option');
      const text = document.createElement('span');
      text.textContent = opt.label;
      li.appendChild(text);
      li.appendChild(makeSvgIcon('M20 6L9 17l-5-5', 'tiptap-select-check'));
      // 阻止 mousedown 默认行为：点击选项不夺走触发器焦点。
      li.addEventListener('mousedown', (e) => e.preventDefault());
      li.addEventListener('mouseenter', () => {
        active = i;
        paintOptions();
      });
      li.addEventListener('click', () => select(i));
      panel?.appendChild(li);
    });

    root.appendChild(overlay);
    root.appendChild(panel);
    paintOptions();
    scrollActiveIntoView();
  }

  trigger.addEventListener('click', () => {
    // 打开态下触发器被透明遮罩盖住，点击落在遮罩上即关闭；这里只需处理「未开 → 开」。
    if (!open) openPanel();
  });

  trigger.addEventListener('keydown', (e) => {
    if (!open) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        openPanel();
      }
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      active = (active + 1) % options.length;
      paintOptions();
      scrollActiveIntoView();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      active = (active - 1 + options.length) % options.length;
      paintOptions();
      scrollActiveIntoView();
    } else if (e.key === 'Home') {
      e.preventDefault();
      active = 0;
      paintOptions();
      scrollActiveIntoView();
    } else if (e.key === 'End') {
      e.preventDefault();
      active = options.length - 1;
      paintOptions();
      scrollActiveIntoView();
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      select(active);
    } else if (e.key === 'Escape') {
      // stopPropagation：只关面板，不触发模态框的 Esc 关闭。
      e.preventDefault();
      e.stopPropagation();
      closePanel();
    } else if (e.key === 'Tab') {
      // 不拦截：关闭后焦点自然流转到下一个控件。
      closePanel();
    }
  });

  return { el: root, isOpen: () => open, focus: () => trigger.focus() };
}

/**
 * 打开「可运行代码块」配置模态框。
 *
 * 两种模式：
 * - 插入模式（默认）：确认后 `setCodeBlock({ language })` 插入新块。
 * - 编辑模式（传 `editPos` + `currentInfo`）：回填当前块的 lang/overrides，确认后
 *   `setNodeMarkup(editPos, ..., { language })` 原地更新该块的 language 属性。
 *   供 CodeBlockNodeView 的语言标签点击触发（创建后修改语言/overrides）。
 *
 * 作者选语言 + 可选 overrides（超时/内存/网络）。
 * 任一 overrides 字段被改动即 dirty；dirty=false 用 'python runnable'（无 JSON）。
 * Esc / 遮罩点击 / 取消按钮 / × → 关闭不改动。
 * 控件对齐站点设计系统：自定义下拉（FormSelect）、INPUT_CLASS 契约输入框、
 * 全站 .ygg-cb 复选框（src/components/ui.rs Checkbox）、BTN_PRIMARY 契约主按钮；
 * 配色全部走 --color-paper-* token，亮暗主题自动切换。
 */
export function openRunnableModal(editor: Editor, editPos?: number, currentInfo?: string): void {
  const isEdit = editPos !== undefined;
  // 编辑模式：从 currentInfo 回填 lang + overrides；插入模式用默认值
  const initialLang = isEdit ? extractLang(currentInfo ?? '') : 'python';
  const initialOverrides = isEdit ? extractOverridesJson(currentInfo ?? '') : '';
  const parsedOverrides = initialOverrides
    ? (() => {
        try {
          return JSON.parse(initialOverrides);
        } catch {
          return null;
        }
      })()
    : null;
  const state = {
    lang: RUNNABLE_LANGS.some((l) => l.value === initialLang) ? initialLang : 'python',
    timeoutSecs: parsedOverrides?.timeout_secs ?? RUNNABLE_DEFAULTS.timeoutSecs,
    memoryMb: parsedOverrides?.memory_mb ?? RUNNABLE_DEFAULTS.memoryMb,
    allowNetwork: parsedOverrides?.allow_network ?? RUNNABLE_DEFAULTS.allowNetwork,
    // 编辑模式下若有 overrides 即 dirty（保留现有 overrides 除非作者改动）
    dirty: isEdit && parsedOverrides !== null,
  };

  const mask = document.createElement('div');
  mask.className = 'tiptap-runnable-modal-mask animate-modal-overlay-enter';

  const modal = document.createElement('div');
  modal.className = 'tiptap-runnable-modal animate-modal-panel-enter';

  // 头部：标题 + × 关闭（对齐 admin 弹窗头部契约，如 mcp.rs PlaintextModal）
  const header = document.createElement('div');
  header.className = 'tiptap-runnable-modal-header';
  const title = document.createElement('div');
  title.className = 'tiptap-runnable-modal-title';
  title.textContent = isEdit ? '编辑可运行代码块' : '插入可运行代码块';
  const closeBtn = document.createElement('button');
  closeBtn.className = 'tiptap-runnable-modal-close';
  closeBtn.type = 'button';
  closeBtn.textContent = '×';
  closeBtn.setAttribute('aria-label', '关闭');
  closeBtn.addEventListener('click', close);
  header.appendChild(title);
  header.appendChild(closeBtn);
  modal.appendChild(header);

  // 语言选择（自定义下拉，对齐 FormSelect；原生 select 弹出层由 OS 渲染、无法跟随主题）
  const langRow = document.createElement('div');
  langRow.className = 'tiptap-runnable-field';
  const langLabel = document.createElement('span');
  langLabel.className = 'tiptap-runnable-label';
  langLabel.textContent = '语言';
  const langSelect = createLangSelect(RUNNABLE_LANGS, state.lang, (value) => {
    state.lang = value;
    updatePreview();
  });
  langRow.appendChild(langLabel);
  langRow.appendChild(langSelect.el);
  modal.appendChild(langRow);

  /** 数字输入行（label + INPUT_CLASS 契约输入框），值改动时联动预览与校验。 */
  function numberField(
    labelText: string,
    inputId: string,
    range: { readonly min: number; readonly max: number },
    initial: number,
    fallback: number,
    onInput: (v: number) => void,
  ): { row: HTMLElement; input: HTMLInputElement } {
    const row = document.createElement('label');
    row.className = 'tiptap-runnable-field';
    const lab = document.createElement('span');
    lab.className = 'tiptap-runnable-label';
    lab.textContent = labelText;
    const input = document.createElement('input');
    input.id = inputId;
    input.className = 'tiptap-runnable-input';
    input.type = 'number';
    input.min = String(range.min);
    input.max = String(range.max);
    input.value = String(initial);
    input.addEventListener('input', () => {
      onInput(Number(input.value) || fallback);
      state.dirty = true;
      updatePreview();
      updateInsertEnabled();
    });
    row.appendChild(lab);
    row.appendChild(input);
    return { row, input };
  }

  const timeoutField = numberField(
    '超时（秒）',
    'runnable-timeout',
    TIMEOUT_RANGE,
    state.timeoutSecs,
    RUNNABLE_DEFAULTS.timeoutSecs,
    (v) => {
      state.timeoutSecs = v;
    },
  );
  modal.appendChild(timeoutField.row);

  const memField = numberField(
    '内存（MB）',
    'runnable-memory',
    MEMORY_RANGE,
    state.memoryMb,
    RUNNABLE_DEFAULTS.memoryMb,
    (v) => {
      state.memoryMb = v;
    },
  );
  modal.appendChild(memField.row);

  // 网络（复选框复用全站 .ygg-cb 结构与样式，见 src/components/ui.rs Checkbox）
  const netRow = document.createElement('div');
  netRow.className = 'tiptap-runnable-field';
  const netLabel = document.createElement('label');
  netLabel.className = 'tiptap-runnable-check-row';
  const netBox = document.createElement('span');
  netBox.className = 'ygg-cb';
  const netInput = document.createElement('input');
  netInput.id = 'runnable-network';
  netInput.type = 'checkbox';
  netInput.checked = state.allowNetwork;
  netInput.addEventListener('change', () => {
    state.allowNetwork = netInput.checked;
    state.dirty = true;
    updatePreview();
  });
  const netMark = document.createElementNS(SVG_NS, 'svg');
  netMark.setAttribute('class', 'ygg-cb-mark');
  netMark.setAttribute('viewBox', '0 0 16 16');
  const netPath = document.createElementNS(SVG_NS, 'path');
  netPath.setAttribute('class', 'ygg-cb-check');
  netPath.setAttribute('d', 'M3.5 8.5l3 3 6-6.5');
  netMark.appendChild(netPath);
  netBox.appendChild(netInput);
  netBox.appendChild(netMark);
  const netText = document.createElement('span');
  netText.textContent = '允许网络';
  netLabel.appendChild(netBox);
  netLabel.appendChild(netText);
  netRow.appendChild(netLabel);
  modal.appendChild(netRow);

  // 预览
  const preview = document.createElement('div');
  preview.className = 'tiptap-runnable-preview';
  modal.appendChild(preview);

  // 按钮
  const actions = document.createElement('div');
  actions.className = 'tiptap-runnable-actions';
  const cancelBtn = document.createElement('button');
  cancelBtn.className = 'cancel';
  cancelBtn.type = 'button';
  cancelBtn.textContent = '取消';
  cancelBtn.addEventListener('click', close);
  const insertBtn = document.createElement('button');
  insertBtn.className = 'insert';
  insertBtn.type = 'button';
  insertBtn.textContent = isEdit ? '保存' : '插入';
  insertBtn.addEventListener('click', insert);
  actions.appendChild(cancelBtn);
  actions.appendChild(insertBtn);
  modal.appendChild(actions);

  mask.appendChild(modal);
  mask.addEventListener('click', (e) => {
    // 仅点击遮罩本身（非卡片）时关闭
    if (e.target === mask) close();
  });

  function updatePreview(): void {
    preview.textContent = `\`\`\`${buildRunnableInfo(state)}`;
  }

  /** 校验数字字段：全合法才启用「插入」。 */
  function updateInsertEnabled(): void {
    const t = Number(timeoutField.input.value);
    const m = Number(memField.input.value);
    insertBtn.disabled = !(
      t >= TIMEOUT_RANGE.min &&
      t <= TIMEOUT_RANGE.max &&
      m >= MEMORY_RANGE.min &&
      m <= MEMORY_RANGE.max
    );
  }

  function insert(): void {
    const language = buildRunnableInfo(state);
    if (isEdit && editPos !== undefined) {
      // 编辑模式：原地更新块的 language 属性（保留块内代码内容）。
      // Tiptap chain 无 setNodeMarkup，用原生 tr。
      const tr = editor.state.tr;
      tr.setNodeMarkup(editPos, undefined, { language });
      editor.view.dispatch(tr);
    } else {
      // 插入模式：新建 codeBlock
      editor.chain().setCodeBlock({ language }).run();
    }
    close();
  }

  function close(): void {
    document.removeEventListener('keydown', onKeydown);
    mask.remove();
    editor.chain().focus().run();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key === 'Enter' && !insertBtn.disabled) {
      // Enter 在 number input 内提交（浏览器原生 number input 的 Enter 不会触发 click）。
      // 语言下拉展开时 Enter 由触发器自身消费（选中高亮项并 stopPropagation 前的
      // preventDefault），此处经 isOpen() 避让；按钮（触发器/取消/插入/×）的 Enter
      // 走原生 click，不在此处理。网络 checkbox 的 tagName 也是 'input'，Enter 触发
      // 提交而非切换（checkbox 原生用 Space 切换），符合模态框 Enter=确认的惯例。
      if (langSelect.isOpen()) return;
      const tag = (document.activeElement?.tagName ?? '').toLowerCase();
      if (tag === 'input') {
        e.preventDefault();
        insert();
      }
    }
  }

  document.body.appendChild(mask);
  // 延迟注册 keydown 监听：openRunnableModal 可能在 slash 命令的 Enter keydown 事件处理中
  // 同步调用。若立即注册，该 Enter 事件继续冒泡到 document 时会被模态框的 onKeydown 捕获，
  // 触发 insert() → close()——模态框在同一 tick 内被创建又被销毁，用户看不到。
  // setTimeout(0) 推迟到下一个宏任务，确保触发事件已完成传播。
  setTimeout(() => document.addEventListener('keydown', onKeydown), 0);
  updatePreview();
  updateInsertEnabled();
  langSelect.focus();
}
