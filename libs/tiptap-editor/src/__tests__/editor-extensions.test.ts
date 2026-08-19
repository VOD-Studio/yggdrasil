import { Editor } from '@tiptap/core';
import { describe, expect, it } from 'vitest';
import { buildExtensions } from '../editor-extensions';
import TiptapEditor from '../index';

/**
 * buildExtensions 工厂测试：comment 是 full 的真子集（装配级断言 + schema 冒烟）。
 *
 * 装配级：数组层只能看到顶层扩展名（StarterKit 的子扩展在内部配置，
 * 不出现在数组里），故 heading/hr 的裁剪靠 schema 冒烟断言兜底。
 * schema 冒烟：真实 Editor（happy-dom）验证节点/标记最终形态——
 * 评论渲染器不支持的东西（标题/表格/任务列表/脚注）不得进 schema。
 */

const base = { getCoordinator: () => null };

function names(extensions: ReadonlyArray<{ name: string }>): string[] {
  return extensions.map((e) => e.name);
}

describe('buildExtensions 装配', () => {
  it('comment 变体裁掉 full-only 扩展，保留上传/代码块/评论专属', () => {
    const ns = names(buildExtensions({ ...base, variant: 'comment' }));
    expect(ns).toContain('image');
    expect(ns).toContain('codeBlock');
    expect(ns).toContain('placeholder');
    expect(ns).toContain('commentBubbleMenu');
    expect(ns).not.toContain('tableKit');
    expect(ns).not.toContain('taskList');
    expect(ns).not.toContain('slashCommand');
    expect(ns).not.toContain('footnoteRef');
  });

  it('full 变体保留完整扩展且无评论专属', () => {
    const ns = names(buildExtensions({ ...base, variant: 'full' }));
    expect(ns).toContain('tableKit');
    expect(ns).toContain('taskList');
    expect(ns).toContain('slashCommand');
    expect(ns).toContain('footnoteRef');
    expect(ns).toContain('image');
    expect(ns).not.toContain('commentBubbleMenu');
    expect(ns).not.toContain('placeholder');
  });
});

describe('comment 变体 schema 冒烟', () => {
  function createCommentEditor(): Editor {
    const el = document.createElement('div');
    document.body.appendChild(el);
    return new Editor({
      element: el,
      extensions: buildExtensions({ ...base, variant: 'comment' }),
    });
  }

  it('schema 无 heading/table/taskList/footnote，保留评论能力集', () => {
    const editor = createCommentEditor();
    // 裁掉：评论渲染器不承载或 UI 过重的节点。
    expect(editor.schema.nodes.heading).toBeUndefined();
    expect(editor.schema.nodes.table).toBeUndefined();
    expect(editor.schema.nodes.taskList).toBeUndefined();
    expect(editor.schema.nodes.horizontalRule).toBeUndefined();
    expect(editor.schema.nodes.footnoteRef).toBeUndefined();
    // 保留：格式/列表/引用/代码块/图片/数学（与服务端评论渲染器能力对齐）。
    expect(editor.schema.nodes.image).toBeDefined();
    expect(editor.schema.nodes.codeBlock).toBeDefined();
    expect(editor.schema.nodes.blockquote).toBeDefined();
    expect(editor.schema.nodes.bulletList).toBeDefined();
    // InlineMath 节点名是 'math'（inline atom），DisplayMath 是 'mathBlock'。
    expect(editor.schema.nodes.math).toBeDefined();
    expect(editor.schema.nodes.mathBlock).toBeDefined();
    expect(editor.schema.marks.bold).toBeDefined();
    expect(editor.schema.marks.italic).toBeDefined();
    expect(editor.schema.marks.strike).toBeDefined();
    expect(editor.schema.marks.code).toBeDefined();
    expect(editor.schema.marks.link).toBeDefined();
    editor.destroy();
  });

  it('markdown 往返：图片/加粗/数学文本不丢失', () => {
    const editor = createCommentEditor();
    editor.commands.setContent('**粗体** $x^2$ ![截图](/uploads/2026/08/a.webp)', {
      contentType: 'markdown',
    });
    const md = editor.getMarkdown();
    expect(md).toContain('**粗体**');
    expect(md).toContain('$x^2$');
    expect(md).toContain('![截图](/uploads/2026/08/a.webp)');
    editor.destroy();
  });
});

describe('TiptapEditor.create 全路径挂载', () => {
  // 回归：init 曾给 full 路径传 `editorProps: undefined`，浅合并冲掉 tiptap
  // 默认的 `editorProps: {}`，createView 读 `editorProps.dispatchTransaction`
  // 抛 TypeError——后台编辑器一挂载就崩。此用例走真实入口覆盖两个变体。
  //
  // EditorOptions 类未具名导出（避免 IIFE named/default 冲突），由 index.ts
  // 挂到 window；此处经类型化常量取用，桥回 create 的参数类型。
  const EditorOptionsCtor = (window as unknown as Record<string, unknown>)
    .EditorOptions as new () => Record<string, unknown>;
  for (const variant of ['full', 'comment'] as const) {
    it(`${variant} 变体经 TiptapEditor.create 成功挂载并销毁`, () => {
      const container = document.createElement('div');
      container.id = `editor-mount-${variant}`;
      document.body.appendChild(container);
      const raw = new EditorOptionsCtor();
      raw.variant = variant;
      // 运行时就是 EditorOptions（同一构造器产出），类型未导出故在命名常量处断言。
      const opts = raw as Parameters<typeof TiptapEditor.create>[1];
      const inst = TiptapEditor.create(container.id, opts);
      expect(inst).not.toBeNull();
      // ProseMirror DOM 已挂进容器（createView 成功）。
      expect(container.querySelector('.ProseMirror')).not.toBeNull();
      inst?.destroy();
      container.remove();
    });
  }
});
