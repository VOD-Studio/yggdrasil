import { Marked, type marked } from 'marked';

/**
 * 文章编辑器（full 变体）专用的 marked 实例。
 *
 * - 服务端 pulldown-cmark 不做 GFM 裸链接：关掉 `url` tokenizer，裸 URL / 裸邮箱保持纯文本。
 * - 单个 `~` 交给 inline-html.ts 的下标 tokenizer；marked 的 GFM `del` 会把单 `~` 也当删除线，
 *   这里只放行 `~~`，其余单 `~` 不成立时按字面量处理。
 *
 * marked 默认是全局单例，tokenizer 注册到它会漏进同页的评论编辑器（评论服务端渲染选项不同），
 * 所以每个 full 编辑器用自己的实例。
 */
export function createArticleMarked(): typeof marked {
  const instance = new Marked();
  instance.use({
    tokenizer: {
      url() {
        return undefined;
      },
      del(src: string) {
        // 返回 undefined = 不匹配；返回 false = 回落 marked 原实现。
        return /^~(?!~)/.test(src) ? undefined : false;
      },
    },
  });
  return instance as unknown as typeof marked;
}
