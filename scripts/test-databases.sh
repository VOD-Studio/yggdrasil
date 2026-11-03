#!/usr/bin/env bash
# Only use a disposable PostgreSQL instance. The Rust tests enforce database names.
set -euo pipefail
: "${PGHOST:?set PGHOST to the disposable PostgreSQL instance}"
: "${PGPORT:?set PGPORT}"
: "${PGUSER:?set PGUSER}"
: "${PGPASSWORD:?set PGPASSWORD}"
for database in ygg_mcp_test ygg_notes_test ygg_backup_test; do
  createdb "$database"
done
url() { echo "postgresql://${PGUSER}:${PGPASSWORD}@${PGHOST}:${PGPORT}/$1"; }

# 每个 #[ignore] 测试都是数据库回归：从 libtest 列表取全集再按库分组，不手写名字。
# 名字过滤 0 匹配时 libtest 照样退出 0，手写清单漏掉的测试会静默永不执行。
# 分组：*::notes_database_* → ygg_notes_test；backup 模块 → ygg_backup_test；其余 → ygg_mcp_test。
# 测试自己断言 current_database()，分错库会直接失败。
listing=$(cargo test --locked --lib -- --list --ignored)
mcp=()
notes=()
backup=()
while IFS= read -r line; do
  case "$line" in
    *': test') name=${line%': test'} ;;
    *) continue ;;
  esac
  case "$name" in
    *::notes_database_*) notes+=("$name") ;;
    *::backup::*) backup+=("$name") ;;
    *) mcp+=("$name") ;;
  esac
done <<<"$listing"
if [ "${#mcp[@]}" -eq 0 ] || [ "${#notes[@]}" -eq 0 ] || [ "${#backup[@]}" -eq 0 ]; then
  echo "ignored 测试分组为空（mcp=${#mcp[@]} notes=${#notes[@]} backup=${#backup[@]}），列表解析失败？" >&2
  exit 1
fi

run() { cargo test --locked --lib -- --ignored --exact --test-threads=1 "$@"; }
DATABASE_URL=$(url ygg_mcp_test) run "${mcp[@]}"
DATABASE_URL=$(url ygg_notes_test) run "${notes[@]}"
DATABASE_URL=$(url ygg_notes_test) YGG_BACKUP_TEST_DATABASE_URL=$(url ygg_backup_test) run "${backup[@]}"
