//! ClickHouse implementation of the `Database` trait.
//!
//! Uses the `clickhouse` crate (natively async) with `FORMAT ArrowStream` to
//! get results directly as Arrow IPC, avoiding JSON intermediate representation.

use std::io::Cursor;

use anyhow::{Result, anyhow};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::reader::StreamReader;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use clickhouse::Client;
use serde::Deserialize;

use crate::database::Database;
use crate::sql_clickhouse::create_clickhouse_query;
use crate::sqp_parser::AnalyzedQuery;

/// ClickHouse database backend.
#[derive(Clone)]
pub struct ClickHouseDatabase {
    client: Client,
}

impl std::fmt::Debug for ClickHouseDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClickHouseDatabase").finish()
    }
}

impl ClickHouseDatabase {
    /// Creates a new `ClickHouseDatabase` with full client configuration.
    ///
    /// # Parameters
    /// - `url`: ClickHouse HTTP endpoint (e.g., "http://localhost:8123")
    /// - `user`: Database user (`None` for default)
    /// - `password`: Database password (`None` for default)
    /// - `database`: Database name (`None` for default)
    /// - `access_token`: Access token for authentication (`None` to skip)
    /// - `compression`: Compression mode: "none" or "lz4" (`None` for default)
    /// - `options`: Additional ClickHouse settings as key-value pairs
    /// - `headers`: Additional HTTP headers as key-value pairs
    #[allow(clippy::too_many_arguments)]
    pub fn from_params(
        url: &str,
        user: Option<&str>,
        password: Option<&str>,
        database: Option<&str>,
        access_token: Option<&str>,
        compression: Option<&str>,
        options: Option<Vec<(String, String)>>,
        headers: Option<Vec<(String, String)>>,
    ) -> Result<Self> {
        let mut client = Client::default().with_url(url);

        if let Some(user) = user {
            client = client.with_user(user);
        }
        if let Some(password) = password {
            client = client.with_password(password);
        }
        if let Some(database) = database {
            client = client.with_database(database);
        }
        if let Some(token) = access_token {
            client = client.with_access_token(token);
        }
        if let Some(comp) = compression {
            let c = match comp {
                "none" => clickhouse::Compression::None,
                "lz4" => clickhouse::Compression::Lz4,
                other => {
                    return Err(anyhow!(
                        "Unknown compression '{}'. Use 'none' or 'lz4'",
                        other
                    ));
                }
            };
            client = client.with_compression(c);
        }
        if let Some(opts) = options {
            for (k, v) in opts {
                client = client.with_option(k, v);
            }
        }
        if let Some(hdrs) = headers {
            for (k, v) in hdrs {
                client = client.with_header(k, v);
            }
        }

        Ok(Self { client })
    }

