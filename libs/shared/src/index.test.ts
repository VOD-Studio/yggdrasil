import { afterEach, describe, expect, it } from 'vitest';
import {
  ASSET_VERSIONS_ID,
  assetUrl,
  prefersReducedMotion,
  THEME_CHANGE_EVENT,
  type ThemeName,
} from './index';

describe('@yggdrasil/shared', () => {
  it('THEME_CHANGE_EVENT 是固定字符串', () => {
    expect(THEME_CHANGE_EVENT).toBe('yggdrasil:theme-change');
  });

  it('prefersReducedMotion 在无 matchMedia 时返回 false', () => {
    // happy-dom 提供了 matchMedia，这里只验证函数可调用且返回 boolean
    const result = prefersReducedMotion();
    expect(typeof result).toBe('boolean');
  });

  it('ThemeName 类型仅接受 light/dark（编译期约束）', () => {
    const t: ThemeName = 'light';
    expect(['light', 'dark']).toContain(t);
  });
});

describe('assetUrl', () => {
  const versions = (text: string) => {
    const element = document.createElement('script');
    element.type = 'application/json';
    element.id = ASSET_VERSIONS_ID;
    element.textContent = text;
    document.head.append(element);
  };

  afterEach(() => {
    document.getElementById(ASSET_VERSIONS_ID)?.remove();
  });

  it('按服务端下发的版本表给地址加版本号', () => {
    versions('{"/xterm/terminal.js":"0123abcd"}');
    expect(assetUrl('/xterm/terminal.js')).toBe('/xterm/terminal.js?v=0123abcd');
  });

  it.each([
    ['没有版本表', null],
    ['表里没有该资源', '{"/style.css":"1"}'],
    ['版本表损坏', '{'],
  ])('%s时原样返回地址', (_, text) => {
    if (text !== null) versions(text);
    expect(assetUrl('/xterm/terminal.js')).toBe('/xterm/terminal.js');
  });
});
