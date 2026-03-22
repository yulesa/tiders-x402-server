use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use duckdb::{Connection, Result as DuckResult};
use std::sync::MutexGuard;

pub fn execute_query(db: &Connection, query: &str) -> DuckResult<Vec<RecordBatch>> {
    let mut stmt = db.prepare(query)?;
    let batches= stmt.query_arrow([])?.collect::<Vec<RecordBatch>>();
    
    Ok(batches)
}

pub fn execute_row_count_query(db: &MutexGuard<Connection>, query: &str) -> DuckResult<usize> {
    let mut stmt = db.prepare(query)?;
    stmt.query_row([], |row| {
        let count: i64 = row.get(0)?;
        Ok(count as usize)
    })
} 

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