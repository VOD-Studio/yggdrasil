import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { describe, expect, it } from 'vitest';
import { CodeBlockBackspaceFix } from './code-block-backspace-fix';

function backspaceAtEnd(code: string): string {
  const editor = new Editor({
    extensions: [StarterKit, CodeBlockBackspaceFix],
    content: {
      type: 'doc',
      content: [{ type: 'codeBlock', content: [{ type: 'text', text: code }] }],
    },
  });
  editor.commands.setTextSelection(editor.state.doc.content.size - 1);
  const handled = editor.view.someProp('handleKeyDown', (f) =>
    f(editor.view, new KeyboardEvent('keydown', { key: 'Backspace' })),
  );
  expect(handled).toBe(true);
  const text = editor.state.doc.textContent;
  editor.destroy();
  return text;
}

describe('CodeBlockBackspaceFix', () => {
  it('普通字符：退一格', () => {
    expect(backspaceAtEnd('abc')).toBe('ab');
  });

  it('emoji（代理对）整体删除，不留孤立代理', () => {
    expect(backspaceAtEnd('a😀')).toBe('a');
  });

  it('ZWJ 组合 emoji 整体删除', () => {
    expect(backspaceAtEnd('x👨‍👩‍👧')).toBe('x');
  });
});
