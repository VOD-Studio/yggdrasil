import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { UploadCoordinator, type UploadEvent } from './upload-coordinator';

/**
 * Coordinator 单元测试。
 *
 * 用 plain-object mock editor 覆盖：
 * - 计数正确性（insert / 成功 / 失败 / retry / destroy）
 * - blob URL 的 createObjectURL / revokeObjectURL 配对
 * - emit 事件载荷（kind / counts / errorMsg）
 * - 幂等性（removeUpload 后 handleNodeDestroyed 不二次减计数）
 *
 * 不依赖真实 Tiptap Editor / ProseMirror schema —— coordinator 对 editor 的访问
 * 全是字符串 key（node.type.name / node.attrs[...]），用 plain object 即可精确 stub。
 */

// ---- fake 节点：模拟 ProseMirror Node 的最小形状 ----
interface FakeNode {
  type: { name: string };
  attrs: Record<string, unknown>;
  nodeSize: number;
}

/**
 * 构造 mock editor。
 * nodes 数组模拟文档里的 image 节点，供 descendants / findImageNodeByUploadId 遍历。
 * insertUploading 会通过 chain.insertContentAt 把新节点 push 进来（测试里手动驱动）。
 */
function makeMockEditor(initialNodes: FakeNode[] = []) {
  const nodes: FakeNode[] = [...initialNodes];
  const dispatched: unknown[] = [];

  // setNodeMarkup / delete 调用后，把 attrs 变更同步回 nodes（让后续 descendants 反映新状态）
  const applyMarkup = (pos: number, attrs: Record<string, unknown>) => {
    if (nodes[pos]) nodes[pos] = { ...nodes[pos], attrs: { ...nodes[pos].attrs, ...attrs } };
  };

  const editor = {
    state: {
      selection: { head: 0 },
      doc: {
        descendants(cb: (node: FakeNode, pos: number) => boolean | undefined) {
          for (let i = 0; i < nodes.length; i++) {
            const descend = cb(nodes[i], i);
            if (descend === false) break;
          }
        },
      },
      tr: {
        delete(from: number, to: number) {
          const marker = { op: 'delete', from, to };
          nodes.splice(from, to - from);
          return marker;
        },
        setNodeMarkup(pos: number, _type: unknown, attrs: Record<string, unknown>) {
          applyMarkup(pos, attrs);
          return { op: 'setNodeMarkup', pos, attrs };
        },
      },
    },
    view: {
      dispatch(tr: unknown) {
        dispatched.push(tr);
      },
    },
    chain() {
      const chain = {
        _inserted: null as {
          pos: number;
          content: { type: string; attrs: Record<string, unknown> };
        } | null,
        focus() {
          return chain;
        },
        insertContentAt(pos: number, content: { type: string; attrs: Record<string, unknown> }) {
          // 把插入的节点追加到 nodes，让后续 descendants 能找到它
          nodes.push({ type: { name: content.type }, nodeSize: 1, attrs: content.attrs });
          chain._inserted = { pos, content };
          return chain;
        },
        run() {},
      };
      return chain;
    },
    _nodes: nodes,
    _dispatched: dispatched,
  };

  return editor;
}

function makeFile(name = 'test.png'): File {
  return new File(['fake-bytes'], name, { type: 'image/png' });
}

