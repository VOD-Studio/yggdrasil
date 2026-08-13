import { describe, expect, it, vi } from 'vitest';
import {
  parseLibraryInsertItems,
  UploadImageNodeView,
  type UploadNodeViewCallbacks,
} from '../upload-image';

/**
 * UploadImageNodeView 单元测试（happy-dom 真实 DOM）。
 *
 * 构造 fake PMNode（plain object）+ 真 happy-dom document，
 * 断言三种态（uploading/error/null）的渲染 DOM、按钮 click 触发回调、destroy 调 onDestroyed。
 *
 * NodeView 不依赖 editor —— 它只读 node.attrs + 操作 DOM，callbacks 由调用方注入。
 */

// 共用的 type sentinel：update() 用 === 比较 node.type，新旧 node 共用同一引用
const IMAGE_TYPE = { name: 'image' };

function makeNode(attrs: {
  src?: string;
  'data-upload-state'?: 'uploading' | 'error' | null;
  'data-upload-id'?: string | null;
  'data-error-msg'?: string | null;
}) {
  return {
    type: IMAGE_TYPE,
    attrs: {
      src: attrs.src ?? 'https://example.com/img.png',
      // 用 in 判断而非 ??，让显式传入的 null 生效（?? 会把 null 当缺失）
      'data-upload-state': 'data-upload-state' in attrs ? attrs['data-upload-state']! : null,
      'data-upload-id': 'data-upload-id' in attrs ? attrs['data-upload-id']! : 'upload-123',
      'data-error-msg': 'data-error-msg' in attrs ? attrs['data-error-msg']! : null,
    },
  } as never; // PMNode 类型注解是编译期的，运行时只需这个形状
}

function makeCallbacks(): UploadNodeViewCallbacks {
  return {
    onRetry: vi.fn<(id: string) => void>(),
    onRemove: vi.fn<(id: string) => void>(),
    onDestroyed: vi.fn<(id: string) => void>(),
  };
}

