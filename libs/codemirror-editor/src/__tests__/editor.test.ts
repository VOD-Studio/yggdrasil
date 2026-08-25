import { syntaxTree } from '@codemirror/language';
import { beforeEach, describe, expect, it } from 'vitest';
import { CodeMirrorInstance, EditorOptions } from '../editor';

type CodeMirrorInternals = {
  view: { state: Parameters<typeof syntaxTree>[0] };
};

describe('CodeMirrorInstance', () => {
  let container: HTMLElement;

  beforeEach(() => {
    container = document.createElement('div');
    container.id = 'test-cm';
    document.body.appendChild(container);
  });

  it('getValue/setValue 往返', () => {
    const inst = new CodeMirrorInstance(container, new EditorOptions());
    inst.setValue('SELECT 1');
    expect(inst.getValue()).toBe('SELECT 1');
    inst.destroy();
  });

  it('初始 value 正确', () => {
    const opts = new EditorOptions();
    opts.value = 'SELECT * FROM posts';
    const inst = new CodeMirrorInstance(container, opts);
    expect(inst.getValue()).toBe('SELECT * FROM posts');
    inst.destroy();
  });

  it('go 语言将 // 注释解析为完整注释节点', () => {
    const source = '// MaxMessages: 超过上限时丢弃最旧消息\npackage main\nfunc main() {}';
    const opts = new EditorOptions();
    opts.language = 'go';
    opts.value = source;
    const inst = new CodeMirrorInstance(container, opts);
    // Test-only private view access verifies the real parser tree.
    const internals = inst as unknown as CodeMirrorInternals;
    const state = internals.view.state;
    const nodes: string[] = [];
    syntaxTree(state).iterate({
      enter: (node) => {
        if (
          node.name === 'LineComment' ||
          node.name === 'PackageClause' ||
          node.name === 'FunctionDecl'
        ) {
          nodes.push(node.name);
        }
      },
    });

    expect(nodes).toEqual(expect.arrayContaining(['LineComment', 'PackageClause', 'FunctionDecl']));
    inst.destroy();
  });

  it('setTheme 不抛错（走 Compartment reconfigure）', () => {
    const inst = new CodeMirrorInstance(container, new EditorOptions());
    expect(() => inst.setTheme('dark')).not.toThrow();
    expect(() => inst.setTheme('light')).not.toThrow();
    inst.destroy();
  });

  it('setSchema 更新 lang-sql 配置', () => {
    const inst = new CodeMirrorInstance(container, new EditorOptions());
    expect(() =>
      inst.setSchema({ tables: [{ name: 'posts', columns: ['id', 'title'] }] }),
    ).not.toThrow();
    inst.destroy();
  });

  it('vim 开关：vim:true 注入，false 不注入', () => {
    const optsOn = new EditorOptions();
    optsOn.vim = true;
    const instOn = new CodeMirrorInstance(container, optsOn);
    instOn.destroy();

    const optsOff = new EditorOptions();
    optsOff.vim = false;
    const instOff = new CodeMirrorInstance(container, optsOff);
    instOff.destroy();
    // happy-dom 无法验证 keymap 行为，仅验证配置加载不抛错
  });

  it('onChange 在内容变更时触发', () => {
    let captured = '';
    const opts = new EditorOptions();
    opts.onChange = (v) => {
      captured = v;
    };
    const inst = new CodeMirrorInstance(container, opts);
    inst.setValue('hello');
    expect(captured).toBe('hello');
    inst.destroy();
  });

  it('onRunShortcut 注入则构造不抛错', () => {
    const opts = new EditorOptions();
    opts.onRunShortcut = () => {};
    const inst = new CodeMirrorInstance(container, opts);
    inst.destroy();
    // happy-dom 无法触发真实按键，仅验证 keymap 扩展加载不抛错
  });
});
