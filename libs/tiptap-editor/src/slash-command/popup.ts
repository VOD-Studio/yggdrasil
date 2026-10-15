import type { SuggestionKeyDownProps, SuggestionProps } from '@tiptap/suggestion';
import type { CommandItem } from './commands';

/** 斜杠命令浮层实例:供 Suggestion render 生命周期驱动。 */
export interface SlashPopup {
  component: HTMLElement;
  updateItems(items: CommandItem[]): void;
  updatePosition(): void;
  onKeyDown(props: SuggestionKeyDownProps): boolean;
  destroy(): void;
}

export function createPopup(props: SuggestionProps<CommandItem>): SlashPopup {
  const component = document.createElement('div');
  component.classList.add('slash-command');

  const list = document.createElement('div');
  list.classList.add('slash-command-list');
  component.appendChild(list);

  let destroyed = false;
  function destroy(): void {
    if (destroyed) return;
    destroyed = true;
    props.editor.off('destroy', destroy);
    component.remove();
  }
  props.editor.on('destroy', destroy);
  let selectedIndex = 0;
  let currentItems: CommandItem[] = [];

  function renderItems(items: CommandItem[]) {
    currentItems = items;
    list.innerHTML = '';
    selectedIndex = 0;

    // 空状态：显示提示，不渲染列表项。
    if (items.length === 0) {
      const empty = document.createElement('div');
      empty.classList.add('slash-command-empty');
      empty.textContent = '无匹配命令';
      list.appendChild(empty);
      return;
    }

    items.forEach((item, index) => {
      const el = document.createElement('div');
      el.classList.add('slash-command-item');
      if (index === 0) el.classList.add('is-selected');

      el.innerHTML = `
        <div class="slash-command-item-icon">${item.icon}</div>
        <div class="slash-command-item-text">
          <div class="slash-command-item-title">${item.title}</div>
          <div class="slash-command-item-desc">${item.description}</div>
        </div>
      `;

      el.addEventListener('click', () => {
        props.command(item);
      });

      el.addEventListener('mouseenter', () => {
        selectedIndex = index;
        updateSelection();
      });

      list.appendChild(el);
    });
  }

  function updateSelection() {
    const children = list.children;
    for (let i = 0; i < children.length; i++) {
      if (i === selectedIndex) {
        children[i].classList.add('is-selected');
      } else {
        children[i].classList.remove('is-selected');
      }
    }
    children[selectedIndex]?.scrollIntoView({ block: 'nearest' });
  }

  function selectItem() {
    if (currentItems[selectedIndex]) {
      props.command(currentItems[selectedIndex]);
    }
  }

  function updatePosition() {
    const rect = props.clientRect?.();
    if (!rect) return;
    component.style.left = `${rect.left}px`;
    component.style.top = `${rect.bottom + 4}px`;
  }

  renderItems(props.items);
  document.body.appendChild(component);
  updatePosition();

  return {
    component,
    updateItems(items: CommandItem[]) {
      renderItems(items);
    },
    updatePosition,
    onKeyDown({ event }: SuggestionKeyDownProps): boolean {
      // 空列表时不拦截键盘：避免 % 0 产生 NaN，也避免吞掉 Enter（让用户正常输入）。
      // Escape 仍拦截（关闭浮层）。
      if (event.key === 'Escape') {
        event.preventDefault();
        return true;
      }
      if (currentItems.length === 0) {
        return false;
      }
      if (event.key === 'ArrowUp') {
        event.preventDefault();
        selectedIndex = (selectedIndex - 1 + currentItems.length) % currentItems.length;
        updateSelection();
        return true;
      }
      if (event.key === 'ArrowDown') {
        event.preventDefault();
        selectedIndex = (selectedIndex + 1) % currentItems.length;
        updateSelection();
        return true;
      }
      if (event.key === 'Enter') {
        event.preventDefault();
        selectItem();
        return true;
      }
      return false;
    },
    destroy,
  };
}
