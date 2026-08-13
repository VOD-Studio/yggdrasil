import { Image } from '@tiptap/extension-image';
import type { Node as PMNode } from '@tiptap/pm/model';
import { UPLOAD_COORDINATOR_STORAGE_KEY, type UploadCoordinator } from './upload-coordinator';

/** NodeView 按钮点击 / destroy 回调注入接口。 */
export interface UploadNodeViewCallbacks {
  onRetry: (uploadId: string) => void;
  onRemove: (uploadId: string) => void;
  /** NodeView.destroy 兜底：节点被 PM 删除时清理 pending + revoke blob。 */
  onDestroyed: (uploadId: string) => void;
}

/**
 * 上传图片占位符 NodeView：plain class，实现 ProseMirror NodeView 接口。
 *
 * 根据 data-upload-state 渲染三种态：
 * - null：普通 img
 * - uploading：img（本地 blob 预览）+ 遮罩（spinner + "上传中…"）
 * - error：img（灰化）+ 遮罩（⚠ + 错误文案 + 重试/移除按钮）
 *
 * NodeView 纯渲染，不发起上传——按钮点击转发给注入的 callbacks（实际是 coordinator）。
 * 属性变化时 ProseMirror 调 update(node)，NodeView 比较新旧 data-upload-state 重渲染遮罩。
 */
export class UploadImageNodeView {
  private node: PMNode;
  private callbacks: UploadNodeViewCallbacks;
  private uploadId: string | null;

  private container: HTMLDivElement;
  private img: HTMLImageElement;
  private overlay: HTMLDivElement | null = null;

  constructor(opts: {
    node: PMNode;
    HTMLAttributes: Record<string, unknown>;
    callbacks: UploadNodeViewCallbacks;
  }) {
    this.node = opts.node;
    this.callbacks = opts.callbacks;
    this.uploadId = (this.node.attrs['data-upload-id'] as string | null) ?? null;

    this.container = document.createElement('div');
    this.container.classList.add('upload-image-container');

    this.img = document.createElement('img');
    this.img.draggable = false;
    Object.entries(opts.HTMLAttributes).forEach(([k, v]) => {
      if (v != null) this.img.setAttribute(k, String(v));
    });
    const src = this.node.attrs.src;
    if (src != null) this.img.src = src;
    this.container.appendChild(this.img);

    this.renderOverlay();
  }

  get dom(): HTMLElement {
    return this.container;
  }

  get contentDOM(): HTMLElement | null {
    return null;
  }

  /** ProseMirror 调用：节点属性变化时重渲染遮罩。返回 false 拒绝非同类节点。 */
  update(node: PMNode): boolean {
    if (node.type !== this.node.type) return false;
    const oldState = this.node.attrs['data-upload-state'];
    const newState = node.attrs['data-upload-state'];
    const oldSrc = this.node.attrs.src;
    const newSrc = node.attrs.src;
    this.node = node;
    this.uploadId = (node.attrs['data-upload-id'] as string | null) ?? null;
    if (oldSrc !== newSrc && newSrc != null) {
      this.img.src = newSrc;
    }
    if (oldState !== newState || this.overlay === null) {
      this.renderOverlay();
    }
    return true;
  }

  /** 遮罩内按钮点击不被 ProseMirror 当编辑，避免误触发事务。 */
  ignoreMutation(): boolean {
    return true;
  }

  /** 事件不被编辑器 stopEvent 拦截（按钮点击要响应）。 */
  stopEvent(_event: Event): boolean {
    return false;
  }

