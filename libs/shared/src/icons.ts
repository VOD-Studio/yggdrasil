/**
 * 站点图标集：从 lucide-static 取 SVG 节点，生成 Rust 端的 `IconName` 与内联数据
 * （`src/components/ui/icons_data.rs`）。要新增图标，把 Lucide 名字加进 ICONS
 * 后运行 `bun run icons`（libs/shared 下）。
 */

export type IconNode = [tag: string, attrs: Record<string, string | number>][];

/** 站点用到的 Lucide 图标，按用途分组；顺序即 Rust 枚举顺序。 */
export const ICONS: readonly string[] = [
  // 导航与外壳
  'search',
  'x',
  'menu',
  'panel-left',
  'house',
  'layout-dashboard',
  'ellipsis',
  'command',
  'keyboard',
  // 主题
  'sun',
  'moon',
  'monitor',
  // 方向
  'chevron-down',
  'chevron-up',
  'chevron-left',
  'chevron-right',
  'arrow-left',
  'arrow-right',
  'arrow-up',
  'arrow-down',
  'arrow-up-right',
  'external-link',
  'corner-down-left',
  // 编辑操作
  'copy',
  'check',
  'plus',
  'minus',
  'pencil',
  'trash',
  'save',
  'send',
  'reply',
  'upload',
  'download',
  'rotate-ccw',
  'rotate-cw',
  'rotate-ccw-clock',
  'refresh-cw',
  'grip-vertical',
  'funnel',
  'sliders-horizontal',
  'list',
  // 查看
  'zoom-in',
  'zoom-out',
  'eye',
  'eye-off',
  'image',
  'image-off',
  // 运行与状态
  'play',
  'square',
  'loader-circle',
  'circle-check',
  'circle-alert',
  'triangle-alert',
  'info',
  'terminal',
  // 内容
  'file-text',
  'book-open',
  'notebook-pen',
  'message-square',
  'tag',
  'calendar',
  'clock',
  'link',
  'rss',
  'archive',
  'folder',
  'inbox',
  'search-x',
  // 账户与系统
  'user',
  'users',
  'lock',
  'key-round',
  'shield',
  'log-out',
  'settings',
  'mail',
  'at-sign',
  'database',
  'server',
  'cpu',
  'hard-drive',
  'chart-column',
  'activity',
];

/** `arrow-up-right` → `ArrowUpRight`。 */
export function variantName(slug: string): string {
  return slug
    .split('-')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join('');
}

function escapeAttr(value: string | number): string {
  return String(value).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
}

/** 把 Lucide 节点序列化为 `<svg>` 的内部标记。 */
export function iconBody(nodes: IconNode): string {
  return nodes
    .map(([tag, attrs]) => {
      const pairs = Object.entries(attrs)
        .filter(([key]) => key !== 'key')
        .map(([key, value]) => `${key}="${escapeAttr(value)}"`)
        .join(' ');
      return `<${tag} ${pairs}/>`;
    })
    .join('');
}

function rustString(value: string): string {
  return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

/** 生成 `src/components/ui/icons_data.rs` 的完整内容。 */
export function iconsRust(nodes: Record<string, IconNode>, version: string): string {
  const missing = ICONS.filter((slug) => !nodes[slug]);
  if (missing.length > 0)
    throw new Error(`lucide-static ${version} 缺少图标：${missing.join(', ')}`);
  const lines = [
    `// 由 libs/shared/scripts/generate-icons.ts 从 lucide-static ${version}（ISC）生成，勿手改。`,
    '// 新增图标：在 libs/shared/src/icons.ts 的 ICONS 中添加后运行 `bun run icons`。',
    '',
    '/// Lucide 图标名。渲染见 [`super::Icon`]。',
    '#[allow(dead_code)] // 图标随页面重写逐步启用',
    '#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]',
    'pub enum IconName {',
    ...ICONS.map((slug) => `    ${variantName(slug)},`),
    '}',
    '',
    'impl IconName {',
    '    /// 全部图标，供图鉴与测试遍历。',
    '    #[allow(dead_code)]',
    `    pub const ALL: [IconName; ${ICONS.length}] = [`,
    ...ICONS.map((slug) => `        IconName::${variantName(slug)},`),
    '    ];',
    '',
    '    /// Lucide 原名，如 `arrow-up-right`。',
    "    pub const fn slug(self) -> &'static str {",
    '        match self {',
    ...ICONS.map((slug) => `            IconName::${variantName(slug)} => ${rustString(slug)},`),
    '        }',
    '    }',
    '',
    '    /// `<svg>` 内部标记（24×24 视口，描边绘制）。',
    "    pub(crate) const fn body(self) -> &'static str {",
    '        match self {',
    ...ICONS.map(
      (slug) =>
        `            IconName::${variantName(slug)} => ${rustString(iconBody(nodes[slug]))},`,
    ),
    '        }',
    '    }',
    '}',
    '',
  ];
  return lines.join('\n');
}
