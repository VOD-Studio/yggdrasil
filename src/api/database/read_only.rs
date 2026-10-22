//! SQL 只读判定（数据导出与 SQL 控制台共用）。
//!
//! 只有不修改数据、不建表、不加行锁的 `SELECT` 类查询，以及不带 `ANALYZE`
//! 的 `EXPLAIN <只读查询>` 才算只读。`EXPLAIN ANALYZE` 会真正执行内层语句，
//! `WITH d AS (DELETE ... RETURNING *) SELECT ...` 会在顶层 WITH 中写数据，
//! 二者都不能按语句外形放行。
//!
//! 遍历范围是查询树（WITH 的各 CTE、集合运算两侧、FROM 中的派生表与嵌套 JOIN）。
//! 表达式内的子查询不展开：PostgreSQL 语法不允许在那里出现数据修改语句或
//! `SELECT INTO`，sqlparser 即使能解析，数据库也会拒绝执行。

use sqlparser::ast::{Query, SetExpr, Statement, TableFactor, TableWithJoins, UtilityOption};

/// 语句是否只读：顶层 `Query` 或非 ANALYZE 的 `EXPLAIN <Query>`，且查询树无副作用。
pub(crate) fn is_read_only_statement(stmt: &Statement) -> bool {
    match stmt {
        Statement::Query(query) => QueryEffects::of(query).is_empty(),
        Statement::Explain {
            analyze,
            statement,
            options,
            ..
        } => {
            !*analyze
                && !options_enable_analyze(options.as_deref())
                && matches!(statement.as_ref(), Statement::Query(query) if QueryEffects::of(query).is_empty())
        }
        _ => false,
    }
}

/// 返回语句本身及其内部会被执行的数据修改语句。
///
/// 内部语句包括 `EXPLAIN` 的内层语句，以及查询树里的 INSERT/UPDATE/DELETE/MERGE
/// （如 `WITH d AS (DELETE FROM t RETURNING *) SELECT ...`、`WITH x AS (...) DELETE ...`）。
/// SQL 控制台护栏要逐条检查这些语句，否则包一层 EXPLAIN ANALYZE 或 CTE
/// 就能绕过"UPDATE/DELETE 必须带 WHERE"。
pub(crate) fn executed_statements(stmt: &Statement) -> Vec<&Statement> {
    let mut out = Vec::new();
    push_executed(stmt, &mut out);
    out
}

fn push_executed<'a>(stmt: &'a Statement, out: &mut Vec<&'a Statement>) {
    out.push(stmt);
    match stmt {
        Statement::Explain { statement, .. } => push_executed(statement, out),
        Statement::Query(query) => {
            for dml in QueryEffects::of(query).dml {
                push_executed(dml, out);
            }
        }
        _ => {}
    }
}

/// `EXPLAIN (ANALYZE ...)` 选项形式是否开启了 ANALYZE。
///
/// 只有参数明确为 PostgreSQL 布尔假值（`false`/`off`/`no`/`0` 及其可接受的前缀）
/// 才视为关闭；无法识别的参数一律按开启处理。
fn options_enable_analyze(options: Option<&[UtilityOption]>) -> bool {
    options.unwrap_or_default().iter().any(|opt| {
        opt.name.value.eq_ignore_ascii_case("analyze")
            && match &opt.arg {
                None => true,
                Some(arg) => !is_pg_false(&arg.to_string()),
            }
    })
}

/// PostgreSQL `parse_bool` 认可的假值：`false`/`no` 的任意非空前缀、`of`/`off`、`0`。
fn is_pg_false(raw: &str) -> bool {
    let value = raw.trim().trim_matches('\'').to_ascii_lowercase();
    !value.is_empty()
        && ("false".starts_with(&value)
            || "no".starts_with(&value)
            || value == "of"
            || value == "off"
            || value == "0")
}

/// 查询树中会产生副作用的节点。
#[derive(Default)]
struct QueryEffects<'a> {
    /// 查询树内的 INSERT/UPDATE/DELETE/MERGE。
    dml: Vec<&'a Statement>,
    /// `SELECT ... INTO`（PostgreSQL 中会建表）。
    select_into: bool,
    /// `FOR UPDATE` / `FOR SHARE` 等行锁子句。
    locks: bool,
}

impl<'a> QueryEffects<'a> {
    fn of(query: &'a Query) -> Self {
        let mut effects = Self::default();
        effects.visit_query(query);
        effects
    }

    fn is_empty(&self) -> bool {
        self.dml.is_empty() && !self.select_into && !self.locks
    }

