/**
 * 从完整 fence info string 提取语言名（首个 token，小写化）。
 *
 * 与后端 parse_fence_info（src/api/code_runner/languages.rs:85）对齐：
 * info string 形如 `python runnable {"timeout_secs":10}`，语言是首个空白分隔的 token，
 * 再 to_lowercase（lowlight 注册名均为小写，大写会导致 Unknown language 抛错）。
 */
export function extractLang(info: string): string {
  const first = info.trim().split(/\s+/)[0];
  return first ? first.toLowerCase() : '';
}

/**
 * 从完整 fence info string 提取 overrides JSON 字符串。
 *
 * 与后端 parse_fence_info（src/api/code_runner/languages.rs:93-97）对齐：
 * info string 形如 `python runnable {"timeout_secs":10}`，提取以 `{` 开头的 token。
 * 无 overrides 时返回空串（Rust 侧空串视为 None）。
 */
export function extractOverridesJson(info: string): string {
  const token = info
    .trim()
    .split(/\s+/)
    .find((t) => t.startsWith('{'));
  return token ?? '';
}