describe('UploadCoordinator', () => {
  let createObjectURLSpy: ReturnType<typeof vi.spyOn>;
  let revokeObjectURLSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    createObjectURLSpy = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:mock-url');
    revokeObjectURLSpy = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
    vi.spyOn(Date, 'now').mockReturnValue(1700000000000);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('insert 后 uploading 计数 +1，pending 存在，节点插入文档', () => {
    const editor = makeMockEditor();
    const upload = vi.fn().mockReturnValue(new Promise(() => {})); // 永不 resolve，停在 uploading
    const emit = vi.fn();
    const coord = new UploadCoordinator(editor as never, upload, emit);

    coord.insertUploading(makeFile());

    // insertUploading 本身不 emit（runUpload 还在 await）；counts 只能通过后续 emit 观察
    expect(emit).not.toHaveBeenCalled();
    expect(createObjectURLSpy).toHaveBeenCalledTimes(1);
    // 节点已插入文档，处于 uploading 态
    const inserted = (editor as never as { _nodes: FakeNode[] })._nodes;
    expect(inserted).toHaveLength(1);
    expect(inserted[0].attrs['data-upload-state']).toBe('uploading');
    expect(inserted[0].attrs['data-upload-id']).not.toBeNull();
    expect(upload).toHaveBeenCalledTimes(1);
    // pending 有一条（runUpload 未完成）
    expect(coord.hasPending(inserted[0].attrs['data-upload-id'] as string)).toBe(true);
  });

  it('上传成功：counts 归 0，emit success，blob 被 revoke，pending 清除', async () => {
    const editor = makeMockEditor();
    const upload = vi.fn().mockResolvedValue('https://cdn.example.com/final.png');
    const emit = vi.fn();
    const coord = new UploadCoordinator(editor as never, upload, emit);

    coord.insertUploading(makeFile());
    // runUpload 是 fire-and-forget，await 一次微任务让它完成
    await Promise.resolve();

    expect(emit).toHaveBeenCalledTimes(1);
    const ev = emit.mock.calls[0][0] as UploadEvent;
    expect(ev.kind).toBe('success');
    expect(ev.fileName).toBe('test.png');
    expect(ev.counts).toEqual({ uploading: 0, error: 0 });

    expect(revokeObjectURLSpy).toHaveBeenCalledTimes(1);
    // 成功后节点 src 应更新为最终 URL，upload 属性被清空
    const node = (editor as never as { _nodes: FakeNode[] })._nodes[0];
    expect(node.attrs.src).toBe('https://cdn.example.com/final.png');
    expect(node.attrs['data-upload-id']).toBeNull();
    // pending 已清
    expect(coord.hasPending(ev.uploadId)).toBe(false);
  });

  it('上传失败：counts 转 error=1，emit error 带 errorMsg，entry 转 error 态', async () => {
    const editor = makeMockEditor();
    const upload = vi.fn().mockRejectedValue(new Error('文件超过大小限制'));
    const emit = vi.fn();
    const coord = new UploadCoordinator(editor as never, upload, emit);

    coord.insertUploading(makeFile());
    await Promise.resolve();

    const ev = emit.mock.calls[0][0] as UploadEvent;
    expect(ev.kind).toBe('error');
    expect(ev.errorMsg).toBe('文件超过大小限制');
    expect(ev.counts).toEqual({ uploading: 0, error: 1 });

    // 节点转 error 态
    const node = (editor as never as { _nodes: FakeNode[] })._nodes[0];
    expect(node.attrs['data-upload-state']).toBe('error');
    expect(node.attrs['data-error-msg']).toBe('文件超过大小限制');
    // pending 仍在（失败保留供重试）
    expect(coord.hasPending(ev.uploadId)).toBe(true);
  });

  it('重试：error 转 uploading，再次成功后归 0', async () => {
    const editor = makeMockEditor();
    // 第一次失败，第二次成功
    const upload = vi
      .fn()
      .mockRejectedValueOnce(new Error('网络错误'))
      .mockResolvedValueOnce('https://cdn.example.com/retry.png');
    const emit = vi.fn();
    const coord = new UploadCoordinator(editor as never, upload, emit);

    coord.insertUploading(makeFile());
    await Promise.resolve(); // 第一次：失败
    const uploadId = (emit.mock.calls[0][0] as UploadEvent).uploadId;
    expect((emit.mock.calls[0][0] as UploadEvent).counts).toEqual({ uploading: 0, error: 1 });

    coord.retryUpload(uploadId);
    // retry 同步调 updateNodeAttrs（不 emit），然后 fire-and-forget runUpload
    expect(upload).toHaveBeenCalledTimes(2);
    await Promise.resolve(); // 第二次：成功

    const successEv = emit.mock.calls[emit.mock.calls.length - 1][0] as UploadEvent;
    expect(successEv.kind).toBe('success');
    expect(successEv.counts).toEqual({ uploading: 0, error: 0 });
    expect(coord.hasPending(uploadId)).toBe(false);
  });

  it('removeUpload（点按钮）：按 state 减计数，emit removed，返回 true/false', async () => {
    const editor = makeMockEditor();
    const upload = vi.fn().mockRejectedValue(new Error('fail')); // 让它停在 error 态
    const emit = vi.fn();
    const coord = new UploadCoordinator(editor as never, upload, emit);

    coord.insertUploading(makeFile());
    await Promise.resolve(); // 失败，error=1
    const uploadId = (emit.mock.calls[0][0] as UploadEvent).uploadId;

    const result = coord.removeUpload(uploadId);
    expect(result).toBe(true);

    const removedEv = emit.mock.calls[emit.mock.calls.length - 1][0] as UploadEvent;
    expect(removedEv.kind).toBe('removed');
    expect(removedEv.counts).toEqual({ uploading: 0, error: 0 });
    expect(revokeObjectURLSpy).toHaveBeenCalled();

    // 不存在的 id 返回 false
    expect(coord.removeUpload('nonexistent')).toBe(false);
  });

  it('handleNodeDestroyed（退格）：revoke blob + 减计数 + emit removed', async () => {
    const editor = makeMockEditor();
    const upload = vi.fn().mockRejectedValue(new Error('fail'));
    const emit = vi.fn();
    const coord = new UploadCoordinator(editor as never, upload, emit);

    coord.insertUploading(makeFile());
    await Promise.resolve(); // 失败，error=1
    const uploadId = (emit.mock.calls[0][0] as UploadEvent).uploadId;

    coord.handleNodeDestroyed(uploadId);

    const ev = emit.mock.calls[emit.mock.calls.length - 1][0] as UploadEvent;
    expect(ev.kind).toBe('removed');
    expect(ev.counts).toEqual({ uploading: 0, error: 0 });
    expect(revokeObjectURLSpy).toHaveBeenCalled();
  });

  it('handleNodeDestroyed 对已成功的 entry 是 no-op', async () => {
    const editor = makeMockEditor();
    const upload = vi.fn().mockResolvedValue('https://cdn.example.com/x.png');
    const emit = vi.fn();
    const coord = new UploadCoordinator(editor as never, upload, emit);

    coord.insertUploading(makeFile());
    await Promise.resolve(); // 成功，pending 已清
    const uploadId = (emit.mock.calls[0][0] as UploadEvent).uploadId;

    const emitCountBefore = emit.mock.calls.length;
    coord.handleNodeDestroyed(uploadId);
    // 成功的 entry 不在 pending，no-op：不 emit，不减计数
    expect(emit.mock.calls.length).toBe(emitCountBefore);
  });

  it('幂等：removeUpload 后再 handleNodeDestroyed 同 id 不二次减计数', async () => {
    const editor = makeMockEditor();
    const upload = vi.fn().mockRejectedValue(new Error('fail'));
    const emit = vi.fn();
    const coord = new UploadCoordinator(editor as never, upload, emit);

    coord.insertUploading(makeFile());
    await Promise.resolve();
    const uploadId = (emit.mock.calls[0][0] as UploadEvent).uploadId;

    coord.removeUpload(uploadId);
    expect((emit.mock.calls[emit.mock.calls.length - 1][0] as UploadEvent).counts).toEqual({
      uploading: 0,
      error: 0,
    });

    const callsBefore = emit.mock.calls.length;
    coord.handleNodeDestroyed(uploadId); // pending 已删，no-op
    expect(emit.mock.calls.length).toBe(callsBefore);
  });

  it('多图并发：计数随每张的状态独立维护', async () => {
    const editor = makeMockEditor();
    // 三张图：一张立即成功，一张失败，一张永远 uploading
    const upload = vi
      .fn()
      .mockResolvedValueOnce('https://cdn.example.com/1.png')
      .mockRejectedValueOnce(new Error('fail'))
      .mockReturnValueOnce(new Promise(() => {}));
    const emit = vi.fn();
    const coord = new UploadCoordinator(editor as never, upload, emit);

    coord.insertUploading(makeFile('1.png'));
    coord.insertUploading(makeFile('2.png'));
    coord.insertUploading(makeFile('3.png'));
    await Promise.resolve(); // 让前两张 settle（第三张永不 resolve）

    // 最终态：uploading=1（第三张）, error=1（第二张）
    const lastEv = emit.mock.calls[emit.mock.calls.length - 1][0] as UploadEvent;
    expect(lastEv.counts).toEqual({ uploading: 1, error: 1 });
  });
});
