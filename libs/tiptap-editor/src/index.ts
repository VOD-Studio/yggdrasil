import { Editor } from '@tiptap/core';
import { ON_RUN_CODE_STORAGE_KEY } from './code-block-view';
import { buildExtensions, type EditorVariant } from './editor-extensions';
import {
  UPLOAD_COORDINATOR_STORAGE_KEY,
  UploadCoordinator,
  type UploadEvent,
} from './upload-coordinator';
import { parseLibraryInsertItems } from './upload-image';
import './style.css';
import './code-highlight.css';
import { icon } from './icons';

/**
 * 编辑器选项。用 class 而非 interface，使其编译后保留运行时构造函数——
 * Rust 侧 wasm-bindgen 的 `#[wasm_bindgen(constructor)]` 会生成 `new EditorOptions()`，
 * 需要一个全局可访问的构造函数（interface 会被 TS 擦除）。
 * 字段全部可选，Rust 通过 setter（set_placeholder/set_on_update/...）逐个赋值。
 *
 * 不用 export（避免 IIFE 的 named export 与 default 冲突），而是挂到 globalThis，
 * 让 wasm-bindgen glue 里的裸标识符 `EditorOptions` 能解析到。
 */
class EditorOptions {
  content?: string;
  placeholder?: string;
  /**
   * 编辑器变体：'full'（后台文章编辑器，默认）| 'comment'（评论区精简子集）。
   * comment 砍掉标题/表格/任务列表/脚注/斜杠命令/源码切换/代码块 NodeView，
   * 获得气泡菜单与 Placeholder；图片上传占位符 UX 两变体完全一致。
   */
  variant?: EditorVariant;
  onUpdate?: (markdown: string) => void;
  onFocus?: () => void;
  onBlur?: () => void;
  editable?: boolean;
  // 图片上传回调
  onImageUpload?: (file: File) => Promise<string>;
  // 素材库选择回调：Rust 注入，slash 命令「素材库」触发，宿主打开 AssetPickerModal；
  // 宿主确认选择后回调 insertImagesFromLibrary 完成插入。
  onPickFromLibrary?: () => void;
  // 编辑器实例创建完成（含 coordinator）后同步触发一次，替代 window.__tiptap_ready 轮询
  onReady?: () => void;
  // 上传状态事件（error/success/removed + counts），替代 window.__tiptap_uploads 轮询
  onUploadEvent?: (event: UploadEvent) => void;
  // 编辑器内运行代码回调：Rust 注入，NodeView 点击「运行」时经 editor.storage 转发。
  // opts: { language(纯语言名), source, overridesJson }；返回格式化结果字符串（Rust 拼好 stdout/stderr/状态）。
  onRunCode?: (opts: {
    language: string;
    source: string;
    overridesJson: string;
  }) => Promise<string>;
  // 往返守卫：由宿主注入，载入正文/源码切回富文本时比较原文与富文本往返后的 Markdown，
  // `true` 表示服务端渲染一致。未注入时不做守卫（评论、笔记、组件图鉴保持现有行为）。
  checkRoundtrip?: (original: string, roundtrip: string) => Promise<boolean>;
}
// wasm-bindgen 生成的 glue 用裸标识符 `new EditorOptions()` 从全局解析，
// IIFE 的 name 只能挂一个全局（TiptapEditor），这里手动把 EditorOptions 也挂到 window 上。
(window as unknown as Record<string, unknown>).EditorOptions = EditorOptions;

/**
 * 源码模式切换按钮图标（Lucide，stroke: currentColor 跟随主题）。
 * - file-code：富文本态下显示，提示切换到源码。
 * - file-text：源码态下显示，提示切换回富文本。
 */
const CODE_ICON_SVG = icon('file-code', 18);
const ARTICLE_ICON_SVG = icon('file-text', 18);
const SOURCE_LOCK_MESSAGE =
  '这篇内容里有富文本模式无法原样保留的写法，已切换到 Markdown 源码编辑。';

class TiptapEditorInstance {
  private editor: Editor | null = null;
  private container: HTMLElement;
  private options: EditorOptions;

  private isSourceMode = false;
  private sourceTextarea: HTMLTextAreaElement | null = null;
  private toggleButton: HTMLButtonElement | null = null;
  private coordinator: UploadCoordinator | null = null;
  private onDestroy?: () => void;
  private pendingOriginal: string | null = null;
  private loadSeq = 0;
  private lockNotice: HTMLParagraphElement | null = null;

  constructor(
    container: HTMLElement,
    options: EditorOptions = new EditorOptions(),
    onDestroy?: () => void,
  ) {
    this.container = container;
    this.options = options;
    this.onDestroy = onDestroy;
    this.init();
  }