  /** 根据 data-upload-state 渲染遮罩。null 时移除遮罩。 */
  private renderOverlay(): void {
    if (this.overlay) {
      this.overlay.remove();
      this.overlay = null;
    }
    const state = this.node.attrs['data-upload-state'];
    if (state == null) {
      this.container.classList.remove('is-uploading', 'is-error');
      return;
    }
    this.overlay = document.createElement('div');
    this.overlay.classList.add('upload-image-overlay');
    if (state === 'uploading') {
      this.container.classList.add('is-uploading');
      this.container.classList.remove('is-error');
      this.overlay.innerHTML =
        '<div class="upload-spinner"></div><div class="upload-overlay-text">上传中…</div>';
    } else if (state === 'error') {
      this.container.classList.add('is-error');
      this.container.classList.remove('is-uploading');
      const msg = this.node.attrs['data-error-msg'] || '上传失败';
      this.overlay.innerHTML =
        '<div class="upload-error-icon">⚠</div>' +
        '<div class="upload-error-msg"></div>' +
        '<div class="upload-error-actions">' +
        '<button type="button" class="upload-btn upload-btn-retry">重试</button>' +
        '<button type="button" class="upload-btn upload-btn-remove">移除</button>' +
        '</div>';
      const msgEl = this.overlay.querySelector('.upload-error-msg') as HTMLElement;
      msgEl.textContent = msg;
      const uploadId = this.node.attrs['data-upload-id'] as string | null;
      this.overlay.querySelector('.upload-btn-retry')?.addEventListener('click', (e) => {
        e.preventDefault();
        if (uploadId) this.callbacks.onRetry(uploadId);
      });
      this.overlay.querySelector('.upload-btn-remove')?.addEventListener('click', (e) => {
        e.preventDefault();
        if (uploadId) this.callbacks.onRemove(uploadId);
      });
    }
    this.container.appendChild(this.overlay);
  }

  destroy(): void {
    // 节点被 ProseMirror 删除（退格/剪切等）：兜底清理 pending + revoke blob，
    // 避免 upload-coordinator 的 pending Map 和 blob URL 泄漏（尤其 error 态节点）。
    if (this.uploadId) {
      this.callbacks.onDestroyed(this.uploadId);
    }
    this.overlay?.remove();
    this.overlay = null;
    this.container.remove();
  }
}

/**
 * 自定义 Image 扩展：继承父类属性，加三个上传状态属性，用自定义 NodeView。
 */
export const UploadImage = Image.configure({ allowBase64: true }).extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      'data-upload-state': {
        default: null,
        parseHTML: (el) => (el as HTMLElement).getAttribute('data-upload-state'),
        renderHTML: (attrs) => {
          const v = attrs['data-upload-state'];
          return v == null ? {} : { 'data-upload-state': v };
        },
      },
      'data-upload-id': {
        default: null,
        parseHTML: (el) => (el as HTMLElement).getAttribute('data-upload-id'),
        renderHTML: (attrs) => {
          const v = attrs['data-upload-id'];
          return v == null ? {} : { 'data-upload-id': v };
        },
      },
      'data-error-msg': {
        default: null,
        parseHTML: (el) => (el as HTMLElement).getAttribute('data-error-msg'),
        renderHTML: (attrs) => {
          const v = attrs['data-error-msg'];
          return v == null ? {} : { 'data-error-msg': v };
        },
      },
    };
  },

  addNodeView() {
    return ({ node, HTMLAttributes, editor }) => {
      const coordinator = (editor.storage as unknown as Record<string, unknown>)[
        UPLOAD_COORDINATOR_STORAGE_KEY
      ] as UploadCoordinator | undefined;
      return new UploadImageNodeView({
        node,
        HTMLAttributes,
        callbacks: {
          onRetry: (id) => coordinator?.retryUpload(id),
          onRemove: (id) => coordinator?.removeUpload(id),
          onDestroyed: (id) => coordinator?.handleNodeDestroyed(id),
        },
      });
    };
  },
});

/** 素材库插入载荷条目（Rust 侧 AssetSelection 的 JSON 形状）。 */
export interface LibraryInsertItem {
  src: string;
  alt?: string;
}

/**
 * 解析素材库插入载荷（JSON-over-bridge，与 overridesJson 同一惯例）。
 *
 * 桥接另一侧是 Rust 的 serde_json：字段名拼写漂移会让插入静默失败，
 * 因此这里宽松解析——非法 JSON / 非数组 / 无有效条目一律返回 null（调用方 no-op），
 * 条目必须有非空 src；alt 非字符串或空串时丢弃（等同未提供）。
 */
export function parseLibraryInsertItems(json: string): LibraryInsertItem[] | null {
  let items: unknown;
  try {
    items = JSON.parse(json);
  } catch {
    return null;
  }
  if (!Array.isArray(items)) return null;
  const parsed: LibraryInsertItem[] = [];
  for (const item of items) {
    if (typeof item !== 'object' || item === null) continue;
    const raw = item as { src?: unknown; alt?: unknown };
    if (typeof raw.src !== 'string' || raw.src.length === 0) continue;
    const alt = typeof raw.alt === 'string' && raw.alt.length > 0 ? raw.alt : undefined;
    parsed.push(alt ? { src: raw.src, alt } : { src: raw.src });
  }
  return parsed.length > 0 ? parsed : null;
}
