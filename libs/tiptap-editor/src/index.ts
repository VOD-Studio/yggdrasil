import {
  ON_RENDER_MERMAID_STORAGE_KEY,
  ON_RUN_CODE_STORAGE_KEY,
  type RunCodeOpts,
} from './code-block-view';
import { buildCommentExtensions, commentEditorProps } from './comment-extensions';
import { buildExtensions } from './editor-extensions';
import { type CommonEditorOptions, EditorInstance, editorLibrary } from './editor-instance';
import { icon } from './icons';
import './style.css';
import './author-style.css';
import './code-highlight.css';

/** 后台编辑器的配置，由 Rust 端创建配置对象后设置。 */
export interface EditorOptions extends CommonEditorOptions {
  /** 兼容发布前已经打开的页面，新评论表单使用 TiptapCommentEditor。 */
  variant?: 'full' | 'comment';
  onPickFromLibrary?: () => void;
  onRunCode?: (opts: RunCodeOpts) => Promise<string>;
  onRenderMermaid?: (source: string) => Promise<string>;
  /** 载入与切回富文本时，检查往返后的 Markdown 是否保留渲染结果。 */
  checkRoundtrip?: (original: string, roundtrip: string) => Promise<boolean>;
}

const CODE_ICON_SVG = icon('file-code', 18);
const ARTICLE_ICON_SVG = icon('file-text', 18);
const SOURCE_LOCK_MESSAGE =
  '这篇内容里有富文本模式无法原样保留的写法，已切换到 Markdown 源码编辑。';

class TiptapEditorInstance extends EditorInstance<EditorOptions> {
  private isSourceMode = false;
  private sourceTextarea: HTMLTextAreaElement | null = null;
  private toggleButton: HTMLButtonElement | null = null;
  private pendingOriginal: string | null = null;
  private loadSeq = 0;
  private lockNotice: HTMLParagraphElement | null = null;

  constructor(container: HTMLElement, options: EditorOptions = {}, onDestroy?: () => void) {
    super(container, options, onDestroy);
    this.init();
  }

  private init() {
    const el = document.createElement('div');
    this.container.appendChild(el);

    if (this.options.variant === 'comment') {
      el.className = 'tiptap-editor tiptap-editor--comment';
      this.mountEditor(el, {
        extensions: buildCommentExtensions(this.getCoordinator, this.options.placeholder),
        editorProps: commentEditorProps,
      });
      this.options.onReady?.();
      return;
    }

    el.className = 'tiptap-editor';
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

    const editor = this.mountEditor(el, {
      extensions: buildExtensions({
        onImageUpload: this.options.onImageUpload,
        onInsertUploading: this.options.onImageUpload
          ? (file) => this.insertUploading(file)
          : undefined,
        onPickFromLibrary: this.options.onPickFromLibrary,
        getCoordinator: this.getCoordinator,
      }),
    });
    const storage = editor.storage as unknown as Record<string, unknown>;
    if (this.options.onRunCode) storage[ON_RUN_CODE_STORAGE_KEY] = this.options.onRunCode;
    if (this.options.onRenderMermaid) {
      storage[ON_RENDER_MERMAID_STORAGE_KEY] = this.options.onRenderMermaid;
    }
    this.options.onReady?.();
  }

  override getMarkdown(): string {
    // 源码模式下直接返回 textarea 内容，确保提交逻辑无需感知视图模式
    if (this.isSourceMode && this.sourceTextarea) {
      return this.sourceTextarea.value;
    }
    // 往返检查期间保存拿到的必须是原文，而不是尚未验证的序列化结果
    if (this.pendingOriginal !== null) {
      return this.pendingOriginal;
    }
    return super.getMarkdown();
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
  override async setMarkdown(content: string): Promise<void> {
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

  override destroy(): void {
    super.destroy();
    this.sourceTextarea = null;
    this.toggleButton = null;
    this.isSourceMode = false;
    this.lockNotice = null;
    this.pendingOriginal = null;
  }
}

export default editorLibrary(TiptapEditorInstance);
