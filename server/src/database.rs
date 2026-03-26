//! Database trait abstraction and shared utilities.
//!
//! Defines the async `Database` trait that all database backends must implement,
//! plus backend-agnostic helpers like Arrow IPC serialization.

use anyhow::Result;
use arrow::datatypes::Schema;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;

use crate::sqp_parser::AnalyzedQuery;

/// Async database abstraction.
///
/// Each backend (DuckDB, Postgres, ClickHouse, ...) implements this trait,
/// allowing the rest of the server to be database-agnostic.
#[async_trait]
pub trait Database: Send + Sync + std::fmt::Debug {
    /// Executes a SQL query and returns the results as Arrow record batches.
    async fn execute_query(&self, sql: &str) -> Result<Vec<RecordBatch>>;

    /// Executes a query expected to return a single integer (e.g., `COUNT(*)`).
    async fn execute_row_count_query(&self, sql: &str) -> Result<usize>;

    /// Returns the Arrow schema of the given table.
    async fn get_table_schema(&self, table_name: &str) -> Result<Schema>;

    /// Generates a backend-specific SQL query string from an analyzed query AST.
    fn create_sql_query(&self, ast: &AnalyzedQuery) -> Result<String>;
}

/// Converts Arrow record batches into Arrow IPC streaming format (backend-agnostic).
///
/// Returns an empty buffer if `batches` is empty.
pub fn serialize_batches_to_arrow_ipc(
    batches: &[RecordBatch],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut buffer = Vec::new();
    if let Some(first_batch) = batches.first() {
        let schema = first_batch.schema();
        let mut writer = StreamWriter::try_new(&mut buffer, &schema)?;
        for batch in batches {
            writer.write(&batch)?;
        }
        writer.finish()?;
    }
    Ok(buffer)
}
