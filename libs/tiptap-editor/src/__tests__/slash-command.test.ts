import type { Editor } from '@tiptap/core';
import type { SuggestionProps } from '@tiptap/suggestion';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  buildSlashCommands,
  type CommandItem,
  createPopup,
  isValidUrl,
  matchCommand,
} from '../slash-command';

/**
 * isValidUrl 纯函数测试。
 *
 * 只允许 http(s):// 和 data:image/ 开头，拒绝 javascript: 等危险或非图片 scheme。
 */

describe('isValidUrl', () => {
  describe('接受', () => {
    it.each([
      ['http://example.com', 'http 协议'],
      ['https://example.com', 'https 协议'],
      ['https://example.com/path?q=1#frag', '带路径/查询/片段'],
      ['HTTP://EXAMPLE.COM', 'http 大写（大小写不敏感）'],
      ['Https://Example.Com', '混合大小写'],
      ['http://localhost:3000/img.png', 'localhost + 端口'],
      ['data:image/png;base64,iVBORw0KGgo=', 'data URL png'],
      ['data:image/jpeg;base64,/9j/4AAQ', 'data URL jpeg'],
      ['data:image/svg+xml,<svg/>', 'data URL svg'],
      ['data:image/', 'data:image 前缀（最小匹配）'],
    ])('%s (%s)', (url) => {
      expect(isValidUrl(url)).toBe(true);
    });
  });

  describe('拒绝', () => {
    it.each([
      ['javascript:alert(1)', 'javascript scheme（XSS 风险）'],
      ['JavaScript:alert(1)', 'javascript 大写'],
      ['ftp://example.com/file', 'ftp scheme'],
      ['file:///etc/passwd', 'file scheme'],
      ['mailto:foo@bar.com', 'mailto scheme'],
      ['data:text/html,<script>', '非 image 的 data URL'],
      ['data:application/octet-stream,', '非 image 的 data URL'],
      ['//example.com/img.png', '协议相对 URL（无 scheme）'],
      ['/absolute/path/img.png', '绝对路径'],
      ['relative/path/img.png', '相对路径'],
      ['./img.png', './ 相对'],
      ['example.com/img.png', '无 scheme 的域名'],
      ['httpfoo://x', 'http 前缀但非 http scheme'],
      [' https://example.com', '前导空格（^ 锚定，不匹配）'],
      ['', '空字符串'],
    ])('%s (%s)', (url) => {
      expect(isValidUrl(url)).toBe(false);
    });
  });

  describe('正则边界（scheme 前缀校验，不锚定结尾）', () => {
    it('https:// 后含末尾空格仍被接受（只校验前缀）', () => {
      // 有意为之：isValidUrl 只校验 scheme 前缀，URL 其余部分由浏览器/服务端校验
      expect(isValidUrl('https://example.com ')).toBe(true);
    });
  });
});

/**
 * matchCommand 纯函数测试。
 *
 * 验证中英文互通：title/description（中文）与 keywords（英文别名）任一命中即匹配。
 */