    /// Creates a new `ClickHouseDatabase` from a user-managed client.
    pub fn from_client(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Database for ClickHouseDatabase {
    async fn execute_query(&self, sql: &str) -> Result<Vec<RecordBatch>> {
        let bytes = self
            .client
            .query(sql)
            .fetch_bytes("ArrowStream")
            .map_err(|e| anyhow!("Failed to prepare ClickHouse query: {}", e))?
            .collect()
            .await
            .map_err(|e| anyhow!("Failed to execute ClickHouse query: {}", e))?;

        if bytes.is_empty() {
            return Ok(vec![]);
        }

        read_arrow_stream(&bytes)
    }

    async fn execute_row_count_query(&self, sql: &str) -> Result<usize> {
        let bytes = self
            .client
            .query(sql)
            .fetch_bytes("ArrowStream")
            .map_err(|e| anyhow!("Failed to prepare row count query: {}", e))?
            .collect()
            .await
            .map_err(|e| anyhow!("Failed to execute row count query: {}", e))?;

        let batches = read_arrow_stream(&bytes)?;
        let batch = batches
            .first()
            .ok_or_else(|| anyhow!("Empty response from row count query"))?;

        if batch.num_rows() == 0 || batch.num_columns() == 0 {
            return Err(anyhow!("No data in row count response"));
        }

        // Extract the first column's first row as the count.
        // ClickHouse may return UInt64 for COUNT(*), so handle multiple integer types.
        let col = batch.column(0);
        let count: i64 = match col.data_type() {
            DataType::UInt64 => {
                arrow::array::cast::as_primitive_array::<arrow::datatypes::UInt64Type>(col).value(0)
                    as i64
            }
            DataType::Int64 => {
                arrow::array::cast::as_primitive_array::<arrow::datatypes::Int64Type>(col).value(0)
            }
            DataType::UInt32 => {
                arrow::array::cast::as_primitive_array::<arrow::datatypes::UInt32Type>(col).value(0)
                    as i64
            }
            DataType::Int32 => {
                arrow::array::cast::as_primitive_array::<arrow::datatypes::Int32Type>(col).value(0)
                    as i64
            }
            other => {
                return Err(anyhow!("Unexpected column type for row count: {:?}", other));
            }
        };

        Ok(count as usize)
    }

    async fn get_table_schema(&self, table_name: &str) -> Result<Schema> {
        let query_str = format!(
            "SELECT name, type FROM system.columns WHERE table = '{}' AND database = currentDatabase()",
            table_name
        );

        let rows = self
            .client
            .query(&query_str)
            .fetch_all::<ColumnInfo>()
            .await
            .map_err(|e| anyhow!("Failed to describe table '{}': {}", table_name, e))?;

        if rows.is_empty() {
            return Err(anyhow!(
                "Table '{}' not found or has no columns",
                table_name
            ));
        }

        let fields: Vec<Field> = rows
            .iter()
            .map(|col| {
                let arrow_type = ch_type_to_arrow(&col.r#type);
                Field::new(&col.name, arrow_type, true)
            })
            .collect();

        Ok(Schema::new(fields))
    }

    fn create_sql_query(&self, ast: &AnalyzedQuery) -> Result<String> {
        create_clickhouse_query(ast)
    }
}

// --- Helper structs ---

/// Row type for `DESCRIBE TABLE` results.
#[derive(Debug, Deserialize, clickhouse::Row)]
struct ColumnInfo {
    name: String,
    r#type: String,
}

// --- Helper functions ---

/// Maps a ClickHouse type string to its Arrow equivalent.
fn ch_type_to_arrow(ch_type: &str) -> DataType {
    // Strip Nullable(...) wrapper
    let inner = if ch_type.starts_with("Nullable(") && ch_type.ends_with(')') {
        &ch_type[9..ch_type.len() - 1]
    } else {
        ch_type
    };

    // Strip LowCardinality(...) wrapper
    let inner = if inner.starts_with("LowCardinality(") && inner.ends_with(')') {
        &inner[15..inner.len() - 1]
    } else {
        inner
    };

    match inner {
        "Bool" | "UInt8" if ch_type.contains("Bool") => DataType::Boolean,
        "Int8" => DataType::Int8,
        "Int16" => DataType::Int16,
        "Int32" => DataType::Int32,
        "Int64" => DataType::Int64,
        "UInt8" => DataType::UInt8,
        "UInt16" => DataType::UInt16,
        "UInt32" => DataType::UInt32,
        "UInt64" => DataType::UInt64,
        "Float32" => DataType::Float32,
        "Float64" => DataType::Float64,
        "String" | "FixedString" => DataType::Utf8,
        "Date" | "Date32" => DataType::Date32,
        "DateTime" | "DateTime64" => {
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None)
        }
        "UUID" => DataType::Utf8,
        _ if inner.starts_with("FixedString(") => DataType::Utf8,
        _ if inner.starts_with("DateTime64(") => {
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None)
        }
        _ if inner.starts_with("DateTime(") => {
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None)
        }
        _ if inner.starts_with("Decimal") => parse_decimal_type(inner),
        _ if inner.starts_with("Enum8") || inner.starts_with("Enum16") => DataType::Utf8,
        _ => DataType::Utf8,
    }
}

/// Parses ClickHouse Decimal types into Arrow Decimal128.
fn parse_decimal_type(ch_type: &str) -> DataType {
    if ch_type.starts_with("Decimal128(") || ch_type.starts_with("Decimal256(") {
        if let Some(s) = extract_single_param(ch_type) {
            return DataType::Decimal128(38, s as i8);
        }
    } else if ch_type.starts_with("Decimal64(") {
        if let Some(s) = extract_single_param(ch_type) {
            return DataType::Decimal128(18, s as i8);
        }
    } else if ch_type.starts_with("Decimal32(") {
        if let Some(s) = extract_single_param(ch_type) {
            return DataType::Decimal128(9, s as i8);
        }
    } else if ch_type.starts_with("Decimal(")
        && let Some((p, s)) = extract_two_params(ch_type)
    {
        return DataType::Decimal128(p.min(38) as u8, s as i8);
    }
    DataType::Decimal128(38, 0)
}

/// Extracts a single numeric parameter from a type string, e.g. `Decimal64(4)` → `4`.
fn extract_single_param(s: &str) -> Option<u32> {
    let start = s.find('(')? + 1;
    let end = s.find(')')?;
    s[start..end].trim().parse().ok()
}

/// Extracts two numeric parameters from a type string, e.g. `Decimal(20, 4)` → `(20, 4)`.
fn extract_two_params(s: &str) -> Option<(u32, u32)> {
    let start = s.find('(')? + 1;
    let end = s.find(')')?;
    let inner = &s[start..end];
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() == 2 {
        let p = parts[0].trim().parse().ok()?;
        let s = parts[1].trim().parse().ok()?;
        Some((p, s))
    } else {
        None
    }
}

/// Reads Arrow IPC stream bytes into a vector of `RecordBatch`es.
fn read_arrow_stream(bytes: &[u8]) -> Result<Vec<RecordBatch>> {
    let cursor = Cursor::new(bytes);
    let reader = StreamReader::try_new(cursor, None)
        .map_err(|e| anyhow!("Failed to read ArrowStream: {}", e))?;

    let mut batches = Vec::new();
    for batch_result in reader {
        let batch = batch_result.map_err(|e| anyhow!("Failed to read Arrow batch: {}", e))?;
        batches.push(batch);
    }
    Ok(batches)
}