  private init() {
    const variant: EditorVariant = this.options.variant ?? 'full';
    const isComment = variant === 'comment';

    const el = document.createElement('div');
    el.className = isComment ? 'tiptap-editor tiptap-editor--comment' : 'tiptap-editor';
    this.container.appendChild(el);

    // 源码模式切换按钮：悬浮于编辑器右上角（仅 full——评论框高度有限，
    // 浮动按钮会遮挡首行文字；评论的格式面由输入规则/气泡菜单/粘贴承载）。
    if (!isComment) {
      this.toggleButton = document.createElement('button');
      this.toggleButton.className = 'tiptap-toggle-btn';
      this.toggleButton.type = 'button';
      this.toggleButton.title = '切换 Markdown 源码';
      this.toggleButton.innerHTML = CODE_ICON_SVG;
      this.toggleButton.addEventListener('click', () => this.toggleSource());
      el.appendChild(this.toggleButton);

      // 源码锁定提示：往返检查判定有损时显示（hidden 属性控制，样式里不写 display）
      this.lockNotice = document.createElement('p');
      this.lockNotice.className = 'tiptap-source-lock';
      this.lockNotice.setAttribute('role', 'status');
      this.lockNotice.hidden = true;
      this.lockNotice.textContent = SOURCE_LOCK_MESSAGE;
      el.appendChild(this.lockNotice);

      // 源码模式 textarea：初始隐藏，与 ProseMirror 共用同一区域
      this.sourceTextarea = document.createElement('textarea');
      this.sourceTextarea.className = 'tiptap-source-textarea';
      this.sourceTextarea.hidden = true;
      this.sourceTextarea.placeholder = '在此输入 Markdown 源码...';
      this.sourceTextarea.spellcheck = false;
      this.sourceTextarea.addEventListener('input', () => {
        // 源码模式下通过 onUpdate 回调同步内容（替代旧版 window.__tiptap_content 缓存）
        if (this.options.onUpdate) {
          this.options.onUpdate(this.sourceTextarea!.value);
        }
      });
      el.appendChild(this.sourceTextarea);
    }

    this.editor = new Editor({
      element: el,
      extensions: buildExtensions({
        variant,
        placeholder: this.options.placeholder,
        onImageUpload: this.options.onImageUpload,
        onInsertUploading: this.options.onImageUpload
          ? (file) => this.coordinator?.insertUploading(file)
          : undefined,
        onPickFromLibrary: this.options.onPickFromLibrary,
        getCoordinator: () => this.coordinator,
      }),
      content: this.options.content || '',
      editable: this.options.editable !== false,
      autofocus: false,
      // 评论模式给 ProseMirror DOM 补无障碍语义（full 由后台页面结构兜底）。
      // 注意：tiptap Editor 用浅合并覆盖默认 options（editorProps 默认为 {}），
      // 传 `editorProps: undefined` 会把默认值冲掉，createView 读
      // `editorProps.dispatchTransaction` 直接抛 TypeError——所以这里用条件
      // 展开，full 路径绝不出现 editorProps 键。
      ...(isComment
        ? {
            editorProps: {
              attributes: {
                role: 'textbox',
                'aria-label': '评论内容',
                'aria-multiline': 'true' as const,
              },
            },
          }
        : {}),
      onUpdate: ({ editor }) => {
        if (this.options.onUpdate) {
          // 脚注 [^id] 现由 atom 节点(footnoteRef/footnoteDef)承载,序列化走
          // renderMarkdown 直接拼字面量,不再经 escapeMarkdownSyntax,无需 unescape。
          this.options.onUpdate(editor.getMarkdown());
        }
      },
      onFocus: () => {
        if (this.options.onFocus) {
          this.options.onFocus();
        }
      },
      onBlur: () => {
        if (this.options.onBlur) {
          this.options.onBlur();
        }
      },
    });

    // 创建上传协调器，挂到 editor.storage 供 NodeView 按实例读取（支持多编辑器实例）。
    // onUploadEvent 透传给 coordinator.emit，未提供时空操作兜底。
    if (this.options.onImageUpload) {
      this.coordinator = new UploadCoordinator(
        this.editor,
        this.options.onImageUpload,
        this.options.onUploadEvent ?? (() => {}),
      );
      // editor.storage 是开放式索引签名；自定义 key 需绕过严格检查（Tiptap 官方扩展 storage 模式）。
      (this.editor.storage as unknown as Record<string, unknown>)[UPLOAD_COORDINATOR_STORAGE_KEY] =
        this.coordinator;
    }

    // 把 onRunCode 回调挂到 editor.storage，供 CodeBlockNodeView 读取（仿 upload-coordinator 范式）。
    if (this.options.onRunCode) {
      (this.editor.storage as unknown as Record<string, unknown>)[ON_RUN_CODE_STORAGE_KEY] =
        this.options.onRunCode;
    }

    // 通知宿主编辑器已就绪（替代 window.__tiptap_ready 轮询）
    this.options.onReady?.();
  }

