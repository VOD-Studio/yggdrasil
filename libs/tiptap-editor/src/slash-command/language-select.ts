export const SVG_NS = 'http://www.w3.org/2000/svg';

/** 描边图标（与 Dioxus 侧 FormSelect 的 chevron / 选中对勾同源）。 */
function makeSvgIcon(pathD: string, className: string): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('class', className);
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '2');
  const path = document.createElementNS(SVG_NS, 'path');
  path.setAttribute('stroke-linecap', 'round');
  path.setAttribute('stroke-linejoin', 'round');
  path.setAttribute('d', pathD);
  svg.appendChild(path);
  return svg;
}

/** createLangSelect 返回的控件句柄。 */
interface LangSelectWidget {
  /** 根元素（relative 容器，挂进表单行）。 */
  el: HTMLElement;
  /** 面板当前是否展开（模态框 Enter 提交据此避让）。 */
  isOpen(): boolean;
  /** 聚焦触发器（模态框打开时的初始焦点）。 */
  focus(): void;
}

/**
 * 自定义语言下拉：对齐 Dioxus 侧 FormSelect（src/components/forms.rs）的视觉与
 * 交互契约——原生 <select> 的弹出列表由 OS 渲染、无法跟随主题，故重写为
 * `button[aria-haspopup=listbox]` + 绝对定位面板：
 * - 打开时透明遮罩拦截外部点击关闭；选项 mousedown 阻止默认行为，焦点始终留在触发器；
 * - 键盘：↑↓ 循环高亮、Enter/Space 选中、Esc 关闭（不冒泡到模态框）、
 *   Home/End 跳首尾、Tab 关闭并自然流转焦点；
 * - 视口下方空间不足且上方更宽余时向上展开（FormSelect should_flip 同款判定）；
 * - 面板进出场复用全站 animate-select-enter（input.css）。
 */
export function createLangSelect(
  options: readonly { value: string; label: string }[],
  initial: string,
  onChange: (value: string) => void,
): LangSelectWidget {
  const root = document.createElement('div');
  root.className = 'tiptap-select';

  const trigger = document.createElement('button');
  trigger.type = 'button';
  trigger.className = 'tiptap-select-trigger';
  trigger.setAttribute('aria-haspopup', 'listbox');
  trigger.setAttribute('aria-expanded', 'false');
  const label = document.createElement('span');
  trigger.appendChild(label);
  trigger.appendChild(makeSvgIcon('M6 9l6 6 6-6', 'tiptap-select-chevron'));
  root.appendChild(trigger);

  let open = false;
  let selected = Math.max(
    0,
    options.findIndex((o) => o.value === initial),
  );
  let active = selected;
  let overlay: HTMLElement | null = null;
  let panel: HTMLElement | null = null;

  label.textContent = options[selected].label;

  /** 视口下方空间不足且上方更宽余时向上展开。 */
  function shouldFlip(): boolean {
    const rect = trigger.getBoundingClientRect();
    // 行高 40px（py-2.5+行盒）×选项数 + 面板 chrome（边框+padding），封顶 254px。
    const panelHeight = Math.min(options.length * 40 + 14, 254);
    const below = window.innerHeight - rect.bottom;
    const above = rect.top;
    return below < panelHeight + 14 && above > below;
  }

  /** 同步 active/selected 的展示态（不重排 DOM，避免 hover 时选项抖动）。 */
  function paintOptions(): void {
    if (!panel) return;
    const items = panel.querySelectorAll<HTMLElement>('.tiptap-select-option');
    items.forEach((li, i) => {
      li.classList.toggle('active', i === active);
      li.classList.toggle('selected', i === selected);
      li.setAttribute('aria-selected', String(i === selected));
    });
  }

  function scrollActiveIntoView(): void {
    (panel?.children[active] as HTMLElement | undefined)?.scrollIntoView({ block: 'nearest' });
  }

  function closePanel(): void {
    if (!open) return;
    open = false;
    trigger.setAttribute('aria-expanded', 'false');
    root.classList.remove('open');
    overlay?.remove();
    panel?.remove();
    overlay = null;
    panel = null;
  }

  function select(i: number): void {
    selected = i;
    label.textContent = options[i].label;
    onChange(options[i].value);
    closePanel();
  }

  function openPanel(): void {
    if (open) return;
    open = true;
    active = selected;
    trigger.setAttribute('aria-expanded', 'true');
    root.classList.add('open');

    overlay = document.createElement('div');
    overlay.className = 'tiptap-select-overlay';
    overlay.addEventListener('click', closePanel);

    panel = document.createElement('ul');
    panel.className = 'tiptap-select-panel animate-select-enter';
    panel.setAttribute('role', 'listbox');
    if (shouldFlip()) panel.classList.add('flip');
    options.forEach((opt, i) => {
      const li = document.createElement('li');
      li.className = 'tiptap-select-option';
      li.setAttribute('role', 'option');
      const text = document.createElement('span');
      text.textContent = opt.label;
      li.appendChild(text);
      li.appendChild(makeSvgIcon('M20 6L9 17l-5-5', 'tiptap-select-check'));
      // 阻止 mousedown 默认行为：点击选项不夺走触发器焦点。
      li.addEventListener('mousedown', (e) => e.preventDefault());
      li.addEventListener('mouseenter', () => {
        active = i;
        paintOptions();
      });
      li.addEventListener('click', () => select(i));
      panel?.appendChild(li);
    });

    root.appendChild(overlay);
    root.appendChild(panel);
    paintOptions();
    scrollActiveIntoView();
  }

  trigger.addEventListener('click', () => {
    // 打开态下触发器被透明遮罩盖住，点击落在遮罩上即关闭；这里只需处理「未开 → 开」。
    if (!open) openPanel();
  });

  trigger.addEventListener('keydown', (e) => {
    if (!open) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        openPanel();
      }
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      active = (active + 1) % options.length;
      paintOptions();
      scrollActiveIntoView();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      active = (active - 1 + options.length) % options.length;
      paintOptions();
      scrollActiveIntoView();
    } else if (e.key === 'Home') {
      e.preventDefault();
      active = 0;
      paintOptions();
      scrollActiveIntoView();
    } else if (e.key === 'End') {
      e.preventDefault();
      active = options.length - 1;
      paintOptions();
      scrollActiveIntoView();
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      select(active);
    } else if (e.key === 'Escape') {
      // stopPropagation：只关面板，不触发模态框的 Esc 关闭。
      e.preventDefault();
      e.stopPropagation();
      closePanel();
    } else if (e.key === 'Tab') {
      // 不拦截：关闭后焦点自然流转到下一个控件。
      closePanel();
    }
  });

  return { el: root, isOpen: () => open, focus: () => trigger.focus() };
}
