/** 原生过渡已接管本次入场，禁用重复的页内动画。 */
export function markPageEntryHandled(wrapper: HTMLElement): void {
  wrapper.setAttribute('data-vt-enter-handled', 'true');
}
