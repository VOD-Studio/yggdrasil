import { describe, expect, it } from 'vitest';
import { CODE_PALETTES, type SyntaxPalette, tmTheme } from './code-palette';

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = Number.parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const TEXT_ROLES: (keyof SyntaxPalette)[] = [
  'foreground',
  'comment',
  'keyword',
  'string',
  'number',
  'function',
  'type',
  'punctuation',
  'regex',
  'inserted',
  'deleted',
  'gutter',
];

// 代码出现在页面底、代码块底与浮起的卡片上，三处都要达标。
const SURFACES = {
  light: ['#f5f5f3', '#fafaf8', '#ffffff'],
  dark: ['#0f0f0e', '#161615', '#20201e'],
} as const;

describe('CODE_PALETTES', () => {
  for (const theme of ['light', 'dark'] as const) {
    it(`keeps ${theme} syntax colors at 4.5:1 or more`, () => {
      const syntax = CODE_PALETTES[theme].syntax;
      for (const role of TEXT_ROLES) {
        for (const surface of SURFACES[theme]) {
          expect(contrast(syntax[role], surface), `${role} on ${surface}`).toBeGreaterThanOrEqual(
            4.5,
          );
        }
      }
    });
  }

  it('does not reuse the retired green accent', () => {
    const all = JSON.stringify(CODE_PALETTES).toLowerCase();
    expect(all).not.toContain('#40a02b');
    expect(all).not.toContain('#a6e3a1');
  });
});

describe('tmTheme', () => {
  it('maps every syntax role to a scope with its color', () => {
    const xml = tmTheme('Test', CODE_PALETTES.light.syntax);
    expect(xml).toContain('<string>Test</string>');
    expect(xml).toContain('<string>#a3471c</string>');
    expect(xml).toMatch(/<string>comment, punctuation\.definition\.comment<\/string>/);
    expect(xml).not.toContain('italic');
  });
});
