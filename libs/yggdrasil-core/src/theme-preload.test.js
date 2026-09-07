import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// 执行 Rust 实际内联到 SSR 页面的脚本，覆盖 WASM 尚未加载时的主题恢复。
const source = readFileSync(resolve(import.meta.dirname, '../../../src/theme.rs'), 'utf8');
const script = source.match(/const THEME_PRELOAD_SCRIPT: &str = r#"([\s\S]*?)"#;/)?.[1];
const style = source.match(/const THEME_ICON_STYLE: &str = r#"([\s\S]*?)"#;/)?.[1];
if (!script || !style) throw new Error('Missing theme preload script or style');
const preload = new Function(script);

beforeEach(() => {
  document.head.innerHTML = `<style>${style}</style>`;
  document.body.innerHTML = `<button class="theme-toggle">
    <svg class="theme-icon-light"></svg>
    <svg class="theme-icon-dark"></svg>
    <svg class="theme-icon-system"></svg>
  </button>`;
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.removeItem('yggdrasil-theme');
  document.documentElement.removeAttribute('data-theme-mode');
  document.documentElement.classList.remove('dark');
  document.head.innerHTML = '';
  document.body.innerHTML = '';
});

describe('SSR theme preload before hydration', () => {
  it.each([
    ['light', true, 'light', false],
    ['light', false, 'light', false],
    ['dark', true, 'dark', true],
    ['dark', false, 'dark', true],
    [null, true, 'system', true],
    [null, false, 'system', false],
    ['invalid', true, 'system', true],
  ])('stored=%s, systemDark=%s', (stored, systemDark, mode, dark) => {
    if (stored === null) localStorage.removeItem('yggdrasil-theme');
    else localStorage.setItem('yggdrasil-theme', stored);
    vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: systemDark });

    preload();

    expect(document.documentElement.getAttribute('data-theme-mode')).toBe(mode);
    expect(document.documentElement.classList.contains('dark')).toBe(dark);
    expect(localStorage.getItem('yggdrasil-theme')).toBe(stored);
    const visible = [...document.querySelectorAll('.theme-toggle svg')].filter(
      (icon) => getComputedStyle(icon).display !== 'none',
    );
    expect(visible.map((icon) => icon.getAttribute('class'))).toEqual([`theme-icon-${mode}`]);
  });

  it('storage unavailable still follows the system', () => {
    vi.spyOn(window, 'localStorage', 'get').mockImplementation(() => {
      throw new Error('Storage disabled');
    });
    vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: true });

    preload();

    expect(document.documentElement.getAttribute('data-theme-mode')).toBe('system');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });
});
