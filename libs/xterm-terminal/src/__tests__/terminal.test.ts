import { FitAddon } from '@xterm/addon-fit';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TerminalInstance, XtermOptions } from '../terminal';

// --- ResizeObserver mock：happy-dom 不实现，手动提供回调钩子
//     （同 @yggdrasil/core hash-scroll.test.ts 惯例） ---
let resizeCallback: (() => void) | null = null;
const roDisconnect = vi.fn();
vi.stubGlobal(
  'ResizeObserver',
  class {
    constructor(cb: () => void) {
      resizeCallback = cb;
    }
    observe() {}
    disconnect() {
      roDisconnect();
    }
  },
);

describe('XtermOptions', () => {
  it('可无参构造，字段全部 undefined', () => {
    const opts = new XtermOptions();
    expect(opts.theme).toBeUndefined();
    expect(opts.fontFamily).toBeUndefined();
    expect(opts.fontSize).toBeUndefined();
    expect(opts.onReady).toBeUndefined();
  });

  it('可设置字段', () => {
    const opts = new XtermOptions();
    opts.theme = 'dark';
    opts.fontSize = 14;
    expect(opts.theme).toBe('dark');
    expect(opts.fontSize).toBe(14);
  });
});

describe('TerminalInstance', () => {
  beforeEach(() => {
    resizeCallback = null;
    roDisconnect.mockClear();
  });

  it('挂载到容器并触发 onReady', () => {
    const container = document.createElement('div');
    let ready = false;
    const opts = new XtermOptions();
    opts.onReady = () => {
      ready = true;
    };

    const inst = new TerminalInstance(container, opts);
    expect(ready).toBe(true);

    // xterm.js 在容器内创建 .xterm 元素
    expect(container.querySelector('.xterm')).toBeTruthy();

    inst.destroy();
  });

  it('writeAll 清屏后重写 stdout + stderr', () => {
    const container = document.createElement('div');
    const inst = new TerminalInstance(container, new XtermOptions());

    // 不报错即通过（xterm.js 在 happy-dom 下 write 是 no-op 渲染）
    inst.writeAll('hello\n', 'error\n');
    inst.clear();

    inst.destroy();
  });

  it('容器 resize 时通过 ResizeObserver 自动重新 fit', () => {
    const container = document.createElement('div');
    const fitSpy = vi.spyOn(FitAddon.prototype, 'fit');
    const inst = new TerminalInstance(container, new XtermOptions());

    expect(fitSpy).toHaveBeenCalledTimes(1); // 构造时的初始 fit
    expect(resizeCallback).not.toBeNull();

    resizeCallback?.();
    expect(fitSpy).toHaveBeenCalledTimes(2); // resize 回调触发的自动 fit

    inst.destroy();
    fitSpy.mockRestore();
  });

  it('destroy 时断开 ResizeObserver，避免销毁后继续 fit', () => {
    const container = document.createElement('div');
    const inst = new TerminalInstance(container, new XtermOptions());

    inst.destroy();
    expect(roDisconnect).toHaveBeenCalledTimes(1);
  });
});
