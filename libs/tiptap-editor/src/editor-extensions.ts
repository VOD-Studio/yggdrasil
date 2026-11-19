import type { AnyExtension } from '@tiptap/core';
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
import { TaskItem, TaskList } from '@tiptap/extension-list';
import { TableKit } from '@tiptap/extension-table';
import { Markdown } from '@tiptap/markdown';
import StarterKit from '@tiptap/starter-kit';
import { CodeBlockBackspaceFix } from './code-block-backspace-fix';
import { CodeBlockNodeView } from './code-block-view';
import { DefinitionDescription, DefinitionList, DefinitionTerm } from './definition-list';
import { Details, DetailsSummary } from './details';
import { imageFileHandler, LINK_OPTIONS } from './editor-shared-extensions';
import { FootnoteDef, FootnoteNumbering, FootnoteRef } from './footnote';
import { GfmAlert } from './gfm-alert';
import { lowlight } from './highlight';
import { HtmlEntity } from './html-entity';
import { Abbr, HtmlMark, Kbd, RawInlineHtml, Subscript, Superscript } from './inline-html';
import { LinkDefinition, ReferenceLink } from './link-reference';
import { LooseBulletList, LooseListItem, LooseOrderedList } from './list-markdown';
import { createArticleMarked } from './marked-instance';
import { DisplayMath, InlineMath } from './math';
import { SlashCommand } from './slash-command';
import { TaskInputRule } from './task-input-rule';
import type { UploadCoordinator } from './upload-coordinator';
import { UploadImage } from './upload-image';

interface BuildExtensionsParams {
  onImageUpload?: (file: File) => Promise<string>;
  onInsertUploading?: (file: File) => void;
  onPickFromLibrary?: () => void;
  getCoordinator: () => UploadCoordinator | null;
}

/** 数学、脚注、提示块与定义列表在 Markdown 之后注册，以便收集 Markdown spec。 */
export function buildExtensions(params: BuildExtensionsParams): AnyExtension[] {
  return [
    StarterKit.configure({
      heading: { levels: [1, 2, 3, 4, 5, 6] },
      link: false,
      bulletList: false,
      orderedList: false,
      listItem: false,
      codeBlock: false,
    }),
    Markdown.configure({ marked: createArticleMarked() }),
    InlineMath,
    DisplayMath,
    FootnoteRef,
    FootnoteDef,
    FootnoteNumbering,
    GfmAlert,
    DefinitionList,
    DefinitionTerm,
    DefinitionDescription,
    Details,
    DetailsSummary,
    LinkDefinition,
    HtmlEntity,
    RawInlineHtml,
    Subscript,
    Superscript,
    Kbd,
    HtmlMark,
    Abbr,
    ReferenceLink.configure(LINK_OPTIONS),
    LooseBulletList,
    LooseOrderedList,
    LooseListItem,
    CodeBlockLowlight.configure({ lowlight }).extend({
      addNodeView() {
        return ({ node, editor, getPos }) => new CodeBlockNodeView({ node, editor, getPos });
      },
    }),
    CodeBlockBackspaceFix,
    TableKit,
    UploadImage,
    TaskList,
    TaskItem.configure({ nested: true }),
    TaskInputRule,
    SlashCommand.configure({
      onImageUpload: params.onImageUpload,
      onInsertUploading: params.onInsertUploading,
      onPickFromLibrary: params.onPickFromLibrary,
    }),
    imageFileHandler(params.getCoordinator),
  ];
}
