// Stable extension and public exports; interaction modules own their own lifetimes.
import { Extension } from '@tiptap/core';
import { PluginKey } from '@tiptap/pm/state';
import { Suggestion } from '@tiptap/suggestion';
import {
  buildSlashCommands,
  type CommandItem,
  matchCommand,
  type SlashCommandOptions,
} from './slash-command/commands';
import { createPopup, type SlashPopup } from './slash-command/popup';

export {
  buildSlashCommands,
  type CommandItem,
  isValidUrl,
  matchCommand,
  type SlashCommandOptions,
} from './slash-command/commands';
export { createPopup } from './slash-command/popup';
export { buildRunnableInfo, type RunnableInfoOpts } from './slash-command/runnable-info';
export { openRunnableModal } from './slash-command/runnable-modal';

const SlashCommandPluginKey = new PluginKey('slashCommand');

/**
 * 斜杠命令扩展。
 *
 * `onImageUpload` / `onPickFromLibrary` 通过 `addOptions` 注入，
 * "上传图片"/"素材库"命令据此决定是否出现（见 buildSlashCommands）。
 */
export const SlashCommand = Extension.create<SlashCommandOptions>({
  name: 'slashCommand',

  addOptions() {
    return {
      onImageUpload: undefined,
      onInsertUploading: undefined,
      onPickFromLibrary: undefined,
    };
  },

  addProseMirrorPlugins() {
    // 命令集由注入回调决定（gating 逻辑见 buildSlashCommands，抽出以便单元测试）。
    const COMMANDS = buildSlashCommands(this.options);

    return [
      Suggestion<CommandItem>({
        pluginKey: SlashCommandPluginKey,
        editor: this.editor,
        char: '/',
        items: ({ query }) => {
          return COMMANDS.filter((item) => matchCommand(item, query));
        },
        render() {
          let popup: SlashPopup | null = null;

          return {
            onStart(props) {
              popup = createPopup(props);
            },
            onUpdate(props) {
              if (!popup) return;
              popup.updateItems(props.items);
              popup.updatePosition();
            },
            onKeyDown(props) {
              if (!popup) return false;
              return popup.onKeyDown(props);
            },
            onExit() {
              if (popup) {
                popup.destroy();
                popup = null;
              }
            },
          };
        },
        command: ({ editor, range, props: item }) => {
          // 防御性重算 range：Suggestion 的 range 在某些输入路径下会过时
          //（停在首次输入 '/' 的位置，不随后续字符更新），导致 deleteRange 只删 '/'，
          // 命令文本（如 'code'）残留进新节点。基于当前 selection 重算：
          // 从光标往前在同一段落内找触发字符 '/'，删到光标。
          const { from } = editor.state.selection;
          const $from = editor.state.doc.resolve(from);
          const text = $from.parent.textBetween(0, $from.parentOffset, '', '');
          const slashIdx = text.lastIndexOf('/');
          const effectiveRange =
            slashIdx >= 0 ? { from: $from.start() + slashIdx, to: from } : range;
          item.command({ editor, range: effectiveRange });
        },
      }),
    ];
  },
});
