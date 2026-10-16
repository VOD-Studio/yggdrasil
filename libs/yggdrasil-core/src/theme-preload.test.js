import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// 执行 Rust 实际内联到 SSR 页面的脚本，覆盖 WASM 尚未加载时的主题恢复。
const source = readFileSync(resolve(import.meta.dirname, '../../../src/theme.rs'), 'utf8');
const script = source.match(/const THEME_PRELOAD_SCRIPT: &str = r#"([\s\S]*?)"#;/)?.[1];
const style = source.match(/const THEME_SEGMENT_STYLE: &str = r#"([\s\S]*?)"#;/)?.[1];
if (!script || !style) throw new Error('Missing theme preload script or style');
const preload = new Function(script);

beforeEach(() => {
  // 选中项的底色用可识别的具体值，便于从计算样式里判断哪一段被选中。
  document.head.innerHTML = `<style>:root { --color-surface-2: rgb(1, 2, 3); --color-surface-3: rgb(1, 2, 3); }${style}</style>`;
  document.body.innerHTML = `<div class="theme-seg">
    <button data-mode="light"></button>
    <button data-mode="dark"></button>
    <button data-mode="system"></button>
  </div>`;
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
    const selected = [...document.querySelectorAll('.theme-seg [data-mode]')].filter(
      (item) => getComputedStyle(item).backgroundColor === 'rgb(1, 2, 3)',
    );
    expect(selected.map((item) => item.getAttribute('data-mode'))).toEqual([mode]);
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
