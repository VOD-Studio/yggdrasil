import { afterEach, describe, expect, it, vi } from 'vitest';
import { beginTransition } from './view-transition-lifecycle';

function transition(): ViewTransition {
  return {
    ready: Promise.resolve(),
    updateCallbackDone: Promise.resolve(),
    finished: new Promise<void>(() => {}),
    skipTransition: vi.fn(),
    types: new Set<string>(),
  } as ViewTransition;
}

afterEach(() => beginTransition('route').finish());

describe('native transition ownership', () => {
  it('cancels the previous animation and settles its cleanup synchronously', () => {
    const old = beginTransition('theme');
    const native = transition();
    const cleanup = vi.fn();
    old.attach(native);
    old.onCancel(cleanup);

    const current = beginTransition('route');

    expect(native.skipTransition).toHaveBeenCalledOnce();
    expect(cleanup).toHaveBeenCalledOnce();
    expect(old.isCurrent()).toBe(false);
    expect(current.isCurrent()).toBe(true);
  });

  it('reports the next owner kind so interruption preserves pending navigation', () => {
    const owner = beginTransition('route');
    const before = vi.fn();
    const after = vi.fn();
    owner.onCancel(before);
    beginTransition('theme');
    owner.onCancel(after);
    expect(before).toHaveBeenCalledWith('theme');
    expect(after).toHaveBeenCalledWith('theme');
  });

  it('late completion cannot release the newer owner', () => {
    const old = beginTransition('route');
    const current = beginTransition('theme');
    old.finish();
    expect(current.isCurrent()).toBe(true);
  });

  it('normal completion does not invoke cancellation cleanup', () => {
    const owner = beginTransition('route');
    const cleanup = vi.fn();
    const native = transition();
    owner.onCancel(cleanup);
    owner.attach(native);
    owner.finish();
    beginTransition('theme');
    expect(cleanup).not.toHaveBeenCalled();
    expect(native.skipTransition).not.toHaveBeenCalled();
  });

  it('skips a native transition attached after its owner was interrupted', () => {
    const owner = beginTransition('route');
    const current = beginTransition('theme');
    const native = transition();
    owner.attach(native);
    expect(native.skipTransition).toHaveBeenCalledOnce();
    expect(current.isCurrent()).toBe(true);
  });

  it('a failed cancellation cleanup cannot block the next owner', () => {
    const owner = beginTransition('route');
    const cleanup = vi.fn();
    owner.onCancel(() => {
      throw new Error('disposed DOM');
    });
    owner.onCancel(cleanup);
    expect(beginTransition('theme').isCurrent()).toBe(true);
    expect(cleanup).toHaveBeenCalledOnce();
  });

  it('observes all rejecting native promises when update or capture fails', async () => {
    const owner = beginTransition('route');
    owner.attach({
      ready: Promise.reject(new Error('capture skipped')),
      updateCallbackDone: Promise.reject(new Error('update failed')),
      finished: Promise.reject(new Error('update failed')),
      skipTransition: vi.fn(),
      types: new Set<string>(),
    } as ViewTransition);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(owner.isCurrent()).toBe(true);
  });
});