    fn visit_query(&mut self, query: &'a Query) {
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.visit_query(&cte.query);
            }
        }
        if !query.locks.is_empty() {
            self.locks = true;
        }
        self.visit_set_expr(&query.body);
    }

    fn visit_set_expr(&mut self, body: &'a SetExpr) {
        match body {
            SetExpr::Select(select) => {
                if select.into.is_some() {
                    self.select_into = true;
                }
                for table in &select.from {
                    self.visit_table_with_joins(table);
                }
            }
            SetExpr::Query(query) => self.visit_query(query),
            SetExpr::SetOperation { left, right, .. } => {
                self.visit_set_expr(left);
                self.visit_set_expr(right);
            }
            SetExpr::Insert(stmt)
            | SetExpr::Update(stmt)
            | SetExpr::Delete(stmt)
            | SetExpr::Merge(stmt) => self.dml.push(stmt),
            SetExpr::Values(_) | SetExpr::Table(_) => {}
        }
    }

    fn visit_table_with_joins(&mut self, table: &'a TableWithJoins) {
        self.visit_table_factor(&table.relation);
        for join in &table.joins {
            self.visit_table_factor(&join.relation);
        }
    }

    fn visit_table_factor(&mut self, factor: &'a TableFactor) {
        match factor {
            TableFactor::Derived { subquery, .. } => self.visit_query(subquery),
            TableFactor::NestedJoin {
                table_with_joins, ..
            } => self.visit_table_with_joins(table_with_joins),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_one(sql: &str) -> Statement {
        let mut stmts =
            sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::PostgreSqlDialect {}, sql)
                .unwrap_or_else(|e| panic!("{sql}: {e}"));
        assert_eq!(stmts.len(), 1, "{sql}");
        stmts.remove(0)
    }

    #[test]
    fn read_only_statement_accepts_plain_queries_and_plain_explain() {
        for sql in [
            "SELECT 1",
            "SELECT id FROM posts WHERE id > 0 ORDER BY id LIMIT 5",
            "WITH t AS (SELECT 1) SELECT * FROM t",
            "SELECT * FROM (SELECT id FROM posts) AS s JOIN tags ON true",
            "SELECT 1 UNION SELECT 2",
            "VALUES (1), (2)",
            "EXPLAIN SELECT * FROM posts",
            "EXPLAIN VERBOSE SELECT 1",
            "EXPLAIN (FORMAT JSON) SELECT * FROM posts",
            "EXPLAIN (ANALYZE false) SELECT 1",
            "EXPLAIN (ANALYZE off, COSTS) SELECT 1",
            "EXPLAIN (ANALYZE 0) SELECT 1",
        ] {
            assert!(is_read_only_statement(&parse_one(sql)), "应判只读：{sql}");
        }
    }

    #[test]
    fn read_only_statement_rejects_explain_analyze_in_any_form() {
        for sql in [
            "EXPLAIN ANALYZE DELETE FROM posts",
            "EXPLAIN ANALYZE SELECT 1",
            "EXPLAIN (ANALYZE) DELETE FROM posts WHERE id = 1",
            "EXPLAIN (ANALYZE true) SELECT 1",
            "EXPLAIN (FORMAT JSON, ANALYZE on) SELECT 1",
            "EXPLAIN (analyze 1) SELECT 1",
        ] {
            assert!(!is_read_only_statement(&parse_one(sql)), "应拒绝：{sql}");
        }
    }

    #[test]
    fn read_only_statement_rejects_explain_of_non_query() {
        // 内层必须同样满足只读查询规则，即便不带 ANALYZE。
        for sql in [
            "EXPLAIN DELETE FROM posts WHERE id = 1",
            "EXPLAIN INSERT INTO posts (id) VALUES (1)",
            "EXPLAIN WITH d AS (DELETE FROM posts RETURNING *) SELECT * FROM d",
        ] {
            assert!(!is_read_only_statement(&parse_one(sql)), "应拒绝：{sql}");
        }
    }

    #[test]
    fn read_only_statement_rejects_side_effects_inside_queries() {
        for sql in [
            "WITH d AS (DELETE FROM posts RETURNING *) SELECT * FROM d",
            "WITH u AS (UPDATE posts SET title = 'x' WHERE id = 1 RETURNING id) SELECT 1",
            "WITH i AS (INSERT INTO tags (name) VALUES ('x') RETURNING id) SELECT * FROM i",
            "WITH x AS (SELECT 1) DELETE FROM posts WHERE id = 1",
            "SELECT * INTO t FROM posts",
            "SELECT * FROM posts FOR UPDATE",
            "SELECT * FROM posts FOR SHARE SKIP LOCKED",
            "SELECT * FROM (SELECT * FROM posts FOR UPDATE) AS s",
        ] {
            assert!(!is_read_only_statement(&parse_one(sql)), "应拒绝：{sql}");
        }
    }

    #[test]
    fn read_only_statement_rejects_non_query_statements() {
        for sql in [
            "DELETE FROM posts WHERE id = 1",
            "INSERT INTO posts (id) VALUES (1)",
            "TRUNCATE posts",
            "SHOW search_path",
        ] {
            assert!(!is_read_only_statement(&parse_one(sql)), "应拒绝：{sql}");
        }
    }

    #[test]
    fn executed_statements_exposes_dml_behind_explain_and_cte() {
        let explain = parse_one("EXPLAIN ANALYZE DELETE FROM posts");
        let executed = executed_statements(&explain);
        assert!(executed.iter().any(|s| matches!(s, Statement::Delete(_))));

        let cte = parse_one("WITH d AS (UPDATE posts SET title = 'x' RETURNING *) SELECT * FROM d");
        let executed = executed_statements(&cte);
        assert!(executed.iter().any(|s| matches!(s, Statement::Update(_))));

        let select = parse_one("SELECT 1");
        assert_eq!(executed_statements(&select).len(), 1);
    }
}
