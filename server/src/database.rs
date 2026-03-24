//! DuckDB query execution and Arrow IPC serialization.
//!
//! Isolates all direct DuckDB interaction so the query handler only
//! deals with request parsing, payment logic, and response building.

use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use duckdb::{Connection, Result as DuckResult};
use std::sync::MutexGuard;

/// Executes a SQL query against DuckDB and returns the results as Arrow record batches.
pub fn execute_query(db: &Connection, query: &str) -> DuckResult<Vec<RecordBatch>> {
    let mut stmt = db.prepare(query)?;
    let batches= stmt.query_arrow([])?.collect::<Vec<RecordBatch>>();
    
    Ok(batches)
}

/// Executes a query expected to return a single integer (e.g., `COUNT(*)`).
/// Used during the estimation step to determine the price before payment.
pub fn execute_row_count_query(db: &MutexGuard<Connection>, query: &str) -> DuckResult<usize> {
    let mut stmt = db.prepare(query)?;
    stmt.query_row([], |row| {
        let count: i64 = row.get(0)?;
        Ok(count as usize)
    })
} 

/// Converts Arrow record batches into the Arrow IPC streaming format.
/// Returns an empty buffer if `batches` is empty.
pub fn serialize_batches_to_arrow_ipc(batches: &[RecordBatch]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
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