describe('matchCommand', () => {
  /** 构造最小命令项（只含匹配需要的字段）。 */
  const mk = (title: string, description: string, keywords?: string) =>
    ({ title, description, keywords, icon: '', command: () => {} }) as Parameters<
      typeof matchCommand
    >[0];

  describe('中文标题命中', () => {
    it.each([
      ['代码块', '插入代码块', 'code codeblock pre 代码', '代码', 'title 含中文'],
      ['标题 1', '大标题', 'h1 heading 标题', '标题', '标题'],
      ['链接', '插入链接', 'link url a href 链接', '链接', '链接'],
    ])('%s 搜 %s', (title, desc, keywords, query) => {
      expect(matchCommand(mk(title, desc, keywords), query)).toBe(true);
    });
  });

  describe('英文 keywords 别名命中（核心：中英文互通）', () => {
    it.each([
      ['代码块', '插入代码块', 'code codeblock pre 代码', 'code', '/code 命中「代码块」'],
      ['代码块', '插入代码块', 'code codeblock pre 代码', 'pre', '/pre 命中'],
      [
        '可运行代码块',
        '插入可被读者执行的代码块',
        'code run runnable execute 代码 运行',
        'run',
        '/run 命中',
      ],
      ['标题 1', '大标题', 'h1 heading 标题', 'h1', '/h1 命中'],
      ['标题 1', '大标题', 'h1 heading 标题', 'heading', '/heading 命中'],
      ['无序列表', '创建无序列表', 'bullet list ul 列表', 'bullet', '/bullet 命中'],
      ['无序列表', '创建无序列表', 'bullet list ul 列表', 'ul', '/ul 命中'],
      ['任务列表', '创建任务列表', 'task todo checklist 列表', 'todo', '/todo 命中'],
      ['引用', '插入引用块', 'quote blockquote 引用', 'quote', '/quote 命中'],
      ['分割线', '插入水平分割线', 'hr rule divider 分割', 'hr', '/hr 命中'],
      ['表格', '插入 3×3 表格', 'table 表格', 'table', '/table 命中'],
      ['链接', '插入链接', 'link url a href 链接', 'link', '/link 命中'],
      ['链接', '插入链接', 'link url a href 链接', 'href', '/href 命中'],
    ])('「%s」搜 %s (%s)', (title, desc, keywords, query) => {
      expect(matchCommand(mk(title, desc, keywords ?? undefined), query)).toBe(true);
    });
  });

  describe('不命中', () => {
    it.each<[string, string, string | undefined, string, string]>([
      ['代码块', '插入代码块', 'code codeblock 代码', 'image', '无关词'],
      ['链接', '插入链接', 'link url', 'code', '跨命令误命中'],
      ['', '', undefined, 'anything', '空命令'],
    ])('「%s」搜 %s (%s)', (title, desc, keywords, query) => {
      expect(matchCommand(mk(title, desc, keywords), query)).toBe(false);
    });
  });

  describe('大小写不敏感', () => {
    it.each([
      ['CODE', '大写 query'],
      ['Code', '混合大小写'],
      ['code', '小写'],
    ])('搜 %s 都命中「代码块」(%s)', (query) => {
      expect(matchCommand(mk('代码块', '插入代码块', 'code codeblock'), query)).toBe(true);
    });
  });

  describe('无 keywords 字段时仍按 title/description 匹配', () => {
    it('keywords 缺省，搜 title 中文仍命中', () => {
      expect(matchCommand(mk('引用', '插入引用块'), '引用')).toBe(true);
    });
    it('keywords 缺省，搜英文不命中（回归：不会因缺字段报错）', () => {
      expect(matchCommand(mk('引用', '插入引用块'), 'quote')).toBe(false);
    });
  });
});

/**
 * createPopup 空状态测试（happy-dom）。
 *
 * 搜索无结果时浮层应显示「无匹配命令」提示，而非空白卡片。
 */
describe('createPopup 空状态', () => {
  afterEach(() => {
    document.body.innerHTML = '';
  });

  /** 构造最小 SuggestionProps mock。 */
  function mockProps(
    items: Parameters<typeof matchCommand>[0][],
  ): SuggestionProps<Parameters<typeof matchCommand>[0]> {
    return {
      items,
      editor: {} as any,
      range: {} as any,
      query: '',
      text: '',
      command: () => {},
      clientRect: () => null,
    } as unknown as SuggestionProps<Parameters<typeof matchCommand>[0]>;
  }

  it('items 为空时显示「无匹配命令」提示', () => {
    const popup = createPopup(mockProps([]));
    document.body.appendChild(popup.component);
    const empty = document.querySelector('.slash-command-empty');
    expect(empty).not.toBeNull();
    expect(empty?.textContent).toBe('无匹配命令');
    // 且不渲染任何列表项
    expect(document.querySelectorAll('.slash-command-item')).toHaveLength(0);
  });

  it('items 非空时不显示空状态提示', () => {
    const item = { title: '代码块', description: '插入代码块', icon: '<>', command: () => {} };
    const popup = createPopup(mockProps([item]));
    document.body.appendChild(popup.component);
    expect(document.querySelector('.slash-command-empty')).toBeNull();
    expect(document.querySelectorAll('.slash-command-item')).toHaveLength(1);
  });

  it('空列表时 Enter 不被拦截（return false，让回车正常输入）', () => {
    const popup = createPopup(mockProps([]));
    const handled = popup.onKeyDown({
      event: new KeyboardEvent('keydown', { key: 'Enter' }),
    } as any);
    expect(handled).toBe(false);
  });

  it('空列表时 ArrowUp/Down 不被拦截（避免 % 0 产生 NaN）', () => {
    const popup = createPopup(mockProps([]));
    expect(
      popup.onKeyDown({ event: new KeyboardEvent('keydown', { key: 'ArrowUp' }) } as any),
    ).toBe(false);
    expect(
      popup.onKeyDown({ event: new KeyboardEvent('keydown', { key: 'ArrowDown' }) } as any),
    ).toBe(false);
  });

  it('空列表时 Escape 仍关闭浮层（return true）', () => {
    const popup = createPopup(mockProps([]));
    const handled = popup.onKeyDown({
      event: new KeyboardEvent('keydown', { key: 'Escape' }),
    } as any);
    expect(handled).toBe(true);
  });

  it('updateItems 传空数组时切换到空状态', () => {
    const item = { title: '代码块', description: '插入代码块', icon: '<>', command: () => {} };
    const popup = createPopup(mockProps([item]));
    document.body.appendChild(popup.component);
    expect(document.querySelector('.slash-command-empty')).toBeNull();
    popup.updateItems([]);
    expect(document.querySelector('.slash-command-empty')).not.toBeNull();
  });
});

