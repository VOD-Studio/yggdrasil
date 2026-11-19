import { Editor, type EditorOptions as TiptapOptions } from '@tiptap/core';
import {
  UPLOAD_COORDINATOR_STORAGE_KEY,
  UploadCoordinator,
  type UploadEvent,
} from './upload-coordinator';
import { parseLibraryInsertItems } from './upload-image';

export interface CommonEditorOptions {
  content?: string;
  placeholder?: string;
  onUpdate?: (markdown: string) => void;
  onFocus?: () => void;
  onBlur?: () => void;
  editable?: boolean;
  onImageUpload?: (file: File) => Promise<string>;
  onReady?: () => void;
  onUploadEvent?: (event: UploadEvent) => void;
}

/** 两套编辑器共用富文本实例、图片上传和销毁逻辑。 */
export class EditorInstance<Options extends CommonEditorOptions = CommonEditorOptions> {
  protected editor: Editor | null = null;
  private coordinator: UploadCoordinator | null = null;

  constructor(
    protected readonly container: HTMLElement,
    protected readonly options: Options,
    private onDestroy: (() => void) | undefined,
  ) {}

  protected getCoordinator = () => this.coordinator;

  protected mountEditor(
    element: HTMLElement,
    config: Pick<TiptapOptions, 'extensions'> & Partial<Pick<TiptapOptions, 'editorProps'>>,
  ): Editor {
    const editor = new Editor({
      element,
      ...config,
      content: this.options.content || '',
      editable: this.options.editable !== false,
      autofocus: false,
      onUpdate: ({ editor }) => {
        this.options.onUpdate?.(editor.getMarkdown());
      },
      onFocus: () => this.options.onFocus?.(),
      onBlur: () => this.options.onBlur?.(),
    });
    this.editor = editor;

    if (this.options.onImageUpload) {
      this.coordinator = new UploadCoordinator(
        editor,
        this.options.onImageUpload,
        this.options.onUploadEvent ?? (() => {}),
      );
      (editor.storage as unknown as Record<string, unknown>)[UPLOAD_COORDINATOR_STORAGE_KEY] =
        this.coordinator;
    }
    return editor;
  }

  getMarkdown(): string {
    return this.editor?.getMarkdown() || '';
  }

  async setMarkdown(content: string): Promise<void> {
    this.editor?.commands.setContent(content, { emitUpdate: false, contentType: 'markdown' });
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

  /** 关闭上传失败提示，同时清理编辑器内对应的占位节点。 */
  removeUploadByUploadId(uploadId: string): boolean {
    return this.coordinator?.removeUpload(uploadId) ?? false;
  }

  /** 工具栏、粘贴和拖放共用图片上传协调器。 */
  insertUploading(file: File): void {
    this.coordinator?.insertUploading(file);
  }

  /** 在当前选区批量插入素材库选中的图片。 */
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
    this.coordinator = null;
    this.container.innerHTML = '';
    this.onDestroy?.();
    this.onDestroy = undefined;
  }
}

/** 同一容器只保留一个编辑器实例。 */
export function editorLibrary<Options, Instance extends { destroy(): void }>(
  EditorClass: new (
    container: HTMLElement,
    options: Options | undefined,
    onDestroy: () => void,
  ) => Instance,
) {
  const instances = new Map<HTMLElement, Instance>();
  return {
    _instances: instances,

    create(container: HTMLElement, options?: Options): Instance {
      instances.get(container)?.destroy();
      let instance!: Instance;
      instance = new EditorClass(container, options, () => {
        if (instances.get(container) === instance) instances.delete(container);
      });
      instances.set(container, instance);
      return instance;
    },
  };
}