  getMarkdown(): string {
    // 源码模式下直接返回 textarea 内容，确保提交逻辑无需感知视图模式
    if (this.isSourceMode && this.sourceTextarea) {
      return this.sourceTextarea.value;
    }
    // 往返检查期间保存拿到的必须是原文，而不是尚未验证的序列化结果
    if (this.pendingOriginal !== null) {
      return this.pendingOriginal;
    }
    return this.editor?.getMarkdown() || '';
  }

  /**
   * 在富文本模式与 Markdown 源码模式之间切换。
   * 富文本 → 源码用 getMarkdown 导出；源码 → 富文本先做往返检查，有损则留在源码模式并锁定。
   */
  toggleSource(): void {
    if (!this.editor || !this.sourceTextarea || !this.toggleButton) return;
    if (!this.isSourceMode) {
      this.showSource(this.editor.getMarkdown(), { focus: true, locked: false });
    } else {
      void this.leaveSource();
    }
  }

  /** 往返后的 Markdown 与原文渲染一致才返回 true；检查失败按有损处理。 */
  private async roundtripPreserves(original: string): Promise<boolean> {
    const check = this.options.checkRoundtrip;
    if (!check || !this.editor) return true;
    const roundtrip = this.editor.getMarkdown();
    if (roundtrip === original) return true;
    try {
      return await check(original, roundtrip);
    } catch {
      return false;
    }
  }

  private setLockNotice(visible: boolean): void {
    if (this.lockNotice) this.lockNotice.hidden = !visible;
  }

  /** 富文本 → 源码：把 markdown 放进 textarea 并显示。 */
  private showSource(markdown: string, opts: { focus: boolean; locked: boolean }): void {
    if (!this.editor || !this.sourceTextarea || !this.toggleButton) return;
    // ProseMirror 的实际 DOM 节点（.ProseMirror），用于切换显隐。
    const proseMirrorDom = this.editor.view.dom;
    this.sourceTextarea.value = markdown;
    // 必须在 display:'none' 之前读取滚动比例——隐藏后 scrollTop 会被浏览器归零
    const pmRatio = this.getScrollRatio(proseMirrorDom);
    proseMirrorDom.style.display = 'none';
    this.sourceTextarea.hidden = false;
    this.applyScrollRatio(this.sourceTextarea, pmRatio);
    if (opts.focus) {
      // focus() 会触发浏览器把光标所在行滚动进可视区域，光标默认在内容末尾，
      // 会把 textarea 拉到底部覆盖上面设好的滚动位置。因此先记录、focus 后再恢复。
      const scrollTopBeforeFocus = this.sourceTextarea.scrollTop;
      this.sourceTextarea.focus();
      this.sourceTextarea.scrollTop = scrollTopBeforeFocus;
    }
    this.toggleButton.innerHTML = ARTICLE_ICON_SVG;
    this.toggleButton.title = '切换富文本';
    this.isSourceMode = true;
    this.setLockNotice(opts.locked);
  }

  /** 源码 → 富文本：往返检查通过才切换，否则留在源码模式并显示锁定提示。 */
  private async leaveSource(): Promise<void> {
    const editor = this.editor;
    const textarea = this.sourceTextarea;
    const button = this.toggleButton;
    if (!editor || !textarea || !button) return;

    const md = textarea.value;
    // 先记录源码视图的滚动比例（setContent 会重建文档，必须在替换前拿到比例）
    const sourceRatio = this.getScrollRatio(textarea);
    const seq = ++this.loadSeq;
    this.pendingOriginal = null;
    // ProseMirror 当前隐藏，用户看不到这次载入
    editor.commands.setContent(md, { emitUpdate: false, contentType: 'markdown' });
    button.disabled = true;
    const ok = await this.roundtripPreserves(md);
    button.disabled = false;
    if (seq !== this.loadSeq || !this.editor || textarea.value !== md) return;
    if (!ok) {
      this.setLockNotice(true);
      return;
    }
    this.setLockNotice(false);
    const proseMirrorDom = editor.view.dom;
    textarea.hidden = true;
    proseMirrorDom.style.display = '';
    // 等待 DOM 布局更新后，按比例同步富文本视图滚动位置
    requestAnimationFrame(() => {
      this.applyScrollRatio(proseMirrorDom, sourceRatio);
    });
    button.innerHTML = CODE_ICON_SVG;
    button.title = '切换 Markdown 源码';
    this.isSourceMode = false;
    // 注意：不调用 editor.commands.focus()，它会强制滚动到光标位置（默认文档末尾），破坏比例同步
  }

