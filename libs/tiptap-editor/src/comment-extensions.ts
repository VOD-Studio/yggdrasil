import type { AnyExtension, EditorOptions } from '@tiptap/core';
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
import { Placeholder } from '@tiptap/extension-placeholder';
import { Markdown } from '@tiptap/markdown';
import StarterKit from '@tiptap/starter-kit';
import { CodeBlockBackspaceFix } from './code-block-backspace-fix';
import { CommentBubbleMenu } from './comment-bubble-menu';
import { imageFileHandler, LINK_OPTIONS } from './editor-shared-extensions';
import { lowlight } from './highlight';
import { DisplayMath, InlineMath } from './math';
import type { UploadCoordinator } from './upload-coordinator';
import { UploadImage } from './upload-image';

export function buildCommentExtensions(
  getCoordinator: () => UploadCoordinator | null,
  placeholder = '写下你的想法…',
): AnyExtension[] {
  return [
    StarterKit.configure({
      heading: false,
      horizontalRule: false,
      link: LINK_OPTIONS,
      codeBlock: false,
    }),
    Markdown,
    InlineMath,
    DisplayMath,
    CodeBlockLowlight.configure({ lowlight }),
    CodeBlockBackspaceFix,
    UploadImage,
    imageFileHandler(getCoordinator),
    Placeholder.configure({ placeholder }),
    CommentBubbleMenu,
  ];
}

export const commentEditorProps: EditorOptions['editorProps'] = {
  attributes: {
    role: 'textbox',
    'aria-label': '评论内容',
    'aria-multiline': 'true',
  },
};
