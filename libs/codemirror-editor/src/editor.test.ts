import { ensureSyntaxTree } from '@codemirror/language';
import type { EditorState } from '@codemirror/state';
import { beforeEach, describe, expect, it } from 'vitest';
import { CodeMirrorInstance, type EditorOptions } from './editor';
import CodeMirrorEditor from './index';

type CodeMirrorInternals = {
  view: { state: EditorState };
};

describe('CodeMirrorInstance', () => {
  let container: HTMLElement;

  beforeEach(() => {
    container = document.createElement('div');
    container.id = 'test-cm';
    document.body.appendChild(container);
  });

  it('开启 Vim 时报告模式，热切换后仍能收到模式变化', async () => {
    const modes: string[] = [];
    const opts: EditorOptions = {};
    opts.vim = true;
    opts.onVimModeChange = (mode) => modes.push(mode);
    const inst = new CodeMirrorInstance(container, opts);
    await inst.ready;
    expect(modes).toEqual(['normal']);
    await inst.setVim(true);
    expect(modes).toEqual(['normal']);
    await inst.setVim(false);
    await inst.setVim(true);
    expect(modes).toEqual(['normal', 'normal']);
    inst.destroy();
  });

  it('getValue/setValue 往返', () => {
    const inst = new CodeMirrorInstance(container, {});
    inst.setValue('SELECT 1');
    expect(inst.getValue()).toBe('SELECT 1');
    inst.destroy();
  });

  it('注册表随销毁清理，旧句柄不会误删同 ID 新实例', () => {
    const old = CodeMirrorEditor.create(container);
    const current = CodeMirrorEditor.create(container);
    old.destroy();
    expect(CodeMirrorEditor._instances.get(container)).toBe(current);
    current.destroy();
    expect(CodeMirrorEditor._instances.has(container)).toBe(false);
  });

  it('初始 value 正确', () => {
    const opts: EditorOptions = {};
    opts.value = 'SELECT * FROM posts';
    const inst = new CodeMirrorInstance(container, opts);
    expect(inst.getValue()).toBe('SELECT * FROM posts');
    inst.destroy();
  });

  it('go 语言将 // 注释解析为完整注释节点', async () => {
    const source = '// MaxMessages: 超过上限时丢弃最旧消息\npackage main\nfunc main() {}';
    const opts: EditorOptions = {};
    opts.language = 'go';
    opts.value = source;
    const inst = new CodeMirrorInstance(container, opts);
    await inst.ready;
    // 读取真实语法树以验证独立语法包已生效。
    const internals = inst as unknown as CodeMirrorInternals;
    const state = internals.view.state;
    const nodes: string[] = [];
    const tree = ensureSyntaxTree(state, state.doc.length, 5000);
    expect(tree).not.toBeNull();
    tree?.iterate({
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

  it('rust 语言将 // 注释、fn 函数与 println! 宏解析为对应节点', async () => {
    const source = 'fn main() {\n    // test\n    println!("Hello world");\n}';
    const opts: EditorOptions = {};
    opts.language = 'rust';
    opts.value = source;
    const inst = new CodeMirrorInstance(container, opts);
    await inst.ready;
    // 读取真实语法树以验证独立语法包已生效。
    const internals = inst as unknown as CodeMirrorInternals;
    const state = internals.view.state;
    const nodes: string[] = [];
    const tree = ensureSyntaxTree(state, state.doc.length, 5000);
    expect(tree).not.toBeNull();
    tree?.iterate({
      enter: (node) => {
        if (
          node.name === 'LineComment' ||
          node.name === 'FunctionItem' ||
          node.name === 'MacroInvocation'
        ) {
          nodes.push(node.name);
        }
      },
    });

    expect(nodes).toEqual(
      expect.arrayContaining(['LineComment', 'FunctionItem', 'MacroInvocation']),
    );
    inst.destroy();
  });

  // 与 /admin/runner 的 SUPPORTED_LANGS（python/node/go/rust/bun）一一对应；
  // go/rust 由上方独立用例覆盖，这里锁定 python/node/bun 的 grammar 映射，
  // 防止新增分支时重蹈 rust 落入 SQL 兜底的覆辙。
  it.each([
    {
      lang: 'python',
      source: 'def main():\n    # test\n    print(f"hi {1 + 1}")\n',
      expected: ['FunctionDefinition', 'Comment', 'FormatString'],
    },
    {
      lang: 'node',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: ${x} 是喂给 parser 的 JS 源码字面量，非笔误
      source: 'const x = 1;\n// test\nconsole.log(`hi ${x}`);\n',
      expected: ['VariableDeclaration', 'LineComment', 'TemplateString'],
    },
    {
      lang: 'bun',
      source: 'interface User { name: string }\n// test\nconst u: User = { name: "x" };\n',
      expected: ['InterfaceDeclaration', 'TypeAnnotation', 'LineComment'],
    },
  ])('$lang 语言解析出预期语法节点', async ({ lang, source, expected }) => {
    const opts: EditorOptions = {};
    opts.language = lang;
    opts.value = source;
    const inst = new CodeMirrorInstance(container, opts);
    await inst.ready;
    // 读取真实语法树以验证独立语法包已生效。
    const internals = inst as unknown as CodeMirrorInternals;
    const names: string[] = [];
    const state = internals.view.state;
    const tree = ensureSyntaxTree(state, state.doc.length, 5000);
    expect(tree).not.toBeNull();
    tree?.iterate({
      enter: (node) => {
        names.push(node.name);
      },
    });
    expect(names).toEqual(expect.arrayContaining(expected));
    inst.destroy();
  });

  it('setTheme 不抛错（走 Compartment reconfigure）', () => {
    const inst = new CodeMirrorInstance(container, {});
    expect(() => inst.setTheme('dark')).not.toThrow();
    expect(() => inst.setTheme('light')).not.toThrow();
    inst.destroy();
  });

  it('setSchema 更新 lang-sql 配置', () => {
    const inst = new CodeMirrorInstance(container, {});
    expect(() =>
      inst.setSchema({ tables: [{ name: 'posts', columns: ['id', 'title'] }] }),
    ).not.toThrow();
    inst.destroy();
  });

  it('迅速切换语言时只应用最后一次选择并保留文档', async () => {
    const value = 'def main():\n    pass';
    const inst = new CodeMirrorInstance(container, { language: 'sql', value });
    await Promise.all([inst.setLanguage('rust'), inst.setLanguage('python'), inst.ready]);
    const state = (inst as unknown as CodeMirrorInternals).view.state;
    expect(ensureSyntaxTree(state, state.doc.length, 5000)?.toString()).toContain(
      'FunctionDefinition',
    );
    expect(inst.getValue()).toBe(value);
    inst.destroy();
  });

  it('Vim 加载过程中关闭或销毁时忽略迟到的模块', async () => {
    const modes: string[] = [];
    const inst = new CodeMirrorInstance(container, { onVimModeChange: (mode) => modes.push(mode) });
    const loading = inst.setVim(true);
    await inst.setVim(false);
    await loading;
    expect(modes).toEqual([]);
    const loadingAgain = inst.setVim(true);
    inst.destroy();
    await loadingAgain;
    expect(modes).toEqual([]);
  });

  it('onChange 在内容变更时触发', () => {
    let captured = '';
    const opts: EditorOptions = {};
    opts.onChange = (v) => {
      captured = v;
    };
    const inst = new CodeMirrorInstance(container, opts);
    inst.setValue('hello');
    expect(captured).toBe('hello');
    inst.destroy();
  });

  it('onRunShortcut 注入则构造不抛错', () => {
    const opts: EditorOptions = {};
    opts.onRunShortcut = () => {};
    const inst = new CodeMirrorInstance(container, opts);
    inst.destroy();
    // happy-dom 无法触发真实按键，仅验证 keymap 扩展加载不抛错
  });
});
