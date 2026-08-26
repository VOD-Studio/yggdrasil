//! SQL 补全用 schema 数据模型。
//!
//! 由 `get_db_schema` server function（`api/database/schema.rs`）从
//! `information_schema` 查询填充，两端共享序列化：server 端构造真实数据，
//! WASM 端序列化（`serde_wasm_bindgen::to_value`）后传给 CodeMirror
//! （`bridges::codemirror::EditorInstance::set_schema`）做 SQL 补全。
//!
//! 放在 `models` 而非 `bridges::codemirror`：这是 server function 的返回值 DTO，
//! CodeMirror 只是其中一个消费方，不应反过来让 API 层依赖 UI 桥接层。

use serde::{Deserialize, Serialize};

/// SQL 补全用 schema 数据，由 `get_db_schema` server function 填充。
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct SqlSchema {
    pub tables: Vec<SqlTable>,
}

/// 单张表的补全数据：表名 + 列名列表。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SqlTable {
    pub name: String,
    pub columns: Vec<String>,
}
