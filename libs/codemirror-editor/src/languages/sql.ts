import { PostgreSQL, sql } from '@codemirror/lang-sql';
import type { SqlSchema } from '../editor';

/** 转换从 PostgreSQL 读取的表和列，供 CodeMirror 提供补全。 */
export function sqlExtension(schema: SqlSchema) {
  return sql({
    dialect: PostgreSQL,
    schema: schema.tables.map((table) => ({
      label: table.name,
      type: 'table',
      detail: 'table',
      columns: table.columns.map((column) => ({ label: column, type: 'column' })),
    })),
    upperCaseKeywords: true,
  });
}
