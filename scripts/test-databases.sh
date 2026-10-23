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
export DATABASE_URL="postgresql://${PGUSER}:${PGPASSWORD}@${PGHOST}:${PGPORT}/ygg_mcp_test"
cargo test --locked private_post_workflow_database -- --ignored
cargo test --locked post_write_service_database -- --ignored
export DATABASE_URL="postgresql://${PGUSER}:${PGPASSWORD}@${PGHOST}:${PGPORT}/ygg_notes_test"
cargo test --locked notes_database_ -- --ignored
export YGG_BACKUP_TEST_DATABASE_URL="postgresql://${PGUSER}:${PGPASSWORD}@${PGHOST}:${PGPORT}/ygg_backup_test"
cargo test --locked backup_round_trip_and_failed_restore_rolls_back_database -- --ignored
