//! DuckDB implementation of the `Database` trait.
//!
//! Wraps a `duckdb::Connection` behind `Arc<Mutex<…>>` and uses
//! `spawn_blocking` for all operations since DuckDB is not async.

use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use arrow::datatypes::Schema;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use duckdb::Connection;

use crate::database::Database;
use crate::duckdb_reader::{create_duckdb_query, get_duckdb_table_schema};
use crate::sqp_parser::AnalyzedQuery;

/// DuckDB database backend.
#[derive(Debug, Clone)]
pub struct DuckDbDatabase {
    conn: Arc<Mutex<Connection>>,
}

impl DuckDbDatabase {
    /// Creates a new `DuckDbDatabase` from a pre-configured connection.
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// Creates a new `DuckDbDatabase` by opening a database at the given path.
    pub fn from_path(path: &str) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| anyhow!("Failed to open DuckDB at {}: {}", path, e))?;
        Ok(Self::new(conn))
    }
}

#[async_trait]
impl Database for DuckDbDatabase {
    async fn execute_query(&self, sql: &str) -> Result<Vec<RecordBatch>> {
        let conn = self.conn.clone();
        let sql = sql.to_string();
        tokio::task::spawn_blocking(move || {
            let db = conn.lock()
                .map_err(|e| anyhow!("Failed to lock database: {}", e))?;
            let mut stmt = db.prepare(&sql)?;
            let batches = stmt.query_arrow([])?.collect::<Vec<RecordBatch>>();
            Ok(batches)
        })
        .await
        .map_err(|e| anyhow!("Task join error: {}", e))?
    }

    async fn execute_row_count_query(&self, sql: &str) -> Result<usize> {
        let conn = self.conn.clone();
        let sql = sql.to_string();
        tokio::task::spawn_blocking(move || {
            let db = conn.lock()
                .map_err(|e| anyhow!("Failed to lock database: {}", e))?;
            let mut stmt = db.prepare(&sql)?;
            let count: i64 = stmt.query_row([], |row| row.get(0))?;
            Ok(count as usize)
        })
        .await
        .map_err(|e| anyhow!("Task join error: {}", e))?
    }

    async fn get_table_schema(&self, table_name: &str) -> Result<Schema> {
        let conn = self.conn.clone();
        let table_name = table_name.to_string();
        tokio::task::spawn_blocking(move || {
            let db = conn.lock()
                .map_err(|e| anyhow!("Failed to lock database: {}", e))?;
            get_duckdb_table_schema(&db, &table_name)
        })
        .await
        .map_err(|e| anyhow!("Task join error: {}", e))?
    }

    fn create_sql_query(&self, ast: &AnalyzedQuery) -> Result<String> {
        create_duckdb_query(ast)
    }
}