describe('UploadImageNodeView', () => {
  describe('rendering by state', () => {
    it('uploading 态：渲染 spinner + "上传中…"，容器有 is-uploading', () => {
      const view = new UploadImageNodeView({
        node: makeNode({ 'data-upload-state': 'uploading', 'data-upload-id': 'u1' }),
        HTMLAttributes: {},
        callbacks: makeCallbacks(),
      });

      const dom = view.dom;
      expect(dom.classList.contains('is-uploading')).toBe(true);
      expect(dom.classList.contains('is-error')).toBe(false);
      expect(dom.querySelector('.upload-spinner')).not.toBeNull();
      expect(dom.querySelector('.upload-overlay-text')?.textContent).toBe('上传中…');
    });

    it('error 态：渲染错误图标 + msg + 重试/移除按钮，容器有 is-error', () => {
      const view = new UploadImageNodeView({
        node: makeNode({
          'data-upload-state': 'error',
          'data-upload-id': 'u2',
          'data-error-msg': '文件超过大小限制',
        }),
        HTMLAttributes: {},
        callbacks: makeCallbacks(),
      });

      const dom = view.dom;
      expect(dom.classList.contains('is-error')).toBe(true);
      expect(dom.classList.contains('is-uploading')).toBe(false);
      expect(dom.querySelector('.upload-error-icon')).not.toBeNull();
      expect(dom.querySelector('.upload-error-msg')?.textContent).toBe('文件超过大小限制');
      expect(dom.querySelector('.upload-btn-retry')).not.toBeNull();
      expect(dom.querySelector('.upload-btn-remove')).not.toBeNull();
    });

    it('error 态缺 msg：fallback 显示"上传失败"', () => {
      const view = new UploadImageNodeView({
        node: makeNode({
          'data-upload-state': 'error',
          'data-upload-id': 'u3',
          'data-error-msg': null,
        }),
        HTMLAttributes: {},
        callbacks: makeCallbacks(),
      });

      expect(view.dom.querySelector('.upload-error-msg')?.textContent).toBe('上传失败');
    });

    it('null 态（普通图）：无 overlay，无 is-uploading/is-error class', () => {
      const view = new UploadImageNodeView({
        node: makeNode({ 'data-upload-state': null }),
        HTMLAttributes: {},
        callbacks: makeCallbacks(),
      });

      const dom = view.dom;
      expect(dom.querySelector('.upload-image-overlay')).toBeNull();
      expect(dom.classList.contains('is-uploading')).toBe(false);
      expect(dom.classList.contains('is-error')).toBe(false);
    });
  });

  describe('update()', () => {
    it('切换态（uploading → error）：overlay 重新渲染', () => {
      const view = new UploadImageNodeView({
        node: makeNode({ 'data-upload-state': 'uploading', 'data-upload-id': 'u4' }),
        HTMLAttributes: {},
        callbacks: makeCallbacks(),
      });
      expect(view.dom.querySelector('.upload-spinner')).not.toBeNull();

      const ok = view.update(
        makeNode({
          'data-upload-state': 'error',
          'data-upload-id': 'u4',
          'data-error-msg': '失败',
        }) as never,
      );

      expect(ok).toBe(true);
      expect(view.dom.querySelector('.upload-spinner')).toBeNull();
      expect(view.dom.querySelector('.upload-error-icon')).not.toBeNull();
      expect(view.dom.classList.contains('is-uploading')).toBe(false);
      expect(view.dom.classList.contains('is-error')).toBe(true);
    });

    it('拒绝非同类 node：返回 false', () => {
      const view = new UploadImageNodeView({
        node: makeNode({ 'data-upload-state': null }),
        HTMLAttributes: {},
        callbacks: makeCallbacks(),
      });

      const ok = view.update({ type: { name: 'paragraph' }, attrs: {} } as never);
      expect(ok).toBe(false);
    });

    it('src 变化时更新 img.src', () => {
      const view = new UploadImageNodeView({
        node: makeNode({ src: 'old.png', 'data-upload-state': null }),
        HTMLAttributes: {},
        callbacks: makeCallbacks(),
      });
      expect(view.dom.querySelector('img')?.getAttribute('src')).toBe('old.png');

      view.update(makeNode({ src: 'new.png', 'data-upload-state': null }) as never);
      expect(view.dom.querySelector('img')?.getAttribute('src')).toBe('new.png');
    });
  });

  describe('button callbacks', () => {
    it('点击"重试"按钮触发 onRetry(uploadId)', () => {
      const callbacks = makeCallbacks();
      const view = new UploadImageNodeView({
        node: makeNode({
          'data-upload-state': 'error',
          'data-upload-id': 'retry-me',
          'data-error-msg': 'x',
        }),
        HTMLAttributes: {},
        callbacks,
      });

      view.dom
        .querySelector('.upload-btn-retry')
        ?.dispatchEvent(new Event('click', { bubbles: true }));
      expect(callbacks.onRetry).toHaveBeenCalledWith('retry-me');
    });

    it('点击"移除"按钮触发 onRemove(uploadId)', () => {
      const callbacks = makeCallbacks();
      const view = new UploadImageNodeView({
        node: makeNode({
          'data-upload-state': 'error',
          'data-upload-id': 'remove-me',
          'data-error-msg': 'x',
        }),
        HTMLAttributes: {},
        callbacks,
      });

      view.dom
        .querySelector('.upload-btn-remove')
        ?.dispatchEvent(new Event('click', { bubbles: true }));
      expect(callbacks.onRemove).toHaveBeenCalledWith('remove-me');
    });
  });

  describe('destroy()', () => {
    it('调用 onDestroyed(uploadId) —— 兜底清理 blob 的接线点', () => {
      const callbacks = makeCallbacks();
      const view = new UploadImageNodeView({
        node: makeNode({
          'data-upload-state': 'error',
          'data-upload-id': 'destroy-me',
          'data-error-msg': 'x',
        }),
        HTMLAttributes: {},
        callbacks,
      });

      view.destroy();
      expect(callbacks.onDestroyed).toHaveBeenCalledWith('destroy-me');
    });

    it('uploadId 为 null 时 destroy 不调 onDestroyed（普通图无上传状态）', () => {
      const callbacks = makeCallbacks();
      const view = new UploadImageNodeView({
        node: makeNode({ 'data-upload-state': null, 'data-upload-id': null }),
        HTMLAttributes: {},
        callbacks,
      });

      view.destroy();
      expect(callbacks.onDestroyed).not.toHaveBeenCalled();
    });
  });
});

/**
 * parseLibraryInsertItems 纯函数测试。
 *
 * 桥接契约：Rust 侧 AssetSelection 经 serde_json 序列化为 [{ "src", "alt?" }]，
 * 字段名漂移或载荷非法时插入必须静默 no-op（返回 null），不得抛异常进编辑器。
 */
describe('parseLibraryInsertItems', () => {
  it('完整载荷：多张、带 alt，保持顺序', () => {
    const json = '[{"src":"/uploads/a.webp","alt":"封面"},{"src":"/uploads/b.webp","alt":"插图"}]';
    expect(parseLibraryInsertItems(json)).toEqual([
      { src: '/uploads/a.webp', alt: '封面' },
      { src: '/uploads/b.webp', alt: '插图' },
    ]);
  });

  it('alt 缺省 / 空串 / 非字符串：统一丢弃', () => {
    const json = '[{"src":"/a.webp"},{"src":"/b.webp","alt":""},{"src":"/c.webp","alt":123}]';
    expect(parseLibraryInsertItems(json)).toEqual([
      { src: '/a.webp' },
      { src: '/b.webp' },
      { src: '/c.webp' },
    ]);
  });

  it('src 缺失或为空串的条目被过滤，其余保留', () => {
    const json = '[{"alt":"无 src"},{"src":""},{"src":"/ok.webp"},null,42]';
    expect(parseLibraryInsertItems(json)).toEqual([{ src: '/ok.webp' }]);
  });

  it.each([
    ['非 JSON', 'not json'],
    ['对象而非数组', '{"src":"/a.webp"}'],
    ['空数组', '[]'],
    ['全部条目无效', '[{"alt":"x"},null]'],
  ])('%s：返回 null（调用方 no-op）', (_label, json) => {
    expect(parseLibraryInsertItems(json)).toBeNull();
  });
});