/**
 * buildSlashCommands 命令集 gating 与「素材库」命令行为测试。
 *
 * gating 是编辑器与宿主的契约边界：Rust 侧注入哪些回调，slash 菜单就出现哪些命令。
 */
describe('buildSlashCommands', () => {
  const titles = (cmds: CommandItem[]) => cmds.map((c) => c.title);

  it('无回调：无「上传图片」无「素材库」，保留「图片链接」', () => {
    const ts = titles(buildSlashCommands({}));
    expect(ts).not.toContain('上传图片');
    expect(ts).not.toContain('素材库');
    expect(ts).toContain('图片链接');
  });

  it('仅 onImageUpload：出现「上传图片」，仍无「素材库」', () => {
    const ts = titles(buildSlashCommands({ onImageUpload: async () => '' }));
    expect(ts).toContain('上传图片');
    expect(ts).not.toContain('素材库');
  });

  it('仅 onPickFromLibrary：出现「素材库」，仍无「上传图片」', () => {
    const ts = titles(buildSlashCommands({ onPickFromLibrary: () => {} }));
    expect(ts).toContain('素材库');
    expect(ts).not.toContain('上传图片');
  });

  it('两回调齐备：「素材库」位于「上传图片」与「图片链接」之间', () => {
    const ts = titles(
      buildSlashCommands({ onImageUpload: async () => '', onPickFromLibrary: () => {} }),
    );
    expect(ts.indexOf('素材库')).toBeGreaterThan(ts.indexOf('上传图片'));
    expect(ts.indexOf('素材库')).toBeLessThan(ts.indexOf('图片链接'));
  });

  it('「素材库」命令可被 素材 / library / 图片 搜索命中', () => {
    const cmd = buildSlashCommands({ onPickFromLibrary: () => {} }).find(
      (c) => c.title === '素材库',
    );
    if (!cmd) throw new Error('素材库命令不存在');
    for (const q of ['素材', 'library', '图片']) {
      expect(matchCommand(cmd, q)).toBe(true);
    }
  });

  it('「素材库」命令：先 deleteRange 再触发宿主回调', () => {
    const onPickFromLibrary = vi.fn();
    const cmd = buildSlashCommands({ onPickFromLibrary }).find((c) => c.title === '素材库');
    if (!cmd) throw new Error('素材库命令不存在');
    const run = vi.fn();
    const deleteRange = vi.fn().mockReturnValue({ run });
    const focus = vi.fn().mockReturnValue({ deleteRange });
    const chain = vi.fn().mockReturnValue({ focus });
    const editor = { chain } as unknown as Editor;
    const range = { from: 4, to: 7 };
    cmd.command({ editor, range });
    expect(deleteRange).toHaveBeenCalledWith(range);
    expect(run).toHaveBeenCalledTimes(1);
    expect(onPickFromLibrary).toHaveBeenCalledTimes(1);
  });
});
