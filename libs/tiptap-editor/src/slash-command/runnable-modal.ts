import type { Editor } from '@tiptap/core';
import { extractLang, extractOverridesJson } from '../highlight';
import { createLangSelect, SVG_NS } from './language-select';
import {
  buildRunnableInfo,
  MEMORY_RANGE,
  RUNNABLE_DEFAULTS,
  RUNNABLE_LANGS,
  TIMEOUT_RANGE,
} from './runnable-info';

const activeModals = new WeakMap<Editor, () => void>();

/**
 * 打开「可运行代码块」配置模态框。
 *
 * 两种模式：
 * - 插入模式（默认）：确认后 `setCodeBlock({ language })` 插入新块。
 * - 编辑模式（传 `editPos` + `currentInfo`）：回填当前块的 lang/overrides，确认后
 *   `setNodeMarkup(editPos, ..., { language })` 原地更新该块的 language 属性。
 *   供 CodeBlockNodeView 的语言标签点击触发（创建后修改语言/overrides）。
 *
 * 作者选语言 + 可选 overrides（超时/内存/网络）。
 * 任一 overrides 字段被改动即 dirty；dirty=false 用 'python runnable'（无 JSON）。
 * Esc / 遮罩点击 / 取消按钮 / × → 关闭不改动。
 * 控件对齐站点设计系统：自定义下拉（FormSelect）、INPUT_CLASS 契约输入框、
 * 全站 .ygg-cb 复选框（src/components/ui.rs Checkbox）、BTN_PRIMARY 契约主按钮；
 * 配色全部走 --color-paper-* token，亮暗主题自动切换。
 */
