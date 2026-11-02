// @vitest-environment happy-dom

import { Editor } from '@tiptap/core';
import { TaskItem, TaskList } from '@tiptap/extension-list';
import StarterKit from '@tiptap/starter-kit';
import { beforeEach, describe, expect, it } from 'vitest';
import { TaskInputRule } from './task-input-rule';

/**
 * TaskInputRule 单元测试(happy-dom 真实 DOM + 真实 Editor)。
 *
 * 真实输入是逐字符的:打 `- ` 时 BulletList input rule 先触发,把这行变成
 * bulletList > listItem;再打 `[ ] ` 时 appendTransaction 监听到 listItem
 * 文本变化,把 bulletList 升级成 taskList。
 *
 * 测试用 insertContentAt(无 applyInputRules)模拟逐段输入:它走正常 dispatch,
 * 既触发 input rule(经 handleTextInput 路径外的 plugin)也触发 appendTransaction。
 * 为贴近逐字符时序,分两步:先 `- `(触发 BulletList),再 `[ ] `(触发升级)。
 *
 * 46 个回归测试见 upload-coordinator/upload-image/slash-command。
 */

/** 等待异步(input rule 的 setTimeout + appendTransaction)。 */
function flush() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function makeEditor() {
  return new Editor({
    element: document.body,
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
      TaskList,
      TaskItem.configure({ nested: true }),
      TaskInputRule,
    ],
    content: { type: 'doc', content: [{ type: 'paragraph' }] },
  });
}

/**
 * 在当前选区插入文本,贴近真实逐字符输入的时序。
 * 若文本是列表 marker(如 "- " "* "),用 applyInputRules 触发 BulletList input rule;
 * 否则普通插入——后续靠 appendTransaction 接力升级。
 */
function typeText(editor: Editor, text: string) {
  const isListMarker = /^\s*[-+*]\s$/.test(text);
  editor.commands.insertContentAt(editor.state.selection.from, text, {
    applyInputRules: isListMarker,
  });
}

/** 取 JSON 文档首个块节点。getJSON content 是 Node|Text 联合,断言 any 窄化。 */
function firstBlock(editor: Editor): any {
  return editor.getJSON().content?.[0];
}

describe('TaskInputRule (appendTransaction 升级方案)', () => {
  let editor: Editor;

  beforeEach(() => {
    document.body.innerHTML = '';
    editor = makeEditor();
  });

  it('打 "- " 再打 "[ ] " 后升级成未勾选任务列表', async () => {
    // 步骤 1:打 "- " → BulletList input rule 触发
    typeText(editor, '- ');
    await flush();
    expect(firstBlock(editor).type).toBe('bulletList');

    // 步骤 2:在 listItem 内打 "[ ] " → appendTransaction 升级成 taskList
    typeText(editor, '[ ] ');
    await flush();

    const block = firstBlock(editor);
    expect(block.type).toBe('taskList');
    const taskItem = block.content?.[0];
    expect(taskItem?.type).toBe('taskItem');
    expect(taskItem?.attrs?.checked).toBe(false);
    // 前缀 "[ ] " 应被删除,不残留为文本(空段落时 content 为空,也满足不含 [)
    const paraText = taskItem?.content?.[0]?.textContent ?? '';
    expect(paraText).not.toContain('[');
    // 光标应落在命中 taskItem 的段落内(pos 3 = doc>taskList>taskItem>paragraph 内),
    // 而非被甩到下一行(替换区域之后)。
    expect(editor.state.selection.from).toBe(3);
  });

  it('打 "[x] " 升级成已勾选任务列表项', async () => {
    typeText(editor, '- ');
    await flush();
    typeText(editor, '[x] ');
    await flush();

    const block = firstBlock(editor);
    expect(block.type).toBe('taskList');
    expect(block.content?.[0]?.attrs?.checked).toBe(true);
  });

  it('打 "[X] "(大写)也算已勾选', async () => {
    typeText(editor, '- ');
    await flush();
    typeText(editor, '[X] ');
    await flush();

    const block = firstBlock(editor);
    expect(block.type).toBe('taskList');
    expect(block.content?.[0]?.attrs?.checked).toBe(true);
  });

  it('打普通文本不升级(保持 bulletList)', async () => {
    typeText(editor, '- ');
    await flush();
    typeText(editor, '普通项');
    await flush();

    const block = firstBlock(editor);
    expect(block.type).toBe('bulletList');
    expect(block.content?.[0]?.type).toBe('listItem');
  });

  it('星号 marker 列表打 "[ ] " 也升级(* 触发的是 bulletList)', async () => {
    // BulletList 对 * + - 三种 marker 都创建 bulletList 节点,
    // appendTransaction 识别到 listItem 内的 [ ] 前缀即升级,与 marker 无关。
    typeText(editor, '* ');
    await flush();
    typeText(editor, '[ ] ');
    await flush();

    const block = firstBlock(editor);
    expect(block.type).toBe('taskList');
  });

  it('升级后段落文本不含前缀残留(畸形文档会导致后续 Enter 异常)', async () => {
    // 核心回归:stripPrefix 必须作用在段落 content 而非 listItem content,
    // 否则 cut 会切进段落标签边界,产生畸形文档(文本前缀残留、nodeSize 异常)。
    typeText(editor, '- ');
    await flush();
    typeText(editor, '[ ] 未完成');
    await flush();

    const taskItem = firstBlock(editor).content?.[0];
    const paraText = taskItem?.content?.[0]?.content?.[0]?.text;
    // 段落文本应严格为"未完成",无前导空格或残留方括号
    expect(paraText).toBe('未完成');
  });

  it('升级后空项 Enter 退出列表(与斜杠命令行为一致)', async () => {
    // 完整 Enter 链路:升级 → Enter 新建空项 → Enter 退出
    typeText(editor, '- ');
    await flush();
    typeText(editor, '[ ] 123');
    await flush();
    expect(firstBlock(editor).type).toBe('taskList');

    // 第一次 Enter:在内容项末尾分裂出空 taskItem
    editor.commands.splitListItem('taskItem');
    const after1 = firstBlock(editor);
    expect(after1.content?.length).toBe(2);
    // 第二项应为空 taskItem:段落无文本内容(getJSON 对空段落省略 content 数组)
    const secondItem = after1.content?.[1];
    const secondParaContent = secondItem?.content?.[0]?.content;
    expect(secondParaContent === undefined || secondParaContent?.length === 0).toBe(true);

    // 第二次 Enter:空 taskItem(列表最后一项)→ splitListItem return false,
    // 由 Enter 的 fallback(ProseMirror baseKeymap)退出列表,产生普通段落。
    editor.commands.splitListItem('taskItem');

    const json = editor.getJSON();
    const lastType = json.content?.[json.content.length - 1]?.type;
    expect(lastType).toBe('paragraph');
  });
});
