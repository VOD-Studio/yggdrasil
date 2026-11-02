import { FitAddon } from '@xterm/addon-fit';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import XtermTerminal from './index';
import { TerminalInstance, type XtermOptions } from './terminal';

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

describe('TerminalInstance', () => {
  beforeEach(() => {
    resizeCallback = null;
    roDisconnect.mockClear();
  });

  it('挂载到容器并触发 onReady', () => {
    const container = document.createElement('div');
    let ready = false;
    const opts: XtermOptions = {};
    opts.onReady = () => {
      ready = true;
    };

    const inst = new TerminalInstance(container, opts);
    expect(ready).toBe(true);

    // xterm.js 在容器内创建 .xterm 元素
    expect(container.querySelector('.xterm')).toBeTruthy();

    inst.destroy();
  });

  it('注册表随销毁清理，旧句柄不会误删同 ID 新实例', () => {
    const container = document.createElement('div');
    container.id = 'terminal-replacement';
    document.body.appendChild(container);
    const old = XtermTerminal.create(container);
    const current = XtermTerminal.create(container);
    old.destroy();
    expect(XtermTerminal._instances.get(container)).toBe(current);
    current.destroy();
    expect(XtermTerminal._instances.has(container)).toBe(false);
    container.remove();
  });

  it('writeAll 清屏后重写 stdout + stderr', () => {
    const container = document.createElement('div');
    const inst = new TerminalInstance(container, {});

    // 不报错即通过（xterm.js 在 happy-dom 下 write 是 no-op 渲染）
    inst.writeAll('hello\n', 'error\n');
    inst.clear();

    inst.destroy();
  });

  it('容器 resize 时通过 ResizeObserver 自动重新 fit', () => {
    const container = document.createElement('div');
    const fitSpy = vi.spyOn(FitAddon.prototype, 'fit');
    const inst = new TerminalInstance(container, {});

    expect(fitSpy).toHaveBeenCalledTimes(1); // 构造时的初始 fit
    expect(resizeCallback).not.toBeNull();

    resizeCallback?.();
    expect(fitSpy).toHaveBeenCalledTimes(2); // resize 回调触发的自动 fit

    inst.destroy();
    fitSpy.mockRestore();
  });

  it('destroy 时断开 ResizeObserver，避免销毁后继续 fit', () => {
    const container = document.createElement('div');
    const inst = new TerminalInstance(container, {});

    inst.destroy();
    expect(roDisconnect).toHaveBeenCalledTimes(1);
  });
});
