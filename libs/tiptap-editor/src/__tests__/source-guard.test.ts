// @vitest-environment happy-dom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import TiptapEditor from '../index';

/**
 * 往返守卫：宿主注入 checkRoundtrip 后，载入正文/源码切回富文本时有损内容必须
 * 锁定在源码模式，且保存拿到的始终是原文。
 */

type Options = NonNullable<Parameters<typeof TiptapEditor.create>[1]>;
type Instance = NonNullable<ReturnType<typeof TiptapEditor.create>>;

const LOSSY = 'H ~2~ O';
let inst: Instance | null = null;

function mount(checkRoundtrip?: (a: string, b: string) => Promise<boolean>): Instance {
  const el = document.createElement('div');
  el.id = 'guard-host';
  document.body.appendChild(el);
  const created = TiptapEditor.create('guard-host', { checkRoundtrip } as unknown as Options);
  if (!created) throw new Error('编辑器未挂载');
  inst = created;
  return created;
}

const lock = () => document.querySelector<HTMLElement>('.tiptap-source-lock');
const textarea = () => document.querySelector<HTMLTextAreaElement>('.tiptap-source-textarea');

beforeEach(() => {
  document.body.innerHTML = '';
});

afterEach(() => {
  inst?.destroy();
  inst = null;
});

describe('源码锁定守卫', () => {
  it('服务端判定有损：切到源码并锁定，getMarkdown 返回原文', async () => {
    const check = vi.fn().mockResolvedValue(false);
    const editor = mount(check);
    await editor.setMarkdown(LOSSY);
    expect(check).toHaveBeenCalledTimes(1);
    expect(editor.getMarkdown()).toBe(LOSSY);
    expect(lock()?.hidden).toBe(false);
    expect(textarea()?.hidden).toBe(false);
    expect(textarea()?.value).toBe(LOSSY);
  });

  it('检查失败（reject）按有损处理', async () => {
    const editor = mount(vi.fn().mockRejectedValue(new Error('x')));
    await editor.setMarkdown(LOSSY);
    expect(lock()?.hidden).toBe(false);
    expect(textarea()?.hidden).toBe(false);
    expect(editor.getMarkdown()).toBe(LOSSY);
  });

  it('服务端判定一致：不锁定，getMarkdown 为富文本序列化结果', async () => {
    const editor = mount(vi.fn().mockResolvedValue(true));
    await editor.setMarkdown(LOSSY);
    expect(lock()?.hidden).toBe(true);
    expect(textarea()?.hidden).toBe(true);
    expect(editor.getMarkdown()).not.toBe('');
  });

  it('往返逐字相同时不发起检查', async () => {
    const check = vi.fn().mockResolvedValue(false);
    const editor = mount(check);
    await editor.setMarkdown('普通段落');
    expect(check).not.toHaveBeenCalled();
    expect(lock()?.hidden).toBe(true);
  });

  it('检查进行中保存拿到的是原文', async () => {
    let resolve!: (v: boolean) => void;
    const editor = mount(() => new Promise<boolean>((r) => (resolve = r)));
    const pending = editor.setMarkdown(LOSSY);
    expect(editor.getMarkdown()).toBe(LOSSY);
    resolve(true);
    await pending;
    expect(editor.getMarkdown()).not.toBe(LOSSY);
  });

  it('锁定后点击切换会重新检查，仍有损则留在源码模式', async () => {
    const check = vi.fn().mockResolvedValue(false);
    const editor = mount(check);
    await editor.setMarkdown(LOSSY);
    editor.toggleSource();
    await vi.waitFor(() => expect(check).toHaveBeenCalledTimes(2));
    await Promise.resolve();
    expect(textarea()?.hidden).toBe(false);
    expect(lock()?.hidden).toBe(false);
    expect(editor.getMarkdown()).toBe(LOSSY);
  });

  it('未注入守卫时 setMarkdown 同步完成且不锁定', async () => {
    const editor = mount();
    await editor.setMarkdown(LOSSY);
    expect(lock()?.hidden).toBe(true);
    expect(textarea()?.hidden).toBe(true);
  });
});
