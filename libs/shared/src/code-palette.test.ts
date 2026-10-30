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
  'bright',
  'foreground',
  'secondary',
  'string',
  'muted',
  'comment',
  'subtle',
  'clay',
  'gutter',
];

// Clay 的层级：越靠前与底色对比越强。
const LADDER: (keyof SyntaxPalette)[] = [
  'bright',
  'foreground',
  'secondary',
  'string',
  'muted',
  'comment',
  'subtle',
];

// 只有字符串和函数定义带色相，其余语法灰 R = G = B。
const NEUTRAL_ROLES: (keyof SyntaxPalette)[] = ['secondary', 'muted', 'comment', 'subtle'];

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

  for (const theme of ['light', 'dark'] as const) {
    it(`orders ${theme} syntax colors by contrast on the code block`, () => {
      const syntax = CODE_PALETTES[theme].syntax;
      const ratios = LADDER.map((role) => contrast(syntax[role], syntax.background));
      for (let i = 1; i < ratios.length; i++) {
        expect(ratios[i], `${LADDER[i]} below ${LADDER[i - 1]}`).toBeLessThan(ratios[i - 1]);
      }
    });

    it(`keeps ${theme} structural grays neutral`, () => {
      const syntax = CODE_PALETTES[theme].syntax;
      for (const role of NEUTRAL_ROLES) {
        const hex = syntax[role];
        expect(hex.slice(1, 3), role).toBe(hex.slice(3, 5));
        expect(hex.slice(3, 5), role).toBe(hex.slice(5, 7));
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
  it('marks function definitions with clay and never uses italic', () => {
    const palette = CODE_PALETTES.light.syntax;
    const xml = tmTheme('Test', palette);
    expect(xml).toContain('<string>Test</string>');
    expect(xml).toMatch(
      new RegExp(
        `<string>entity\\.name\\.function</string>\\s*<key>settings</key>\\s*<dict>\\s*<key>foreground</key>\\s*<string>${palette.clay}</string>`,
      ),
    );
    expect(xml).not.toContain('italic');
  });
});
