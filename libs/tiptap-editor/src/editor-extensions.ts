/**
 * 编辑器扩展装配工厂：full（后台文章编辑器）与 comment（评论区）两个变体。
 *
 * comment 是 full 的**真子集**——同一 bundle、同一批扩展实现，只裁剪装配：
 *
 * | 扩展                | full | comment | 说明 |
 * |---------------------|------|---------|------|
 * | StarterKit          | ✔    | ✔（裁剪）| comment 关 heading/hr（评论标题服务端转 strong） |
 * | Markdown            | ✔    | ✔       | getMarkdown 进出都靠它 |
 * | InlineMath/DisplayMath | ✔ | ✔       | 评论渲染器支持 $..$，输入规则给 WYSIWYG |
 * | Footnote*           | ✔    | —       | 评论用脚注无意义 |
 * | CodeBlockLowlight   | ✔+NV | ✔       | comment 去掉 NodeView（无运行/无 mermaid 预览） |
 * | CodeBlockBackspaceFix | ✔  | ✔       | |
 * | TableKit            | ✔    | —       | 评论框表格 UI 过重 |
 * | UploadImage         | ✔    | ✔       | 上传占位符 UX 与后台完全一致（需求核心） |
 * | TaskList/TaskItem/InputRule | ✔ | — | |
 * | SlashCommand        | ✔    | —       | 评论无斜杠命令 |
 * | FileHandler         | ✔    | ✔       | 粘贴/拖放图片 → coordinator |
 * | Placeholder         | —    | ✔       | full 用 CSS br 方案（勿动，防回归） |
 * | CommentBubbleMenu   | —    | ✔       | 评论唯一格式 UI（选中浮出） |
 */

import type { AnyExtension } from '@tiptap/core';
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
import { FileHandler } from '@tiptap/extension-file-handler';
import { TaskItem, TaskList } from '@tiptap/extension-list';
import { Placeholder } from '@tiptap/extension-placeholder';
import { TableKit } from '@tiptap/extension-table';
import { Markdown } from '@tiptap/markdown';
import StarterKit from '@tiptap/starter-kit';
import { CodeBlockBackspaceFix } from './code-block-backspace-fix';
import { CodeBlockNodeView } from './code-block-view';
import { CommentBubbleMenu } from './comment-bubble-menu';
import { FootnoteDef, FootnoteNumbering, FootnoteRef } from './footnote';
import { lowlight } from './highlight';
import { DisplayMath, InlineMath } from './math';
import { SlashCommand } from './slash-command';
import { TaskInputRule } from './task-input-rule';
import type { UploadCoordinator } from './upload-coordinator';
import { UploadImage } from './upload-image';

export type EditorVariant = 'full' | 'comment';

export interface BuildExtensionsParams {
  variant: EditorVariant;
  /** 评论模式 Placeholder 扩展文案；full 忽略（走既有 CSS 占位）。 */
  placeholder?: string;
  onImageUpload?: (file: File) => Promise<string>;
  /** slash 命令「上传图片」走占位符上传（仅 full；闭包延迟读 coordinator）。 */
  onInsertUploading?: (file: File) => void;
  onPickFromLibrary?: () => void;
  /** FileHandler 粘贴/拖放入口延迟读 coordinator（它在 editor 创建后才实例化）。 */
  getCoordinator: () => UploadCoordinator | null;
}

/** 按变体装配扩展数组。数组顺序敏感：数学/脚注节点必须在 Markdown 之后注册。 */
export function buildExtensions(params: BuildExtensionsParams): AnyExtension[] {
  const isComment = params.variant === 'comment';

  const extensions: AnyExtension[] = [
    StarterKit.configure({
      // 评论无标题层级（服务端渲染器会把标题转 <strong>）与分割线；
      // 用条件展开而非 undefined 赋值，避免 configure 深合并吞掉默认值。
      ...(isComment
        ? { heading: false as const, horizontalRule: false as const }
        : { heading: { levels: [1, 2, 3] as const } }),
      link: {
        openOnClick: false,
        autolink: true,
        linkOnPaste: true,
        HTMLAttributes: { rel: 'noopener noreferrer', target: '_blank' },
      },
      codeBlock: false,
    }),
    Markdown,
    // 数学公式节点必须在 Markdown 之后注册（MarkdownManager 在 onBeforeCreate
    // 收集 markdown spec；见 math.ts 注释）。
    InlineMath,
    DisplayMath,
  ];

  // 脚注仅 full（同样依赖 markdown spec 收集时机，须在 Markdown 之后）。
  if (!isComment) {
    extensions.push(FootnoteRef, FootnoteDef, FootnoteNumbering);
  }

  // 代码块：full 带 NodeView（语言选择/mermaid 预览/运行按钮），comment 纯高亮。
  if (isComment) {
    extensions.push(CodeBlockLowlight.configure({ lowlight }));
  } else {
    extensions.push(
      CodeBlockLowlight.configure({ lowlight }).extend({
        addNodeView() {
          return ({ node, editor, getPos }) => new CodeBlockNodeView({ node, editor, getPos });
        },
      }),
    );
  }
  extensions.push(CodeBlockBackspaceFix);

  // 注意保持 full 原有相对顺序：TableKit → UploadImage → TaskList → … → SlashCommand。
  if (!isComment) {
    extensions.push(TableKit);
  }

  // 图片上传（占位符 UX 核心）：两变体共享 UploadImage + FileHandler。
  extensions.push(UploadImage);

  if (!isComment) {
    extensions.push(
      TaskList,
      TaskItem.configure({ nested: true }),
      // 让手动输入 - [ ] / - [x] 直接创建任务列表(priority 1000 抢在 BulletList 前)
      TaskInputRule,
      // 把宿主注入的回调透传给斜杠命令扩展（闭包延迟读取 coordinator）。
      SlashCommand.configure({
        onImageUpload: params.onImageUpload,
        onInsertUploading: params.onInsertUploading,
        onPickFromLibrary: params.onPickFromLibrary,
      }),
    );
  }

  extensions.push(
    FileHandler.configure({
      allowedMimeTypes: ['image/jpeg', 'image/png', 'image/gif', 'image/webp'],
      onPaste: (_editor, files) => {
        files.forEach((file) => {
          params.getCoordinator()?.insertUploading(file);
        });
      },
      onDrop: (_editor, files, pos) => {
        files.forEach((file) => {
          params.getCoordinator()?.insertUploading(file, pos);
        });
      },
    }),
  );

  if (isComment) {
    extensions.push(
      Placeholder.configure({
        placeholder: params.placeholder ?? '写下你的想法…',
      }),
      CommentBubbleMenu,
    );
  }

  return extensions;
}