export function openRunnableModal(editor: Editor, editPos?: number, currentInfo?: string): void {
  activeModals.get(editor)?.();
  if (editor.isDestroyed) return;
  let closed = false;
  let keydownTimer: ReturnType<typeof setTimeout> | undefined;
  const onDestroy = () => dispose(false);
  const isEdit = editPos !== undefined;
  // 编辑模式：从 currentInfo 回填 lang + overrides；插入模式用默认值
  const initialLang = isEdit ? extractLang(currentInfo ?? '') : 'python';
  const initialOverrides = isEdit ? extractOverridesJson(currentInfo ?? '') : '';
  const parsedOverrides = initialOverrides
    ? (() => {
        try {
          return JSON.parse(initialOverrides);
        } catch {
          return null;
        }
      })()
    : null;
  const state = {
    lang: RUNNABLE_LANGS.some((l) => l.value === initialLang) ? initialLang : 'python',
    timeoutSecs: parsedOverrides?.timeout_secs ?? RUNNABLE_DEFAULTS.timeoutSecs,
    memoryMb: parsedOverrides?.memory_mb ?? RUNNABLE_DEFAULTS.memoryMb,
    allowNetwork: parsedOverrides?.allow_network ?? RUNNABLE_DEFAULTS.allowNetwork,
    // 编辑模式下若有 overrides 即 dirty（保留现有 overrides 除非作者改动）
    dirty: isEdit && parsedOverrides !== null,
  };

  const mask = document.createElement('div');
  mask.className = 'tiptap-runnable-modal-mask animate-modal-overlay-enter';

  const modal = document.createElement('div');
  modal.className = 'tiptap-runnable-modal animate-modal-panel-enter';

  // 头部：标题 + × 关闭（对齐 admin 弹窗头部契约，如 mcp.rs PlaintextModal）
  const header = document.createElement('div');
  header.className = 'tiptap-runnable-modal-header';
  const title = document.createElement('div');
  title.className = 'tiptap-runnable-modal-title';
  title.textContent = isEdit ? '编辑可运行代码块' : '插入可运行代码块';
  const closeBtn = document.createElement('button');
  closeBtn.className = 'tiptap-runnable-modal-close';
  closeBtn.type = 'button';
  closeBtn.textContent = '×';
  closeBtn.setAttribute('aria-label', '关闭');
  closeBtn.addEventListener('click', close);
  header.appendChild(title);
  header.appendChild(closeBtn);
  modal.appendChild(header);

  // 语言选择（自定义下拉，对齐 FormSelect；原生 select 弹出层由 OS 渲染、无法跟随主题）
  const langRow = document.createElement('div');
  langRow.className = 'tiptap-runnable-field';
  const langLabel = document.createElement('span');
  langLabel.className = 'tiptap-runnable-label';
  langLabel.textContent = '语言';
  const langSelect = createLangSelect(RUNNABLE_LANGS, state.lang, (value) => {
    state.lang = value;
    updatePreview();
  });
  langRow.appendChild(langLabel);
  langRow.appendChild(langSelect.el);
  modal.appendChild(langRow);

  /** 数字输入行（label + INPUT_CLASS 契约输入框），值改动时联动预览与校验。 */
  function numberField(
    labelText: string,
    inputId: string,
    range: { readonly min: number; readonly max: number },
    initial: number,
    fallback: number,
    onInput: (v: number) => void,
  ): { row: HTMLElement; input: HTMLInputElement } {
    const row = document.createElement('label');
    row.className = 'tiptap-runnable-field';
    const lab = document.createElement('span');
    lab.className = 'tiptap-runnable-label';
    lab.textContent = labelText;
    const input = document.createElement('input');
    input.id = inputId;
    input.className = 'tiptap-runnable-input';
    input.type = 'number';
    input.min = String(range.min);
    input.max = String(range.max);
    input.value = String(initial);
    input.addEventListener('input', () => {
      onInput(Number(input.value) || fallback);
      state.dirty = true;
      updatePreview();
      updateInsertEnabled();
    });
    row.appendChild(lab);
    row.appendChild(input);
    return { row, input };
  }

  const timeoutField = numberField(
    '超时（秒）',
    'runnable-timeout',
    TIMEOUT_RANGE,
    state.timeoutSecs,
    RUNNABLE_DEFAULTS.timeoutSecs,
    (v) => {
      state.timeoutSecs = v;
    },
  );
  modal.appendChild(timeoutField.row);

  const memField = numberField(
    '内存（MB）',
    'runnable-memory',
    MEMORY_RANGE,
    state.memoryMb,
    RUNNABLE_DEFAULTS.memoryMb,
    (v) => {
      state.memoryMb = v;
    },
  );
  modal.appendChild(memField.row);

  // 网络（复选框复用全站 .ygg-cb 结构与样式，见 src/components/ui.rs Checkbox）
  const netRow = document.createElement('div');
  netRow.className = 'tiptap-runnable-field';
  const netLabel = document.createElement('label');
  netLabel.className = 'tiptap-runnable-check-row';
  const netBox = document.createElement('span');
  netBox.className = 'ygg-cb';
  const netInput = document.createElement('input');
  netInput.id = 'runnable-network';
  netInput.type = 'checkbox';
  netInput.checked = state.allowNetwork;
  netInput.addEventListener('change', () => {
    state.allowNetwork = netInput.checked;
    state.dirty = true;
    updatePreview();
  });
  const netMark = document.createElementNS(SVG_NS, 'svg');
  netMark.setAttribute('class', 'ygg-cb-mark');
  netMark.setAttribute('viewBox', '0 0 16 16');
  const netPath = document.createElementNS(SVG_NS, 'path');
  netPath.setAttribute('class', 'ygg-cb-check');
  netPath.setAttribute('d', 'M3.5 8.5l3 3 6-6.5');
  netMark.appendChild(netPath);
  netBox.appendChild(netInput);
  netBox.appendChild(netMark);
  const netText = document.createElement('span');
  netText.textContent = '允许网络';
  netLabel.appendChild(netBox);
  netLabel.appendChild(netText);
  netRow.appendChild(netLabel);
  modal.appendChild(netRow);

  // 预览
  const preview = document.createElement('div');
  preview.className = 'tiptap-runnable-preview';
  modal.appendChild(preview);

  // 按钮
  const actions = document.createElement('div');
  actions.className = 'tiptap-runnable-actions';
  const cancelBtn = document.createElement('button');
  cancelBtn.className = 'cancel';
  cancelBtn.type = 'button';
  cancelBtn.textContent = '取消';
  cancelBtn.addEventListener('click', close);
  const insertBtn = document.createElement('button');
  insertBtn.className = 'insert';
  insertBtn.type = 'button';
  insertBtn.textContent = isEdit ? '保存' : '插入';
  insertBtn.addEventListener('click', insert);
  actions.appendChild(cancelBtn);
  actions.appendChild(insertBtn);
  modal.appendChild(actions);

  mask.appendChild(modal);
  mask.addEventListener('click', (e) => {
    // 仅点击遮罩本身（非卡片）时关闭
    if (e.target === mask) close();
  });

  function updatePreview(): void {
    preview.textContent = `\`\`\`${buildRunnableInfo(state)}`;
  }

  /** 校验数字字段：全合法才启用「插入」。 */
  function updateInsertEnabled(): void {
    const t = Number(timeoutField.input.value);
    const m = Number(memField.input.value);
    insertBtn.disabled = !(
      t >= TIMEOUT_RANGE.min &&
      t <= TIMEOUT_RANGE.max &&
      m >= MEMORY_RANGE.min &&
      m <= MEMORY_RANGE.max
    );
  }

  function insert(): void {
    const language = buildRunnableInfo(state);
    if (isEdit && editPos !== undefined) {
      // 编辑模式：原地更新块的 language 属性（保留块内代码内容）。
      // Tiptap chain 无 setNodeMarkup，用原生 tr。
      const tr = editor.state.tr;
      tr.setNodeMarkup(editPos, undefined, { language });
      editor.view.dispatch(tr);
    } else {
      // 插入模式：新建 codeBlock
      editor.chain().setCodeBlock({ language }).run();
    }
    close();
  }

  function close(): void {
    dispose(true);
  }

  function dispose(restoreFocus: boolean): void {
    if (closed) return;
    closed = true;
    clearTimeout(keydownTimer);
    document.removeEventListener('keydown', onKeydown);
    editor.off('destroy', onDestroy);
    activeModals.delete(editor);
    mask.remove();
    // Tiptap 的 focus command 会延迟到下一帧；快速重开时会从新模态框抢走焦点，
    // 使 Escape 落到编辑器而被其快捷键消费。同步恢复已有 selection 即可。
    if (restoreFocus && !editor.isDestroyed) editor.view.focus();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key === 'Enter' && !insertBtn.disabled) {
      // Enter 在 number input 内提交（浏览器原生 number input 的 Enter 不会触发 click）。
      // 语言下拉展开时 Enter 由触发器自身消费（选中高亮项并 stopPropagation 前的
      // preventDefault），此处经 isOpen() 避让；按钮（触发器/取消/插入/×）的 Enter
      // 走原生 click，不在此处理。网络 checkbox 的 tagName 也是 'input'，Enter 触发
      // 提交而非切换（checkbox 原生用 Space 切换），符合模态框 Enter=确认的惯例。
      if (langSelect.isOpen()) return;
      const tag = (document.activeElement?.tagName ?? '').toLowerCase();
      if (tag === 'input') {
        e.preventDefault();
        insert();
      }
    }
  }

  document.body.appendChild(mask);
  // 延迟注册 keydown 监听：openRunnableModal 可能在 slash 命令的 Enter keydown 事件处理中
  // 同步调用。若立即注册，该 Enter 事件继续冒泡到 document 时会被模态框的 onKeydown 捕获，
  // 触发 insert() → close()——模态框在同一 tick 内被创建又被销毁，用户看不到。
  // setTimeout(0) 推迟到下一个宏任务，确保触发事件已完成传播。
  activeModals.set(editor, () => dispose(false));
  editor.on('destroy', onDestroy);
  keydownTimer = setTimeout(() => {
    if (!closed) document.addEventListener('keydown', onKeydown);
  }, 0);
  updatePreview();
  updateInsertEnabled();
  langSelect.focus();
}