  /**
   * 读取可滚动容器的滚动比例（0~1）。
   * 比例 = scrollTop / 可滚动总距离，避免两种模式内容高度不同导致像素无法直接对应。
   */
  private getScrollRatio(el: HTMLElement): number {
    const max = el.scrollHeight - el.clientHeight;
    if (max <= 0) return 0;
    return el.scrollTop / max;
  }

  /**
   * 按比例设置目标容器的滚动位置。
   */
  private applyScrollRatio(el: HTMLElement, ratio: number): void {
    const max = el.scrollHeight - el.clientHeight;
    if (max <= 0) return;
    el.scrollTop = max * ratio;
  }

  /**
   * 载入 Markdown 正文。注入 `checkRoundtrip` 时，Promise 在视图模式确定（往返检查结束）后
   * resolve：有损则切到源码模式并锁定，原文不被改写；未注入时同步完成。
   */
  async setMarkdown(content: string): Promise<void> {
    if (!this.editor) return;
    const seq = ++this.loadSeq;
    this.pendingOriginal = null;
    this.editor.commands.setContent(content, { emitUpdate: false, contentType: 'markdown' });
    if (this.isSourceMode && this.sourceTextarea) {
      this.sourceTextarea.value = content;
      return;
    }
    if (!this.options.checkRoundtrip) return;

    this.pendingOriginal = content;
    const ok = await this.roundtripPreserves(content);
    if (seq !== this.loadSeq || !this.editor) return;
    this.pendingOriginal = null;
    if (!ok) {
      this.showSource(content, { focus: false, locked: true });
      this.options.onUpdate?.(content);
    }
  }

  getHTML(): string {
    return this.editor?.getHTML() || '';
  }

  focus(): void {
    this.editor?.commands.focus();
  }

  blur(): void {
    this.editor?.commands.blur();
  }

  isEmpty(): boolean {
    return this.editor?.isEmpty ?? true;
  }

  /** Rust 侧"×关闭"提示时调用（通过 eval）。返回是否成功删除。 */
  removeUploadByUploadId(uploadId: string): boolean {
    return this.coordinator?.removeUpload(uploadId) ?? false;
  }

  /**
   * 工具栏图片按钮入口：把文件交给 coordinator 走占位符上传
   * （与粘贴/拖放/slash 命令完全同一条路径）。无 coordinator（未注入
   * onImageUpload）时 no-op。
   */
  insertUploading(file: File): void {
    this.coordinator?.insertUploading(file);
  }

  /**
   * 从素材库批量插入图片（Rust 侧 AssetPickerModal 确认后调用）。
   *
   * `json` 为 `[{ "src": string, "alt"?: string }]` 串（JSON-over-bridge，
   * 与 overridesJson 同一惯例）。slash 命令触发时已删掉 /素材 文本、光标停在
   * 删除位置；ProseMirror selection 不随弹窗焦点变化，focus() 即恢复原光标，
   * 全部图片块在一次事务内按勾选顺序插入。非法/空载荷为 no-op。
   */
  insertImagesFromLibrary(json: string): void {
    if (!this.editor) return;
    const items = parseLibraryInsertItems(json);
    if (!items) return;
    const content = items.map((item) => ({ type: 'image', attrs: item }));
    this.editor.chain().focus().insertContent(content).run();
  }

  destroy(): void {
    if (!this.onDestroy && !this.editor) return;
    this.editor?.destroy();
    this.editor = null;
    // coordinator 通过 editor.storage 访问，随 editor 实例一同回收，无需显式清除引用。
    this.coordinator = null;
    // 清理源码模式相关引用（容器 innerHTML 已清空，DOM 会随之移除）
    this.sourceTextarea = null;
    this.toggleButton = null;
    this.isSourceMode = false;
    this.lockNotice = null;
    this.pendingOriginal = null;
    this.container.innerHTML = '';
    this.onDestroy?.();
    this.onDestroy = undefined;
  }
}

const TiptapEditor = {
  _instances: new Map<string, TiptapEditorInstance>(),

  create(
    containerId: string,
    options: EditorOptions = new EditorOptions(),
  ): TiptapEditorInstance | null {
    const container = document.getElementById(containerId);
    if (!container) {
      console.error(`[TiptapEditor] Container not found: #${containerId}`);
      return null;
    }

    const existing = this._instances.get(containerId);
    if (existing) {
      existing.destroy();
    }

    let instance!: TiptapEditorInstance;
    instance = new TiptapEditorInstance(container, options, () => {
      if (this._instances.get(containerId) === instance) this._instances.delete(containerId);
    });
    this._instances.set(containerId, instance);
    return instance;
  },
};

export default TiptapEditor;
